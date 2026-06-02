use super::*;
use crate::contracts::{
    NarrativeActor, NarrativeCausalLink, NarrativeCausalSpineStep, NarrativeEventCard,
    NarrativeEvidenceLayer, NarrativeImpact, NarrativeInterpretiveLayer,
    NarrativeInterpretiveTension, NarrativeOpenGap, NarrativeReaderQuestion,
    NarrativeSectionOutlineItem, NarrativeState, NarrativeTimelineEvent, ReaderArgumentEdge,
    ReaderArgumentGraph, ReaderArgumentNode, ReaderNarrativePlan, ReaderQualityArtifacts,
    ReaderSectionBrief, ResearchClaimLogEntry, ResearchSourceCard,
};
use liquid_research_classic::{
    build_historical_phase_plan, historical_phase_label_is_placeholder,
    historical_phase_state_should_run, HistoricalPhasePlan,
};
use std::collections::HashSet;
use std::path::{Component, Path};

const HISTORICAL_PHASE_ENGINE_ITERATION: i64 = 1;
const HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalPhaseEngineTerminalStatus {
    Accepted,
    Blocked,
}

#[derive(Debug, Clone)]
struct HistoricalPhaseEngineOutcome {
    terminal_status: HistoricalPhaseEngineTerminalStatus,
    output: String,
    artifacts: ResearchControllerArtifacts,
    failure_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct HistoricalEvidenceDocument {
    filename: String,
    title: Option<String>,
    url: Option<String>,
    source_class: Option<String>,
    confidence: Option<String>,
    facts: Vec<String>,
    claims: Vec<String>,
}

async fn persist_historical_phase_engine_artifacts(
    state: &AppState,
    task_id: i64,
    artifacts: &ResearchControllerArtifacts,
) {
    let mut persisted = artifacts.clone();
    compact_engine_appendix_artifacts(&mut persisted);
    persist_research_controller_artifacts(state, task_id, &persisted).await;
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn try_run_historical_phase_engine(
    state: &AppState,
    task: &TaskInfo,
    filenames: &[String],
    cleanup_files: Vec<String>,
    file_prefix: &str,
    file_type: &str,
    user_prompt: &str,
) -> bool {
    if !state.research_historical_phase_engine {
        return false;
    }
    if !historical_phase_state_should_run(
        file_prefix,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(research_source_subject_for_task(task, user_prompt)),
    ) {
        return false;
    }

    let mut controller_events = Vec::new();
    let mut fresh_artifacts = ResearchControllerArtifacts {
        version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
        ..ResearchControllerArtifacts::default()
    };
    if let Some(warning) =
        legacy_artifact_ignore_warning(task.research_controller_artifacts_json.as_deref())
    {
        push_unique_warning(&mut fresh_artifacts.warnings, warning);
    }
    persist_historical_phase_engine_artifacts(state, task.id, &fresh_artifacts).await;

    update_quality_progress(
        state,
        task.id,
        &task.original_name,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        "researching",
        None,
    )
    .await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PLAN,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Historical phase engine is building an isolated evidence-first report."),
        &mut controller_events,
    )
    .await;

    let outcome = build_historical_phase_engine_outcome(
        state,
        task,
        filenames,
        file_prefix,
        file_type,
        user_prompt,
        &mut controller_events,
    )
    .await;
    persist_historical_phase_engine_artifacts(state, task.id, &outcome.artifacts).await;

    match outcome.terminal_status {
        HistoricalPhaseEngineTerminalStatus::Accepted => {
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                RESEARCH_STAGE_FINAL,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                RESEARCH_CONTROLLER_STATUS_COMPLETED,
                Some("Historical phase engine accepted the report and is saving it directly."),
                &mut controller_events,
            )
            .await;
            update_quality_progress(
                state,
                task.id,
                &task.original_name,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                "passed",
                None,
            )
            .await;
        }
        HistoricalPhaseEngineTerminalStatus::Blocked => {
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                RESEARCH_STAGE_FINAL,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                RESEARCH_CONTROLLER_STATUS_FAILED,
                outcome.failure_message.as_deref(),
                &mut controller_events,
            )
            .await;
            update_quality_progress(
                state,
                task.id,
                &task.original_name,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                "blocked",
                outcome.failure_message.as_deref(),
            )
            .await;
        }
    }

    handle_task_completion(
        state,
        task.id,
        Some(outcome.output),
        task.original_name.clone(),
        file_prefix,
        file_type,
        cleanup_files,
        false,
    )
    .await;
    true
}

async fn build_historical_phase_engine_outcome(
    state: &AppState,
    task: &TaskInfo,
    filenames: &[String],
    file_prefix: &str,
    file_type: &str,
    user_prompt: &str,
    controller_events: &mut Vec<ResearchControllerEvent>,
) -> HistoricalPhaseEngineOutcome {
    let subject = research_source_subject_for_task(task, user_prompt);
    let documents = load_historical_evidence_documents(state, filenames).await;
    let mut artifacts = ResearchControllerArtifacts {
        version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
        events: controller_events.clone(),
        ..load_task_research_artifacts(state, task.id)
            .await
            .unwrap_or_default()
    };
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.events = controller_events.clone();
    artifacts.source_cards.clear();
    artifacts.claim_log.clear();
    artifacts.conflict_map.clear();
    artifacts.research_debt.clear();
    artifacts.narrative_state = None;
    artifacts.reader_quality = None;
    artifacts.quality_gate = None;
    artifacts.research_iteration_state = None;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_SOURCE_CARDS,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Building engine-owned Source Cards from local historical evidence inputs."),
        controller_events,
    )
    .await;
    populate_engine_ledgers_from_documents(&mut artifacts, &documents);
    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_SOURCE_CARDS,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Built {} Source Cards from {} historical evidence document(s).",
            artifacts.source_cards.len(),
            documents.len()
        )),
        controller_events,
    )
    .await;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_CLAIM_LOG,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Building engine-owned Claim Log rows without model artifact JSON."),
        controller_events,
    )
    .await;
    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_CLAIM_LOG,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Built {} Claim Log row(s) from historical evidence inputs.",
            artifacts.claim_log.len()
        )),
        controller_events,
    )
    .await;

    let phase_plan = build_historical_phase_plan(&artifacts, subject);
    push_phase_plan_debts(&mut artifacts, &phase_plan);
    build_engine_planning_artifacts(&mut artifacts, &phase_plan, subject);
    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PHASE_STATE,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Historical phase engine built {} ready phase card(s) from a {}-phase plan.",
            artifacts
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len())
                .unwrap_or_default(),
            phase_plan.phases.len()
        )),
        controller_events,
    )
    .await;

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
        evidence_subject: Some(subject),
    };

    let mut failures = Vec::new();
    if let Err(artifact_failures) = validate_research_artifacts(
        &artifacts,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
    ) {
        failures.extend(artifact_failures);
    }

    let mut output = render_historical_phase_engine_output(
        subject,
        &artifacts,
        &phase_plan,
        &research_quality_gate_from_failures(&artifacts, &failures),
    );
    output = normalize_ai_output(&output, file_type);
    if let Err(error) = validate_research_output(&output, &context) {
        failures.push(error);
    }

    if failures.is_empty() {
        close_research_debts_for_gate(
            &mut artifacts.research_debt,
            "historical_phase_engine",
            None,
        );
        artifacts.quality_gate = Some(research_quality_gate_from_failures(&artifacts, &[]));
    } else {
        sync_engine_validation_debt(&mut artifacts, &failures);
        artifacts.quality_gate = Some(research_quality_gate_from_failures(&artifacts, &failures));
        output = render_historical_phase_engine_output(
            subject,
            &artifacts,
            &phase_plan,
            artifacts
                .quality_gate
                .as_ref()
                .expect("quality gate should exist"),
        );
        output = normalize_ai_output(&output, file_type);
    }

    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_QUALITY_GATE,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        if failures.is_empty() {
            RESEARCH_CONTROLLER_STATUS_COMPLETED
        } else {
            RESEARCH_CONTROLLER_STATUS_FAILED
        },
        Some(if failures.is_empty() {
            "Historical phase engine accepted the isolated report."
        } else {
            failures
                .first()
                .map(String::as_str)
                .unwrap_or("Historical phase engine blocked the isolated report.")
        }),
        controller_events,
    )
    .await;

    HistoricalPhaseEngineOutcome {
        terminal_status: if failures.is_empty() {
            HistoricalPhaseEngineTerminalStatus::Accepted
        } else {
            HistoricalPhaseEngineTerminalStatus::Blocked
        },
        output,
        artifacts,
        failure_message: failures.first().cloned(),
    }
}

async fn load_historical_evidence_documents(
    state: &AppState,
    filenames: &[String],
) -> Vec<HistoricalEvidenceDocument> {
    let mut documents = Vec::new();
    let Ok(uploads_root) = fs::canonicalize(&state.uploads_path).await else {
        return documents;
    };
    for filename in filenames {
        if !safe_upload_basename(filename) {
            continue;
        }
        let path = state.uploads_path.join(filename);
        let Ok(canonical_path) = fs::canonicalize(&path).await else {
            continue;
        };
        if !canonical_path.starts_with(&uploads_root) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&canonical_path).await else {
            continue;
        };
        let normalized = if filename.ends_with(".html") {
            crate::application::scraping::strip_html(&content)
        } else {
            content
        };
        documents.push(parse_historical_evidence_document(filename, &normalized));
    }
    documents
}

fn safe_upload_basename(filename: &str) -> bool {
    let path = Path::new(filename);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn parse_historical_evidence_document(filename: &str, content: &str) -> HistoricalEvidenceDocument {
    let mut document = HistoricalEvidenceDocument {
        filename: filename.to_string(),
        ..HistoricalEvidenceDocument::default()
    };
    let mut section: Option<&str> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("# ") {
            if document.title.is_none() {
                document.title = Some(value.trim().to_string());
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("Title:") {
            document.title = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("URL:") {
            document.url = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Source Class:") {
            document.source_class = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Confidence:") {
            document.confidence = Some(value.trim().to_string());
            continue;
        }
        if line.eq_ignore_ascii_case("Facts:") {
            section = Some("facts");
            continue;
        }
        if line.eq_ignore_ascii_case("Claims:") {
            section = Some("claims");
            continue;
        }
        if let Some(value) = line.strip_prefix("Fact:") {
            push_unique_line(&mut document.facts, value);
            continue;
        }
        if let Some(value) = line.strip_prefix("Claim:") {
            push_unique_line(&mut document.claims, value);
            continue;
        }
        if let Some(value) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            match section {
                Some("facts") => push_unique_line(&mut document.facts, value),
                Some("claims") => push_unique_line(&mut document.claims, value),
                _ => {}
            }
        }
    }

    if document.title.is_none() {
        document.title = Some(filename.to_string());
    }
    document
}

fn push_unique_line(target: &mut Vec<String>, value: &str) {
    let normalized = value.trim();
    if normalized.is_empty() {
        return;
    }
    if target.iter().any(|existing| existing == normalized) {
        return;
    }
    target.push(normalized.to_string());
}

fn populate_engine_ledgers_from_documents(
    artifacts: &mut ResearchControllerArtifacts,
    documents: &[HistoricalEvidenceDocument],
) {
    let mut source_index = 1usize;
    let mut claim_index = 1usize;

    for (document_index, document) in documents.iter().enumerate() {
        let safe_document_label = safe_evidence_filename_label(&document.filename, document_index);
        let safe_document_suffix =
            safe_evidence_filename_debt_suffix(&document.filename, document_index);
        let Some(url) = document
            .url
            .as_deref()
            .and_then(liquid_research_core::normalize_absolute_public_evidence_url)
        else {
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: format!("missing-source-url-{safe_document_suffix}"),
                    severity: "high".to_string(),
                    failed_gate: Some("historical_phase_engine".to_string()),
                    missing_evidence: format!(
                        "historical source metadata is missing a public URL in {}",
                        safe_document_label
                    ),
                    required_source_class: Some("authoritative_secondary".to_string()),
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![format!(
                        "Add a public URL and source metadata to {} before rerunning the historical phase engine.",
                        safe_document_label
                    )],
                    status: "open".to_string(),
                },
            );
            continue;
        };

        let source_id = format!("S{source_index}");
        source_index += 1;
        let title = document
            .title
            .as_deref()
            .and_then(|title| engine_safe_evidence_text(title, 120))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("Historical Source {}", source_index - 1));
        let source_class = document
            .source_class
            .as_deref()
            .and_then(|value| engine_safe_evidence_text(value, 64))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| liquid_research_core::infer_source_class(&url).to_string());
        let confidence = document
            .confidence
            .as_deref()
            .and_then(normalize_engine_confidence)
            .or_else(|| Some("high".to_string()));
        let extracted_facts = document
            .facts
            .iter()
            .filter_map(|fact| engine_safe_evidence_text(fact, 220))
            .collect::<Vec<_>>();
        let source_card = ResearchSourceCard {
            id: source_id.clone(),
            url,
            title,
            source_class,
            accessed_at: None,
            extracted_facts,
            limitation: None,
            diagnostics_ref: Some(format!("historical_phase_engine:{}", document.filename)),
            confidence: confidence.clone(),
        };
        artifacts.source_cards.push(source_card);

        let safe_claims = document
            .claims
            .iter()
            .filter_map(|claim| engine_safe_evidence_text(claim, 320))
            .collect::<Vec<_>>();

        if safe_claims.is_empty() {
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: format!("missing-claims-{safe_document_suffix}"),
                    severity: "high".to_string(),
                    failed_gate: Some("historical_phase_engine".to_string()),
                    missing_evidence: format!(
                        "historical evidence file {} does not declare any phase-specific Claim Log rows",
                        safe_document_label
                    ),
                    required_source_class: None,
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![format!(
                        "Add Claim: lines for concrete phase-specific events in {}.",
                        safe_document_label
                    )],
                    status: "open".to_string(),
                },
            );
            continue;
        }

        for claim in &safe_claims {
            if claim.trim().is_empty() {
                continue;
            }
            artifacts.claim_log.push(ResearchClaimLogEntry {
                id: format!("C{claim_index}"),
                claim: claim.clone(),
                claim_type: Some("event".to_string()),
                support_source_card_ids: vec![source_id.clone()],
                support_urls: Vec::new(),
                confidence: confidence.clone(),
                uncertainty_note: None,
                needs_verification: Some(false),
            });
            claim_index += 1;
        }
    }

    if artifacts.source_cards.is_empty() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: "historical-phase-engine-missing-source-cards".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine".to_string()),
                missing_evidence:
                    "historical phase engine could not build any Source Cards from the provided evidence inputs"
                        .to_string(),
                required_source_class: Some("authoritative_secondary".to_string()),
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Provide at least one evidence file with Title, URL, and phase-specific facts."
                        .to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
    if artifacts.claim_log.is_empty() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: "historical-phase-engine-missing-claim-log".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine".to_string()),
                missing_evidence:
                    "historical phase engine could not build any phase-specific Claim Log rows from the provided evidence inputs"
                        .to_string(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Add Claim: lines for each historical phase so the engine can ground phase cards."
                        .to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
}

fn sanitize_debt_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn safe_evidence_filename_label(filename: &str, index: usize) -> String {
    let basename = Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    engine_safe_evidence_text(basename, 80)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("historical evidence file {}", index + 1))
}

fn safe_evidence_filename_debt_suffix(filename: &str, index: usize) -> String {
    sanitize_debt_suffix(&safe_evidence_filename_label(filename, index))
}

fn normalize_engine_confidence(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "high" | "medium" | "low" => Some(normalized),
        _ => None,
    }
}

fn engine_safe_evidence_text(value: &str, limit: usize) -> Option<String> {
    if engine_evidence_text_is_unsafe(value) {
        return None;
    }
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    if compact.chars().count() <= limit {
        Some(compact)
    } else {
        Some(compact.chars().take(limit).collect())
    }
}

fn engine_evidence_text_is_unsafe(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let alias_normalized = lower.replace(['_', '-'], " ");
    let unsafe_markers = [
        "system prompt",
        "resolved prompt",
        "resolved system prompt",
        "resolved user prompt",
        "provider payload",
        "raw provider payload",
        "controller artifact",
        "controller artifact json",
        "source diagnostics",
        "raw diagnostics",
        "raw prompt",
        "raw model",
        "raw model output",
        "api key",
        "apikey",
        "authorization:",
        "bearer ",
        "password",
        "secret",
        "token=",
        ".env",
    ];
    unsafe_markers
        .iter()
        .any(|marker| lower.contains(marker) || alias_normalized.contains(marker))
        || liquid_research_core::research_artifact_text_contains_unsafe_location_reference(value)
}

fn push_phase_plan_debts(
    artifacts: &mut ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
) {
    for debt in &phase_plan.debts {
        upsert_research_debt(&mut artifacts.research_debt, debt.clone());
    }
    for warning in &phase_plan.warnings {
        push_unique_warning(&mut artifacts.warnings, warning.clone());
    }
}

fn build_engine_planning_artifacts(
    artifacts: &mut ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    subject: &str,
) {
    let phase_report =
        liquid_research_classic::stabilize_historical_phase_state(artifacts, subject);
    push_unique_warning(
        &mut artifacts.warnings,
        format!(
            "historical_phase_engine:phases={} ready={} repaired={}",
            phase_report.phase_count,
            phase_report.ready_phase_count,
            phase_report.repaired_event_cards
        ),
    );
    if artifacts.narrative_state.is_none() {
        artifacts.narrative_state = Some(NarrativeState {
            version: 1,
            ..NarrativeState::default()
        });
    }
    let ready_phases = phase_plan
        .phases
        .iter()
        .filter(|phase| phase.readiness.ready)
        .collect::<Vec<_>>();
    let reader_promise = format!(
        "{subject}: phase-by-phase chronology, source layers, interpretive limits, and downstream consequences."
    );
    let evidence_layers = build_evidence_layers(artifacts);
    let claim_grounded_cards = build_claim_grounded_event_cards_from_claim_log(
        &artifacts.claim_log,
        &artifacts.source_cards,
    );
    let state = artifacts
        .narrative_state
        .get_or_insert_with(NarrativeState::default);
    state.version = state.version.max(1);
    state.topic_frame = Some(subject.to_string());
    state.reader_promise = Some(reader_promise);
    if state.event_cards.len() < claim_grounded_cards.len()
        || state
            .event_cards
            .iter()
            .all(|card| historical_phase_label_is_placeholder(&card.label))
    {
        state.event_cards = claim_grounded_cards;
    }
    apply_phase_plan_depth_to_cards(&mut state.event_cards, phase_plan);
    let working_thesis = build_working_thesis(subject, &state.event_cards, &ready_phases);
    state.working_thesis = Some(working_thesis.clone());
    let card_snapshots = state.event_cards.clone();
    state.timeline = ready_phases
        .iter()
        .enumerate()
        .map(|(idx, phase)| NarrativeTimelineEvent {
            id: format!("TL{}", idx + 1),
            label: phase.label.clone(),
            date_anchor: phase.timeframe.clone(),
            significance: Some(format!(
                "{} narrowed the next available strategic or political options.",
                phase.label
            )),
            expected_claim_log_ids: phase.claim_log_ids.clone(),
            expected_source_card_ids: phase.source_ids.clone(),
        })
        .collect();
    if state.timeline.is_empty() {
        state.timeline = card_snapshots
            .iter()
            .enumerate()
            .map(|(idx, card)| NarrativeTimelineEvent {
                id: format!("TL{}", idx + 1),
                label: card.label.clone(),
                date_anchor: card.timeframe.clone(),
                significance: Some(card.outcome.clone().unwrap_or_else(|| card.label.clone())),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            })
            .collect();
    }
    state.section_outline = card_snapshots
        .iter()
        .enumerate()
        .map(|(idx, card)| NarrativeSectionOutlineItem {
            id: format!("SO{}", idx + 1),
            heading: card.label.clone(),
            purpose: Some(format!(
                "{} explains how {} led into the next phase rather than standing as an isolated event.",
                card.label,
                card.trigger.as_deref().unwrap_or("its trigger conditions")
            )),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect();
    state.evidence_layers = evidence_layers;
    state.interpretive_tensions = build_interpretive_tensions_from_cards(&card_snapshots);
    state.impacts = build_impacts_from_cards(&card_snapshots);
    state.reader_questions = build_reader_questions_from_cards(&card_snapshots);
    state.actors = build_actor_rows_from_cards(&card_snapshots);
    state.causal_chain = build_causal_chain_from_cards(&card_snapshots);
    state.open_gaps = build_open_gaps(&artifacts.research_debt);
    state.last_iteration_summary = Some(format!(
        "historical_phase_engine built {} ready phases from {} claim-backed historical rows",
        state.event_cards.len(),
        artifacts.claim_log.len()
    ));
    if let Some(cards) = state.event_cards.get_mut(..) {
        for card in cards.iter_mut() {
            apply_phase_plan_depth_to_card(card, phase_plan);
            card.causal_spine = build_causal_spine(card);
            card.interpretive_layers = build_interpretive_layers(card);
            card.confidence = Some("high".to_string());
            card.open_questions = build_card_open_questions(card, &artifacts.research_debt);
        }
    }
    artifacts.reader_quality = Some(build_reader_quality(&state.event_cards, &working_thesis));
}

fn apply_phase_plan_depth_to_cards(
    cards: &mut [NarrativeEventCard],
    phase_plan: &HistoricalPhasePlan,
) {
    for card in cards {
        apply_phase_plan_depth_to_card(card, phase_plan);
    }
}

fn apply_phase_plan_depth_to_card(card: &mut NarrativeEventCard, phase_plan: &HistoricalPhasePlan) {
    let Some(phase) = phase_plan
        .phases
        .iter()
        .find(|phase| phase.readiness.ready && phase.label.trim() == card.label.trim())
    else {
        return;
    };
    if card
        .timeframe
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.timeframe = phase.timeframe.clone();
    }
    if card.actors.is_empty() {
        card.actors = phase.actors.clone();
    }
    if card
        .region_or_front
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.region_or_front = phase.region_or_front.clone();
    }
    if card
        .trigger
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.trigger = phase.trigger.clone();
    }
    if card
        .development
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.development = phase.expected_development_focus.clone();
    }
    if card
        .outcome
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.outcome = phase.expected_outcome.clone();
    }
    if card.claim_log_ids.is_empty() {
        card.claim_log_ids = phase.claim_log_ids.clone();
    }
    if card.source_ids.is_empty() {
        card.source_ids = phase.source_ids.clone();
    }
}

fn build_claim_grounded_event_cards_from_claim_log(
    claim_log: &[ResearchClaimLogEntry],
    source_cards: &[ResearchSourceCard],
) -> Vec<NarrativeEventCard> {
    let source_ids = source_cards
        .iter()
        .map(|card| card.id.clone())
        .collect::<HashSet<_>>();
    claim_log
        .iter()
        .filter(|claim| !claim.claim.trim().is_empty())
        .filter(|claim| {
            claim
                .support_source_card_ids
                .iter()
                .any(|id| source_ids.contains(id))
        })
        .take(8)
        .map(|claim| {
            let label = claim
                .claim
                .split(':')
                .next()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .unwrap_or(claim.claim.as_str())
                .to_string();
            let actors = extract_actors_from_claim(&claim.claim);
            let region = extract_region_from_claim(&claim.claim);
            let trigger = build_trigger_from_claim(&claim.claim, &label);
            let outcome =
                extract_outcome_from_claim(&claim.claim).or_else(|| Some(claim.claim.clone()));
            let development = Some(build_development_from_claim(
                &claim.claim,
                &label,
                &actors,
                region.as_deref(),
                trigger.as_deref(),
                outcome.as_deref(),
            ));
            NarrativeEventCard {
                label,
                timeframe: extract_timeframe_from_claim(&claim.claim),
                actors,
                region_or_front: region,
                trigger,
                development,
                outcome,
                claim_log_ids: vec![claim.id.clone()],
                source_ids: claim.support_source_card_ids.clone(),
                causal_spine: Vec::new(),
                interpretive_layers: Vec::new(),
                confidence: claim.confidence.clone(),
                open_questions: Vec::new(),
            }
        })
        .collect()
}

fn build_working_thesis(
    subject: &str,
    cards: &[NarrativeEventCard],
    phases: &[&liquid_research_classic::HistoricalResearchPhase],
) -> String {
    let first = cards
        .first()
        .map(|card| card.label.as_str())
        .or_else(|| phases.first().map(|phase| phase.label.as_str()))
        .unwrap_or("the opening crisis");
    let last = cards
        .last()
        .map(|card| card.label.as_str())
        .or_else(|| phases.last().map(|phase| phase.label.as_str()))
        .unwrap_or("the later settlement");
    format!(
        "{subject} is best understood as a chained sequence from {first} to {last}, where each phase changes the next set of military, diplomatic, and political options."
    )
}

fn build_evidence_layers(artifacts: &ResearchControllerArtifacts) -> Vec<NarrativeEvidenceLayer> {
    let mut layers = Vec::new();
    let primary_ids = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            card.source_class.contains("official") || card.source_class.contains("primary")
        })
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    if !primary_ids.is_empty() {
        layers.push(NarrativeEvidenceLayer {
            id: "EL1".to_string(),
            label: "Primary and official anchors".to_string(),
            purpose: Some(
                "Public primary or official material anchors chronology, actors, and settlement claims before interpretation."
                    .to_string(),
            ),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: artifacts
                .claim_log
                .iter()
                .filter(|claim| {
                    claim
                        .support_source_card_ids
                        .iter()
                        .any(|id| primary_ids.iter().any(|expected| expected == id))
                })
                .map(|claim| claim.id.clone())
                .collect(),
            expected_source_card_ids: primary_ids,
        });
    }
    let secondary_ids = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            !card.source_class.contains("official") && !card.source_class.contains("primary")
        })
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    if !secondary_ids.is_empty() {
        layers.push(NarrativeEvidenceLayer {
            id: "EL2".to_string(),
            label: "Interpretive secondary synthesis".to_string(),
            purpose: Some(
                "Secondary synthesis explains why phases matter, where evidence is thin, and which causal links remain interpretive."
                    .to_string(),
            ),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: artifacts.claim_log.iter().map(|claim| claim.id.clone()).collect(),
            expected_source_card_ids: secondary_ids,
        });
    }
    layers
}

fn build_interpretive_tensions_from_cards(
    cards: &[NarrativeEventCard],
) -> Vec<NarrativeInterpretiveTension> {
    cards
        .iter()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeInterpretiveTension {
            id: format!("IT{}", idx + 1),
            question: format!(
                "How much of {} should be read as a direct consequence of {} rather than broader structural pressure around {}?",
                card.label,
                card.trigger.as_deref().unwrap_or(&card.label),
                card.region_or_front.as_deref().unwrap_or("the main theater")
            ),
            competing_readings: Some(format!(
                "{} can be read both as a concrete phase shift and as a pressure point that changed diplomatic, institutional, or coalition behavior for {}.",
                card.development.as_deref().unwrap_or(&card.label),
                if card.actors.is_empty() {
                    "the main actors".to_string()
                } else {
                    card.actors.join(", ")
                }
            )),
            current_status: Some(format!(
                "bounded interpretation; keep {} tied to {} and avoid claims beyond the cited phase row",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_impacts_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeImpact> {
    cards
        .iter()
        .rev()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeImpact {
            id: format!("IM{}", idx + 1),
            label: format!("{} impact", card.label),
            scope: Some("regional and longer-horizon consequences".to_string()),
            implication: Some(format!(
                "{} shaped the settlement, postwar order, or later balance of power because {}.",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_reader_questions_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeReaderQuestion> {
    cards
        .iter()
        .rev()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeReaderQuestion {
            id: format!("RQ{}", idx + 1),
            question: format!(
                "Which source layer would most tighten the remaining uncertainty around {} in {}?",
                card.label,
                card.region_or_front.as_deref().unwrap_or("the phase's main setting")
            ),
            answer_status: Some("partially answered".to_string()),
            answer_plan: Some(format!(
                "Recheck {} with another public source if stronger actor, front, or settlement detail is needed around {}.",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_actor_rows_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeActor> {
    let mut seen = HashSet::new();
    let mut actors = Vec::new();
    for card in cards {
        for actor in &card.actors {
            if !seen.insert(actor.clone()) {
                continue;
            }
            actors.push(NarrativeActor {
                id: format!("A{}", actors.len() + 1),
                label: actor.clone(),
                role: Some(format!("acts most visibly in {}", card.label)),
                relevance: Some(format!(
                    "{} helps define the turning pressure inside {} because {}.",
                    actor,
                    card.label,
                    card.trigger.as_deref().unwrap_or(&card.label)
                )),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            });
        }
    }
    actors
}

fn build_causal_chain_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeCausalLink> {
    cards
        .windows(2)
        .enumerate()
        .map(|(idx, pair)| NarrativeCausalLink {
            id: format!("CL{}", idx + 1),
            cause: pair[0]
                .outcome
                .clone()
                .unwrap_or_else(|| {
                    pair[0]
                        .development
                        .clone()
                        .unwrap_or_else(|| pair[0].label.clone())
                }),
            effect: pair[1]
                .trigger
                .clone()
                .unwrap_or_else(|| pair[1].label.clone()),
            rationale: Some(format!(
                "{} narrowed the options until {} became the next phase because {} changed what {} could still do next.",
                pair[0].label,
                pair[1].label,
                pair[0].outcome.as_deref().unwrap_or(&pair[0].label),
                if pair[1].actors.is_empty() {
                    "the main coalition".to_string()
                } else {
                    pair[1].actors.join(", ")
                }
            )),
            derived_from: Some("claim_grounded_event_cards".to_string()),
            expected_claim_log_ids: pair[0]
                .claim_log_ids
                .iter()
                .chain(pair[1].claim_log_ids.iter())
                .cloned()
                .collect(),
            expected_source_card_ids: pair[0]
                .source_ids
                .iter()
                .chain(pair[1].source_ids.iter())
                .cloned()
                .collect(),
        })
        .collect()
}

fn build_open_gaps(debts: &[ResearchDebtItem]) -> Vec<NarrativeOpenGap> {
    debts
        .iter()
        .filter(|debt| debt.status != "closed")
        .take(4)
        .enumerate()
        .map(|(idx, debt)| NarrativeOpenGap {
            id: format!("OG{}", idx + 1),
            gap_type: debt
                .failed_gate
                .clone()
                .unwrap_or_else(|| "historical_phase_engine".to_string()),
            description: debt.missing_evidence.clone(),
            status: Some("open".to_string()),
            expected_claim_log_ids: Vec::new(),
            expected_source_card_ids: Vec::new(),
        })
        .collect()
}

fn build_causal_spine(card: &NarrativeEventCard) -> Vec<NarrativeCausalSpineStep> {
    let actor_text = if card.actors.is_empty() {
        "the main actors".to_string()
    } else {
        card.actors.join(", ")
    };
    let region_text = card
        .region_or_front
        .as_deref()
        .unwrap_or("the main front")
        .to_string();
    let trigger = card.trigger.as_deref().unwrap_or(&card.label);
    let development = card.development.as_deref().unwrap_or(&card.label);
    let outcome = card.outcome.as_deref().unwrap_or(&card.label);
    let base_reasoning = format!(
        "Because {} and {} are grounded in the same phase-specific claim, this step stays tied to the cited evidence and explains why the next choice followed.",
        trigger,
        outcome
    );
    vec![
        NarrativeCausalSpineStep {
            step_type: "precondition".to_string(),
            description: format!(
                "{} created the precondition for {} by tightening the constraints around {} in {}.",
                trigger,
                card.label,
                actor_text,
                region_text
            ),
            epistemic_status: Some("inference".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} The phase-specific claim identifies the trigger, actors, and front together, so the constraint can be stated without leaving the evidence boundary."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "decision_point".to_string(),
            description: format!(
                "{} became the decision point once {} forced the main actors to choose between widening the conflict or yielding ground.",
                card.label,
                trigger
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} Because the same claim ties trigger, development, and outcome together, the phase can be read as a turning point rather than an isolated episode."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "execution".to_string(),
            description: format!(
                "{} shows how the phase actually unfolded for {} on {}.",
                development,
                actor_text,
                region_text
            ),
            epistemic_status: Some("fact".to_string()),
            reasoning: Some(
                "The execution sentence repeats the same claim-backed names, places, and operational language already anchored in the phase-specific evidence."
                    .to_string(),
            ),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "forward_pressure".to_string(),
            description: format!(
                "{} then pushed the war into the next phase by changing what {} could still do after {}.",
                outcome,
                actor_text,
                card.label
            ),
            epistemic_status: Some("inference".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} As a result, the outcome becomes the pressure point that explains why the following phase opens where it does."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
    ]
}

fn build_interpretive_layers(card: &NarrativeEventCard) -> Vec<NarrativeInterpretiveLayer> {
    let summary = format!(
        "{} in {} with {}",
        card.label,
        card.region_or_front.as_deref().unwrap_or("the main front"),
        if card.actors.is_empty() {
            "the main actors".to_string()
        } else {
            card.actors.join(", ")
        }
    );
    vec![
        NarrativeInterpretiveLayer {
            layer_type: "operations".to_string(),
            interpretation: format!(
                "{summary} matters on the operational layer because {} reorganized tempo, front geometry, and the next available military choices rather than merely adding another dated episode.",
                card.development.as_deref().unwrap_or(&card.label)
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "Because the phase-specific claim already ties actors, front, trigger, and outcome together, the operational reading can explain how one move forced the next constraint without inventing a new event."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeInterpretiveLayer {
            layer_type: "politics_institutions".to_string(),
            interpretation: format!(
                "{summary} also matters on the political or institutional layer because {} changed coalition behavior, settlement pressure, or state capacity beyond the battlefield itself.",
                card.outcome.as_deref().unwrap_or(&card.label)
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "This layer stays grounded because the same claim-backed sentence that describes the outcome also supplies the causal connector that carries the phase into wider diplomatic or political consequences."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
    ]
}

fn build_card_open_questions(card: &NarrativeEventCard, debts: &[ResearchDebtItem]) -> Vec<String> {
    debts
        .iter()
        .filter(|debt| debt.status != "closed")
        .filter(|debt| {
            debt.missing_evidence.contains(&card.label)
                || card
                    .claim_log_ids
                    .iter()
                    .any(|claim_id| debt.missing_evidence.contains(claim_id))
        })
        .map(|debt| debt.missing_evidence.clone())
        .take(2)
        .collect()
}

fn build_reader_quality(
    cards: &[NarrativeEventCard],
    working_thesis: &str,
) -> ReaderQualityArtifacts {
    ReaderQualityArtifacts {
        argument_graph: Some(ReaderArgumentGraph {
            nodes: cards
                .iter()
                .take(3)
                .enumerate()
                .map(|(idx, card)| ReaderArgumentNode {
                    id: format!("RN{}", idx + 1),
                    label: format!("{} as a turning phase", card.label),
                    node_type: Some("phase_turn".to_string()),
                    rationale: Some(format!(
                        "{} is one part of the thesis that the conflict moved by chained constraints rather than disconnected battles because {}.",
                        card.label,
                        card.outcome.as_deref().unwrap_or(&card.label)
                    )),
                    claim_log_ids: card.claim_log_ids.clone(),
                    source_card_ids: card.source_ids.clone(),
                })
                .collect(),
            edges: cards
                .windows(2)
                .enumerate()
                .map(|(idx, pair)| ReaderArgumentEdge {
                    id: format!("RE{}", idx + 1),
                    from_node_id: format!("RN{}", idx + 1),
                    to_node_id: format!("RN{}", idx + 2),
                    relation: "constraint_to_next_phase".to_string(),
                    rationale: Some(format!(
                        "{} narrowed the next choice set until {} became unavoidable in the reader-facing narrative because {} shaped the next options.",
                        pair[0].label,
                        pair[1].label,
                        pair[0].outcome.as_deref().unwrap_or(&pair[0].label)
                    )),
                    claim_log_ids: pair[0]
                        .claim_log_ids
                        .iter()
                        .chain(pair[1].claim_log_ids.iter())
                        .cloned()
                        .collect(),
                    source_card_ids: pair[0]
                        .source_ids
                        .iter()
                        .chain(pair[1].source_ids.iter())
                        .cloned()
                        .collect(),
                })
                .collect(),
        }),
        narrative_plan: Some(ReaderNarrativePlan {
            lead_section_id: Some("phase-1".to_string()),
            section_ids: cards
                .iter()
                .enumerate()
                .map(|(idx, _)| format!("phase-{}", idx + 1))
                .collect(),
            transition_ids: cards
                .windows(2)
                .enumerate()
                .map(|(idx, _)| format!("transition-{}", idx + 1))
                .collect(),
            narrative_arc: Some(working_thesis.to_string()),
            ending_note: Some(
                "End by separating what the evidence confirms from what remains interpretive."
                    .to_string(),
            ),
        }),
        section_briefs: cards
            .iter()
            .take(4)
            .enumerate()
            .map(|(idx, card)| ReaderSectionBrief {
                section_id: Some(format!("phase-{}", idx + 1)),
                key_point: format!(
                    "{} should read as a linked phase with trigger, development, outcome, and consequence because {}.",
                    card.label,
                    card.development.as_deref().unwrap_or(&card.label)
                ),
                reader_goal: Some(
                    "Help the reader follow why one phase narrowed the options for the next."
                        .to_string(),
                ),
                claim_log_ids: card.claim_log_ids.clone(),
                source_card_ids: card.source_ids.clone(),
            })
            .collect(),
        reader_critique: None,
    }
}

fn extract_timeframe_from_claim(claim: &str) -> Option<String> {
    let trimmed = claim.trim();
    let prefix = trimmed.split(':').next().map(str::trim).unwrap_or(trimmed);
    if prefix.chars().any(|ch| ch.is_ascii_digit()) {
        Some(prefix.to_string())
    } else {
        None
    }
}

fn extract_region_from_claim(claim: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let markers = ["에서 ", " in ", " on ", " at ", " across ", " into "];
    let lower = compact.to_ascii_lowercase();
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            let start = idx + marker.len();
            let remainder = compact[start..].trim();
            let end = remainder
                .find(',')
                .or_else(|| remainder.find(';'))
                .or_else(|| remainder.find(" and "))
                .unwrap_or(remainder.len());
            let region = remainder[..end].trim();
            if region.chars().count() >= 3 {
                return Some(region.to_string());
            }
        }
    }
    None
}

fn extract_outcome_from_claim(claim: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let markers = [
        " and ",
        " therefore ",
        " 그 결과 ",
        " 결과적으로 ",
        " 이어졌고",
        " 강화되었다",
        " 확대되었다",
        " 전환했다",
        " 시작되었다",
        " 드러났다",
        " 커졌다",
    ];
    for marker in markers {
        if let Some(idx) = compact.find(marker) {
            let outcome = compact[idx + marker.len()..].trim();
            if outcome.chars().count() >= 20 {
                return Some(outcome.to_string());
            }
        }
    }
    compact
        .split(':')
        .nth(1)
        .map(str::trim)
        .filter(|text| text.chars().count() >= 20)
        .map(ToString::to_string)
}

fn build_trigger_from_claim(claim: &str, label: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let detail = compact
        .split(':')
        .nth(1)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(compact.as_str());
    let markers = [
        " and ",
        " therefore ",
        " 그 결과 ",
        " 결과적으로 ",
        " 이어졌고",
        " 강화되었다",
        " 확대되었다",
        " 전환했다",
        " 시작되었다",
        " 드러났다",
        " 커졌다",
    ];
    let end = markers
        .iter()
        .filter_map(|marker| detail.find(marker))
        .min()
        .unwrap_or(detail.len());
    let trigger = detail[..end].trim();
    if trigger.is_empty() {
        Some(label.to_string())
    } else {
        Some(trigger.to_string())
    }
}

fn build_development_from_claim(
    claim: &str,
    label: &str,
    actors: &[String],
    region: Option<&str>,
    trigger: Option<&str>,
    outcome: Option<&str>,
) -> String {
    let actor_text = if actors.is_empty() {
        "the main actors".to_string()
    } else {
        actors.join(", ")
    };
    let region_text = region.unwrap_or("the main theater");
    let trigger_text = trigger.unwrap_or(label);
    let outcome_text = outcome.unwrap_or(claim);
    format!(
        "{} This phase shows how {} moved through {} around {}. It matters because {} then changed the next diplomatic, political, or military options rather than remaining an isolated episode.",
        compact_claim_text(claim),
        actor_text,
        region_text,
        trigger_text,
        outcome_text
    )
}

fn extract_actors_from_claim(claim: &str) -> Vec<String> {
    let detail = compact_claim_text(claim);
    let detail = detail
        .split(':')
        .nth(1)
        .map(str::trim)
        .unwrap_or(detail.as_str());
    first_clause_before_action(detail)
        .split(&[',', ';', '·'][..])
        .flat_map(|part| part.split(" and "))
        .map(str::trim)
        .map(|part| part.trim_matches(|ch: char| matches!(ch, '.' | ':' | ' ')))
        .filter(|part| part.chars().count() >= 2)
        .filter(|part| !part.chars().any(|ch| ch.is_ascii_digit()))
        .fold(Vec::new(), |mut actors, actor| {
            if !actors.iter().any(|existing| existing == actor) {
                actors.push(actor.to_string());
            }
            actors
        })
}

fn compact_claim_text(claim: &str) -> String {
    claim.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_clause_before_action(text: &str) -> &str {
    let lower = text.to_ascii_lowercase();
    let markers = [
        " turned ",
        " forced ",
        " crossed ",
        " attacked ",
        " invaded ",
        " responded ",
        " widened ",
        " changed ",
        " fixed ",
        " began ",
        " became ",
        " moved ",
        " 겪었고",
        " 겪었으며",
        " 둘러싸고",
        " 통해",
        " 이어졌고",
        " 강화되었다",
        " 드러냈고",
        " 시작되었다",
    ];
    let mut earliest = None;
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            earliest = Some(earliest.map_or(idx, |current: usize| current.min(idx)));
        }
    }
    earliest
        .map(|idx| text[..idx].trim())
        .unwrap_or(text.trim())
}

fn sync_engine_validation_debt(artifacts: &mut ResearchControllerArtifacts, failures: &[String]) {
    for (idx, failure) in failures.iter().enumerate() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: format!("historical-phase-engine-validation-{}", idx + 1),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine_validation".to_string()),
                missing_evidence: failure.clone(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Add or tighten phase-specific claims, fronts, actors, and settlement consequences in the engine-owned evidence inputs.".to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
}

fn render_historical_phase_engine_output(
    subject: &str,
    artifacts: &ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let final_answer = render_historical_phase_engine_final_answer(subject, artifacts);
    let appendix = render_historical_phase_engine_appendix(artifacts, phase_plan, quality_gate);
    format!(
        "## Final Answer\n\n{}\n\n# Verification Appendix\n\n{}",
        final_answer, appendix
    )
}

fn render_historical_phase_engine_final_answer(
    subject: &str,
    artifacts: &ResearchControllerArtifacts,
) -> String {
    let mut sections = Vec::new();
    if let Some(state) = artifacts.narrative_state.as_ref() {
        let thesis = state
            .working_thesis
            .as_deref()
            .unwrap_or(subject)
            .trim()
            .to_string();
        sections.push(format!(
            "{thesis} The report stays inside source-backed phases, then separates chronology, source layers, and interpretive limits."
        ));
        for (idx, card) in state.event_cards.iter().enumerate() {
            sections.push(render_phase_section(idx, card));
        }
        sections.push(render_chronology_and_interpretation(state));
        sections.push(render_source_layers(artifacts));
        sections.push(render_debate_map(state));
        sections.push(render_legacy_and_impact(state));
        sections.push(render_follow_up_questions(state));
    } else {
        sections.push(format!(
            "{subject} could not be turned into a trusted phase sequence because the current evidence inputs are incomplete."
        ));
    }
    sections.join("\n\n")
}

fn render_phase_section(index: usize, card: &NarrativeEventCard) -> String {
    let timeframe = card.timeframe.as_deref().unwrap_or("Undated phase");
    let actors = if card.actors.is_empty() {
        "the main actors".to_string()
    } else {
        card.actors.join(", ")
    };
    let region = card
        .region_or_front
        .as_deref()
        .unwrap_or("the main front or political center");
    let trigger = card.trigger.as_deref().unwrap_or(&card.label);
    let development = card.development.as_deref().unwrap_or(&card.label);
    let outcome = card.outcome.as_deref().unwrap_or(&card.label);
    let layer_one = card
        .interpretive_layers
        .first()
        .map(|layer| layer.interpretation.as_str())
        .unwrap_or("This phase matters because it changes the next set of options.");
    let layer_two = card
        .interpretive_layers
        .get(1)
        .map(|layer| layer.interpretation.as_str())
        .unwrap_or(
            "A second layer comes from how the same pressure reaches beyond the immediate clash.",
        );
    format!(
        "### {}. {}\n\nIn {}, {} moved through {} because {}. {} As a result, {}. Read in sequence, this phase matters because {} {}",
        index + 1,
        card.label,
        timeframe,
        actors,
        region,
        trigger,
        development,
        outcome,
        layer_one,
        layer_two
    )
}

fn render_chronology_and_interpretation(state: &NarrativeState) -> String {
    let first = state
        .event_cards
        .first()
        .map(|card| card.label.as_str())
        .unwrap_or("the opening phase");
    let last = state
        .event_cards
        .last()
        .map(|card| card.label.as_str())
        .unwrap_or("the closing phase");
    format!(
        "### Chronology And Interpretation\n\nThe chronology matters because the report follows how {} narrowed the next options until {} became possible. The interpretation stays bounded: it explains pressure, choice, and consequence only where the same phase-specific evidence row anchors those links.",
        first, last
    )
}

fn render_source_layers(artifacts: &ResearchControllerArtifacts) -> String {
    let primary = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            card.source_class.contains("official") || card.source_class.contains("primary")
        })
        .count();
    let secondary = artifacts.source_cards.len().saturating_sub(primary);
    format!(
        "### Source Layers\n\nThe source layers are split between {} primary or official anchor(s) and {} secondary synthesis source(s). Primary or official material secures dates, actors, fronts, and settlement anchors; secondary synthesis is used to connect why one phase forced the next and to mark interpretive limits where the evidence remains thinner.",
        primary, secondary
    )
}

fn render_debate_map(state: &NarrativeState) -> String {
    let debate = state
        .interpretive_tensions
        .first()
        .map(|tension| tension.question.as_str())
        .unwrap_or("which causal layer carried the most weight in the transition between phases");
    format!(
        "### Debate Map\n\nThe core debate map is not whether the conflict had turning points, but how much of the shift should be read through operations, diplomacy, domestic politics, or long-run institutional pressure. The most important bounded question here is: {}.",
        debate
    )
}

fn render_legacy_and_impact(state: &NarrativeState) -> String {
    let impact = state
        .impacts
        .first()
        .and_then(|impact| impact.implication.as_deref())
        .unwrap_or("the later settlement and political order changed because the conflict reshaped what each side could still negotiate from.");
    format!(
        "### Legacy And Impact\n\nThe later impact matters because {} This keeps the report from ending at the last battle and instead connects the final phase to the settlement, postwar order, or longer strategic legacy.",
        impact
    )
}

fn render_follow_up_questions(state: &NarrativeState) -> String {
    let question = state
        .reader_questions
        .first()
        .map(|question| question.question.as_str())
        .unwrap_or("which source layer would most improve the weakest phase card if the report needed another verification pass");
    format!(
        "### Follow-up Questions\n\nThe next bounded question is {}. That follow-up is kept separate from the accepted chronology so the reader can see what is established, what remains interpretive, and what would benefit from another public source layer.",
        question
    )
}

fn render_historical_phase_engine_appendix(
    artifacts: &ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let serialized = serialized_appendix_artifacts(artifacts, quality_gate);
    format!(
        "## Phase Plan\n| Phase | Timeframe | Actors | Region/Front | Claim IDs | Source IDs | Ready |\n| --- | --- | --- | --- | --- | --- | --- |\n{}\n## Phase Cards\n{}\n## Source Audit\n| ID | URL | Source | Class | Checked Fact | Limitation |\n| --- | --- | --- | --- | --- | --- |\n{}\n## Claim Log\n| ID | Claim | Support | Confidence | Uncertainty |\n| --- | --- | --- | --- | --- |\n{}\n## Limits, Conflicts, And Research Debt\n{}\n## Quality Gate\n{}\n[RESEARCH_ARTIFACT_JSON]\n```json\n{}\n```",
        render_phase_plan_rows(phase_plan),
        render_phase_card_rows(artifacts),
        render_source_audit_rows(artifacts),
        render_claim_log_rows(artifacts),
        render_limits_rows(artifacts),
        render_quality_gate(quality_gate),
        serialized
    )
}

fn serialized_appendix_artifacts(
    artifacts: &ResearchControllerArtifacts,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let mut appendix_artifacts = artifacts.clone();
    appendix_artifacts.events.clear();
    appendix_artifacts.quality_gate = Some(quality_gate.clone());
    compact_engine_appendix_artifacts(&mut appendix_artifacts);
    let value = compact_engine_appendix_artifact_value(&appendix_artifacts);
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

fn compact_engine_appendix_artifact_value(
    artifacts: &ResearchControllerArtifacts,
) -> serde_json::Value {
    let event_cards = artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            state
                .event_cards
                .iter()
                .take(8)
                .map(compact_engine_event_card_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let narrative_state = artifacts.narrative_state.as_ref().map(|state| {
        serde_json::json!({
            "version": state.version,
            "topic_frame": state.topic_frame.as_ref().map(|value| truncate_engine_artifact_text(value, 100)),
            "working_thesis": state.working_thesis.as_ref().map(|value| truncate_engine_artifact_text(value, 140)),
            "event_cards": event_cards,
        })
    });
    serde_json::json!({
        "version": artifacts.version,
        "source_cards": artifacts.source_cards.iter().take(8).map(compact_engine_source_card_value).collect::<Vec<_>>(),
        "claim_log": artifacts.claim_log.iter().take(8).map(compact_engine_claim_value).collect::<Vec<_>>(),
        "conflict_map": artifacts.conflict_map.iter().take(3).collect::<Vec<_>>(),
        "research_debt": artifacts.research_debt.iter().filter(|debt| debt.status != "closed").take(4).map(compact_engine_debt_value).collect::<Vec<_>>(),
        "narrative_state": narrative_state,
        "quality_gate": artifacts.quality_gate,
        "warnings": artifacts.warnings.iter().take(4).map(|value| truncate_engine_artifact_text(value, 100)).collect::<Vec<_>>(),
    })
}

fn compact_engine_source_card_value(card: &ResearchSourceCard) -> serde_json::Value {
    serde_json::json!({
        "id": card.id,
        "url": card.url,
        "title": truncate_engine_artifact_text(&card.title, 80),
        "source_class": truncate_engine_artifact_text(&card.source_class, 40),
        "accessed_at": card.accessed_at,
        "extracted_facts": Vec::<String>::new(),
        "limitation": card.limitation.as_ref().map(|value| truncate_engine_artifact_text(value, 80)),
        "diagnostics_ref": serde_json::Value::Null,
        "confidence": card.confidence,
    })
}

fn compact_engine_claim_value(claim: &ResearchClaimLogEntry) -> serde_json::Value {
    serde_json::json!({
        "id": claim.id,
        "claim": truncate_engine_artifact_text(&claim.claim, 180),
        "claim_type": claim.claim_type,
        "support_source_card_ids": claim.support_source_card_ids,
        "support_urls": claim.support_urls,
        "confidence": claim.confidence,
        "uncertainty_note": claim.uncertainty_note.as_ref().map(|value| truncate_engine_artifact_text(value, 80)),
        "needs_verification": claim.needs_verification,
    })
}

fn compact_engine_event_card_value(card: &NarrativeEventCard) -> serde_json::Value {
    serde_json::json!({
        "label": truncate_engine_artifact_text(&card.label, 80),
        "timeframe": card.timeframe.as_ref().map(|value| truncate_engine_artifact_text(value, 60)),
        "actors": card.actors.iter().take(5).map(|value| truncate_engine_artifact_text(value, 40)).collect::<Vec<_>>(),
        "region_or_front": card.region_or_front.as_ref().map(|value| truncate_engine_artifact_text(value, 60)),
        "trigger": card.trigger.as_ref().map(|value| truncate_engine_artifact_text(value, 100)),
        "development": card.development.as_ref().map(|value| truncate_engine_artifact_text(value, 130)),
        "outcome": card.outcome.as_ref().map(|value| truncate_engine_artifact_text(value, 110)),
        "claim_log_ids": card.claim_log_ids,
        "source_ids": card.source_ids,
        "causal_spine": card.causal_spine.iter().take(1).map(|step| serde_json::json!({
            "step_type": step.step_type,
            "description": truncate_engine_artifact_text(&step.description, 80),
            "epistemic_status": step.epistemic_status,
            "reasoning": serde_json::Value::Null,
            "limits": Vec::<String>::new(),
            "claim_log_ids": step.claim_log_ids,
            "source_ids": step.source_ids,
        })).collect::<Vec<_>>(),
        "interpretive_layers": card.interpretive_layers.iter().take(1).map(|layer| serde_json::json!({
            "layer_type": layer.layer_type,
            "interpretation": truncate_engine_artifact_text(&layer.interpretation, 90),
            "epistemic_status": layer.epistemic_status,
            "reasoning": serde_json::Value::Null,
            "limits": Vec::<String>::new(),
            "claim_log_ids": layer.claim_log_ids,
            "source_ids": layer.source_ids,
        })).collect::<Vec<_>>(),
        "confidence": card.confidence,
        "open_questions": card.open_questions.iter().take(1).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
    })
}

fn compact_engine_debt_value(debt: &ResearchDebtItem) -> serde_json::Value {
    serde_json::json!({
        "id": debt.id,
        "severity": debt.severity,
        "failed_gate": debt.failed_gate,
        "missing_evidence": truncate_engine_artifact_text(&debt.missing_evidence, 140),
        "required_source_class": debt.required_source_class,
        "candidate_queries": debt.candidate_queries.iter().take(2).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
        "next_check_actions": debt.next_check_actions.iter().take(2).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
        "status": debt.status,
    })
}

fn compact_engine_appendix_artifacts(artifacts: &mut ResearchControllerArtifacts) {
    artifacts.source_cards.truncate(8);
    artifacts.claim_log.truncate(8);
    artifacts.research_debt.truncate(6);
    artifacts.warnings.truncate(6);
    for source in &mut artifacts.source_cards {
        source.title = truncate_engine_artifact_text(&source.title, 96);
        source.extracted_facts.clear();
        source.limitation = source
            .limitation
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 120));
        source.diagnostics_ref = None;
    }
    for claim in &mut artifacts.claim_log {
        claim.claim = truncate_engine_artifact_text(&claim.claim, 220);
        claim.support_urls.clear();
        claim.uncertainty_note = claim
            .uncertainty_note
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 120));
    }
    if let Some(state) = artifacts.narrative_state.as_mut() {
        state.topic_frame = state
            .topic_frame
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 120));
        state.working_thesis = state
            .working_thesis
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 180));
        state.reader_promise = state
            .reader_promise
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 160));
        state.last_iteration_summary = state
            .last_iteration_summary
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 160));
        state.timeline.truncate(8);
        state.actors.clear();
        state.causal_chain.truncate(3);
        for link in &mut state.causal_chain {
            link.cause = truncate_engine_artifact_text(&link.cause, 80);
            link.effect = truncate_engine_artifact_text(&link.effect, 80);
            link.rationale = link
                .rationale
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 100));
        }
        state.evidence_layers.truncate(3);
        state.interpretive_tensions.truncate(1);
        for tension in &mut state.interpretive_tensions {
            tension.question = truncate_engine_artifact_text(&tension.question, 100);
            tension.competing_readings = tension
                .competing_readings
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 100));
        }
        state.impacts.truncate(1);
        for impact in &mut state.impacts {
            impact.implication = impact
                .implication
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 100));
        }
        state.reader_questions.clear();
        state.section_outline.clear();
        state.transition_plan.clear();
        state.open_gaps.truncate(4);
        for item in &mut state.timeline {
            item.significance = item
                .significance
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 90));
        }
        for layer in &mut state.evidence_layers {
            layer.purpose = layer
                .purpose
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 100));
        }
        state.event_cards.truncate(8);
        for card in &mut state.event_cards {
            card.label = truncate_engine_artifact_text(&card.label, 80);
            card.timeframe = card
                .timeframe
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 48));
            card.actors = card
                .actors
                .iter()
                .take(4)
                .map(|value| truncate_engine_artifact_text(value, 48))
                .collect();
            card.region_or_front = card
                .region_or_front
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 80));
            card.trigger = card
                .trigger
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 120));
            card.development = card
                .development
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 140));
            card.outcome = card
                .outcome
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 120));
            card.causal_spine.truncate(1);
            for step in &mut card.causal_spine {
                step.description = truncate_engine_artifact_text(&step.description, 80);
                step.reasoning = None;
                step.limits.truncate(1);
            }
            card.interpretive_layers.truncate(1);
            for layer in &mut card.interpretive_layers {
                layer.interpretation = truncate_engine_artifact_text(&layer.interpretation, 90);
                layer.reasoning = None;
                layer.limits.truncate(1);
            }
            card.open_questions.truncate(1);
        }
    }
    artifacts.reader_quality = None;
}

fn truncate_engine_artifact_text(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        compact
    } else {
        compact.chars().take(limit).collect::<String>()
    }
}

fn render_phase_plan_rows(phase_plan: &HistoricalPhasePlan) -> String {
    if phase_plan.phases.is_empty() {
        return "| none | none | none | none | none | none | no |".to_string();
    }
    phase_plan
        .phases
        .iter()
        .map(|phase| {
            format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                escape_table_cell(&phase.label),
                escape_table_cell(phase.timeframe.as_deref().unwrap_or("")),
                escape_table_cell(&phase.actors.join(", ")),
                escape_table_cell(phase.region_or_front.as_deref().unwrap_or("")),
                escape_table_cell(&phase.claim_log_ids.join(", ")),
                escape_table_cell(&phase.source_ids.join(", ")),
                if phase.readiness.ready { "yes" } else { "no" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_phase_card_rows(artifacts: &ResearchControllerArtifacts) -> String {
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return "- no grounded phase cards".to_string();
    };
    if state.event_cards.is_empty() {
        return "- no grounded phase cards".to_string();
    }
    state
        .event_cards
        .iter()
        .map(|card| {
            format!(
                "- **{}**: {} | actors: {} | region/front: {} | trigger: {} | outcome: {}",
                card.label,
                card.timeframe.as_deref().unwrap_or(""),
                card.actors.join(", "),
                card.region_or_front.as_deref().unwrap_or(""),
                card.trigger.as_deref().unwrap_or(""),
                card.outcome.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_source_audit_rows(artifacts: &ResearchControllerArtifacts) -> String {
    if artifacts.source_cards.is_empty() {
        return "| none | none | none | none | none | none |".to_string();
    }
    artifacts
        .source_cards
        .iter()
        .map(|card| {
            format!(
                "| {} | {} | {} | {} | {} | {} |",
                escape_table_cell(&card.id),
                escape_table_cell(&card.url),
                escape_table_cell(&card.title),
                escape_table_cell(&card.source_class),
                escape_table_cell(
                    card.extracted_facts
                        .first()
                        .map(String::as_str)
                        .unwrap_or("")
                ),
                escape_table_cell(card.limitation.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_claim_log_rows(artifacts: &ResearchControllerArtifacts) -> String {
    if artifacts.claim_log.is_empty() {
        return "| none | none | none | none | none |".to_string();
    }
    artifacts
        .claim_log
        .iter()
        .map(|claim| {
            let support = if !claim.support_source_card_ids.is_empty() {
                claim.support_source_card_ids.join(", ")
            } else {
                claim.support_urls.join(", ")
            };
            format!(
                "| {} | {} | {} | {} | {} |",
                escape_table_cell(&claim.id),
                escape_table_cell(&claim.claim),
                escape_table_cell(&support),
                escape_table_cell(claim.confidence.as_deref().unwrap_or("")),
                escape_table_cell(claim.uncertainty_note.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_limits_rows(artifacts: &ResearchControllerArtifacts) -> String {
    let mut lines = Vec::new();
    for debt in artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
    {
        lines.push(format!("- {}", debt.missing_evidence));
    }
    for warning in &artifacts.warnings {
        lines.push(format!("- warning: {}", warning));
    }
    if lines.is_empty() {
        "No open conflicts or research debt remained after the isolated historical phase pass."
            .to_string()
    } else {
        lines.join("\n")
    }
}

fn render_quality_gate(quality_gate: &ResearchQualityGateArtifact) -> String {
    let failures = if quality_gate.failure_messages.is_empty() {
        "- none".to_string()
    } else {
        quality_gate
            .failure_messages
            .iter()
            .map(|message| format!("- {}", message))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "- status: {}\n- unsupported_claim_count: {}\n- unresolved_conflict_count: {}\n- open_debt_count: {}\n- failure_messages:\n{}",
        quality_gate.status,
        quality_gate.unsupported_claim_count,
        quality_gate.unresolved_conflict_count,
        quality_gate.open_debt_count,
        failures
    )
}

fn escape_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn legacy_artifact_ignore_warning(existing: Option<&str>) -> Option<String> {
    let existing = existing?.trim();
    if existing.is_empty() {
        return None;
    }
    Some(if existing.len() > 32_000 {
        "historical_phase_engine_ignored_oversized_legacy_controller_artifacts".to_string()
    } else {
        "historical_phase_engine_ignored_legacy_controller_artifacts".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_test_dir, test_state_with_historical_phase_engine_enabled};
    use liquid_storage_sqlite::setup_db;

    fn historical_source_document(title: &str, url: &str, claims: &[&str]) -> String {
        format!(
            "# {title}\nURL: {url}\nSource Class: authoritative_secondary\nConfidence: high\nFacts:\n- {title} provides chronology and consequence anchors.\nClaims:\n{}\n",
            claims
                .iter()
                .map(|claim| format!("- {claim}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    async fn insert_historical_task(
        db: &sqlx::SqlitePool,
        topic: &str,
        source_filenames: &[&str],
    ) -> i64 {
        sqlx::query("INSERT INTO tasks (original_name, status, file_prefix, file_type, source_filenames, research_topic, research_intensity, quality_depth) VALUES (?, 'researching', '[AI-Research]', 'md', ?, ?, 'high', 'strict')")
            .bind(topic)
            .bind(serde_json::to_string(source_filenames).unwrap())
            .bind(topic)
            .execute(db)
            .await
            .unwrap()
            .last_insert_rowid()
    }

    async fn load_task(db: &sqlx::SqlitePool, task_id: i64) -> TaskInfo {
        sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    async fn write_source_file(dir: &std::path::Path, filename: &str, body: &str) {
        tokio::fs::create_dir_all(dir).await.unwrap();
        tokio::fs::write(dir.join(filename), body).await.unwrap();
    }

    async fn write_historical_source_set(
        uploads: &std::path::Path,
        prefix: &str,
        rows: &[(&str, &str, &str)],
    ) -> Vec<String> {
        let mut filenames = Vec::new();
        for (index, (title, url, claim)) in rows.iter().enumerate() {
            let filename = format!("{prefix}-{}.md", index + 1);
            write_source_file(
                uploads,
                &filename,
                &historical_source_document(title, url, &[*claim]),
            )
            .await;
            filenames.push(filename);
        }
        filenames
    }

    #[test]
    fn evidence_sanitizer_rejects_prompt_provider_diagnostic_aliases() {
        for unsafe_text in [
            "provider_payload: raw upstream request",
            "raw_provider_payload: hidden provider body",
            "resolved_prompt: hidden resolved prompt",
            "resolved_system_prompt: hidden system prompt",
            "resolved_user_prompt: hidden user prompt",
            "source_diagnostics: raw search diagnostics",
            "controller_artifact_json: raw controller state",
            "raw_model_output: hidden model output",
        ] {
            assert!(
                engine_safe_evidence_text(unsafe_text, 320).is_none(),
                "unsafe evidence marker should be rejected: {unsafe_text}"
            );
        }
    }

    #[test]
    fn unsafe_evidence_filename_is_neutralized_for_debt() {
        let label = safe_evidence_filename_label("provider_payload.md", 0);
        let suffix = safe_evidence_filename_debt_suffix("resolved_prompt.md", 1);

        assert_eq!(label, "historical evidence file 1");
        assert_eq!(suffix, "historical-evidence-file-2");
        assert!(!label.contains("provider"));
        assert!(!suffix.contains("resolved"));
        assert!(!suffix.contains("prompt"));
    }

    #[tokio::test]
    async fn world_war_one_vertical_slice_is_accepted() {
        let dir = temp_test_dir("historical-phase-engine-wwi");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        let source_rows = vec![
            (
                "First Moroccan Crisis evidence",
                "https://www.britannica.com/event/Moroccan-crises",
                "1905-1906년 제1차 모로코 위기: 독일, 프랑스, 영국은 모로코·알헤시라스에서 독일의 모로코 개입과 프랑스 영향권 문제를 둘러싸고 충돌했고, 협상국 협조와 독일 고립 인식이 강화되었다.",
            ),
            (
                "Bosnian Crisis evidence",
                "https://www.britannica.com/event/Bosnian-crisis-of-1908",
                "1908-1909년 보스니아 병합 위기: 오스트리아-헝가리, 세르비아, 러시아는 보스니아·발칸에서 병합 선언과 세르비아 반발, 러시아 후퇴를 겪었고 발칸 위기가 동맹 정치와 직접 결합했다.",
            ),
            (
                "Agadir Crisis evidence",
                "https://history.state.gov/milestones/1899-1913/morocco",
                "1911년 아가디르 위기: 독일, 프랑스, 영국은 모로코·아가디르에서 판터호 파견과 프랑스의 모로코 행동을 두고 충돌했고, 영국과 프랑스의 안보 협력이 더 가시화되었다.",
            ),
            (
                "Balkan Wars evidence",
                "https://www.britannica.com/event/Balkan-Wars",
                "1912-1913년 발칸 전쟁: 발칸 동맹, 오스만 제국, 세르비아, 오스트리아-헝가리는 발칸에서 오스만 후퇴와 세르비아 팽창을 겪었고, 대국 동맹 개입 위험이 커졌다.",
            ),
            (
                "Sarajevo assassination evidence",
                "https://www.iwm.org.uk/history/how-the-first-world-war-began",
                "1914년 6월 사라예보 암살: 오스트리아-헝가리, 세르비아, 가브릴로 프린치프는 사라예보·보스니아에서 프란츠 페르디난트 암살 사건을 둘러싸고 충돌했고, 7월 위기와 최후통첩 판단이 전면화되었다.",
            ),
            (
                "July Ultimatum evidence",
                "https://www.nationalarchives.gov.uk/pathways/firstworldwar/spotlights/origins.htm",
                "1914년 7월 7월 최후통첩: 오스트리아-헝가리, 세르비아, 독일, 러시아는 빈·베오그라드에서 세르비아 최후통첩과 독일 지지를 둘러싸고 외교적 후퇴 공간이 좁아졌고, 동원 판단으로 위기가 이동했다.",
            ),
            (
                "Mobilization crisis evidence",
                "https://www.loc.gov/item/today-in-history/july-28/",
                "1914년 7-8월 러시아·독일 동원 위기: 러시아, 독일, 프랑스는 동유럽·서유럽에서 러시아 동원과 독일 대응 계획을 통해 외교 위기를 군사 일정의 문제로 전환했다.",
            ),
            (
                "Belgium and British entry evidence",
                "https://www.britannica.com/event/World-War-I",
                "1914년 8월 벨기에 침공과 영국 참전: 독일, 벨기에, 영국, 프랑스는 벨기에·서부전선에서 독일의 벨기에 침공과 중립 보장 문제를 겪었고, 영국 참전으로 전쟁이 확대되었다.",
            ),
        ];
        let source_filenames = write_historical_source_set(&uploads, "wwi", &source_rows).await;
        let source_filename_refs = source_filenames
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let topic = "1차 세계대전 전의 위기와 외교";
        let task_id = insert_historical_task(&db, topic, &source_filename_refs).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &source_filenames,
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let filename = completed
            .filename
            .expect("historical engine should save a file");
        let output = tokio::fs::read_to_string(uploads.join(filename))
            .await
            .unwrap();
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(
            completed.quality_status.as_deref(),
            Some("passed"),
            "failure={:?}",
            completed.quality_last_failure
        );
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("passed")
        );
        assert_eq!(output.matches("## Final Answer").count(), 1);
        assert_eq!(output.matches("# Verification Appendix").count(), 1);
        assert!(output.contains("제1차 모로코 위기"));
        assert!(output.contains("### Chronology And Interpretation"));
        assert!(artifacts
            .narrative_state
            .as_ref()
            .is_some_and(|state| state.event_cards.len() >= 8));
    }

    #[tokio::test]
    async fn world_war_two_vertical_slice_is_accepted() {
        let dir = temp_test_dir("historical-phase-engine-wwii");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        let source_rows = vec![
            (
                "Interwar norms evidence",
                "https://history.state.gov/milestones/1914-1920/league",
                "1919-1931년 전간기 평화규범과 집행 공백: 국제연맹, 영국, 프랑스는 유럽·국제연맹에서 베르사유 질서와 국제연맹 규범의 집행 공백을 드러냈고, 만주와 유럽의 시험 사례로 이어졌다.",
            ),
            (
                "Manchuria evidence",
                "https://history.state.gov/milestones/1921-1936/manchuria",
                "1931-1933년 만주사변과 비승인 외교: 일본, 중국, 국제연맹, 미국은 만주·동아시아에서 일본의 만주 점령과 만주국 수립을 둘러싸고 집단안보와 비승인 원칙의 약함을 드러냈다.",
            ),
            (
                "German rearmament evidence",
                "https://www.britannica.com/event/World-War-II/German-rearmament-and-the-Rhineland",
                "1933-1935년 독일 재무장과 베르사유 질서 해체: 독일, 영국, 프랑스는 독일·유럽에서 히틀러 정권의 재무장과 병역 부활을 겪었고 베르사유 제한과 조약 신뢰가 흔들렸다.",
            ),
            (
                "Ethiopia crisis evidence",
                "https://www.britannica.com/event/Italo-Ethiopian-War-1935-1936",
                "1935-1936년 에티오피아 위기와 국제연맹 실패: 이탈리아, 에티오피아, 국제연맹, 영국, 프랑스는 에티오피아·제네바에서 침공과 제재 문제를 겪었고 집단안보 실패가 드러났다.",
            ),
            (
                "Rhineland evidence",
                "https://www.iwm.org.uk/history/the-road-to-war",
                "1936년 라인란트 재무장: 독일, 프랑스, 영국은 라인란트·서유럽에서 독일군의 라인란트 진입과 로카르노 체제 위기를 겪었고, 서방의 군사 대응 회피가 독일의 위험 감수를 키웠다.",
            ),
            (
                "Spanish Civil War evidence",
                "https://www.britannica.com/event/Spanish-Civil-War",
                "1936-1939년 스페인 내전과 불간섭 실패: 스페인 공화파, 프랑코 세력, 독일, 이탈리아, 소련은 스페인·지중해에서 불간섭 원칙 약화와 외국 지원을 겪었고 추축 협력과 제한된 민주국가 대응이 뚜렷해졌다.",
            ),
            (
                "Austria Sudeten evidence",
                "https://www.britannica.com/event/Anschluss",
                "1938년 오스트리아 병합과 수데텐 위기: 독일, 오스트리아, 체코슬로바키아, 영국, 프랑스는 오스트리아·수데텐란트에서 병합과 수데텐 문제를 겪었고, 강압 외교가 뮌헨 협정으로 이어졌다.",
            ),
            (
                "Munich evidence",
                "https://www.britannica.com/event/Munich-Agreement",
                "1938년 9월 뮌헨 협정: 독일, 영국, 프랑스, 체코슬로바키아는 뮌헨·수데텐란트에서 수데텐란트 양보와 전쟁 회피를 결정했고, 프라하 점령으로 유화의 한계가 드러났다.",
            ),
            (
                "Prague Poland evidence",
                "https://history.state.gov/milestones/1937-1945/munich",
                "1939년 3-4월 프라하 점령과 폴란드 보장: 독일, 체코슬로바키아, 영국, 프랑스, 폴란드는 프라하·폴란드에서 독일의 프라하 점령과 폴란드 보장을 겪었고 유화에서 억지로 전환했다.",
            ),
            (
                "Nazi Soviet Pact evidence",
                "https://www.britannica.com/event/German-Soviet-Nonaggression-Pact",
                "1939년 8-9월 독소불가침조약과 폴란드 침공: 독일, 소련, 폴란드, 영국, 프랑스는 모스크바·폴란드에서 불가침조약과 폴란드 침공을 겪었고 제2차 세계대전이 시작되었다.",
            ),
        ];
        let source_filenames = write_historical_source_set(&uploads, "wwii", &source_rows).await;
        let source_filename_refs = source_filenames
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let topic = "2차대전 전의 각국 외교와 그 분석";
        let task_id = insert_historical_task(&db, topic, &source_filename_refs).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &source_filenames,
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let filename = completed
            .filename
            .expect("historical engine should save a file");
        let output = tokio::fs::read_to_string(uploads.join(filename))
            .await
            .unwrap();
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(
            completed.quality_status.as_deref(),
            Some("passed"),
            "failure={:?}",
            completed.quality_last_failure
        );
        assert!(output.contains("### Legacy And Impact"));
        assert!(output.contains("뮌헨 협정"));
        assert!(!output.contains("근거 연결 국면"));
        assert!(artifacts
            .narrative_state
            .as_ref()
            .is_some_and(|state| state.event_cards.len() >= 8));
    }

    #[tokio::test]
    async fn placeholder_legacy_cards_are_rejected_as_truth() {
        let dir = temp_test_dir("historical-phase-engine-placeholder");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "wwi.md",
            &historical_source_document(
                "World War I chronology",
                "https://www.britannica.com/event/World-War-I",
                &[
                    "1914 July Crisis in Sarajevo and the alliance system: Austria-Hungary, Serbia, Germany, Russia, France, and Britain turned assassination fallout into general war and fixed the opening fronts in Europe.",
                    "1914 Opening campaigns in Belgium and northern France: German armies crossed Belgium, French and British forces responded, and the Marne stopped a quick decision before trench warfare hardened the western front.",
                    "1915-1916 Eastern and southern widening: Russia, Austria-Hungary, Italy, and the Ottoman fronts expanded the war and forced both coalitions to spread manpower, logistics, and diplomacy across multiple theaters.",
                    "1916 Verdun and the Somme: France, Britain, and Germany fought attritional campaigns that consumed manpower, locked strategy into industrial endurance, and changed the next year's political and military choices.",
                    "1917 Upheaval and intervention: the Russian Revolutions, U-boat escalation, and United States entry altered coalition capacity, legitimacy, and the longer war balance.",
                    "1918 Allied offensives and the armistice settlement: German spring offensives failed, Allied counteroffensives broke imperial staying power, and the armistice opened the postwar order and treaty phase.",
                ],
            ),
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War I background, development, impact, and significance",
            &["wwi.md"],
        )
        .await;
        sqlx::query("UPDATE tasks SET research_controller_artifacts_json = ? WHERE id = ?")
            .bind(
                serde_json::json!({
                    "version": 1,
                    "narrative_state": {
                        "version": 1,
                        "event_cards": [
                            {"label": "Phase", "claim_log_ids": ["C999"], "source_ids": ["S999"]},
                            {"label": "Section 1", "claim_log_ids": ["C998"], "source_ids": ["S998"]}
                        ]
                    }
                })
                .to_string(),
            )
            .bind(task_id)
            .execute(&db)
            .await
            .unwrap();
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["wwi.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War I background, development, impact, and significance",
            )
            .await
        );

        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let labels = artifacts
            .narrative_state
            .as_ref()
            .map(|state| {
                state
                    .event_cards
                    .iter()
                    .map(|card| card.label.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(labels
            .iter()
            .all(|label| !historical_phase_label_is_placeholder(label)));
        assert!(artifacts.warnings.iter().any(|warning| {
            warning == "historical_phase_engine_ignored_legacy_controller_artifacts"
        }));
    }

    #[tokio::test]
    async fn missing_source_or_claim_is_blocked() {
        let dir = temp_test_dir("historical-phase-engine-blocked");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "incomplete.md",
            "# Incomplete\nFacts:\n- This file has no URL and no claim rows.\n",
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War I background, development, impact, and significance",
            &["incomplete.md"],
        )
        .await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["incomplete.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War I background, development, impact, and significance",
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(completed.quality_status.as_deref(), Some("blocked"));
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("failed")
        );
        assert!(artifacts.research_debt.iter().any(|debt| {
            debt.missing_evidence.contains("missing a public URL")
                || debt
                    .missing_evidence
                    .contains("could not build any phase-specific Claim Log")
        }));
    }

    #[tokio::test]
    async fn oversized_legacy_artifact_is_ignored() {
        let dir = temp_test_dir("historical-phase-engine-oversized");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "wwii.md",
            &historical_source_document(
                "World War II chronology",
                "https://www.britannica.com/event/World-War-II",
                &[
                    "1939 German and Soviet moves in Poland: Germany, the Soviet Union, Poland, Britain, and France turned the European crisis into open war and fixed the first diplomatic and military alignments.",
                    "1940 Western collapse and the Battle of Britain: German campaigns overran France and the Low Countries, Britain held the air and maritime approaches, and the war split into continental occupation and offshore resistance.",
                    "1941 Operation Barbarossa and the global widening: Germany invaded the Soviet Union, Japan attacked Pearl Harbor, and the conflict became a truly global coalition war.",
                    "1942-1943 Midway, Stalingrad, and North Africa: the United States, Britain, Germany, Japan, and the Soviet Union reversed the initiative across sea, land, and imperial theaters.",
                    "1944 Allied return to Western Europe: the Normandy landings, Soviet advances, and strategic bombing narrowed Axis options and pushed the war toward simultaneous collapse on multiple fronts.",
                    "1945 Final offensives, surrender, and postwar settlement: Berlin fell, Japan surrendered after devastating final campaigns, and the wartime coalition turned toward occupation, partition, and a new international order.",
                ],
            ),
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War II background, development, impact, and significance",
            &["wwii.md"],
        )
        .await;
        let oversized = format!(
            "{{\"version\":1,\"warnings\":[\"{}\"],\"source_cards\":[],\"claim_log\":[],\"conflict_map\":[],\"research_debt\":[]}}",
            "x".repeat(40_000)
        );
        sqlx::query("UPDATE tasks SET research_controller_artifacts_json = ? WHERE id = ?")
            .bind(oversized)
            .bind(task_id)
            .execute(&db)
            .await
            .unwrap();
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["wwii.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War II background, development, impact, and significance",
            )
            .await
        );

        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let json = serde_json::to_string(&artifacts).unwrap();
        assert!(json.len() < 20_000, "artifact_json_len={}", json.len());
        assert!(artifacts.warnings.iter().any(|warning| {
            warning == "historical_phase_engine_ignored_oversized_legacy_controller_artifacts"
        }));
    }

    #[tokio::test]
    async fn non_historical_requests_noop_even_when_flag_enabled() {
        let dir = temp_test_dir("historical-phase-engine-noop");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), dir.join("uploads"), true);
        let task_id = sqlx::query("INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Tech guide', 'researching', '[AI-Research]', 'md', 'modern C++ work-stealing scheduler implementation guide', 'high', 'strict')")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
        let task = load_task(&db, task_id).await;

        assert!(
            !try_run_historical_phase_engine(
                &state,
                &task,
                &[],
                Vec::new(),
                "[AI-Research]",
                "md",
                "modern C++ work-stealing scheduler implementation guide",
            )
            .await
        );
    }

    #[tokio::test]
    async fn rendered_output_contains_exactly_one_final_answer_and_appendix() {
        let mut artifacts = ResearchControllerArtifacts {
            version: 1,
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://www.britannica.com/event/World-War-I".to_string(),
                title: "World War I".to_string(),
                source_class: "authoritative_secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["World War I chronology anchor".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1914 July Crisis in Sarajevo and the alliance system turned assassination fallout into general war.".to_string(),
                claim_type: Some("event".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            ..ResearchControllerArtifacts::default()
        };
        let plan = build_historical_phase_plan(&artifacts, "World War I");
        build_engine_planning_artifacts(&mut artifacts, &plan, "World War I");
        let output = render_historical_phase_engine_output(
            "World War I",
            &artifacts,
            &plan,
            &ResearchQualityGateArtifact {
                status: "passed".to_string(),
                failure_messages: Vec::new(),
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            },
        );

        assert_eq!(output.matches("## Final Answer").count(), 1);
        assert_eq!(output.matches("# Verification Appendix").count(), 1);
    }
}
