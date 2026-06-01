use super::*;

#[derive(Debug, Clone)]
pub(super) struct TaskResearchValidationFailure {
    pub(super) output: String,
    pub(super) message: String,
}

pub(super) async fn execute_claimed_task(state: Arc<AppState>, task: TaskInfo) {
    if matches!(
        task.file_prefix.as_deref(),
        Some("[Scrape]") | Some("[Scrape+KO]")
    ) {
        execute_scrape_task(&state, task).await;
        return;
    }

    let model_input = task.model.clone().unwrap_or_default();
    let filenames: Vec<String> = serde_json::from_str(
        &task
            .source_filenames
            .clone()
            .unwrap_or_else(|| "[]".to_string()),
    )
    .unwrap_or_default();
    let cleanup_files: Vec<String> = serde_json::from_str(
        &task
            .cleanup_files
            .clone()
            .unwrap_or_else(|| "[]".to_string()),
    )
    .unwrap_or_default();
    let file_prefix = task.file_prefix.clone().unwrap_or_default();
    let file_type = task.file_type.clone().unwrap_or_else(|| "md".to_string());
    let system_prompt = crate::research_design::hydrate_html_design_prompt_for_execution(
        &task.system_prompt.clone().unwrap_or_default(),
    );
    let user_prompt = task.user_prompt.clone().unwrap_or_default();
    let (source, model_name) = if let Some(model) = model_input.strip_prefix("cli:") {
        ("cli", model)
    } else if let Some(model) = model_input.strip_prefix("pi:") {
        ("pi", model)
    } else {
        ("ollama", model_input.as_str())
    };

    execute_ai_task_with_quality_loop(
        &state,
        task,
        filenames,
        cleanup_files,
        &file_prefix,
        &file_type,
        model_name,
        source,
        &system_prompt,
        &user_prompt,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_ai_task_with_quality_loop(
    state: &AppState,
    task: TaskInfo,
    filenames: Vec<String>,
    cleanup_files: Vec<String>,
    file_prefix: &str,
    file_type: &str,
    model_name: &str,
    source: &str,
    system_prompt: &str,
    user_prompt: &str,
) {
    let runtime = AppModelRuntime::new(state);
    let diagnostics_repo =
        crate::application::ai_runtime::StateResearchDiagnosticsRepository::new(state);
    let source_acquisition = DefaultSourceAcquisition;
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        let result_text = runtime
            .execute(ModelRuntimeRequest {
                task_id: task.id,
                filenames,
                model_name,
                source,
                system_prompt,
                user_prompt,
                research_subject_prompt: None,
                file_prefix,
                web_search_requested: task.web_search_requested.as_deref(),
                web_search_provider_override: task.web_search_provider.as_deref(),
                research_intensity: task.research_intensity.as_deref(),
                fallback_used: task.fallback_used.as_deref() == Some("true"),
                fallback_reason: task.fallback_reason.as_deref(),
            })
            .await;
        handle_task_completion(
            state,
            task.id,
            result_text,
            task.original_name,
            file_prefix,
            file_type,
            cleanup_files,
            true,
        )
        .await;
        return;
    }

    let max_iterations = normalized_quality_max_iterations(
        file_prefix,
        task.quality_max_iterations,
        task.research_intensity.as_deref(),
    );
    let mut attempt_user_prompt = user_prompt.to_string();
    let mut last_quality_failure: Option<String> = None;
    let mut pending_repair_hint_urls = HashSet::new();
    let mut controller_events = Vec::new();

    for iteration in 1..=max_iterations {
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_PLAN,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_RUNNING,
            Some("Preparing the research pass and quality criteria."),
            &mut controller_events,
        )
        .await;
        update_quality_progress(
            state,
            task.id,
            &task.original_name,
            iteration,
            max_iterations,
            "researching",
            last_quality_failure.as_deref(),
        )
        .await;
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_SEARCH,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_RUNNING,
            Some("Gathering or refreshing evidence for this research pass."),
            &mut controller_events,
        )
        .await;

        let result_text = runtime
            .execute(ModelRuntimeRequest {
                task_id: task.id,
                filenames: filenames.clone(),
                model_name,
                source,
                system_prompt,
                user_prompt: &attempt_user_prompt,
                research_subject_prompt: Some(research_source_subject_for_task(&task, user_prompt)),
                file_prefix,
                web_search_requested: task.web_search_requested.as_deref(),
                web_search_provider_override: task.web_search_provider.as_deref(),
                research_intensity: task.research_intensity.as_deref(),
                fallback_used: task.fallback_used.as_deref() == Some("true"),
                fallback_reason: task.fallback_reason.as_deref(),
            })
            .await;

        let Some(output) = result_text else {
            handle_task_completion(
                state,
                task.id,
                None,
                task.original_name,
                file_prefix,
                file_type,
                cleanup_files,
                true,
            )
            .await;
            return;
        };
        let normalized_output = normalize_ai_output(&output, file_type);
        if normalized_output.trim().len() < 10 {
            handle_task_completion(
                state,
                task.id,
                Some(normalized_output),
                task.original_name,
                file_prefix,
                file_type,
                cleanup_files,
                true,
            )
            .await;
            return;
        }
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_SOURCE_CARDS,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_COMPLETED,
            Some("Source-card material should now be present in the normalized draft."),
            &mut controller_events,
        )
        .await;
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_CLAIM_LOG,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_COMPLETED,
            Some("Claim-log material should now be present in the normalized draft."),
            &mut controller_events,
        )
        .await;
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_DRAFT,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_COMPLETED,
            Some("A draft research artifact was produced and normalized."),
            &mut controller_events,
        )
        .await;
        update_research_controller_progress(
            state,
            task.id,
            &task.original_name,
            RESEARCH_STAGE_QUALITY_GATE,
            iteration,
            max_iterations,
            RESEARCH_CONTROLLER_STATUS_RUNNING,
            Some("Checking source coverage, topic relevance, controller sections, and known failure patterns."),
            &mut controller_events,
        )
        .await;
        persist_iteration_research_artifacts(
            state,
            task.id,
            &controller_events,
            file_type,
            &normalized_output,
            task_uses_local_pi(&task),
            task.research_intensity.as_deref(),
            task.quality_depth.as_deref(),
            task.research_topic.as_deref(),
            task.research_instructions.as_deref(),
            Some(research_source_subject_for_task(&task, user_prompt)),
        )
        .await;
        let _enrichment_report = run_narrative_enrichment_stage(
            state,
            &task,
            &runtime,
            model_name,
            source,
            file_prefix,
            user_prompt,
            iteration,
            max_iterations,
            &mut controller_events,
        )
        .await;
        let finalized_output = finalize_task_research_output(
            state,
            task.id,
            &normalized_output,
            file_prefix,
            file_type,
        )
        .await;
        let current_diagnostics = diagnostics_repo.load(task.id).await;
        let independently_acquired_urls =
            collect_repair_search_known_urls(current_diagnostics.as_ref());
        refresh_pending_repair_hint_urls(
            &mut pending_repair_hint_urls,
            &[],
            &independently_acquired_urls,
        );

        match validate_task_research_output(
            state,
            task.id,
            &finalized_output,
            file_prefix,
            file_type,
            if pending_repair_hint_urls.is_empty() {
                None
            } else {
                Some(&pending_repair_hint_urls)
            },
        )
        .await
        {
            Ok(validated_output) => {
                update_research_controller_progress(
                    state,
                    task.id,
                    &task.original_name,
                    RESEARCH_STAGE_FINAL,
                    iteration,
                    max_iterations,
                    RESEARCH_CONTROLLER_STATUS_COMPLETED,
                    Some("Quality gate passed; final output is being saved."),
                    &mut controller_events,
                )
                .await;
                update_quality_progress(
                    state,
                    task.id,
                    &task.original_name,
                    iteration,
                    max_iterations,
                    "passed",
                    None,
                )
                .await;
                handle_task_completion(
                    state,
                    task.id,
                    Some(validated_output),
                    task.original_name,
                    file_prefix,
                    file_type,
                    cleanup_files,
                    false,
                )
                .await;
                return;
            }
            Err(failure) if iteration < max_iterations => {
                update_research_controller_progress(
                    state,
                    task.id,
                    &task.original_name,
                    RESEARCH_STAGE_REPAIR_PLANNING,
                    iteration,
                    max_iterations,
                    RESEARCH_CONTROLLER_STATUS_RUNNING,
                    Some(&failure.message),
                    &mut controller_events,
                )
                .await;
                update_research_controller_progress(
                    state,
                    task.id,
                    &task.original_name,
                    RESEARCH_STAGE_EVIDENCE_REPAIR,
                    iteration,
                    max_iterations,
                    RESEARCH_CONTROLLER_STATUS_RUNNING,
                    Some("Building targeted evidence-repair instructions from persisted artifacts and diagnostics."),
                    &mut controller_events,
                )
                .await;
                update_quality_progress(
                    state,
                    task.id,
                    &task.original_name,
                    iteration,
                    max_iterations,
                    "repairing",
                    Some(&failure.message),
                )
                .await;
                last_quality_failure = Some(failure.message.clone());
                let artifacts = load_task_research_artifacts(state, task.id).await;
                let diagnostics = diagnostics_repo.load(task.id).await;
                let independently_acquired_urls =
                    collect_repair_search_known_urls(diagnostics.as_ref());
                refresh_pending_repair_hint_urls(
                    &mut pending_repair_hint_urls,
                    &[],
                    &independently_acquired_urls,
                );
                let repair_search_hints = match repair_search_subject(
                    user_prompt,
                    artifacts.as_ref(),
                    diagnostics.as_ref(),
                ) {
                    Some(subject) => {
                        let queries =
                            collect_repair_search_queries(artifacts.as_ref(), diagnostics.as_ref());
                        let known_urls = independently_acquired_urls.clone();
                        source_acquisition
                            .collect_repair_search_hints(&subject, &queries, &known_urls)
                            .await
                    }
                    None => Vec::new(),
                };
                refresh_pending_repair_hint_urls(
                    &mut pending_repair_hint_urls,
                    &repair_search_hints,
                    &independently_acquired_urls,
                );
                attempt_user_prompt = build_quality_repair_prompt(
                    user_prompt,
                    &failure.message,
                    iteration + 1,
                    max_iterations,
                    artifacts.as_ref(),
                    diagnostics.as_ref(),
                    &repair_search_hints,
                );
            }
            Err(failure) => {
                update_research_controller_progress(
                    state,
                    task.id,
                    &task.original_name,
                    RESEARCH_STAGE_UNTRUSTED,
                    iteration,
                    max_iterations,
                    RESEARCH_CONTROLLER_STATUS_FAILED,
                    Some(&failure.message),
                    &mut controller_events,
                )
                .await;
                update_quality_progress(
                    state,
                    task.id,
                    &task.original_name,
                    iteration,
                    max_iterations,
                    "untrusted",
                    Some(&failure.message),
                )
                .await;
                handle_untrusted_research_completion(
                    state,
                    task.id,
                    failure.output,
                    task.original_name,
                    file_prefix,
                    file_type,
                    cleanup_files,
                    &failure.message,
                )
                .await;
                return;
            }
        }
    }
}

pub(super) fn research_source_subject_for_task<'a>(
    task: &'a TaskInfo,
    fallback_prompt: &'a str,
) -> &'a str {
    task.research_topic
        .as_deref()
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .unwrap_or(fallback_prompt)
}

pub(super) async fn update_quality_progress(
    state: &AppState,
    task_id: i64,
    original_name: &str,
    current_iteration: i64,
    max_iterations: i64,
    quality_status: &str,
    last_failure: Option<&str>,
) {
    let _ = sqlx::query("UPDATE tasks SET quality_current_iteration = ?, quality_max_iterations = ?, quality_status = ?, quality_last_failure = ? WHERE id = ?")
        .bind(current_iteration)
        .bind(max_iterations)
        .bind(quality_status)
        .bind(last_failure)
        .bind(task_id)
        .execute(&state.db)
        .await;
    let controller_progress =
        sqlx::query_as::<_, (Option<String>, Option<i64>, Option<i64>)>(
            "SELECT research_controller_stage, research_controller_iteration, research_controller_max_iterations FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let _ = state.tx.send(TaskUpdateEvent {
        id: task_id,
        status: "researching".to_string(),
        original_name: original_name.to_string(),
        quality_current_iteration: Some(current_iteration),
        quality_max_iterations: Some(max_iterations),
        quality_status: Some(quality_status.to_string()),
        research_controller_stage: controller_progress
            .as_ref()
            .and_then(|(stage, _, _)| stage.clone()),
        research_controller_iteration: controller_progress
            .as_ref()
            .and_then(|(_, iteration, _)| *iteration),
        research_controller_max_iterations: controller_progress
            .as_ref()
            .and_then(|(_, _, max)| *max),
    });
}

pub(super) async fn update_research_controller_progress(
    state: &AppState,
    task_id: i64,
    original_name: &str,
    stage: &str,
    iteration: i64,
    max_iterations: i64,
    status: &str,
    detail: Option<&str>,
    events: &mut Vec<ResearchControllerEvent>,
) {
    events.push(ResearchControllerEvent {
        stage: stage.to_string(),
        iteration,
        max_iterations,
        status: status.to_string(),
        detail: detail.map(|detail| detail.chars().take(500).collect()),
    });
    if events.len() > RESEARCH_CONTROLLER_ARTIFACT_LIMIT {
        let drain_count = events.len() - RESEARCH_CONTROLLER_ARTIFACT_LIMIT;
        events.drain(0..drain_count);
    }
    let mut artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.events = events.clone();
    let artifacts_json = serde_json::to_string(&artifacts).unwrap_or_else(|_| "{}".to_string());

    let _ = sqlx::query("UPDATE tasks SET research_controller_stage = ?, research_controller_iteration = ?, research_controller_max_iterations = ?, research_controller_artifacts_json = ? WHERE id = ?")
        .bind(stage)
        .bind(iteration)
        .bind(max_iterations)
        .bind(artifacts_json)
        .bind(task_id)
        .execute(&state.db)
        .await;
    let _ = state.tx.send(TaskUpdateEvent {
        id: task_id,
        status: "researching".to_string(),
        original_name: original_name.to_string(),
        quality_current_iteration: None,
        quality_max_iterations: None,
        quality_status: None,
        research_controller_stage: Some(stage.to_string()),
        research_controller_iteration: Some(iteration),
        research_controller_max_iterations: Some(max_iterations),
    });
}

pub(super) async fn load_task_research_artifacts(
    state: &AppState,
    task_id: i64,
) -> Option<ResearchControllerArtifacts> {
    let json = sqlx::query_scalar::<_, Option<String>>(
        "SELECT research_controller_artifacts_json FROM tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .flatten()?;
    serde_json::from_str(&json).ok()
}

pub(super) async fn persist_research_controller_artifacts(
    state: &AppState,
    task_id: i64,
    artifacts: &ResearchControllerArtifacts,
) {
    let Ok(artifacts_json) = serde_json::to_string(artifacts) else {
        return;
    };
    let _ = sqlx::query("UPDATE tasks SET research_controller_artifacts_json = ? WHERE id = ?")
        .bind(artifacts_json)
        .bind(task_id)
        .execute(&state.db)
        .await;
}

pub(super) async fn persist_research_source_diagnostics(
    state: &AppState,
    task_id: i64,
    diagnostics: ResearchSourceDiagnosticsEnvelope,
) {
    let repo = crate::application::ai_runtime::StateResearchDiagnosticsRepository::new(state);
    let _ = repo.persist(task_id, diagnostics).await;
}

pub(super) fn task_uses_local_pi(task: &TaskInfo) -> bool {
    task.engine_kind.as_deref() == Some("pi_ollama")
        || task
            .model
            .as_deref()
            .is_some_and(|model| model.trim_start().starts_with("pi:"))
        || task
            .resolved_model
            .as_deref()
            .is_some_and(|model| model.trim_start().starts_with("pi:"))
}

#[allow(unused_imports)]
pub(super) use liquid_research_classic::{
    has_local_pi_source_pack_source_card_scaffold, has_supported_claim_log_for_scaffold,
    local_pi_repaired_claim_log_for_iteration, local_pi_source_pack_scaffold_cards_for_iteration,
    preserve_local_pi_scaffolded_source_cards_for_iteration,
    scaffold_claim_has_direct_public_url_support, scaffold_support_url_is_public_full_url,
    scaffold_trust_block_failure, source_card_id_is_public_support,
    MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR,
};

pub(super) async fn persist_iteration_research_artifacts(
    state: &AppState,
    task_id: i64,
    events: &[ResearchControllerEvent],
    file_type: &str,
    normalized_output: &str,
    is_local_pi: bool,
    research_intensity: Option<&str>,
    quality_depth: Option<&str>,
    research_topic: Option<&str>,
    research_instructions: Option<&str>,
    evidence_subject: Option<&str>,
) {
    let mut artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.events = events.to_vec();
    let diagnostics = load_research_source_diagnostics(state, task_id).await;
    match parse_research_artifact_block(normalized_output, file_type) {
        Ok(mut parsed) => {
            let mut scaffold_authorized = has_local_pi_source_pack_source_card_scaffold(&artifacts);
            if let Some(source_cards) = local_pi_source_pack_scaffold_cards_for_iteration(
                &artifacts,
                Some(&parsed),
                None,
                diagnostics.as_ref(),
                is_local_pi,
            ) {
                parsed.source_cards = source_cards;
                scaffold_authorized = true;
                push_unique_warning(
                    &mut parsed.warnings,
                    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string(),
                );
            }
            preserve_local_pi_scaffolded_source_cards_for_iteration(
                &artifacts,
                &mut parsed,
                is_local_pi,
            );
            let repair_scaffold_authorized =
                scaffold_authorized && has_local_pi_source_pack_source_card_scaffold(&parsed);
            if let Some(claim_log) = local_pi_repaired_claim_log_for_iteration(
                &artifacts,
                parsed.claim_log.is_empty(),
                repair_scaffold_authorized,
                normalized_output,
                &parsed.source_cards,
                is_local_pi,
            ) {
                parsed.claim_log = claim_log;
                push_unique_warning(
                    &mut parsed.warnings,
                    PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING.to_string(),
                );
            }
            let trusted_source_urls =
                trusted_artifact_merge_source_urls(&artifacts, diagnostics.as_ref());
            merge_research_controller_artifacts_with_trusted_source_urls(
                &mut artifacts,
                parsed,
                &trusted_source_urls,
            );
            let quality_context = ResearchQualityContext {
                file_prefix: "[AI-Research]",
                file_type,
                web_search_requested: true,
                research_intensity,
                quality_depth,
                research_topic,
                research_instructions,
                evidence_subject,
            };
            repair_historical_planning_scaffold_from_visible_output(
                normalized_output,
                &mut artifacts,
                &quality_context,
                Some(&trusted_source_urls),
            );
            close_research_debts_for_gate(&mut artifacts.research_debt, "artifact_parse", None);
            normalize_deferred_conflicts_to_actionable_debt(&mut artifacts);
        }
        Err(error) => {
            let mut scaffold_authorized = has_local_pi_source_pack_source_card_scaffold(&artifacts);
            if let Some(source_cards) = local_pi_source_pack_scaffold_cards_for_iteration(
                &artifacts,
                None,
                Some(&error),
                diagnostics.as_ref(),
                is_local_pi,
            ) {
                artifacts.source_cards = source_cards;
                scaffold_authorized = true;
                push_unique_warning(
                    &mut artifacts.warnings,
                    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string(),
                );
            }
            if let Some(claim_log) = local_pi_repaired_claim_log_for_iteration(
                &artifacts,
                true,
                scaffold_authorized,
                normalized_output,
                &artifacts.source_cards,
                is_local_pi,
            ) {
                artifacts.claim_log = claim_log;
                push_unique_warning(
                    &mut artifacts.warnings,
                    PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING.to_string(),
                );
            }
            let trusted_source_urls =
                trusted_artifact_merge_source_urls(&artifacts, diagnostics.as_ref());
            let quality_context = ResearchQualityContext {
                file_prefix: "[AI-Research]",
                file_type,
                web_search_requested: true,
                research_intensity,
                quality_depth,
                research_topic,
                research_instructions,
                evidence_subject,
            };
            repair_historical_planning_scaffold_from_visible_output(
                normalized_output,
                &mut artifacts,
                &quality_context,
                Some(&trusted_source_urls),
            );
            push_unique_warning(&mut artifacts.warnings, error.clone());
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: "artifact-parse".to_string(),
                    severity: "high".to_string(),
                    failed_gate: Some("artifact_parse".to_string()),
                    missing_evidence: error,
                    required_source_class: None,
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![
                        "Emit a valid machine-readable research artifact JSON block in the verification appendix.".to_string(),
                    ],
                    status: "open".to_string(),
                },
            );
        }
    }
    persist_research_controller_artifacts(state, task_id, &artifacts).await;
}

pub(super) async fn finalize_task_research_output(
    state: &AppState,
    task_id: i64,
    normalized_output: &str,
    file_prefix: &str,
    file_type: &str,
) -> String {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return normalized_output.to_string();
    }
    let task = match sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&state.db)
        .await
    {
        Ok(task) => task,
        Err(_) => return normalized_output.to_string(),
    };
    let artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    if artifacts.source_cards.is_empty()
        || (artifacts.claim_log.is_empty()
            && !has_local_pi_source_pack_source_card_scaffold(&artifacts))
    {
        return normalized_output.to_string();
    }
    let diagnostics = load_research_source_diagnostics(state, task_id).await;
    let evidence_subject =
        research_source_diagnostics_subject(task.research_source_diagnostics_json.as_deref());
    let context = ResearchQualityContext {
        file_prefix,
        file_type,
        web_search_requested: task
            .web_search_requested
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or_else(|| {
                state
                    .research_implementation
                    .research_allows_web_search(file_prefix)
            }),
        research_intensity: task.research_intensity.as_deref(),
        quality_depth: task.quality_depth.as_deref(),
        research_topic: task.research_topic.as_deref(),
        research_instructions: task.research_instructions.as_deref(),
        evidence_subject: evidence_subject.as_deref(),
    };
    let finalized = finalize_research_output(
        normalized_output,
        &artifacts,
        diagnostics.as_ref(),
        &context,
    );
    let mut persisted_artifacts = artifacts;
    if finalized.diagnostics.repaired_final_answer {
        push_unique_warning(
            &mut persisted_artifacts.warnings,
            "artifact_finalizer_repaired_final_answer".to_string(),
        );
    }
    if finalized.diagnostics.synthesized_final_answer {
        push_unique_warning(
            &mut persisted_artifacts.warnings,
            "artifact_finalizer_synthesized_final_answer".to_string(),
        );
    }
    if persisted_artifacts.quality_gate.is_none() {
        persisted_artifacts.quality_gate = Some(ResearchQualityGateArtifact {
            status: "pending".to_string(),
            failure_messages: Vec::new(),
            unsupported_claim_count: 0,
            unresolved_conflict_count: 0,
            open_debt_count: 0,
        });
    }
    persist_research_controller_artifacts(state, task_id, &persisted_artifacts).await;
    finalized.output
}

pub(super) async fn validate_task_research_output(
    state: &AppState,
    task_id: i64,
    output: &str,
    file_prefix: &str,
    file_type: &str,
    transient_prohibited_repair_hint_urls: Option<&HashSet<String>>,
) -> Result<String, TaskResearchValidationFailure> {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return Ok(output.to_string());
    }

    let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| TaskResearchValidationFailure {
            output: output.to_string(),
            message: format!("Research quality gate could not load task metadata: {e}"),
        })?;
    let web_search_requested = task
        .web_search_requested
        .as_deref()
        .map(|value| value == "true")
        .unwrap_or_else(|| {
            state
                .research_implementation
                .research_allows_web_search(file_prefix)
        });
    let evidence_subject =
        research_source_diagnostics_subject(task.research_source_diagnostics_json.as_deref());
    let context = ResearchQualityContext {
        file_prefix,
        file_type,
        web_search_requested,
        research_intensity: task.research_intensity.as_deref(),
        quality_depth: task.quality_depth.as_deref(),
        research_topic: task.research_topic.as_deref(),
        research_instructions: task.research_instructions.as_deref(),
        evidence_subject: evidence_subject.as_deref(),
    };
    let mut artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    let diagnostics = load_research_source_diagnostics(state, task_id).await;
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    normalize_deferred_conflicts_to_actionable_debt(&mut artifacts);
    let mut validation_artifacts = artifacts.clone();
    validation_artifacts.quality_gate = Some(research_quality_gate_from_failures(
        &validation_artifacts,
        &[],
    ));
    let output_for_validation = if validation_artifacts.source_cards.is_empty()
        || (validation_artifacts.claim_log.is_empty()
            && !has_local_pi_source_pack_source_card_scaffold(&validation_artifacts))
    {
        output.to_string()
    } else {
        finalize_research_output(
            output,
            &validation_artifacts,
            diagnostics.as_ref(),
            &context,
        )
        .output
    };

    let mut failures = Vec::new();
    if let Err(error) = validate_research_output(&output_for_validation, &context) {
        failures.push(error);
    }
    if let Some(prohibited_urls) = transient_prohibited_repair_hint_urls {
        if let Err(error) = validate_transient_repair_hint_evidence_provenance(
            &output_for_validation,
            file_type,
            prohibited_urls,
        ) {
            failures.push(error);
        }
    }
    match validate_research_artifacts(
        &artifacts,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
    ) {
        Ok(()) => {
            close_research_debts_for_gate(&mut artifacts.research_debt, "artifact_quality", None);
            artifacts.quality_gate = Some(research_quality_gate_from_failures(&artifacts, &[]));
        }
        Err(artifact_failures) => {
            for failure in &artifact_failures {
                failures.push(format!("Research artifact gate failed: {failure}"));
            }
            sync_research_debts_for_gate(
                &mut artifacts.research_debt,
                "artifact_quality",
                &artifact_failures,
                task.research_topic.as_deref(),
            );
            artifacts.quality_gate = Some(research_quality_gate_from_failures(
                &artifacts,
                &artifact_failures,
            ));
        }
    }
    if let Some(failure) = scaffold_trust_block_failure(
        &artifacts,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
    ) {
        failures.push(failure);
    }
    if failures.is_empty() {
        close_research_debts_for_gate(&mut artifacts.research_debt, "quality_gate", None);
        artifacts.quality_gate = Some(research_quality_gate_from_failures(&artifacts, &[]));
    } else {
        sync_research_debts_for_gate(
            &mut artifacts.research_debt,
            "quality_gate",
            &failures,
            task.research_topic.as_deref(),
        );
        artifacts.quality_gate = Some(research_quality_gate_from_failures(&artifacts, &failures));
    }
    persist_research_controller_artifacts(state, task_id, &artifacts).await;
    let synchronized_output =
        finalize_research_output(output, &artifacts, diagnostics.as_ref(), &context).output;

    if failures.is_empty() {
        Ok(synchronized_output)
    } else {
        Err(TaskResearchValidationFailure {
            output: synchronized_output,
            message: failures.join("; "),
        })
    }
}

pub(super) fn research_source_diagnostics_subject(
    diagnostics_json: Option<&str>,
) -> Option<String> {
    let diagnostics = diagnostics_json?;
    if let Ok(envelope) = serde_json::from_str::<ResearchSourceDiagnosticsEnvelope>(diagnostics) {
        if let Some(subject) = envelope
            .subject
            .as_deref()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
        {
            return Some(subject.to_string());
        }
        return envelope
            .source_pack
            .as_ref()
            .and_then(|report| report.subject.as_deref())
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .map(ToOwned::to_owned);
    }
    serde_json::from_str::<serde_json::Value>(diagnostics)
        .ok()?
        .get("subject")?
        .as_str()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .map(ToOwned::to_owned)
}
