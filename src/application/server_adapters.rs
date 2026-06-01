use crate::application::engine_presets::executable_exists;
use crate::application::engine_presets::{
    default_engine_preset_by_id, default_engine_preset_to_engine_preset, default_engine_presets,
    resolve_engine_for_research, test_engine_preset_status, SqliteEnginePresetRepository,
};
use crate::application::scraping::{
    append_reference_once, extract_geeknews_original_url, guarded_get, is_geeknews_url,
    normalize_multiline_linked_images, normalize_reference_links, read_limited_response_text,
    MAX_SCRAPE_BODY_BYTES,
};
use crate::application::tasks::{
    is_delete_cancellable_task_status, is_retryable_task, merge_retry_engine_metadata,
    retry_task_metadata, run_ai_task, DELETE_CANCELLABLE_TASK_STATUS_SQL_LIST,
    TASK_CANCELLED_MESSAGE,
};
use crate::contracts::{
    EnginePreset as LegacyEnginePreset, TaskInfo as LegacyTaskInfo, TaskMetadata,
    TaskUpdateEvent as LegacyTaskUpdateEvent,
};
use crate::state::AppState;
use futures::future::BoxFuture;
use liquid_files::{
    markdown_preview_text, DocumentGraphNode, DocumentRelationshipGraph, DocumentRelationships,
    FileListItem, FileMetadata, TagInfo, TagStore,
};
use liquid_protocol::{
    AppConfig, DrawerAssignment, DrawerInfo, DrawerPayload, EnginePreset,
    EnginePresetCreatePayload, EnginePresetUpdatePayload, ModelOption, MultiResearchRequest,
    RetryTaskPayload, ScrapRequest, ScrapeTaskInput, TaskInfo, TaskUpdateEvent,
    TopicResearchRequest,
};
use liquid_research_core::normalize_absolute_public_evidence_url;
use liquid_runtime::engine_presets::has_reserved_model_prefix;
use liquid_server::context::{
    ConfigViewService, DrawerService, EnginePresetService, FileService, LoadedFileContent,
    ModelSelection, ResearchRequestPayload, ResearchSubmissionService, ScrapeSubmissionError,
    ScrapeSubmissionService, ServerContext, ServerServiceError, StaticAssets, TaskService,
    TranslationSubmissionService,
};
use liquid_storage_sqlite::{
    normalize_tag_slug, NewEnginePresetRecord, SqliteDocumentLinkRepository, SqliteFileRepository,
    SqliteTagRepository, SqliteWorkspaceStore,
};
use liquid_workspace::{
    has_research_request as workspace_has_research_request, research_request_info_from_task,
    WorkspaceStore,
};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::broadcast;
use uuid::Uuid;

pub(crate) fn build_server_context(state: Arc<AppState>) -> Arc<ServerContext> {
    let (server_tx, _) = broadcast::channel(100);
    let mut state_rx = state.tx.subscribe();
    let forward_tx = server_tx.clone();
    tokio::spawn(async move {
        while let Ok(event) = state_rx.recv().await {
            let _ = forward_tx.send(protocol_task_update_event(&event));
        }
    });
    let adapter = Arc::new(RootServerAdapter { state, server_tx });
    Arc::new(ServerContext {
        files: adapter.clone(),
        tasks: adapter.clone(),
        research: adapter.clone(),
        scraping: adapter.clone(),
        translation: adapter.clone(),
        config: adapter.clone(),
        engine_presets: adapter.clone(),
        drawers: adapter,
        static_assets: StaticAssets {
            root_dir: "static".into(),
        },
    })
}

struct RootServerAdapter {
    state: Arc<AppState>,
    server_tx: broadcast::Sender<TaskUpdateEvent>,
}

fn protocol_task_update_event(event: &LegacyTaskUpdateEvent) -> TaskUpdateEvent {
    TaskUpdateEvent {
        id: event.id,
        status: event.status.clone(),
        original_name: event.original_name.clone(),
        quality_current_iteration: event.quality_current_iteration,
        quality_max_iterations: event.quality_max_iterations,
        quality_status: event.quality_status.clone(),
        research_controller_stage: event.research_controller_stage.clone(),
        research_controller_iteration: event.research_controller_iteration,
        research_controller_max_iterations: event.research_controller_max_iterations,
    }
}

fn protocol_engine_preset(preset: LegacyEnginePreset) -> EnginePreset {
    EnginePreset {
        id: preset.id,
        name: preset.name,
        engine_kind: preset.engine_kind,
        provider: preset.provider,
        model: preset.model,
        command: preset.command,
        args_json: preset.args_json,
        base_url: preset.base_url,
        runtime_profile: preset.runtime_profile,
        default_intensity: preset.default_intensity,
        web_search_enabled: preset.web_search_enabled,
        fallback_execution: preset.fallback_execution,
        enabled: preset.enabled,
        is_default: preset.is_default,
        install_hint: preset.install_hint,
        limits_json: preset.limits_json,
        allowed_tools_json: preset.allowed_tools_json,
        allowed_skills_json: preset.allowed_skills_json,
        last_test_status: preset.last_test_status,
        last_test_at: preset.last_test_at,
        last_test_message: preset.last_test_message,
        created_at: preset.created_at,
        updated_at: preset.updated_at,
    }
}

impl RootServerAdapter {
    async fn hydrate_file_tags(&self, items: &mut [FileListItem]) -> Result<(), sqlx::Error> {
        TagStore::hydrate_file_tags(&SqliteTagRepository::new(&self.state.db), items).await
    }

    async fn has_research_request(&self, file: &FileMetadata) -> bool {
        workspace_has_research_request(&SqliteWorkspaceStore::new(&self.state.db), file)
            .await
            .unwrap_or(false)
    }

    async fn build_content_preview(&self, file: &FileMetadata) -> Option<String> {
        if file.file_type != "md" {
            return None;
        }
        let content = fs::read_to_string(self.state.uploads_path.join(&file.filename))
            .await
            .ok()?;
        let preview = markdown_preview_text(&content, 180);
        (!preview.is_empty()).then_some(preview)
    }

    async fn list_files_with_metadata(
        &self,
        files: Vec<FileMetadata>,
    ) -> Result<Vec<FileListItem>, sqlx::Error> {
        let mut items = Vec::with_capacity(files.len());
        for metadata in files {
            let content_preview = self.build_content_preview(&metadata).await;
            let has_research_request = self.has_research_request(&metadata).await;
            items.push(FileListItem {
                metadata,
                content_preview,
                has_research_request,
                tags: Vec::new(),
            });
        }
        self.hydrate_file_tags(&mut items).await?;
        Ok(items)
    }
}

#[derive(Debug)]
enum TagUpdateError {
    InvalidInput,
    Storage,
}

fn user_tag_labels(raw_tags: Vec<String>) -> Result<Vec<String>, TagUpdateError> {
    let mut seen = HashSet::new();
    let mut labels = Vec::new();
    for label in raw_tags {
        let label = label.trim();
        if label.is_empty() {
            continue;
        }
        let Some(slug) = normalize_tag_slug(label) else {
            continue;
        };
        if seen.insert(slug) {
            labels.push(label.to_string());
        }
    }

    if labels.len() > 24 || labels.iter().any(|label| label.chars().count() > 48) {
        return Err(TagUpdateError::InvalidInput);
    }

    Ok(labels)
}

async fn user_tag_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    labels: Vec<String>,
) -> Result<Vec<i64>, TagUpdateError> {
    let mut tag_ids = Vec::with_capacity(labels.len());
    for label in labels {
        let tag_id = ensure_user_tag_id(tx, &label)
            .await
            .map_err(|_| TagUpdateError::Storage)?;
        tag_ids.push(tag_id);
    }
    Ok(tag_ids)
}

async fn ensure_user_tag_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    label: &str,
) -> Result<i64, sqlx::Error> {
    let Some(slug) = normalize_tag_slug(label) else {
        return Err(sqlx::Error::Protocol("empty tag slug".to_string()));
    };

    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM tags WHERE slug = ? AND kind = 'user'")
            .bind(&slug)
            .fetch_optional(&mut **tx)
            .await?
    {
        return Ok(id);
    }

    let existing_kind = sqlx::query_scalar::<_, String>("SELECT kind FROM tags WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(&mut **tx)
        .await?;
    let insert_slug = if existing_kind
        .as_deref()
        .is_some_and(|existing| existing != "user")
    {
        format!("user-{slug}")
    } else {
        slug
    };

    sqlx::query("INSERT OR IGNORE INTO tags (label, slug, kind) VALUES (?, ?, 'user')")
        .bind(label)
        .bind(&insert_slug)
        .execute(&mut **tx)
        .await?;

    sqlx::query_scalar::<_, i64>("SELECT id FROM tags WHERE slug = ? AND kind = 'user'")
        .bind(insert_slug)
        .fetch_one(&mut **tx)
        .await
}

async fn replace_user_tag_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    file_id: i64,
    tag_ids: &[i64],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM file_tags
         WHERE file_id = ?
           AND source = 'user'
           AND tag_id IN (SELECT id FROM tags WHERE kind = 'user')",
    )
    .bind(file_id)
    .execute(&mut **tx)
    .await?;

    for tag_id in tag_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, 'user')",
        )
        .bind(file_id)
        .bind(tag_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

impl FileService for RootServerAdapter {
    fn list_files(&self) -> BoxFuture<'_, Result<Vec<FileListItem>, ServerServiceError>> {
        Box::pin(async move {
            let files = SqliteFileRepository::new(&self.state.db)
                .list_files()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            self.list_files_with_metadata(files)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn upload_file(
        &self,
        original_name: String,
        file_type: String,
        data: Vec<u8>,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let unique_filename = format!("{}-{}", Uuid::new_v4(), original_name);
            let path = self.state.uploads_path.join(&unique_filename);
            fs::write(&path, data)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            SqliteFileRepository::new(&self.state.db)
                .create_file(&unique_filename, &original_name, &file_type, "draft")
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            Ok(())
        })
    }

    fn push_content(
        &self,
        title: String,
        content: String,
        status: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let unique_filename = format!("{}-pushed.md", Uuid::new_v4());
            let path = self.state.uploads_path.join(&unique_filename);
            fs::write(&path, content)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            match SqliteFileRepository::new(&self.state.db)
                .create_file(&unique_filename, &title, "md", &status)
                .await
            {
                Ok(_) => Ok(()),
                Err(_) => {
                    let _ = fs::remove_file(&path).await;
                    Err(ServerServiceError::Storage)
                }
            }
        })
    }

    fn update_status(
        &self,
        filename: String,
        status: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let rows = SqliteFileRepository::new(&self.state.db)
                .update_status_by_filename(&filename, &status, status != "published")
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            if rows > 0 {
                Ok(())
            } else {
                Err(ServerServiceError::NotFound)
            }
        })
    }

    fn update_title(
        &self,
        filename: String,
        title: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            SqliteFileRepository::new(&self.state.db)
                .update_title_by_filename(&filename, &title)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            Ok(())
        })
    }

    fn update_file_metadata(
        &self,
        filename: String,
        title: String,
        tags: Vec<String>,
    ) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>> {
        Box::pin(async move {
            let files_repo = SqliteFileRepository::new(&self.state.db);
            let file_id = match files_repo.find_file_id_by_filename(&filename).await {
                Ok(Some(file_id)) => file_id,
                Ok(None) => return Err(ServerServiceError::NotFound),
                Err(_) => return Err(ServerServiceError::Storage),
            };
            let labels = user_tag_labels(tags).map_err(|error| match error {
                TagUpdateError::InvalidInput => ServerServiceError::BadRequest,
                TagUpdateError::Storage => ServerServiceError::Storage,
            })?;

            let mut tx = self
                .state
                .db
                .begin()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            match sqlx::query("UPDATE files SET original_name = ? WHERE id = ?")
                .bind(title)
                .bind(file_id)
                .execute(&mut *tx)
                .await
            {
                Ok(result) if result.rows_affected() > 0 => {}
                Ok(_) => return Err(ServerServiceError::NotFound),
                Err(_) => return Err(ServerServiceError::Storage),
            }

            let tag_ids = user_tag_ids(&mut tx, labels)
                .await
                .map_err(|error| match error {
                    TagUpdateError::InvalidInput => ServerServiceError::BadRequest,
                    TagUpdateError::Storage => ServerServiceError::Storage,
                })?;

            replace_user_tag_ids(&mut tx, file_id, &tag_ids)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            tx.commit().await.map_err(|_| ServerServiceError::Storage)?;

            SqliteTagRepository::new(&self.state.db)
                .load_file_tags(file_id)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn list_tags(&self) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>> {
        Box::pin(async move {
            SqliteTagRepository::new(&self.state.db)
                .list_tags()
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn update_tags(
        &self,
        filename: String,
        tags: Vec<String>,
    ) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>> {
        Box::pin(async move {
            let files_repo = SqliteFileRepository::new(&self.state.db);
            let file_id = match files_repo.find_file_id_by_filename(&filename).await {
                Ok(Some(file_id)) => file_id,
                Ok(None) => return Err(ServerServiceError::NotFound),
                Err(_) => return Err(ServerServiceError::Storage),
            };
            let labels = user_tag_labels(tags).map_err(|error| match error {
                TagUpdateError::InvalidInput => ServerServiceError::BadRequest,
                TagUpdateError::Storage => ServerServiceError::Storage,
            })?;

            let mut tx = self
                .state
                .db
                .begin()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            let tag_ids = user_tag_ids(&mut tx, labels)
                .await
                .map_err(|error| match error {
                    TagUpdateError::InvalidInput => ServerServiceError::BadRequest,
                    TagUpdateError::Storage => ServerServiceError::Storage,
                })?;
            replace_user_tag_ids(&mut tx, file_id, &tag_ids)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            tx.commit().await.map_err(|_| ServerServiceError::Storage)?;

            SqliteTagRepository::new(&self.state.db)
                .load_file_tags(file_id)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn delete_file(&self, filename: String) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let rows = SqliteFileRepository::new(&self.state.db)
                .delete_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            if rows == 0 {
                return Err(ServerServiceError::NotFound);
            }
            let path = self.state.uploads_path.join(&filename);
            if path.exists() {
                let _ = fs::remove_file(path).await;
            }
            Ok(())
        })
    }

    fn load_renderable_content(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<LoadedFileContent, ServerServiceError>> {
        Box::pin(async move {
            let file = SqliteFileRepository::new(&self.state.db)
                .load_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            let path = self.state.uploads_path.join(&file.filename);
            let content = fs::read_to_string(path)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            let has_research_request = self.has_research_request(&file).await;
            let file_type = file.file_type;
            Ok(LoadedFileContent {
                content: if file_type == "md" {
                    normalize_multiline_linked_images(&content)
                } else {
                    content
                },
                file_type,
                has_research_request,
            })
        })
    }

    fn load_raw_content(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<String, ServerServiceError>> {
        Box::pin(async move {
            let file = SqliteFileRepository::new(&self.state.db)
                .load_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            fs::read_to_string(self.state.uploads_path.join(&file.filename))
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn load_research_request(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<ResearchRequestPayload, ServerServiceError>> {
        Box::pin(async move {
            let files_repo = SqliteFileRepository::new(&self.state.db);
            let file = files_repo
                .load_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            let workspace = SqliteWorkspaceStore::new(&self.state.db);
            let task = workspace
                .load_latest_research_request_task(file.id, &file.filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            let research_controller_artifacts_json =
                task.research_controller_artifacts_json.clone();
            let research_source_diagnostics_json = task.research_source_diagnostics_json.clone();
            let info = research_request_info_from_task(&workspace, file.id, task, None, None)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            Ok(ResearchRequestPayload {
                info,
                research_controller_artifacts_json,
                research_source_diagnostics_json,
            })
        })
    }

    fn load_file_relationships(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<DocumentRelationships, ServerServiceError>> {
        Box::pin(async move {
            let file_id = SqliteFileRepository::new(&self.state.db)
                .find_file_id_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            SqliteDocumentLinkRepository::new(&self.state.db)
                .load_relationships(file_id)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn load_relationship_graph(
        &self,
        filename: String,
        direction: String,
    ) -> BoxFuture<'_, Result<DocumentRelationshipGraph, ServerServiceError>> {
        Box::pin(async move {
            let root = SqliteFileRepository::new(&self.state.db)
                .load_source_document_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .map(|file| DocumentGraphNode {
                    id: file.id,
                    filename: file.filename,
                    title: file.title,
                })
                .ok_or(ServerServiceError::NotFound)?;
            SqliteDocumentLinkRepository::new(&self.state.db)
                .load_relationship_graph(root, &direction)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn search_files(
        &self,
        query: String,
    ) -> BoxFuture<'_, Result<Vec<FileListItem>, ServerServiceError>> {
        Box::pin(async move {
            let files = SqliteFileRepository::new(&self.state.db)
                .list_files()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            let mut matches = Vec::new();
            let q = query.to_lowercase();
            for file in files {
                if file.original_name.to_lowercase().contains(&q) {
                    matches.push(file);
                    continue;
                }

                let tag_match = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*)
                     FROM file_tags
                     JOIN tags ON tags.id = file_tags.tag_id
                     WHERE file_tags.file_id = ? AND lower(tags.label) LIKE ?",
                )
                .bind(file.id)
                .bind(format!("%{q}%"))
                .fetch_one(&self.state.db)
                .await
                .map(|count| count > 0)
                .unwrap_or(false);
                if tag_match {
                    matches.push(file);
                    continue;
                }

                let path = self.state.uploads_path.join(&file.filename);
                if let Ok(content) = fs::read_to_string(path).await {
                    if content.to_lowercase().contains(&q) {
                        matches.push(file);
                    }
                }
            }
            self.list_files_with_metadata(matches)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn load_translation_target(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<ModelSelection, ServerServiceError>> {
        Box::pin(async move {
            let file = SqliteFileRepository::new(&self.state.db)
                .load_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            Ok(ModelSelection {
                original_name: file.original_name,
                file_type: file.file_type,
            })
        })
    }
}

impl TaskService for RootServerAdapter {
    fn list_tasks(&self) -> BoxFuture<'_, Result<Vec<TaskInfo>, ServerServiceError>> {
        Box::pin(async move {
            sqlx::query_as::<_, TaskInfo>(
                "SELECT * FROM tasks WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 30",
            )
            .fetch_all(&self.state.db)
            .await
            .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn subscribe_updates(&self) -> broadcast::Receiver<TaskUpdateEvent> {
        self.server_tx.subscribe()
    }

    fn retry_task(
        &self,
        id: i64,
        payload: RetryTaskPayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let task = sqlx::query_as::<_, LegacyTaskInfo>(
                "SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(id)
            .fetch_optional(&self.state.db)
            .await
            .ok()
            .flatten()
            .ok_or(ServerServiceError::NotFound)?;

            if !is_retryable_task(&task) {
                return Err(ServerServiceError::BadRequest);
            }

            let mut model = task.model.clone().unwrap_or_default();
            let system_prompt = task.system_prompt.clone().unwrap_or_default();
            let user_prompt = task.user_prompt.clone().unwrap_or_default();
            let filenames: Vec<String> = serde_json::from_str(
                &task
                    .source_filenames
                    .clone()
                    .unwrap_or_else(|| "[]".to_string()),
            )
            .unwrap_or_default();
            let file_prefix = task.file_prefix.clone().unwrap_or_default();
            let file_type = task.file_type.clone().unwrap_or_default();
            let cleanup_files: Vec<String> = serde_json::from_str(
                &task
                    .cleanup_files
                    .clone()
                    .unwrap_or_else(|| "[]".to_string()),
            )
            .unwrap_or_default();
            let mut task_metadata = retry_task_metadata(&task);
            if payload.engine_preset_id.is_some() {
                let resolved = resolve_engine_for_research(
                    &self.state.db,
                    payload.engine_preset_id,
                    None,
                    payload.research_intensity.clone(),
                )
                .await
                .map_err(|error| match error {
                    crate::application::engine_presets::EngineResolutionError::NotFound => {
                        ServerServiceError::NotFound
                    }
                    crate::application::engine_presets::EngineResolutionError::InvalidRequest => {
                        ServerServiceError::BadRequest
                    }
                    crate::application::engine_presets::EngineResolutionError::Storage => {
                        ServerServiceError::Storage
                    }
                })?;
                model = resolved.model_input;
                merge_retry_engine_metadata(&mut task_metadata, resolved.metadata);
            } else if let Some(intensity) = payload.research_intensity {
                if !liquid_protocol::is_valid_research_intensity(&intensity) {
                    return Err(ServerServiceError::BadRequest);
                }
                task_metadata.research_intensity = Some(intensity);
            }

            if let Some(iterations) = payload.research_quality_max_iterations {
                task_metadata.quality_max_iterations = Some(iterations);
            }
            if let Some(depth) = payload.research_quality_depth {
                if !liquid_protocol::is_valid_research_quality_depth(&depth) {
                    return Err(ServerServiceError::BadRequest);
                }
                task_metadata.quality_depth = Some(depth);
            }

            if model.is_empty() && file_prefix != "[Scrape]" {
                return Err(ServerServiceError::BadRequest);
            }
            let derive_task = payload.derive_task.unwrap_or(false);
            let queued_task_id = run_ai_task(
                Arc::clone(&self.state),
                vec![],
                filenames,
                task.original_name,
                model,
                system_prompt,
                user_prompt,
                &file_prefix,
                &file_type,
                cleanup_files,
                if derive_task { None } else { Some(task.id) },
                Some(task_metadata),
            )
            .await;
            if queued_task_id == 0 {
                Err(ServerServiceError::Storage)
            } else {
                Ok(())
            }
        })
    }

    fn delete_task(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let task = sqlx::query_as::<
                _,
                (String, String, Option<i64>, Option<i64>, Option<String>),
            >(
                "SELECT status, original_name, quality_current_iteration, quality_max_iterations, quality_status FROM tasks WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(id)
            .fetch_optional(&self.state.db)
            .await
            .map_err(|_| ServerServiceError::Storage)?;

            let Some((
                status,
                original_name,
                quality_current_iteration,
                quality_max_iterations,
                quality_status,
            )) = task
            else {
                return Ok(());
            };

            if is_delete_cancellable_task_status(&status) {
                let query = format!(
                    "UPDATE tasks SET status = 'interrupted', error_message = ? WHERE id = ? AND status IN ({DELETE_CANCELLABLE_TASK_STATUS_SQL_LIST})",
                );
                let rows_affected = sqlx::query(&query)
                    .bind(TASK_CANCELLED_MESSAGE)
                    .bind(id)
                    .execute(&self.state.db)
                    .await
                    .map(|result| result.rows_affected());

                match rows_affected {
                    Ok(rows) if rows > 0 => {
                        let _ = self.state.tx.send(LegacyTaskUpdateEvent {
                            id,
                            status: "interrupted".to_string(),
                            original_name,
                            quality_current_iteration,
                            quality_max_iterations,
                            quality_status,
                            research_controller_stage: None,
                            research_controller_iteration: None,
                            research_controller_max_iterations: None,
                        });
                        if let Some(handle) = self.state.active_tasks.lock().await.remove(&id) {
                            handle.abort();
                        }
                        self.state.queue_notify.notify_waiters();
                    }
                    Ok(_) => {}
                    Err(_) => return Err(ServerServiceError::Storage),
                }
                return Ok(());
            }

            sqlx::query("UPDATE tasks SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(id)
                .execute(&self.state.db)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            Ok(())
        })
    }
}

impl ResearchSubmissionService for RootServerAdapter {
    fn submit_multi_research(
        &self,
        payload: MultiResearchRequest,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let mut fnames = Vec::new();
            let mut onames = Vec::new();
            let mut file_ids = Vec::new();
            let files_repo = SqliteFileRepository::new(&self.state.db);
            for fname in &payload.filenames {
                if let Ok(Some(file)) = files_repo.load_file_by_filename(fname).await {
                    file_ids.push(file.id);
                    fnames.push(file.filename);
                    onames.push(file.original_name);
                }
            }
            if fnames.is_empty() {
                return Err(ServerServiceError::NotFound);
            }
            let format = payload.format.as_deref().unwrap_or("md");
            let research_mode = self
                .state
                .research_implementation
                .normalize_research_mode(&payload.mode);
            let default_research_type = if fnames.len() > 1 {
                "synthesis"
            } else {
                "deep"
            };
            let research_type = self
                .state
                .research_implementation
                .normalize_research_type(payload.research_type.as_deref(), default_research_type);
            let engine = resolve_engine_for_research(
                &self.state.db,
                payload.engine_preset_id,
                payload.model.clone(),
                payload.research_intensity.clone(),
            )
            .await
            .map_err(|error| match error {
                crate::application::engine_presets::EngineResolutionError::NotFound => {
                    ServerServiceError::NotFound
                }
                crate::application::engine_presets::EngineResolutionError::InvalidRequest => {
                    ServerServiceError::BadRequest
                }
                crate::application::engine_presets::EngineResolutionError::Storage => {
                    ServerServiceError::Storage
                }
            })?;
            let html_design_prompt =
                (format == "html").then(crate::research_design::build_html_design_prompt);
            let system = self
                .state
                .research_implementation
                .build_research_system_prompt(research_mode, format, html_design_prompt.as_deref());
            let user_prompt = if research_type == "follow_up" {
                self.state
                    .research_implementation
                    .build_follow_up_research_user_prompt(payload.instructions.as_deref())
            } else {
                self.state
                    .research_implementation
                    .build_document_research_user_prompt(payload.instructions.as_deref())
            };
            let title = if onames.len() > 1 {
                format!("Multi-Doc ({} docs)", onames.len())
            } else {
                onames[0].clone()
            };
            let metadata = TaskMetadata {
                source_file_ids: None,
                research_type: Some(research_type.to_string()),
                research_mode: Some(research_mode.to_string()),
                research_format: Some(format.to_string()),
                research_topic: None,
                research_instructions: payload.instructions.clone(),
                prompt_version: Some("research-controller-v19".to_string()),
                quality_max_iterations: payload.research_quality_max_iterations,
                quality_depth: payload.research_quality_depth.clone(),
                ..engine.metadata
            };
            run_ai_task(
                Arc::clone(&self.state),
                file_ids,
                fnames,
                title,
                engine.model_input,
                system,
                user_prompt,
                "[Research]",
                format,
                vec![],
                None,
                Some(metadata),
            )
            .await;
            Ok(())
        })
    }

    fn submit_topic_research(
        &self,
        payload: TopicResearchRequest,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let format = payload.format.as_deref().unwrap_or("md");
            let research_mode = self
                .state
                .research_implementation
                .normalize_research_mode(&payload.mode);
            let research_type = self
                .state
                .research_implementation
                .normalize_research_type(payload.research_type.as_deref(), "initial");
            let engine = resolve_engine_for_research(
                &self.state.db,
                payload.engine_preset_id,
                payload.model.clone(),
                payload.research_intensity.clone(),
            )
            .await
            .map_err(|error| match error {
                crate::application::engine_presets::EngineResolutionError::NotFound => {
                    ServerServiceError::NotFound
                }
                crate::application::engine_presets::EngineResolutionError::InvalidRequest => {
                    ServerServiceError::BadRequest
                }
                crate::application::engine_presets::EngineResolutionError::Storage => {
                    ServerServiceError::Storage
                }
            })?;
            let html_design_prompt =
                (format == "html").then(crate::research_design::build_html_design_prompt);
            let system = self
                .state
                .research_implementation
                .build_research_system_prompt(research_mode, format, html_design_prompt.as_deref());
            let user_prompt = self
                .state
                .research_implementation
                .build_topic_research_user_prompt(&payload.topic, payload.instructions.as_deref());
            let metadata = TaskMetadata {
                source_file_ids: None,
                research_type: Some(research_type.to_string()),
                research_mode: Some(research_mode.to_string()),
                research_format: Some(format.to_string()),
                research_topic: Some(payload.topic.clone()),
                research_instructions: payload.instructions.clone(),
                prompt_version: Some("research-controller-v19".to_string()),
                quality_max_iterations: payload.research_quality_max_iterations,
                quality_depth: payload.research_quality_depth.clone(),
                ..engine.metadata
            };
            run_ai_task(
                Arc::clone(&self.state),
                vec![],
                vec![],
                payload.topic,
                engine.model_input,
                system,
                user_prompt,
                "[AI-Research]",
                format,
                vec![],
                None,
                Some(metadata),
            )
            .await;
            Ok(())
        })
    }
}

impl ScrapeSubmissionService for RootServerAdapter {
    fn submit_scrape(
        &self,
        payload: ScrapRequest,
    ) -> BoxFuture<'_, Result<i64, ScrapeSubmissionError>> {
        Box::pin(async move {
            let Some(mut url_str) = normalize_absolute_public_evidence_url(&payload.url) else {
                return Err(ScrapeSubmissionError::BadRequest("Invalid URL"));
            };
            let mode = payload.mode.unwrap_or_else(|| "general".to_string());
            if !liquid_protocol::is_valid_scrape_mode(&mode) {
                return Err(ScrapeSubmissionError::BadRequest("Invalid scrape mode"));
            }

            let translate = payload.translate.unwrap_or(false);
            let model = payload.model.unwrap_or_default();
            if translate && model.is_empty() {
                return Err(ScrapeSubmissionError::BadRequest(
                    "Model is required for scrape+translate",
                ));
            }

            let mut references = normalize_reference_links(payload.references.unwrap_or_default())
                .into_iter()
                .filter_map(|reference| normalize_absolute_public_evidence_url(&reference))
                .collect::<Vec<_>>();
            if mode == "geeknews" {
                if !is_geeknews_url(&url_str) {
                    return Err(ScrapeSubmissionError::BadRequest(
                        "GeekNews URL is required",
                    ));
                }
                let geeknews_url = url_str.clone();
                let geeknews_url_parsed = url::Url::parse(&geeknews_url)
                    .map_err(|_| ScrapeSubmissionError::BadRequest("Invalid URL"))?;
                let geeknews_response = guarded_get(&geeknews_url_parsed)
                    .await
                    .map_err(|_| ScrapeSubmissionError::BadRequest("Failed to load GeekNews URL"))?
                    .error_for_status()
                    .map_err(|_| {
                        ScrapeSubmissionError::BadRequest("Failed to load GeekNews URL")
                    })?;
                let geeknews_page =
                    read_limited_response_text(geeknews_response, MAX_SCRAPE_BODY_BYTES)
                        .await
                        .map_err(|_| {
                            ScrapeSubmissionError::BadRequest("Failed to load GeekNews URL")
                        })?;
                let Some(original_url) =
                    extract_geeknews_original_url(&geeknews_page, &geeknews_url)
                else {
                    return Err(ScrapeSubmissionError::BadRequest("Original URL not found"));
                };
                append_reference_once(&mut references, geeknews_url);
                let Some(normalized_original_url) =
                    normalize_absolute_public_evidence_url(&original_url)
                else {
                    return Err(ScrapeSubmissionError::BadRequest("Original URL not found"));
                };
                url_str = normalized_original_url;
            }

            let input = ScrapeTaskInput {
                url: url_str.clone(),
                references,
                mode: Some(mode),
            };
            let scrape_input_json =
                serde_json::to_string(&input).map_err(|_| ScrapeSubmissionError::Storage)?;
            let file_prefix = if translate { "[Scrape+KO]" } else { "[Scrape]" };
            let result = sqlx::query("INSERT INTO tasks (original_name, status, model, user_prompt, file_prefix, file_type) VALUES (?, 'queued', ?, ?, ?, 'md')")
                .bind(&input.url)
                .bind(&model)
                .bind(&scrape_input_json)
                .bind(file_prefix)
                .execute(&self.state.db)
                .await
                .map_err(|_| ScrapeSubmissionError::Storage)?;
            let task_id = result.last_insert_rowid();
            let _ = self.state.tx.send(LegacyTaskUpdateEvent {
                id: task_id,
                status: "queued".to_string(),
                original_name: input.url,
                quality_current_iteration: None,
                quality_max_iterations: None,
                quality_status: None,
                research_controller_stage: None,
                research_controller_iteration: None,
                research_controller_max_iterations: None,
            });
            self.state.queue_notify.notify_waiters();
            Ok(task_id)
        })
    }
}

impl TranslationSubmissionService for RootServerAdapter {
    fn submit_translation(
        &self,
        filename: String,
        model: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let file = SqliteFileRepository::new(&self.state.db)
                .load_file_by_filename(&filename)
                .await
                .map_err(|_| ServerServiceError::Storage)?
                .ok_or(ServerServiceError::NotFound)?;
            let system = "You are a professional technical translator. PRESERVE all Markdown formatting. Output ONLY the translated Korean text.".to_string();
            run_ai_task(
                Arc::clone(&self.state),
                vec![file.id],
                vec![file.filename],
                file.original_name,
                model,
                system,
                "Translate the following content to Korean:".to_string(),
                "[KO]",
                "md",
                vec![],
                None,
                None,
            )
            .await;
            Ok(())
        })
    }
}

impl ConfigViewService for RootServerAdapter {
    fn app_config(&self) -> AppConfig {
        AppConfig {
            ai_workers: self.state.ai_workers,
            local_ai_workers: self.state.local_ai_workers,
            ai_task_timeout_secs: self.state.ai_task_timeout_secs,
            cli_launch_mode: self.state.cli_launch_mode.to_string(),
            cli_launcher_available: self.state.cli_launch_mode.can_launch_cli(),
        }
    }

    fn available_models(&self) -> BoxFuture<'_, Vec<ModelOption>> {
        Box::pin(async move {
            let mut models = Vec::new();
            let client = reqwest::Client::new();
            if let Ok(res) = client.get("http://localhost:11434/api/tags").send().await {
                if let Ok(data) = res.json::<serde_json::Value>().await {
                    if let Some(ollama_models) = data["models"].as_array() {
                        for m in ollama_models {
                            if let Some(name) = m["name"].as_str() {
                                models.push(ModelOption {
                                    name: name.to_string(),
                                    source: "ollama".to_string(),
                                });
                            }
                        }
                    }
                }
            }
            if self.state.cli_launch_mode.can_launch_cli() {
                for tool in ["claude", "gemini", "codex"] {
                    if executable_exists(tool) {
                        models.push(ModelOption {
                            name: tool.to_string(),
                            source: "cli".to_string(),
                        });
                    }
                }
            }
            models
        })
    }
}

impl EnginePresetService for RootServerAdapter {
    fn list_engine_presets(&self) -> BoxFuture<'_, Result<Vec<EnginePreset>, ServerServiceError>> {
        Box::pin(async move {
            let repo = SqliteEnginePresetRepository::new(&self.state.db);
            let mut presets: Vec<EnginePreset> = default_engine_presets()
                .into_iter()
                .map(default_engine_preset_to_engine_preset)
                .map(protocol_engine_preset)
                .collect();
            let user_presets = repo
                .list_custom_presets()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            presets.extend(user_presets.into_iter().map(protocol_engine_preset));
            Ok(presets)
        })
    }

    fn create_engine_preset(
        &self,
        payload: EnginePresetCreatePayload,
    ) -> BoxFuture<'_, Result<i64, ServerServiceError>> {
        Box::pin(async move {
            let repo = SqliteEnginePresetRepository::new(&self.state.db);
            let name = payload.name.trim();
            let Some(template) = default_engine_preset_by_id(payload.template_id) else {
                return Err(ServerServiceError::BadRequest);
            };
            if name.is_empty() {
                return Err(ServerServiceError::BadRequest);
            }
            let model = if template.engine_kind == "cli" {
                template.model.clone()
            } else {
                let model_override = payload
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                if model_override
                    .as_deref()
                    .is_some_and(has_reserved_model_prefix)
                {
                    return Err(ServerServiceError::BadRequest);
                }
                model_override.or_else(|| template.model.clone())
            };
            let record = NewEnginePresetRecord {
                name: name.to_string(),
                engine_kind: template.engine_kind.clone(),
                provider: template.provider.clone(),
                model,
                command: template.command.clone(),
                args_json: template.args_json.clone(),
                base_url: template.base_url.clone(),
                runtime_profile: template.runtime_profile.clone(),
                default_intensity: template.default_intensity.clone(),
                web_search_enabled: template.web_search_enabled.clone(),
                fallback_execution: template.fallback_execution.clone(),
                install_hint: Some(format!("Created from built-in preset: {}", template.name)),
                limits_json: template.limits_json.clone(),
                allowed_tools_json: template.allowed_tools_json.clone(),
                allowed_skills_json: template.allowed_skills_json.clone(),
            };
            repo.insert_custom_preset(&record)
                .await
                .map_err(|error| match error {
                    sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                        ServerServiceError::Conflict
                    }
                    _ => ServerServiceError::Storage,
                })
        })
    }

    fn update_engine_preset(
        &self,
        id: i64,
        payload: EnginePresetUpdatePayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let repo = SqliteEnginePresetRepository::new(&self.state.db);
            if id < 0 {
                return Err(ServerServiceError::MethodNotAllowed);
            }
            let name = payload.name.trim();
            if name.is_empty() {
                return Err(ServerServiceError::BadRequest);
            }
            let preset = match repo.load_custom_preset_by_id(id).await {
                Ok(Some(preset)) => preset,
                Ok(None) => return Err(ServerServiceError::NotFound),
                Err(_) => return Err(ServerServiceError::Storage),
            };
            let model = if preset.engine_kind == "cli" {
                preset.model
            } else {
                let model_override = payload
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                if model_override
                    .as_deref()
                    .is_some_and(has_reserved_model_prefix)
                {
                    return Err(ServerServiceError::BadRequest);
                }
                model_override.or(preset.model)
            };
            match repo
                .update_custom_preset_name_and_model(id, name, model.as_deref())
                .await
            {
                Ok(rows_affected) if rows_affected > 0 => Ok(()),
                Ok(_) => Err(ServerServiceError::NotFound),
                Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                    Err(ServerServiceError::Conflict)
                }
                Err(_) => Err(ServerServiceError::Storage),
            }
        })
    }

    fn delete_engine_preset(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let repo = SqliteEnginePresetRepository::new(&self.state.db);
            if id < 0 {
                return Err(ServerServiceError::MethodNotAllowed);
            }
            match repo.delete_custom_preset(id).await {
                Ok(rows_affected) if rows_affected > 0 => Ok(()),
                Ok(_) => Err(ServerServiceError::NotFound),
                Err(_) => Err(ServerServiceError::Storage),
            }
        })
    }

    fn test_engine_preset(
        &self,
        id: i64,
    ) -> BoxFuture<'_, Result<(String, String), ServerServiceError>> {
        Box::pin(async move {
            let repo = SqliteEnginePresetRepository::new(&self.state.db);
            let preset = if id < 0 {
                default_engine_preset_by_id(id).ok_or(ServerServiceError::NotFound)?
            } else {
                match repo.load_enabled_custom_preset_by_id(id).await {
                    Ok(Some(preset)) => preset,
                    Ok(None) => return Err(ServerServiceError::NotFound),
                    Err(_) => return Err(ServerServiceError::Storage),
                }
            };
            let (status, message) = test_engine_preset_status(
                &preset,
                &self.state.data_dir,
                self.state.cli_launch_mode,
            )
            .await;
            if id > 0 {
                match repo.record_test_result(id, &status, &message).await {
                    Ok(rows_affected) if rows_affected > 0 => {}
                    Ok(_) => return Err(ServerServiceError::NotFound),
                    Err(_) => return Err(ServerServiceError::Storage),
                }
            }
            Ok((status, message))
        })
    }
}

impl DrawerService for RootServerAdapter {
    fn list_drawers(&self) -> BoxFuture<'_, Result<Vec<DrawerInfo>, ServerServiceError>> {
        Box::pin(async move {
            let sql = r#"
                SELECT
                    drawers.id,
                    drawers.name,
                    drawers.description,
                    drawers.created_at,
                    COUNT(files.id) AS file_count
                FROM drawers
                LEFT JOIN files
                    ON files.drawer_id = drawers.id
                    AND files.status = 'published'
                GROUP BY drawers.id, drawers.name, drawers.description, drawers.created_at
                ORDER BY drawers.name COLLATE NOCASE ASC
            "#;
            sqlx::query_as::<_, DrawerInfo>(sql)
                .fetch_all(&self.state.db)
                .await
                .map_err(|_| ServerServiceError::Storage)
        })
    }

    fn create_drawer(
        &self,
        payload: DrawerPayload,
    ) -> BoxFuture<'_, Result<i64, ServerServiceError>> {
        Box::pin(async move {
            let name = payload.name.trim();
            if name.is_empty() {
                return Err(ServerServiceError::BadRequest);
            }
            sqlx::query("INSERT INTO drawers (name, description) VALUES (?, ?)")
                .bind(name)
                .bind(payload.description)
                .execute(&self.state.db)
                .await
                .map(|result| result.last_insert_rowid())
                .map_err(|error| match error {
                    sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                        ServerServiceError::Conflict
                    }
                    _ => ServerServiceError::Storage,
                })
        })
    }

    fn update_drawer(
        &self,
        id: i64,
        payload: DrawerPayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let name = payload.name.trim();
            if name.is_empty() {
                return Err(ServerServiceError::BadRequest);
            }
            match sqlx::query("UPDATE drawers SET name = ?, description = ? WHERE id = ?")
                .bind(name)
                .bind(payload.description)
                .bind(id)
                .execute(&self.state.db)
                .await
            {
                Ok(result) if result.rows_affected() > 0 => Ok(()),
                Ok(_) => Err(ServerServiceError::NotFound),
                Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                    Err(ServerServiceError::Conflict)
                }
                Err(_) => Err(ServerServiceError::Storage),
            }
        })
    }

    fn delete_drawer(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let mut tx = self
                .state
                .db
                .begin()
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            if sqlx::query("UPDATE files SET drawer_id = NULL WHERE drawer_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .is_err()
            {
                return Err(ServerServiceError::Storage);
            }
            match sqlx::query("DELETE FROM drawers WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
            {
                Ok(result) if result.rows_affected() > 0 => {
                    tx.commit().await.map_err(|_| ServerServiceError::Storage)?;
                    Ok(())
                }
                Ok(_) => Err(ServerServiceError::NotFound),
                Err(_) => Err(ServerServiceError::Storage),
            }
        })
    }

    fn update_file_drawer(
        &self,
        filename: String,
        payload: DrawerAssignment,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>> {
        Box::pin(async move {
            let file = match SqliteFileRepository::new(&self.state.db)
                .load_file_by_filename(&filename)
                .await
            {
                Ok(Some(file)) => file,
                Ok(None) => return Err(ServerServiceError::NotFound),
                Err(_) => return Err(ServerServiceError::Storage),
            };

            let Some(drawer_id) = payload.drawer_id else {
                sqlx::query("UPDATE files SET drawer_id = NULL WHERE filename = ?")
                    .bind(&filename)
                    .execute(&self.state.db)
                    .await
                    .map_err(|_| ServerServiceError::Storage)?;
                return Ok(());
            };

            if file.status != "published" {
                return Err(ServerServiceError::BadRequest);
            }
            let drawer_exists =
                match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM drawers WHERE id = ?")
                    .bind(drawer_id)
                    .fetch_one(&self.state.db)
                    .await
                {
                    Ok(count) => count > 0,
                    Err(_) => return Err(ServerServiceError::Storage),
                };
            if !drawer_exists {
                return Err(ServerServiceError::NotFound);
            }
            sqlx::query("UPDATE files SET drawer_id = ? WHERE filename = ?")
                .bind(drawer_id)
                .bind(&filename)
                .execute(&self.state.db)
                .await
                .map_err(|_| ServerServiceError::Storage)?;
            Ok(())
        })
    }
}
