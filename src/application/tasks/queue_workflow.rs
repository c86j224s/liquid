use super::*;

pub(crate) async fn run_ai_task(
    state: Arc<AppState>,
    file_ids: Vec<i64>,
    filenames: Vec<String>,
    original_name: String,
    model_input: String,
    system_prompt: String,
    user_prompt: String,
    file_prefix: &str,
    file_type: &str,
    cleanup_files: Vec<String>,
    existing_task_id: Option<i64>,
    task_metadata: Option<TaskMetadata>,
) -> i64 {
    let source_filenames_json =
        serde_json::to_string(&filenames).unwrap_or_else(|_| "[]".to_string());
    let source_file_ids_json =
        serde_json::to_string(&file_ids).unwrap_or_else(|_| "[]".to_string());
    let cleanup_files_json =
        serde_json::to_string(&cleanup_files).unwrap_or_else(|_| "[]".to_string());
    let task_metadata = task_metadata.unwrap_or_default();
    let web_search_requested = task_metadata
        .web_search_requested
        .clone()
        .unwrap_or_else(|| {
            state
                .research_implementation
                .research_allows_web_search(file_prefix)
                .to_string()
        });
    let model_name = model_input
        .strip_prefix("cli:")
        .or_else(|| model_input.strip_prefix("pi:"))
        .unwrap_or(model_input.as_str());
    let model_source = if model_input.starts_with("cli:") {
        "cli"
    } else if model_input.starts_with("pi:") {
        "pi"
    } else {
        "ollama"
    };
    let web_search_provider = task_metadata
        .web_search_provider
        .clone()
        .unwrap_or_else(|| {
            state
                .research_implementation
                .web_search_provider_for(model_source, model_name, web_search_requested == "true")
                .to_string()
        });
    let source_file_ids = task_metadata
        .source_file_ids
        .clone()
        .unwrap_or(source_file_ids_json);
    let quality_max_iterations = normalized_quality_max_iterations(
        file_prefix,
        task_metadata.quality_max_iterations,
        task_metadata.research_intensity.as_deref(),
    );
    let quality_depth = normalized_quality_depth(
        file_prefix,
        task_metadata.quality_depth.as_deref(),
        task_metadata.research_intensity.as_deref(),
    );
    let stored_system_prompt =
        crate::research_design::redact_html_design_prompt_for_storage(&system_prompt);

    let task_id = if let Some(id) = existing_task_id {
        let controller_max_iterations =
            research_controller_max_iterations(file_prefix, quality_max_iterations);
        let rows_affected = sqlx::query("UPDATE tasks SET status = 'queued', error_message = NULL, file_id = NULL, filename = NULL, created_at = CASE WHEN status IN ('failed', 'interrupted', 'completed') THEN CURRENT_TIMESTAMP ELSE created_at END, model = ?, system_prompt = ?, user_prompt = ?, source_file_ids = ?, source_filenames = ?, file_prefix = ?, file_type = ?, cleanup_files = ?, research_type = ?, research_mode = ?, research_format = ?, research_topic = ?, research_instructions = ?, prompt_version = ?, web_search_requested = ?, web_search_provider = ?, engine_preset_id = ?, engine_preset_name = ?, engine_kind = ?, resolved_model = ?, research_intensity = ?, fallback_used = ?, fallback_reason = ?, quality_current_iteration = 0, quality_max_iterations = ?, quality_status = NULL, quality_depth = ?, quality_last_failure = NULL, research_controller_stage = NULL, research_controller_iteration = 0, research_controller_max_iterations = ?, research_controller_artifacts_json = NULL, research_source_diagnostics_json = NULL, resolved_system_prompt = NULL, resolved_user_prompt = NULL WHERE id = ?")
            .bind(&model_input)
            .bind(&stored_system_prompt)
            .bind(&user_prompt)
            .bind(&source_file_ids)
            .bind(&source_filenames_json)
            .bind(file_prefix)
            .bind(file_type)
            .bind(&cleanup_files_json)
            .bind(&task_metadata.research_type)
            .bind(&task_metadata.research_mode)
            .bind(&task_metadata.research_format)
            .bind(&task_metadata.research_topic)
            .bind(&task_metadata.research_instructions)
            .bind(&task_metadata.prompt_version)
            .bind(&web_search_requested)
            .bind(&web_search_provider)
            .bind(task_metadata.engine_preset_id)
            .bind(&task_metadata.engine_preset_name)
            .bind(&task_metadata.engine_kind)
            .bind(&task_metadata.resolved_model)
            .bind(&task_metadata.research_intensity)
            .bind(&task_metadata.fallback_used)
            .bind(&task_metadata.fallback_reason)
            .bind(quality_max_iterations)
            .bind(&quality_depth)
            .bind(controller_max_iterations)
            .bind(id)
            .execute(&state.db)
            .await
            .map(|result| result.rows_affected())
            .unwrap_or(0);
        if rows_affected != 1 {
            return 0;
        }
        id
    } else {
        let controller_max_iterations =
            research_controller_max_iterations(file_prefix, quality_max_iterations);
        let result = match sqlx::query("INSERT INTO tasks (original_name, status, model, system_prompt, user_prompt, source_file_ids, source_filenames, file_prefix, file_type, cleanup_files, research_type, research_mode, research_format, research_topic, research_instructions, prompt_version, web_search_requested, web_search_provider, engine_preset_id, engine_preset_name, engine_kind, resolved_model, research_intensity, fallback_used, fallback_reason, quality_current_iteration, quality_max_iterations, quality_status, quality_depth, quality_last_failure, research_controller_stage, research_controller_iteration, research_controller_max_iterations, research_controller_artifacts_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, NULL, ?, NULL, NULL, 0, ?, NULL)")
            .bind(&original_name)
            .bind("queued")
            .bind(&model_input)
            .bind(&stored_system_prompt)
            .bind(&user_prompt)
            .bind(&source_file_ids)
            .bind(&source_filenames_json)
            .bind(file_prefix)
            .bind(file_type)
            .bind(&cleanup_files_json)
            .bind(&task_metadata.research_type)
            .bind(&task_metadata.research_mode)
            .bind(&task_metadata.research_format)
            .bind(&task_metadata.research_topic)
            .bind(&task_metadata.research_instructions)
            .bind(&task_metadata.prompt_version)
            .bind(&web_search_requested)
            .bind(&web_search_provider)
            .bind(task_metadata.engine_preset_id)
            .bind(&task_metadata.engine_preset_name)
            .bind(&task_metadata.engine_kind)
            .bind(&task_metadata.resolved_model)
            .bind(&task_metadata.research_intensity)
            .bind(&task_metadata.fallback_used)
            .bind(&task_metadata.fallback_reason)
            .bind(quality_max_iterations)
            .bind(&quality_depth)
            .bind(controller_max_iterations)
            .execute(&state.db).await {
                Ok(r) => r,
                Err(_) => return 0,
            };
        result.last_insert_rowid()
    };

    let _ = state.tx.send(task_update_event(
        task_id,
        "queued",
        original_name.clone(),
        None,
    ));
    state.queue_notify.notify_waiters();
    task_id
}

pub(crate) fn spawn_ai_queue_workers(
    state: Arc<AppState>,
    cloud_worker_count: usize,
    local_worker_count: usize,
) {
    for _ in 0..cloud_worker_count.max(1) {
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            ai_queue_worker(worker_state, QueueLane::Cloud).await;
        });
    }
    for _ in 0..local_worker_count.max(1) {
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            ai_queue_worker(worker_state, QueueLane::Local).await;
        });
    }
}

async fn ai_queue_worker(state: Arc<AppState>, lane: QueueLane) {
    loop {
        while let Some(task) = claim_next_ai_task(&state, lane).await {
            execute_claimed_task_with_cancellation(Arc::clone(&state), task).await;
        }

        state.queue_notify.notified().await;
    }
}

async fn execute_claimed_task_with_cancellation(state: Arc<AppState>, task: TaskInfo) {
    let task_id = task.id;
    let original_name = task.original_name.clone();
    let task_state = Arc::clone(&state);
    let join_handle = tokio::spawn(async move {
        execute_claimed_task(task_state, task).await;
    });
    let abort_handle = join_handle.abort_handle();
    state
        .active_tasks
        .lock()
        .await
        .insert(task_id, abort_handle);

    let result = join_handle.await;
    state.active_tasks.lock().await.remove(&task_id);

    if result.as_ref().is_err_and(|error| error.is_cancelled()) {
        mark_task_interrupted_if_present(&state, task_id, &original_name).await;
    }
}

pub(super) async fn mark_task_interrupted_if_present(
    state: &AppState,
    task_id: i64,
    original_name: &str,
) {
    let rows_affected = sqlx::query(
        "UPDATE tasks SET status = 'interrupted', error_message = ? WHERE id = ? AND status IN ('processing', 'translating', 'researching', 'scraping')",
    )
    .bind(TASK_CANCELLED_MESSAGE)
    .bind(task_id)
    .execute(&state.db)
    .await
    .map(|result| result.rows_affected())
    .unwrap_or(0);

    if rows_affected > 0 {
        let _ = state.tx.send(TaskUpdateEvent {
            id: task_id,
            status: "interrupted".to_string(),
            original_name: original_name.to_string(),
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
        });
    }
}
