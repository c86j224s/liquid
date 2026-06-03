use super::*;
use crate::application::ports::classic_research_implementation;

#[derive(Debug, Clone)]
pub struct ResearchBenchmarkDebugCaseResult {
    pub case_id: String,
    pub title: String,
    pub category: String,
    pub mode: ResearchBenchmarkMode,
    pub data_dir: PathBuf,
    pub task_id: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub quality_status: Option<String>,
    pub quality_last_failure: Option<String>,
    pub research_controller_stage: Option<String>,
    pub research_controller_iteration: Option<i64>,
    pub research_controller_max_iterations: Option<i64>,
    pub output_filename: Option<String>,
    pub final_output: Option<String>,
    pub research_controller_artifacts_json: Option<String>,
    pub research_source_diagnostics_json: Option<String>,
    pub resolved_system_prompt: Option<String>,
    pub resolved_user_prompt: Option<String>,
    pub model_input: String,
}

impl ResearchBenchmarkDebugCaseResult {
    pub fn safe_result(&self) -> ResearchBenchmarkCaseResult {
        ResearchBenchmarkCaseResult {
            case_id: self.case_id.clone(),
            title: self.title.clone(),
            category: self.category.clone(),
            mode: self.mode,
            task_id: self.task_id,
            status: self.status.clone(),
            error_message: self.error_message.clone(),
            quality_status: self.quality_status.clone(),
            quality_last_failure: self.quality_last_failure.clone(),
            research_controller_stage: self.research_controller_stage.clone(),
            research_controller_iteration: self.research_controller_iteration,
            research_controller_max_iterations: self.research_controller_max_iterations,
            output_filename: self.output_filename.clone(),
            final_output: self.final_output.clone(),
            model_input: self.model_input.clone(),
        }
    }
}

pub(super) fn benchmark_ai_task_timeout_secs(timeout_secs: u64) -> u64 {
    timeout_secs.max(1)
}

pub async fn run_research_benchmark_case(
    input: ResearchBenchmarkCaseInput,
) -> Result<ResearchBenchmarkCaseResult, String> {
    run_research_benchmark_case_debug(input)
        .await
        .map(|result| result.safe_result())
}

pub async fn run_research_benchmark_case_debug(
    input: ResearchBenchmarkCaseInput,
) -> Result<ResearchBenchmarkDebugCaseResult, String> {
    let data_dir = setup_data_dir(&input.data_dir).map_err(|error| error.to_string())?;
    let uploads_path = data_dir.join("uploads");
    fs::create_dir_all(&uploads_path)
        .await
        .map_err(|error| error.to_string())?;
    let db = setup_db(&data_dir)
        .await
        .map_err(|error| error.to_string())?;
    let (tx, _) = broadcast::channel(32);
    let cli_launch_mode = input
        .cli_launch_mode
        .as_deref()
        .unwrap_or("auto")
        .parse::<CliLaunchMode>()
        .map_err(|error| format!("invalid cli launch mode: {error}"))?;
    let model_input = match input.mode {
        ResearchBenchmarkMode::Fixture => input
            .model_input
            .clone()
            .unwrap_or_else(|| "cli:fixture-research-bench".to_string()),
        ResearchBenchmarkMode::Live => input
            .model_input
            .clone()
            .ok_or_else(|| "live benchmark mode requires model_input".to_string())?,
        ResearchBenchmarkMode::Replay => {
            return Err(
                "replay benchmark mode must use replay_research_benchmark_case".to_string(),
            );
        }
    };
    let state = Arc::new(AppState {
        db,
        data_dir: data_dir.clone(),
        uploads_path,
        tx,
        queue_notify: Notify::new(),
        active_tasks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        ai_workers: 1,
        local_ai_workers: 1,
        ai_task_timeout_secs: benchmark_ai_task_timeout_secs(input.ai_task_timeout_secs),
        cli_launch_mode,
        research_implementation_id: "classic",
        research_implementation: classic_research_implementation(),
        research_historical_phase_engine: false,
        benchmark_fixture: match input.mode {
            ResearchBenchmarkMode::Fixture => Some(build_benchmark_fixture(&input)),
            ResearchBenchmarkMode::Live | ResearchBenchmarkMode::Replay => None,
        },
    });
    let research_mode = benchmark_research_mode(&input.category);
    let system_prompt =
        state
            .research_implementation
            .build_research_system_prompt(research_mode, "md", None);
    let user_prompt = state
        .research_implementation
        .build_topic_research_user_prompt(&input.prompt, None);
    let metadata = TaskMetadata {
        source_file_ids: Some("[]".to_string()),
        research_type: Some("initial".to_string()),
        research_mode: Some(research_mode.to_string()),
        research_format: Some("md".to_string()),
        research_topic: Some(input.prompt.clone()),
        research_instructions: None,
        prompt_version: Some("research-controller-v19".to_string()),
        web_search_requested: Some("true".to_string()),
        web_search_provider: match input.mode {
            ResearchBenchmarkMode::Fixture => Some("fixture-source-pack".to_string()),
            ResearchBenchmarkMode::Live | ResearchBenchmarkMode::Replay => None,
        },
        engine_kind: Some(match input.mode {
            ResearchBenchmarkMode::Fixture => "fixture".to_string(),
            ResearchBenchmarkMode::Live => benchmark_engine_kind(&model_input).to_string(),
            ResearchBenchmarkMode::Replay => "replay".to_string(),
        }),
        resolved_model: Some(benchmark_model_name(&model_input).to_string()),
        research_intensity: Some(input.research_intensity.clone()),
        quality_max_iterations: Some(input.quality_max_iterations),
        quality_depth: Some(input.quality_depth.clone()),
        ..TaskMetadata::default()
    };
    let task_id = run_ai_task(
        Arc::clone(&state),
        Vec::new(),
        Vec::new(),
        input.title.clone(),
        model_input.clone(),
        system_prompt,
        user_prompt,
        "[AI-Research]",
        "md",
        Vec::new(),
        None,
        Some(metadata),
    )
    .await;
    if task_id == 0 {
        return Err("failed to queue benchmark task".to_string());
    }

    let lane = benchmark_queue_lane(&model_input);
    let task = claim_next_ai_task(&state, lane)
        .await
        .ok_or_else(|| "failed to claim queued benchmark task".to_string())?;
    if task.id != task_id {
        return Err(format!(
            "claimed benchmark task {} but expected {}",
            task.id, task_id
        ));
    }
    execute_claimed_task(Arc::clone(&state), task).await;

    let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    let final_output = match task.filename.as_deref() {
        Some(filename) => fs::read_to_string(state.uploads_path.join(filename))
            .await
            .ok(),
        None => None,
    };
    Ok(ResearchBenchmarkDebugCaseResult {
        case_id: input.case_id,
        title: input.title,
        category: input.category,
        mode: input.mode,
        data_dir,
        task_id,
        status: task.status,
        error_message: task.error_message,
        quality_status: task.quality_status,
        quality_last_failure: task.quality_last_failure,
        research_controller_stage: task.research_controller_stage,
        research_controller_iteration: task.research_controller_iteration,
        research_controller_max_iterations: task.research_controller_max_iterations,
        output_filename: task.filename,
        final_output,
        research_controller_artifacts_json: task.research_controller_artifacts_json,
        research_source_diagnostics_json: task.research_source_diagnostics_json,
        resolved_system_prompt: task.resolved_system_prompt,
        resolved_user_prompt: task.resolved_user_prompt,
        model_input,
    })
}

pub fn replay_research_benchmark_case(
    input: ResearchReplayCaseInput,
) -> Result<ResearchBenchmarkCaseResult, String> {
    replay_research_benchmark_case_debug(input).map(|result| result.safe_result())
}

pub fn replay_research_benchmark_case_debug(
    input: ResearchReplayCaseInput,
) -> Result<ResearchBenchmarkDebugCaseResult, String> {
    let artifact_processor = DefaultArtifactProcessor;
    let mut artifacts =
        serde_json::from_str::<ResearchControllerArtifacts>(&input.controller_artifacts_json)
            .map_err(|error| format!("invalid replay controller artifacts JSON: {error}"))?;
    normalize_deferred_conflicts_to_actionable_debt(&mut artifacts);
    let diagnostics = serde_json::from_str::<ResearchSourceDiagnosticsEnvelope>(
        &input.research_source_diagnostics_json,
    )
    .map_err(|error| format!("invalid replay source diagnostics JSON: {error}"))?;
    let context = ResearchQualityContext {
        file_prefix: "[AI-Research]",
        file_type: "md",
        web_search_requested: true,
        research_intensity: Some(input.research_intensity.as_str()),
        quality_depth: Some(input.quality_depth.as_str()),
        research_topic: Some(input.prompt.as_str()),
        research_instructions: None,
        evidence_subject: diagnostics.subject.as_deref(),
    };
    let finalized = artifact_processor.finalize(
        &normalize_ai_output(&input.draft_output, "md"),
        &artifacts,
        Some(&diagnostics),
        &context,
    );
    let mut finalized_artifacts = artifacts.clone();
    finalized_artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    if finalized.diagnostics.repaired_final_answer {
        push_unique_warning(
            &mut finalized_artifacts.warnings,
            "artifact_finalizer_repaired_final_answer".to_string(),
        );
    }
    if finalized.diagnostics.synthesized_final_answer {
        push_unique_warning(
            &mut finalized_artifacts.warnings,
            "artifact_finalizer_synthesized_final_answer".to_string(),
        );
    }
    let quality_result = artifact_processor
        .validate_output(&finalized.output, &context)
        .map_err(|error| format!("Research quality gate failed: {error}"));
    match artifact_processor.validate_artifacts(
        &finalized_artifacts,
        Some(input.research_intensity.as_str()),
        Some(input.quality_depth.as_str()),
    ) {
        Ok(()) => {}
        Err(errors) => {
            return Err(format!(
                "Research artifact gate failed: {}",
                errors.join("; ")
            ));
        }
    }
    let mut failure_messages = quality_result
        .as_ref()
        .err()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(failure) = scaffold_trust_block_failure(
        &finalized_artifacts,
        Some(input.research_intensity.as_str()),
        Some(input.quality_depth.as_str()),
    ) {
        failure_messages.push(failure);
    }
    finalized_artifacts.quality_gate = Some(research_quality_gate_from_failures(
        &finalized_artifacts,
        &failure_messages,
    ));
    let synchronized_output = artifact_processor
        .finalize(
            &normalize_ai_output(&input.draft_output, "md"),
            &finalized_artifacts,
            Some(&diagnostics),
            &context,
        )
        .output;
    let quality_status = if failure_messages.is_empty() {
        Some("passed".to_string())
    } else {
        Some("untrusted".to_string())
    };
    let quality_last_failure = failure_messages.first().cloned();
    Ok(ResearchBenchmarkDebugCaseResult {
        case_id: input.case_id,
        title: input.title,
        category: input.category,
        mode: ResearchBenchmarkMode::Replay,
        data_dir: PathBuf::new(),
        task_id: 0,
        status: "completed".to_string(),
        error_message: None,
        quality_status,
        quality_last_failure,
        research_controller_stage: Some(if failure_messages.is_empty() {
            RESEARCH_STAGE_FINAL.to_string()
        } else {
            RESEARCH_STAGE_UNTRUSTED.to_string()
        }),
        research_controller_iteration: Some(1),
        research_controller_max_iterations: Some(1),
        output_filename: None,
        final_output: Some(synchronized_output),
        research_controller_artifacts_json: Some(
            serde_json::to_string(&finalized_artifacts).unwrap_or_else(|_| "{}".to_string()),
        ),
        research_source_diagnostics_json: Some(
            serde_json::to_string(&diagnostics).unwrap_or_else(|_| "{}".to_string()),
        ),
        resolved_system_prompt: None,
        resolved_user_prompt: None,
        model_input: input.model_input,
    })
}
