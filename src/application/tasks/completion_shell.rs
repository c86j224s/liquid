use super::*;

pub(super) async fn execute_scrape_task(state: &AppState, task: TaskInfo) {
    let source_acquisition = DefaultSourceAcquisition;
    let scrape_input = parse_scrape_task_input(task.user_prompt.as_deref(), &task.original_name);
    let translate = task.file_prefix.as_deref() == Some("[Scrape+KO]");
    let ScrapeResult {
        title,
        markdown,
        diagnostics,
    } = match source_acquisition
        .scrape_to_markdown(&scrape_input.url, &scrape_input.references)
        .await
    {
        Ok(result) => result,
        Err(diagnostics) => {
            persist_research_source_diagnostics(
                state,
                task.id,
                ResearchSourceDiagnosticsEnvelope {
                    version: 1,
                    subject: Some(scrape_input.url.clone()),
                    source_pack: None,
                    scrapes: vec![diagnostics.clone()],
                    context_packing: None,
                },
            )
            .await;
            let error_message = friendly_scrape_failure_message(
                diagnostics.status_class.as_str(),
                diagnostics.failure_reason.as_deref(),
                diagnostics.http_status_code,
                diagnostics.insufficiency_reason.as_deref(),
            )
            .unwrap_or_else(|| {
                diagnostics
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| "스크랩 처리 중 오류가 발생했습니다.".to_string())
            });
            fail_task(state, task.id, &task.original_name, error_message).await;
            return;
        }
    };
    persist_research_source_diagnostics(
        state,
        task.id,
        ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some(scrape_input.url.clone()),
            source_pack: None,
            scrapes: vec![diagnostics],
            context_packing: None,
        },
    )
    .await;

    let unique_filename = format!("{}-scraped.md", Uuid::new_v4());
    let path = state.uploads_path.join(&unique_filename);
    let display_name = scrape_task_identity_name(&title, &scrape_input.url);
    if let Err(e) = fs::write(&path, markdown).await {
        fail_task(state, task.id, &display_name, e.to_string()).await;
        return;
    }

    if !translate {
        let file_id = match sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, 'md', 'draft')",
        )
        .bind(&unique_filename)
        .bind(&display_name)
        .execute(&state.db)
        .await
        {
            Ok(result) => result.last_insert_rowid(),
            Err(e) => {
                let _ = fs::remove_file(&path).await;
                fail_task(state, task.id, &display_name, e.to_string()).await;
                return;
            }
        };
        let tag_labels = task_system_tag_labels(state, task.id, "[Scrape]", None).await;
        if assign_file_tag_labels(&state.db, file_id, &tag_labels, "task")
            .await
            .is_err()
        {
            let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
                .bind(&unique_filename)
                .execute(&state.db)
                .await;
            let _ = fs::remove_file(&path).await;
            fail_task(
                state,
                task.id,
                &display_name,
                "Failed to assign scrape tags",
            )
            .await;
            return;
        }

        let completed_rows = match sqlx::query("UPDATE tasks SET original_name = ?, status = 'completed', error_message = NULL, file_id = ?, filename = ? WHERE id = ?")
            .bind(&display_name)
            .bind(file_id)
            .bind(&unique_filename)
            .bind(task.id)
            .execute(&state.db).await {
                Ok(result) => result.rows_affected(),
                Err(e) => {
                    let _ = sqlx::query("DELETE FROM files WHERE filename = ?").bind(&unique_filename).execute(&state.db).await;
                    let _ = fs::remove_file(&path).await;
                    fail_task(state, task.id, &display_name, e.to_string()).await;
                    return;
                }
            };
        if completed_rows != 1 {
            let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
                .bind(&unique_filename)
                .execute(&state.db)
                .await;
            let _ = fs::remove_file(&path).await;
            fail_task(
                state,
                task.id,
                &display_name,
                "Failed to complete scrape task",
            )
            .await;
            return;
        }
        create_document_links_best_effort(state, task.id, file_id).await;
        let _ = state.tx.send(TaskUpdateEvent {
            id: task.id,
            status: "completed".to_string(),
            original_name: display_name,
            quality_current_iteration: task.quality_current_iteration,
            quality_max_iterations: task.quality_max_iterations,
            quality_status: task.quality_status.clone(),
            research_controller_stage: task.research_controller_stage.clone(),
            research_controller_iteration: task.research_controller_iteration,
            research_controller_max_iterations: task.research_controller_max_iterations,
        });
        return;
    }

    let model_input = task.model.clone().unwrap_or_default();
    let source_filenames_json =
        serde_json::to_string(&vec![unique_filename.clone()]).unwrap_or_else(|_| "[]".to_string());
    let cleanup_files_json =
        serde_json::to_string(&vec![unique_filename.clone()]).unwrap_or_else(|_| "[]".to_string());
    let system_prompt = "You are a professional technical translator. PRESERVE all Markdown formatting. Output ONLY the translated Korean text.".to_string();
    let user_prompt = "Translate the following content to Korean:".to_string();

    let updated = sqlx::query("UPDATE tasks SET original_name = ?, status = 'translating', error_message = NULL, system_prompt = ?, user_prompt = ?, source_filenames = ?, file_prefix = '[KO]', file_type = 'md', cleanup_files = ? WHERE id = ?")
        .bind(&display_name)
        .bind(&system_prompt)
        .bind(&user_prompt)
        .bind(&source_filenames_json)
        .bind(&cleanup_files_json)
        .bind(task.id)
        .execute(&state.db).await
        .map(|r| r.rows_affected())
        .unwrap_or(0);
    if updated == 0 {
        let _ = fs::remove_file(&path).await;
        fail_task(
            state,
            task.id,
            &display_name,
            "Failed to transition scraped task to translation",
        )
        .await;
        return;
    }

    let _ = state.tx.send(TaskUpdateEvent {
        id: task.id,
        status: "translating".to_string(),
        original_name: display_name.clone(),
        quality_current_iteration: task.quality_current_iteration,
        quality_max_iterations: task.quality_max_iterations,
        quality_status: task.quality_status.clone(),
        research_controller_stage: task.research_controller_stage.clone(),
        research_controller_iteration: task.research_controller_iteration,
        research_controller_max_iterations: task.research_controller_max_iterations,
    });
    let (source, model_name) = if let Some(model) = model_input.strip_prefix("cli:") {
        ("cli", model)
    } else if let Some(model) = model_input.strip_prefix("pi:") {
        ("pi", model)
    } else {
        ("ollama", model_input.as_str())
    };
    let runtime = AppModelRuntime::new(state);
    let result_text = runtime
        .execute(ModelRuntimeRequest {
            task_id: task.id,
            filenames: vec![unique_filename.clone()],
            model_name,
            source,
            system_prompt: &system_prompt,
            user_prompt: &user_prompt,
            research_subject_prompt: None,
            file_prefix: "[KO]",
            web_search_requested: None,
            web_search_provider_override: None,
            research_intensity: None,
            fallback_used: false,
            fallback_reason: None,
        })
        .await;
    handle_task_completion(
        state,
        task.id,
        result_text,
        display_name,
        "[KO]",
        "md",
        vec![unique_filename],
        true,
    )
    .await;
}

pub(super) async fn fail_task(
    state: &AppState,
    task_id: i64,
    original_name: &str,
    error_message: impl Into<String>,
) {
    let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
        .bind(error_message.into())
        .bind(task_id)
        .execute(&state.db)
        .await;
    let _ = state.tx.send(TaskUpdateEvent {
        id: task_id,
        status: "failed".to_string(),
        original_name: original_name.to_string(),
        quality_current_iteration: None,
        quality_max_iterations: None,
        quality_status: None,
        research_controller_stage: None,
        research_controller_iteration: None,
        research_controller_max_iterations: None,
    });
}

pub(super) async fn create_document_links_best_effort(
    state: &AppState,
    task_id: i64,
    output_file_id: i64,
) {
    if let Err(err) = liquid_workspace::create_document_links_for_task_output(
        &liquid_storage_sqlite::SqliteWorkspaceStore::new(&state.db),
        task_id,
        output_file_id,
    )
    .await
    {
        eprintln!(
            "Failed to create document links for task {task_id}, output file {output_file_id}: {err}"
        );
    }
}

pub(super) async fn task_progress_snapshot(
    state: &AppState,
    task_id: i64,
) -> Option<TaskProgressSnapshot> {
    sqlx::query_as::<
        _,
        (
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
        ),
    >(
        "SELECT quality_current_iteration, quality_max_iterations, quality_status, research_controller_stage, research_controller_iteration, research_controller_max_iterations FROM tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
}

pub(super) async fn send_task_update_from_progress(
    state: &AppState,
    task_id: i64,
    status: &str,
    original_name: String,
) {
    let progress = task_progress_snapshot(state, task_id).await;
    let _ = state.tx.send(task_update_event_from_progress(
        task_id,
        status,
        original_name,
        progress.as_ref(),
    ));
}

pub(super) async fn task_system_tag_labels(
    state: &AppState,
    task_id: i64,
    file_prefix: &str,
    quality_status_override: Option<&str>,
) -> Vec<String> {
    let metadata = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT model, resolved_model, engine_kind, quality_status FROM tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let (model, resolved_model, engine_kind, quality_status) =
        metadata.unwrap_or((None, None, None, None));
    system_tag_labels_for_task(
        file_prefix,
        quality_status_override.or(quality_status.as_deref()),
        model.as_deref(),
        resolved_model.as_deref(),
        engine_kind.as_deref(),
    )
}

pub(super) async fn handle_untrusted_research_completion(
    state: &AppState,
    task_id: i64,
    normalized_output: String,
    original_name: String,
    file_prefix: &str,
    file_type: &str,
    cleanup_files: Vec<String>,
    failure_message: &str,
) {
    let ext = if file_type == "html" { "html" } else { "md" };
    let new_filename = format!("{}-untrusted-ai.{}", Uuid::new_v4(), ext);
    let new_path = state.uploads_path.join(&new_filename);
    let final_title = strip_legacy_title_metadata(&original_name).0;
    if let Err(e) = fs::write(&new_path, normalized_output).await {
        let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
            .bind(e.to_string())
            .bind(task_id)
            .execute(&state.db)
            .await;
        send_task_update_from_progress(state, task_id, "failed", original_name).await;
        return;
    }

    let file_id = match sqlx::query(
        "INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, ?, 'draft')",
    )
    .bind(&new_filename)
    .bind(&final_title)
    .bind(file_type)
    .execute(&state.db)
    .await
    {
        Ok(result) => result.last_insert_rowid(),
        Err(e) => {
            let _ = fs::remove_file(&new_path).await;
            let _ =
                sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
                    .bind(e.to_string())
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
            send_task_update_from_progress(state, task_id, "failed", original_name).await;
            return;
        }
    };
    let tag_labels = task_system_tag_labels(state, task_id, file_prefix, Some("untrusted")).await;
    if assign_file_tag_labels(&state.db, file_id, &tag_labels, "task")
        .await
        .is_err()
    {
        let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
            .bind(&new_filename)
            .execute(&state.db)
            .await;
        let _ = fs::remove_file(&new_path).await;
        let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
            .bind("Failed to assign research tags")
            .bind(task_id)
            .execute(&state.db)
            .await;
        send_task_update_from_progress(state, task_id, "failed", original_name).await;
        return;
    }

    let rows_affected = sqlx::query(
        "UPDATE tasks SET status = 'completed', error_message = NULL, quality_status = 'untrusted', quality_last_failure = ?, file_id = ?, filename = ? WHERE id = ?",
    )
    .bind(failure_message)
    .bind(file_id)
    .bind(&new_filename)
    .bind(task_id)
    .execute(&state.db)
    .await
    .map(|result| result.rows_affected())
    .unwrap_or(0);
    if rows_affected == 0 {
        let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
            .bind(&new_filename)
            .execute(&state.db)
            .await;
        let _ = fs::remove_file(&new_path).await;
        return;
    }

    create_document_links_best_effort(state, task_id, file_id).await;
    send_task_update_from_progress(state, task_id, "completed", original_name).await;
    for cf in cleanup_files {
        let cp = state.uploads_path.join(cf);
        let _ = fs::remove_file(cp).await;
    }
}

pub(super) async fn handle_task_completion(
    state: &AppState,
    task_id: i64,
    result_text: Option<String>,
    original_name: String,
    file_prefix: &str,
    file_type: &str,
    cleanup_files: Vec<String>,
    validate_research: bool,
) {
    if let Some(t) = result_text {
        let mut normalized_output = normalize_ai_output(&t, file_type);
        if normalized_output.trim().len() < 10 {
            let _ =
                sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
                    .bind("AI output too short or empty")
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
            let _ = state.tx.send(TaskUpdateEvent {
                id: task_id,
                status: "failed".to_string(),
                original_name: original_name.clone(),
                quality_current_iteration: None,
                quality_max_iterations: None,
                quality_status: None,
                research_controller_stage: None,
                research_controller_iteration: None,
                research_controller_max_iterations: None,
            });
            return;
        }
        if validate_research {
            match validate_task_research_output(
                state,
                task_id,
                &normalized_output,
                file_prefix,
                file_type,
                None,
            )
            .await
            {
                Ok(validated_output) => normalized_output = validated_output,
                Err(failure) => {
                    let _ = sqlx::query(
                        "UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?",
                    )
                    .bind(failure.message)
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
                    let _ = state.tx.send(TaskUpdateEvent {
                        id: task_id,
                        status: "failed".to_string(),
                        original_name: original_name.clone(),
                        quality_current_iteration: None,
                        quality_max_iterations: None,
                        quality_status: None,
                        research_controller_stage: None,
                        research_controller_iteration: None,
                        research_controller_max_iterations: None,
                    });
                    return;
                }
            }
        }
        let ext = if file_type == "html" { "html" } else { "md" };
        let new_filename = format!("{}-ai.{}", Uuid::new_v4(), ext);
        let new_path = state.uploads_path.join(&new_filename);
        let final_title = strip_legacy_title_metadata(&original_name).0;
        if let Err(e) = fs::write(&new_path, normalized_output).await {
            let _ =
                sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
                    .bind(e.to_string())
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
            let _ = state.tx.send(TaskUpdateEvent {
                id: task_id,
                status: "failed".to_string(),
                original_name: original_name.clone(),
                quality_current_iteration: None,
                quality_max_iterations: None,
                quality_status: None,
                research_controller_stage: None,
                research_controller_iteration: None,
                research_controller_max_iterations: None,
            });
            return;
        }

        let file_id = match sqlx::query("INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, ?, 'draft')").bind(&new_filename).bind(&final_title).bind(file_type).execute(&state.db).await {
            Ok(result) => result.last_insert_rowid(),
            Err(e) => {
            let _ = fs::remove_file(&new_path).await;
            let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?").bind(e.to_string()).bind(task_id).execute(&state.db).await;
            let _ = state.tx.send(TaskUpdateEvent {
                id: task_id,
                status: "failed".to_string(),
                original_name: original_name.clone(),
                quality_current_iteration: None,
                quality_max_iterations: None,
                quality_status: None,
                research_controller_stage: None,
                research_controller_iteration: None,
                research_controller_max_iterations: None,
            });
            return;
            }
        };
        let tag_labels = task_system_tag_labels(state, task_id, file_prefix, None).await;
        if assign_file_tag_labels(&state.db, file_id, &tag_labels, "task")
            .await
            .is_err()
        {
            let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
                .bind(&new_filename)
                .execute(&state.db)
                .await;
            let _ = fs::remove_file(&new_path).await;
            let _ =
                sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
                    .bind("Failed to assign task tags")
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
            let _ = state.tx.send(TaskUpdateEvent {
                id: task_id,
                status: "failed".to_string(),
                original_name: original_name.clone(),
                quality_current_iteration: None,
                quality_max_iterations: None,
                quality_status: None,
                research_controller_stage: None,
                research_controller_iteration: None,
                research_controller_max_iterations: None,
            });
            return;
        }

        let completed_rows = match sqlx::query(
            "UPDATE tasks SET status = 'completed', file_id = ?, filename = ? WHERE id = ?",
        )
        .bind(file_id)
        .bind(&new_filename)
        .bind(task_id)
        .execute(&state.db)
        .await
        {
            Ok(result) => result.rows_affected(),
            Err(e) => {
                let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
                    .bind(&new_filename)
                    .execute(&state.db)
                    .await;
                let _ = fs::remove_file(&new_path).await;
                let _ = sqlx::query(
                    "UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?",
                )
                .bind(e.to_string())
                .bind(task_id)
                .execute(&state.db)
                .await;
                let _ = state.tx.send(TaskUpdateEvent {
                    id: task_id,
                    status: "failed".to_string(),
                    original_name: original_name.clone(),
                    quality_current_iteration: None,
                    quality_max_iterations: None,
                    quality_status: None,
                    research_controller_stage: None,
                    research_controller_iteration: None,
                    research_controller_max_iterations: None,
                });
                return;
            }
        };
        if completed_rows == 0 {
            let _ = sqlx::query("DELETE FROM files WHERE filename = ?")
                .bind(&new_filename)
                .execute(&state.db)
                .await;
            let _ = fs::remove_file(&new_path).await;
            return;
        }

        create_document_links_best_effort(state, task_id, file_id).await;
        let quality_progress = sqlx::query_as::<
            _,
            (
                Option<i64>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<i64>,
            ),
        >(
            "SELECT quality_current_iteration, quality_max_iterations, quality_status, research_controller_stage, research_controller_iteration, research_controller_max_iterations FROM tasks WHERE id = ?",
            )
            .bind(task_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

        let _ = state.tx.send(TaskUpdateEvent {
            id: task_id,
            status: "completed".to_string(),
            original_name: original_name.clone(),
            quality_current_iteration: quality_progress
                .as_ref()
                .and_then(|(current, _, _, _, _, _)| *current),
            quality_max_iterations: quality_progress
                .as_ref()
                .and_then(|(_, max, _, _, _, _)| *max),
            quality_status: quality_progress
                .as_ref()
                .and_then(|(_, _, status, _, _, _)| status.clone()),
            research_controller_stage: quality_progress
                .as_ref()
                .and_then(|(_, _, _, stage, _, _)| stage.clone()),
            research_controller_iteration: quality_progress
                .as_ref()
                .and_then(|(_, _, _, _, iteration, _)| *iteration),
            research_controller_max_iterations: quality_progress
                .as_ref()
                .and_then(|(_, _, _, _, _, max)| *max),
        });

        // Success! Cleanup original files if requested
        for cf in cleanup_files {
            let cp = state.uploads_path.join(cf);
            let _ = fs::remove_file(cp).await;
        }
    } else {
        if let Ok(Some((status, error_message))) = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, error_message FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_optional(&state.db)
        .await
        {
            if status == "failed"
                && error_message
                    .as_deref()
                    .is_some_and(|e| !e.trim().is_empty())
            {
                let _ = state.tx.send(TaskUpdateEvent {
                    id: task_id,
                    status: "failed".to_string(),
                    original_name: original_name.clone(),
                    quality_current_iteration: None,
                    quality_max_iterations: None,
                    quality_status: None,
                    research_controller_stage: None,
                    research_controller_iteration: None,
                    research_controller_max_iterations: None,
                });
                return;
            }
        }
        let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
            .bind("AI task failed without output")
            .bind(task_id)
            .execute(&state.db)
            .await;
        let _ = state.tx.send(TaskUpdateEvent {
            id: task_id,
            status: "failed".to_string(),
            original_name: original_name.clone(),
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
        });
        // Optional: Should we cleanup even on failure?
        // If the original was never in the DB, it's a leak.
        // For now, let's only cleanup on success to allow manual recovery if needed.
    }
}
