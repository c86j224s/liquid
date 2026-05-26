use crate::db::{ensure_tag_id, normalize_tag_slug};
use crate::models::{
    friendly_scrape_failure_message, AppConfig, DocumentGraphEdge, DocumentGraphNode,
    DocumentLinkInfo, DocumentRelationshipGraph, DocumentRelationships, FileListItem, FileMetadata,
    FileMetadataUpdate, ModelOption, PushRequest, ResearchControllerArtifacts,
    ResearchControllerArtifactsSummary, ResearchRequestInfo, ResearchScrapeDiagnosticsSummary,
    ResearchSourceDiagnosticsEnvelope, ResearchSourceDiagnosticsSummary, ResearchSourceDocument,
    SearchQuery, StatusUpdate, TagInfo, TagUpdate, TaskInfo, TitleUpdate,
};
use crate::scraping::normalize_multiline_linked_images;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use pulldown_cmark::{html, Options, Parser as MarkdownParser};
use serde::Deserialize;
use sqlx::{sqlite::SqlitePool, Row};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::fs;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct RelationshipGraphQuery {
    depth: Option<u8>,
    direction: Option<String>,
}

pub(crate) fn valid_file_status(status: &str) -> bool {
    matches!(status, "draft" | "published" | "archived")
}

pub(crate) async fn list_files(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, FileMetadata>("SELECT * FROM files ORDER BY uploaded_at DESC")
        .fetch_all(&state.db)
        .await
    {
        Ok(files) => {
            let mut items = Vec::with_capacity(files.len());
            for metadata in files {
                let content_preview = build_content_preview(&state, &metadata).await;
                let has_research_request = has_research_request(&state, &metadata).await;
                items.push(FileListItem {
                    metadata,
                    content_preview,
                    has_research_request,
                    tags: Vec::new(),
                });
            }
            if hydrate_file_tags(&state.db, &mut items).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(items).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn hydrate_file_tags(db: &SqlitePool, items: &mut [FileListItem]) -> Result<(), sqlx::Error> {
    if items.is_empty() {
        return Ok(());
    }

    let placeholders = vec!["?"; items.len()].join(",");
    let sql = format!(
        "SELECT file_tags.file_id, tags.id, tags.label, tags.slug, tags.kind, file_tags.source
         FROM file_tags
         JOIN tags ON tags.id = file_tags.tag_id
         WHERE file_tags.file_id IN ({placeholders})
         ORDER BY tags.kind, tags.label"
    );
    let mut query = sqlx::query(&sql);
    for item in items.iter() {
        query = query.bind(item.metadata.id);
    }

    let rows = query.fetch_all(db).await?;
    let mut by_file: HashMap<i64, Vec<TagInfo>> = HashMap::new();
    for row in rows {
        let file_id: i64 = row.get("file_id");
        by_file.entry(file_id).or_default().push(TagInfo {
            id: row.get("id"),
            label: row.get("label"),
            slug: row.get("slug"),
            kind: row.get("kind"),
            source: row.get("source"),
        });
    }

    for item in items {
        item.tags = by_file.remove(&item.metadata.id).unwrap_or_default();
    }

    Ok(())
}

async fn has_research_request(state: &AppState, file: &FileMetadata) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasks
         WHERE (file_id = ? OR filename = ?)
           AND file_prefix IN ('[Research]', '[AI-Research]')",
    )
    .bind(file.id)
    .bind(&file.filename)
    .fetch_one(&state.db)
    .await
    .map(|count| count > 0)
    .unwrap_or(false)
}

async fn build_content_preview(state: &AppState, file: &FileMetadata) -> Option<String> {
    if file.file_type != "md" {
        return None;
    }
    let content = fs::read_to_string(state.uploads_path.join(&file.filename))
        .await
        .ok()?;
    let preview = markdown_preview_text(&content, 180);
    (!preview.is_empty()).then_some(preview)
}

fn markdown_preview_text(content: &str, limit: usize) -> String {
    let mut text = String::new();
    let mut in_fence = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches(['-', '*', '+', '>'])
            .trim();
        if cleaned.is_empty() {
            continue;
        }

        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(cleaned);
        if text.chars().count() >= limit {
            break;
        }
    }

    let mut clipped: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        clipped.push('…');
    }
    clipped
}

pub(crate) async fn upload_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let original_name = field.file_name().unwrap_or("unnamed").to_string();
            let file_type = if original_name.ends_with(".md") {
                "md"
            } else if original_name.ends_with(".html") {
                "html"
            } else {
                return (StatusCode::BAD_REQUEST, "Unsupported file type").into_response();
            };
            let data = match field.bytes().await {
                Ok(b) => b,
                Err(_) => return (StatusCode::BAD_REQUEST, "Failed to read file").into_response(),
            };
            let unique_filename = format!("{}-{}", Uuid::new_v4(), original_name);
            let path = state.uploads_path.join(&unique_filename);
            if fs::write(&path, data).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            if sqlx::query("INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, ?, 'draft')").bind(&unique_filename).bind(&original_name).bind(file_type).execute(&state.db).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            return StatusCode::CREATED.into_response();
        }
    }
    StatusCode::BAD_REQUEST.into_response()
}

pub(crate) async fn push_content(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PushRequest>,
) -> impl IntoResponse {
    let status = payload.status.unwrap_or_else(|| "draft".to_string());
    if !valid_file_status(&status) {
        return StatusCode::BAD_REQUEST;
    }

    let unique_filename = format!("{}-pushed.md", Uuid::new_v4());
    let path = state.uploads_path.join(&unique_filename);
    if fs::write(&path, payload.content).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    match sqlx::query(
        "INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, 'md', ?)",
    )
    .bind(&unique_filename)
    .bind(&payload.title)
    .bind(status)
    .execute(&state.db)
    .await
    {
        Ok(_) => StatusCode::CREATED,
        Err(_) => {
            let _ = fs::remove_file(&path).await;
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub(crate) async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(AppConfig {
        ai_workers: state.ai_workers,
        local_ai_workers: state.local_ai_workers,
        ai_task_timeout_secs: state.ai_task_timeout_secs,
        cli_launch_mode: state.cli_launch_mode.to_string(),
        cli_launcher_available: state.cli_launch_mode.can_launch_cli(),
    })
    .into_response()
}

pub(crate) async fn get_available_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
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
    append_cli_models(
        &mut models,
        state.cli_launch_mode,
        crate::engine_presets::executable_exists,
    );
    Json(models).into_response()
}

pub(crate) fn append_cli_models(
    models: &mut Vec<ModelOption>,
    launch_mode: crate::cli_launcher::CliLaunchMode,
    executable_exists: impl Fn(&str) -> bool,
) {
    if !launch_mode.can_launch_cli() {
        return;
    }
    let cli_tools = vec!["claude", "gemini", "codex"];
    for tool in cli_tools {
        if executable_exists(tool) {
            models.push(ModelOption {
                name: tool.to_string(),
                source: "cli".to_string(),
            });
        }
    }
}

pub(crate) async fn update_status(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<StatusUpdate>,
) -> impl IntoResponse {
    if !valid_file_status(&payload.status) {
        return StatusCode::BAD_REQUEST;
    }

    let sql = if payload.status == "published" {
        "UPDATE files SET status = ? WHERE filename = ?"
    } else {
        "UPDATE files SET status = ?, drawer_id = NULL WHERE filename = ?"
    };

    match sqlx::query(sql)
        .bind(&payload.status)
        .bind(&filename)
        .execute(&state.db)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::OK,
        Ok(_) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(crate) async fn update_title(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<TitleUpdate>,
) -> impl IntoResponse {
    match sqlx::query("UPDATE files SET original_name = ? WHERE filename = ?")
        .bind(&payload.title)
        .bind(&filename)
        .execute(&state.db)
        .await
    {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(crate) async fn update_file_metadata(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<FileMetadataUpdate>,
) -> impl IntoResponse {
    let title = payload.title.trim();
    if title.is_empty() || title.chars().count() > 256 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let file_id = match sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(file_id)) => file_id,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let labels = match user_tag_labels(payload.tags) {
        Ok(labels) => labels,
        Err(TagUpdateError::InvalidInput) => return StatusCode::BAD_REQUEST.into_response(),
        Err(TagUpdateError::Storage) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match sqlx::query("UPDATE files SET original_name = ? WHERE id = ?")
        .bind(title)
        .bind(file_id)
        .execute(&mut *tx)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => {}
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }

    let tag_ids = match user_tag_ids(&mut tx, labels).await {
        Ok(tag_ids) => tag_ids,
        Err(TagUpdateError::InvalidInput) => return StatusCode::BAD_REQUEST.into_response(),
        Err(TagUpdateError::Storage) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if replace_user_tag_ids(&mut tx, file_id, &tag_ids)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    match load_file_tags(&state.db, file_id).await {
        Ok(tags) => Json(tags).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn list_tags(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, TagInfo>(
        "SELECT id, label, slug, kind, '' AS source FROM tags ORDER BY kind, label",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(tags) => Json(tags).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn update_tags(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<TagUpdate>,
) -> impl IntoResponse {
    let file_id = match sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(file_id)) => file_id,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let labels = match user_tag_labels(payload.tags) {
        Ok(labels) => labels,
        Err(TagUpdateError::InvalidInput) => return StatusCode::BAD_REQUEST.into_response(),
        Err(TagUpdateError::Storage) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let tag_ids = match user_tag_ids(&mut tx, labels).await {
        Ok(tag_ids) => tag_ids,
        Err(TagUpdateError::InvalidInput) => return StatusCode::BAD_REQUEST.into_response(),
        Err(TagUpdateError::Storage) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if replace_user_tag_ids(&mut tx, file_id, &tag_ids)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    match load_file_tags(&state.db, file_id).await {
        Ok(tags) => Json(tags).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

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

async fn load_file_tags(db: &SqlitePool, file_id: i64) -> Result<Vec<TagInfo>, sqlx::Error> {
    sqlx::query_as::<_, TagInfo>(
        "SELECT tags.id, tags.label, tags.slug, tags.kind, file_tags.source
         FROM file_tags
         JOIN tags ON tags.id = file_tags.tag_id
         WHERE file_tags.file_id = ?
         ORDER BY tags.kind, tags.label",
    )
    .bind(file_id)
    .fetch_all(db)
    .await
}

#[cfg(test)]
pub(crate) async fn assign_file_tags(
    db: &SqlitePool,
    file_id: i64,
    labels: &[&str],
    source: &str,
) -> Result<(), sqlx::Error> {
    let labels: Vec<String> = labels.iter().map(|label| (*label).to_string()).collect();
    assign_file_tag_labels(db, file_id, &labels, source).await
}

pub(crate) async fn assign_file_tag_labels(
    db: &SqlitePool,
    file_id: i64,
    labels: &[String],
    source: &str,
) -> Result<(), sqlx::Error> {
    if labels.is_empty() {
        return Ok(());
    }

    for label in labels {
        let tag_id = ensure_tag_id(db, label, "system").await?;
        sqlx::query("INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, ?)")
            .bind(file_id)
            .bind(tag_id)
            .bind(source)
            .execute(db)
            .await?;
    }
    Ok(())
}

pub(crate) fn system_tags_for_file_prefix(file_prefix: &str) -> Vec<&'static str> {
    match file_prefix {
        "[AI-Research]" => vec!["Research"],
        "[Research]" => vec!["Research"],
        "[KO]" => vec!["Translation"],
        "[Scrape]" => vec!["Scrape"],
        "[Scrape+KO]" => vec!["Scrape", "Translation"],
        _ => Vec::new(),
    }
}

pub(crate) fn system_tag_labels_for_task(
    file_prefix: &str,
    quality_status: Option<&str>,
    model: Option<&str>,
    resolved_model: Option<&str>,
    engine_kind: Option<&str>,
) -> Vec<String> {
    let mut labels: Vec<String> = system_tags_for_file_prefix(file_prefix)
        .into_iter()
        .map(str::to_string)
        .collect();

    if matches!(
        quality_status.unwrap_or_default(),
        "untrusted" | "low_confidence" | "no_confidence"
    ) {
        labels.push("Low Confidence".to_string());
    }

    let model_label = resolved_model
        .filter(|value| !value.trim().is_empty())
        .or_else(|| model.and_then(normalized_model_label));
    if let Some(model_label) = model_label {
        labels.push(format!("Model: {}", model_label.trim()));
    }

    if let Some(engine_kind) = engine_kind.filter(|value| !value.trim().is_empty()) {
        labels.push(format!("Engine: {}", engine_kind.trim()));
    }

    labels.sort();
    labels.dedup();
    labels
}

fn normalized_model_label(model: &str) -> Option<&str> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .strip_prefix("cli:")
            .or_else(|| trimmed.strip_prefix("pi:"))
            .unwrap_or(trimmed),
    )
}
pub(crate) async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    match sqlx::query("DELETE FROM files WHERE filename = ?")
        .bind(&filename)
        .execute(&state.db)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => {
            let path = state.uploads_path.join(&filename);
            if path.exists() {
                let _ = fs::remove_file(path).await;
            }
            StatusCode::OK
        }
        Ok(_) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
pub(crate) async fn get_content(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let file = sqlx::query_as::<_, FileMetadata>("SELECT * FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
        .ok();
    if let Some(file) = file {
        let path = state.uploads_path.join(&file.filename);
        let content = match fs::read_to_string(path).await {
            Ok(c) => c,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let content_html = if file.file_type == "md" {
            let content = normalize_multiline_linked_images(&content);
            let mut options = Options::empty();
            options.insert(Options::ENABLE_STRIKETHROUGH);
            options.insert(Options::ENABLE_TABLES);
            let parser = MarkdownParser::new_ext(&content, options);
            let mut output = String::new();
            html::push_html(&mut output, parser);
            if has_research_request(&state, &file).await {
                output = wrap_research_verification_appendix(&output);
            }
            output
        } else {
            content
        };

        let final_page = if file.file_type == "html" {
            content_html
        } else {
            format!(
                r#"<html><head>
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <style>
                * {{ box-sizing: border-box; }}
                body {{ 
                    color: white; 
                    font-family: 'Pretendard', -apple-system, BlinkMacSystemFont, sans-serif; 
                    line-height: 1.6; 
                    padding: 20px; 
                    background: transparent; 
                    max-width: 900px; 
                    margin: 0 auto; 
                    overflow-x: hidden; 
                    word-wrap: break-word;
                }}
                a {{ color: #00d2ff; text-decoration: none; font-weight: 600; text-shadow: 0 0 8px rgba(0, 210, 255, 0.4); transition: all 0.3s ease; }}
                a:hover {{ color: #fff; text-shadow: 0 0 15px rgba(0, 210, 255, 0.8); }}
                pre {{ 
                    background: rgba(0,0,0,0.3); 
                    padding: 1rem; 
                    border-radius: 12px; 
                    overflow-x: auto; 
                    border: 1px solid rgba(255,255,255,0.1); 
                    max-width: 100%;
                    white-space: pre;
                }}
                code {{ font-family: 'Fira Code', monospace; background: rgba(255,255,255,0.1); padding: 0.2rem 0.4rem; border-radius: 4px; font-size: 0.9em; }}
                pre code {{ display: block; background: transparent; padding: 0; border-radius: 0; white-space: inherit; word-break: normal; overflow-wrap: normal; }}
                img {{ max-width: 100%; height: auto; border-radius: 12px; }}
                h1, h2, h3 {{ border-bottom: 1px solid rgba(255,255,255,0.1); padding-bottom: 0.3em; }}
                blockquote {{ border-left: 4px solid var(--accent-color, #6366f1); padding-left: 1em; color: rgba(255,255,255,0.7); font-style: italic; margin: 1.5em 0; }}
                table {{ display: block; width: 100%; max-width: 100%; overflow-x: auto; border-collapse: collapse; margin: 1.5rem 0; }}
                th, td {{ border: 1px solid rgba(255,255,255,0.16); padding: 0.55rem 0.75rem; text-align: left; vertical-align: top; }}
                th {{ background: rgba(255,255,255,0.12); font-weight: 700; }}
                tr:nth-child(even) td {{ background: rgba(255,255,255,0.04); }}
                .research-verification-appendix {{ margin-top: 3rem; border-top: 1px solid rgba(255,255,255,0.18); padding-top: 1rem; color: rgba(255,255,255,0.78); }}
                .research-verification-appendix > summary {{ cursor: pointer; list-style: none; display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.85rem 1rem; border: 1px solid rgba(255,255,255,0.16); border-radius: 8px; background: rgba(255,255,255,0.08); color: rgba(255,255,255,0.9); font-weight: 700; }}
                .research-verification-appendix > summary::-webkit-details-marker {{ display: none; }}
                .research-verification-appendix > summary::after {{ content: '펼치기'; font-size: 0.82rem; color: rgba(255,255,255,0.58); font-weight: 600; }}
                .research-verification-appendix[open] > summary::after {{ content: '접기'; }}
                .research-verification-body {{ padding-top: 1rem; font-size: 0.94rem; }}
                .research-verification-body h1, .research-verification-body h2, .research-verification-body h3 {{ color: rgba(255,255,255,0.82); }}
                .research-verification-body table {{ font-size: 0.9rem; }}
                
                @media (max-width: 600px) {{
                    body {{ padding: 15px; }}
                    h1 {{ font-size: 1.5rem; }}
                }}
                </style></head><body>{}</body></html>"#,
                content_html
            )
        };

        (HeaderMap::new(), Html(final_page)).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn wrap_research_verification_appendix(content_html: &str) -> String {
    let Some(start) = find_verification_heading_start(content_html) else {
        return content_html.to_string();
    };
    if content_html[..start]
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .count()
        < 300
    {
        return content_html.to_string();
    }
    let main = content_html[..start].trim_end();
    let appendix = content_html[start..].trim_start();
    format!(
        r#"{main}
<details class="research-verification-appendix">
<summary>검증 부록, 출처, Claim Log</summary>
<div class="research-verification-body">
{appendix}
</div>
</details>"#
    )
}

fn find_verification_heading_start(content_html: &str) -> Option<usize> {
    let markers = [
        "검증 부록",
        "verification appendix",
        "출처 감사",
        "source audit",
        "source cards",
        "claim log",
        "주장 로그",
        "품질 게이트",
        "quality gate",
        "품질 점수",
        "score section",
        "고우선 검증",
        "high-priority verification",
        "conflict map",
        "ambiguity check",
        "resolution check",
        "research quality",
        "연구 품질",
        "품질 검토",
    ];
    let lower = content_html.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(rel_start) = lower[cursor..].find("<h") {
        let start = cursor + rel_start;
        let Some(level) = lower[start + 2..].chars().next() else {
            break;
        };
        if !matches!(level, '1'..='6') {
            cursor = start + 2;
            continue;
        }
        let Some(open_end_rel) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel + 1;
        let close_tag = format!("</h{level}>");
        let Some(close_rel) = lower[open_end..].find(&close_tag) else {
            break;
        };
        let close = open_end + close_rel;
        let heading_text = strip_html_tags(&content_html[open_end..close]).to_ascii_lowercase();
        if markers.iter().any(|marker| heading_text.contains(marker)) {
            return Some(start);
        }
        cursor = close + close_tag.len();
    }
    None
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) async fn get_raw_content(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let file = sqlx::query_as::<_, FileMetadata>("SELECT * FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
        .ok();
    if let Some(file) = file {
        let path = state.uploads_path.join(&file.filename);
        match fs::read_to_string(path).await {
            Ok(content) => content.into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub(crate) async fn get_research_request(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let file = match sqlx::query_as::<_, FileMetadata>("SELECT * FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
    {
        Ok(file) => file,
        Err(sqlx::Error::RowNotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let task = match sqlx::query_as::<_, TaskInfo>(
        "SELECT * FROM tasks
         WHERE (file_id = ? OR filename = ?)
           AND file_prefix IN ('[Research]', '[AI-Research]')
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(file.id)
    .bind(&file.filename)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(task)) => task,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match research_request_info_from_task(&state.db, file.id, task).await {
        Ok(info) => Json(info).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn research_request_info_from_task(
    db: &SqlitePool,
    output_file_id: i64,
    task: TaskInfo,
) -> Result<ResearchRequestInfo, sqlx::Error> {
    let source_filenames = parse_json_string_array(task.source_filenames.as_deref());
    let source_file_ids = parse_json_i64_array(task.source_file_ids.as_deref());
    let source_documents =
        hydrate_research_source_documents(db, source_file_ids.as_deref(), &source_filenames)
            .await?;
    let relationships = load_document_relationships(db, output_file_id).await?;

    Ok(ResearchRequestInfo {
        task_id: task.id,
        original_name: task.original_name,
        created_at: task.created_at,
        request_prompt: task.user_prompt,
        research_topic: task.research_topic,
        research_instructions: task.research_instructions,
        source_filenames,
        source_documents,
        relationships,
        research_type: task.research_type,
        research_mode: task.research_mode,
        research_format: task.research_format,
        model: task.model,
        engine_preset_name: task.engine_preset_name,
        engine_kind: task.engine_kind,
        resolved_model: task.resolved_model,
        research_intensity: task.research_intensity,
        quality_max_iterations: task.quality_max_iterations,
        quality_depth: task.quality_depth,
        quality_status: task.quality_status,
        quality_last_failure: task.quality_last_failure,
        web_search_requested: task.web_search_requested,
        web_search_provider: task.web_search_provider,
        research_controller_artifacts_summary: summarize_research_controller_artifacts(
            task.research_controller_artifacts_json.as_deref(),
        ),
        research_source_diagnostics_summary: summarize_research_source_diagnostics(
            task.research_source_diagnostics_json.as_deref(),
        ),
    })
}

fn summarize_research_controller_artifacts(
    artifacts_json: Option<&str>,
) -> Option<ResearchControllerArtifactsSummary> {
    let artifacts = serde_json::from_str::<ResearchControllerArtifacts>(artifacts_json?).ok()?;
    Some(ResearchControllerArtifactsSummary {
        version: artifacts.version,
        event_count: artifacts.events.len(),
        source_card_count: artifacts.source_cards.len(),
        claim_count: artifacts.claim_log.len(),
        conflict_count: artifacts.conflict_map.len(),
        open_debt_count: artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
            .count(),
        warning_count: artifacts.warnings.len(),
        quality_gate_status: artifacts
            .quality_gate
            .as_ref()
            .map(|gate| gate.status.clone()),
        quality_gate_failure_count: artifacts
            .quality_gate
            .as_ref()
            .map(|gate| gate.failure_messages.len())
            .unwrap_or(0),
    })
}

fn summarize_research_source_diagnostics(
    diagnostics_json: Option<&str>,
) -> Option<ResearchSourceDiagnosticsSummary> {
    let diagnostics =
        serde_json::from_str::<ResearchSourceDiagnosticsEnvelope>(diagnostics_json?).ok()?;
    Some(ResearchSourceDiagnosticsSummary {
        version: diagnostics.version,
        subject: diagnostics.subject.as_deref().map(redact_urls_in_text),
        source_pack_status: diagnostics
            .source_pack
            .as_ref()
            .map(|report| redact_urls_in_text(report.status.as_str())),
        source_pack_query_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.queries.len())
            .unwrap_or(0),
        source_pack_adopted_source_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.adopted_source_count)
            .unwrap_or(0),
        source_pack_skipped_candidate_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.skipped_candidates.len())
            .unwrap_or(0),
        scrape_count: diagnostics.scrapes.len(),
        scrape_failure_count: diagnostics
            .scrapes
            .iter()
            .filter(|scrape| scrape.failure_reason.is_some())
            .count(),
        scrapes: diagnostics
            .scrapes
            .iter()
            .map(|scrape| {
                let failure_reason = scrape.failure_reason.as_deref().map(redact_urls_in_text);
                let insufficiency_reason = scrape
                    .insufficiency_reason
                    .as_deref()
                    .map(redact_urls_in_text);
                ResearchScrapeDiagnosticsSummary {
                    status_class: scrape.status_class.clone(),
                    user_message: friendly_scrape_failure_message(
                        scrape.status_class.as_str(),
                        failure_reason.as_deref(),
                        scrape.http_status_code,
                        insufficiency_reason.as_deref(),
                    ),
                    failure_reason,
                    http_status_code: scrape.http_status_code,
                    sufficiency_result: scrape.sufficiency_result.clone(),
                    insufficiency_reason,
                    original_url_host: redact_url_host(&scrape.original_url),
                    final_url_host: scrape.final_url.as_deref().and_then(redact_url_host),
                    reference_link_count: scrape.reference_links.len(),
                }
            })
            .collect(),
        context_packing: diagnostics.context_packing,
    })
}

fn redact_url_host(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

fn redact_urls_in_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if token.starts_with("http://") || token.starts_with("https://") {
                let trimmed = token.trim_matches(|ch: char| ",.;)]}\"'".contains(ch));
                if let Some(host) = redact_url_host(trimmed) {
                    token.replacen(trimmed, &format!("<redacted-url:{host}>"), 1)
                } else {
                    "<redacted-url>".to_string()
                }
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn hydrate_research_source_documents(
    db: &SqlitePool,
    source_file_ids: Option<&[i64]>,
    source_filenames: &[String],
) -> Result<Vec<ResearchSourceDocument>, sqlx::Error> {
    let mut documents = Vec::new();
    let mut seen_filenames = HashSet::new();

    if let Some(source_file_ids) = source_file_ids {
        for file_id in source_file_ids {
            if let Some(document) = load_research_source_document_by_id(db, *file_id).await? {
                seen_filenames.insert(document.filename.clone());
                documents.push(document);
            }
        }
    }

    if source_file_ids.is_none() || documents.len() < source_filenames.len() {
        for filename in source_filenames {
            if seen_filenames.contains(filename) {
                continue;
            }
            if let Some(document) = load_research_source_document_by_filename(db, filename).await? {
                seen_filenames.insert(document.filename.clone());
                documents.push(document);
            }
        }
    }

    Ok(documents)
}

async fn load_research_source_document_by_id(
    db: &SqlitePool,
    file_id: i64,
) -> Result<Option<ResearchSourceDocument>, sqlx::Error> {
    sqlx::query_as::<_, ResearchSourceDocument>(
        "SELECT id, filename, original_name AS title FROM files WHERE id = ?",
    )
    .bind(file_id)
    .fetch_optional(db)
    .await
}

async fn load_research_source_document_by_filename(
    db: &SqlitePool,
    filename: &str,
) -> Result<Option<ResearchSourceDocument>, sqlx::Error> {
    sqlx::query_as::<_, ResearchSourceDocument>(
        "SELECT id, filename, original_name AS title FROM files WHERE filename = ?",
    )
    .bind(filename)
    .fetch_optional(db)
    .await
}

pub(crate) async fn get_file_relationships(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let file_id = match sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(file_id)) => file_id,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match load_document_relationships(&state.db, file_id).await {
        Ok(relationships) => Json(relationships).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_relationship_graph(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Query(query): Query<RelationshipGraphQuery>,
) -> impl IntoResponse {
    let depth = query.depth.unwrap_or(1);
    if depth != 1 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let direction = query.direction.as_deref().unwrap_or("both");
    if !matches!(direction, "both" | "sources" | "derivatives") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let root = match sqlx::query_as::<_, ResearchSourceDocument>(
        "SELECT id, filename, original_name AS title FROM files WHERE filename = ?",
    )
    .bind(&filename)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(file)) => DocumentGraphNode {
            id: file.id,
            filename: file.filename,
            title: file.title,
        },
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match load_document_relationship_graph(&state.db, root, direction).await {
        Ok(graph) => Json(graph).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn load_document_relationships(
    db: &SqlitePool,
    file_id: i64,
) -> Result<DocumentRelationships, sqlx::Error> {
    let sources = load_document_links(db, file_id, true).await?;
    let derivatives = load_document_links(db, file_id, false).await?;
    Ok(DocumentRelationships {
        sources,
        derivatives,
    })
}

async fn load_document_links(
    db: &SqlitePool,
    file_id: i64,
    sources: bool,
) -> Result<Vec<DocumentLinkInfo>, sqlx::Error> {
    let (file_column, linked_column) = if sources {
        ("from_file_id", "to_file_id")
    } else {
        ("to_file_id", "from_file_id")
    };
    let sql = format!(
        "SELECT document_links.id,
                document_links.from_file_id,
                document_links.to_file_id,
                document_links.relation_type,
                document_links.created_by_task_id,
                document_links.created_at,
                linked.id AS document_id,
                linked.filename AS document_filename,
                linked.original_name AS document_title
         FROM document_links
         JOIN files AS linked ON linked.id = document_links.{linked_column}
         WHERE document_links.{file_column} = ?
         ORDER BY document_links.created_at DESC, document_links.id DESC"
    );
    let rows = sqlx::query(&sql).bind(file_id).fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| DocumentLinkInfo {
            id: row.get("id"),
            from_file_id: row.get("from_file_id"),
            to_file_id: row.get("to_file_id"),
            relation_type: row.get("relation_type"),
            created_by_task_id: row.get("created_by_task_id"),
            created_at: row.get("created_at"),
            document: ResearchSourceDocument {
                id: row.get("document_id"),
                filename: row.get("document_filename"),
                title: row.get("document_title"),
            },
        })
        .collect())
}

async fn load_document_relationship_graph(
    db: &SqlitePool,
    root: DocumentGraphNode,
    direction: &str,
) -> Result<DocumentRelationshipGraph, sqlx::Error> {
    let edges = load_document_graph_edges(db, root.id, direction).await?;
    let mut node_ids = vec![root.id];
    let mut seen = HashSet::from([root.id]);
    for edge in &edges {
        if seen.insert(edge.from_file_id) {
            node_ids.push(edge.from_file_id);
        }
        if seen.insert(edge.to_file_id) {
            node_ids.push(edge.to_file_id);
        }
    }

    let mut node_lookup = load_document_graph_nodes(db, &node_ids).await?;
    let mut nodes = Vec::with_capacity(node_ids.len());
    nodes.push(root.clone());
    node_lookup.remove(&root.id);
    for node_id in node_ids.into_iter().skip(1) {
        if let Some(node) = node_lookup.remove(&node_id) {
            nodes.push(node);
        }
    }

    Ok(DocumentRelationshipGraph { root, nodes, edges })
}

async fn load_document_graph_edges(
    db: &SqlitePool,
    root_id: i64,
    direction: &str,
) -> Result<Vec<DocumentGraphEdge>, sqlx::Error> {
    let where_clause = match direction {
        "sources" => "from_file_id = ?",
        "derivatives" => "to_file_id = ?",
        _ => "from_file_id = ? OR to_file_id = ?",
    };
    let sql = format!(
        "SELECT id, from_file_id, to_file_id, relation_type, created_by_task_id, created_at
         FROM document_links
         WHERE {where_clause}
         ORDER BY created_at DESC, id DESC"
    );
    let mut query = sqlx::query(&sql).bind(root_id);
    if direction == "both" {
        query = query.bind(root_id);
    }
    let rows = query.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| DocumentGraphEdge {
            id: row.get("id"),
            from_file_id: row.get("from_file_id"),
            to_file_id: row.get("to_file_id"),
            relation_type: row.get("relation_type"),
            created_by_task_id: row.get("created_by_task_id"),
            created_at: row.get("created_at"),
        })
        .collect())
}

async fn load_document_graph_nodes(
    db: &SqlitePool,
    node_ids: &[i64],
) -> Result<HashMap<i64, DocumentGraphNode>, sqlx::Error> {
    if node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = vec!["?"; node_ids.len()].join(",");
    let sql = format!(
        "SELECT id, filename, original_name AS title
         FROM files
         WHERE id IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for node_id in node_ids {
        query = query.bind(*node_id);
    }

    let rows = query.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let node = DocumentGraphNode {
                id: row.get("id"),
                filename: row.get("filename"),
                title: row.get("title"),
            };
            (node.id, node)
        })
        .collect())
}

fn parse_json_string_array(value: Option<&str>) -> Vec<String> {
    value
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

fn parse_json_i64_array(value: Option<&str>) -> Option<Vec<i64>> {
    value.and_then(|raw| serde_json::from_str::<Vec<i64>>(raw).ok())
}

pub(crate) async fn search_files(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    let files = match sqlx::query_as::<_, FileMetadata>("SELECT * FROM files")
        .fetch_all(&state.db)
        .await
    {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut matches = Vec::new();
    let q = query.q.to_lowercase();

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
        .fetch_one(&state.db)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);
        if tag_match {
            matches.push(file);
            continue;
        }

        let path = state.uploads_path.join(&file.filename);
        if let Ok(content) = fs::read_to_string(path).await {
            if content.to_lowercase().contains(&q) {
                matches.push(file);
            }
        }
    }

    let mut results = Vec::with_capacity(matches.len());
    for metadata in matches {
        let content_preview = build_content_preview(&state, &metadata).await;
        let has_research_request = has_research_request(&state, &metadata).await;
        results.push(FileListItem {
            metadata,
            content_preview,
            has_research_request,
            tags: Vec::new(),
        });
    }
    if hydrate_file_tags(&state.db, &mut results).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Json(results).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_launcher::CliLaunchMode;
    use crate::db::setup_db;
    use crate::models::StatusUpdate;
    use crate::test_support::{response_status, temp_test_dir, test_state};
    use axum::{
        body::to_bytes,
        extract::{Path, Query, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    };
    use sqlx::Row;
    use std::fs as std_fs;
    use std::sync::Arc;

    #[test]
    fn test_cli_models_hidden_when_launcher_disabled() {
        let mut models = Vec::new();

        append_cli_models(&mut models, CliLaunchMode::Disabled, |_| true);

        assert!(models.is_empty());
    }

    #[test]
    fn test_cli_models_visible_when_launcher_can_run_and_tools_exist() {
        let mut models = Vec::new();

        append_cli_models(&mut models, CliLaunchMode::Unsandboxed, |tool| {
            matches!(tool, "claude" | "codex")
        });

        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .any(|model| model.source == "cli" && model.name == "claude"));
        assert!(models
            .iter()
            .any(|model| model.source == "cli" && model.name == "codex"));
    }

    #[test]
    fn wraps_research_verification_sections_as_collapsed_appendix() {
        let html = "<h1>Final Answer</h1><p>본문 ".to_string()
            + &"충분한 독자용 내용 ".repeat(40)
            + "</p><h1>검증 부록</h1><h2>Claim Log</h2><table><tr><td>근거</td></tr></table>";

        let wrapped = wrap_research_verification_appendix(&html);

        assert!(wrapped.contains(r#"<details class="research-verification-appendix">"#));
        assert!(wrapped.contains("<summary>검증 부록, 출처, Claim Log</summary>"));
        assert!(wrapped.contains("<h1>검증 부록</h1>"));
        assert!(wrapped.find("<details").unwrap() < wrapped.find("<h1>검증 부록</h1>").unwrap());
    }

    #[test]
    fn does_not_wrap_short_documents_that_start_with_verification() {
        let html = "<h1>출처 감사</h1><table><tr><td>근거</td></tr></table>";

        let wrapped = wrap_research_verification_appendix(html);

        assert_eq!(wrapped, html);
    }

    #[tokio::test]
    async fn detects_research_markdown_files_from_task_metadata() {
        let dir = temp_test_dir("research-marker-detection");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let file = FileMetadata {
            id: 1,
            filename: "abc-untrusted-ai.md".to_string(),
            original_name: "테스트".to_string(),
            file_type: "md".to_string(),
            status: "draft".to_string(),
            drawer_id: None,
            uploaded_at: chrono::Utc::now(),
        };
        sqlx::query("INSERT INTO files (id, filename, original_name, file_type, status) VALUES (1, 'abc-untrusted-ai.md', '테스트', 'md', 'draft')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tasks (file_id, filename, original_name, status, file_prefix) VALUES (1, 'abc-untrusted-ai.md', '테스트', 'completed', '[AI-Research]')")
            .execute(&db)
            .await
            .unwrap();

        assert!(has_research_request(&state, &file).await);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn does_not_treat_plain_ai_markdown_title_as_research() {
        let dir = temp_test_dir("plain-ai-detection");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let file = FileMetadata {
            id: 1,
            filename: "abc-ai.md".to_string(),
            original_name: "[AI] 일반 요약".to_string(),
            file_type: "md".to_string(),
            status: "draft".to_string(),
            drawer_id: None,
            uploaded_at: chrono::Utc::now(),
        };
        sqlx::query("INSERT INTO files (id, filename, original_name, file_type, status) VALUES (1, 'abc-ai.md', '[AI] 일반 요약', 'md', 'draft')")
            .execute(&db)
            .await
            .unwrap();

        assert!(!has_research_request(&state, &file).await);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_research_request_returns_linked_task_metadata() {
        let dir = temp_test_dir("research-request");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let source_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('source.md', 'Source Document', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('research.md', '[AI-Research] Test topic', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (
                file_id, filename, original_name, status, user_prompt, source_file_ids, source_filenames,
                file_prefix, file_type, research_type, research_mode, research_format,
                research_topic, research_instructions, engine_preset_name, engine_kind,
                research_intensity, quality_max_iterations, quality_depth, quality_status,
                quality_last_failure, web_search_requested, web_search_provider,
                research_controller_artifacts_json, research_source_diagnostics_json
             ) VALUES (?, 'research.md', 'Test topic', 'completed', 'prompt body', ?,
                '[\"source.md\"]', '[AI-Research]', 'md', 'initial', 'general', 'md',
                'Test topic', 'focus details', 'Codex CLI', 'cli', 'high', 15, 'strict',
                'trusted', 'minor debt', 'true', 'codex-search', ?, ?)",
        )
        .bind(file_id)
        .bind(format!("[{source_id}]"))
        .bind(
            r#"{"version":1,"events":[{"stage":"draft","iteration":1,"max_iterations":3,"status":"running"}],"source_cards":[{"id":"S1","url":"https://example.com/report?token=secret","title":"Example","source_class":"official_or_primary","extracted_facts":["fact"],"confidence":"high"}],"claim_log":[{"id":"C1","claim":"supported","support_source_card_ids":["S1"]}],"conflict_map":[],"research_debt":[],"quality_gate":{"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}}"#,
        )
        .bind(
            r#"{"version":1,"subject":"https://example.com/report?token=secret","source_pack":{"subject":"Test topic","status":"success","reason":null,"queries":[],"seeded_source_count":1,"discovered_source_count":0,"adopted_source_count":1,"adopted_candidates":[],"skipped_candidates":[]},"scrapes":[{"original_url":"https://example.com/report?token=secret","normalized_url":"https://example.com/report?token=secret","final_url":"https://api.example.com/final#frag","status_class":"ok","failure_reason":"failed to fetch https://example.com/report?token=secret","http_status_code":403,"extraction_strategy":null,"title":null,"content_type":null,"raw_body_bytes":null,"raw_body_chars":null,"extracted_html_chars":10,"markdown_chars":10,"sufficiency_result":"sufficient","insufficiency_reason":null,"reference_links":["https://docs.example.com/a?x=1"],"accessed_at":"2026-05-13T00:00:00Z","raw_capture":{"mode":"omitted","path":null,"hash":null,"omitted_reason":"disabled"}}],"context_packing":{"strategy":"artifact_ledgers","included_source_card_count":1,"omitted_source_card_count":0,"included_excerpt_chars":10,"omitted_raw_chars":0,"total_raw_chars":10,"active_debt_count":0,"unresolved_conflict_count":0,"notes":["safe"]}}"#,
        )
        .execute(&db)
        .await
        .unwrap();
        crate::db::create_document_links_for_task_output(&db, 1, file_id)
            .await
            .unwrap();

        let response = get_research_request(State(state), Path("research.md".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["task_id"], 1);
        assert_eq!(payload["original_name"], "Test topic");
        assert_eq!(payload["research_topic"], "Test topic");
        assert_eq!(payload["research_instructions"], "focus details");
        assert_eq!(payload["source_filenames"][0], "source.md");
        assert_eq!(payload["source_documents"][0]["id"], source_id);
        assert_eq!(payload["source_documents"][0]["filename"], "source.md");
        assert_eq!(payload["source_documents"][0]["title"], "Source Document");
        assert_eq!(
            payload["relationships"]["sources"][0]["relation_type"],
            "derived_from"
        );
        assert_eq!(
            payload["relationships"]["sources"][0]["document"]["title"],
            "Source Document"
        );
        assert_eq!(payload["quality_last_failure"], "minor debt");
        assert_eq!(payload["web_search_provider"], "codex-search");
        assert!(payload.get("research_controller_artifacts_json").is_none());
        assert!(payload.get("research_source_diagnostics_json").is_none());
        assert_eq!(
            payload["research_controller_artifacts_summary"]["source_card_count"],
            1
        );
        assert_eq!(
            payload["research_controller_artifacts_summary"]["quality_gate_status"],
            "passed"
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["original_url_host"],
            "example.com"
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["subject"],
            "<redacted-url:example.com>"
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["final_url_host"],
            "api.example.com"
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["reference_link_count"],
            1
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["failure_reason"],
            "failed to fetch <redacted-url:example.com>"
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["http_status_code"],
            403
        );
        assert_eq!(
            payload["research_source_diagnostics_summary"]["scrapes"][0]["user_message"],
            "스크랩 처리 중 예기치 않은 오류가 발생했습니다. 입력 URL을 다시 확인하고, 같은 문제가 반복되면 기술 세부를 함께 확인해 주세요.\n\nHTTP 상태: 403\n진단 분류: ok\n기술 세부: failed to fetch <redacted-url:example.com>"
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_research_request_returns_soft_deleted_task_metadata() {
        let dir = temp_test_dir("soft-deleted-research-request");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('research.md', '[AI-Research] Test topic', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (
                file_id, filename, original_name, status, user_prompt, file_prefix, deleted_at
             ) VALUES (?, 'research.md', 'Test topic', 'completed', 'prompt body',
                '[AI-Research]', CURRENT_TIMESTAMP)",
        )
        .bind(file_id)
        .execute(&db)
        .await
        .unwrap();

        let response = get_research_request(State(state), Path("research.md".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["original_name"], "Test topic");
        assert_eq!(payload["request_prompt"], "prompt body");

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_research_request_rejects_non_research_outputs() {
        let dir = temp_test_dir("non-research-request");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('summary.md', '[AI] Summary', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (file_id, filename, original_name, status, file_prefix)
             VALUES (?, 'summary.md', 'Summary', 'completed', '[AI]')",
        )
        .bind(file_id)
        .execute(&db)
        .await
        .unwrap();

        assert_eq!(
            response_status(
                get_research_request(State(state), Path("summary.md".to_string())).await
            ),
            StatusCode::NOT_FOUND
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_file_relationships_returns_sources_and_derivatives() {
        let dir = temp_test_dir("file-relationships");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO files (id, filename, original_name, file_type, status) VALUES
             (1, 'source.md', 'Source', 'md', 'draft'),
             (2, 'output.md', 'Output', 'md', 'draft'),
             (3, 'child.md', 'Child', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO document_links (from_file_id, to_file_id, relation_type, created_by_task_id)
             VALUES
             (2, 1, 'derived_from', 11),
             (3, 2, 'followed_up_from', 12)",
        )
        .execute(&db)
        .await
        .unwrap();

        let response = get_file_relationships(State(state), Path("output.md".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["sources"][0]["relation_type"], "derived_from");
        assert_eq!(payload["sources"][0]["document"]["filename"], "source.md");
        assert_eq!(
            payload["derivatives"][0]["relation_type"],
            "followed_up_from"
        );
        assert_eq!(
            payload["derivatives"][0]["document"]["filename"],
            "child.md"
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_relationship_graph_returns_normalized_depth_one_graph() {
        let dir = temp_test_dir("relationship-graph");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO files (id, filename, original_name, file_type, status) VALUES
             (1, 'source.md', 'Source', 'md', 'draft'),
             (2, 'output.md', 'Output', 'md', 'draft'),
             (3, 'child.md', 'Child', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO document_links (id, from_file_id, to_file_id, relation_type, created_by_task_id)
             VALUES
             (21, 2, 1, 'derived_from', 11),
             (22, 3, 2, 'followed_up_from', 12)",
        )
        .execute(&db)
        .await
        .unwrap();

        let response = get_relationship_graph(
            State(state),
            Path("output.md".to_string()),
            Query(RelationshipGraphQuery {
                depth: Some(1),
                direction: Some("both".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["root"]["id"], 2);
        assert_eq!(payload["root"]["filename"], "output.md");
        assert_eq!(payload["nodes"].as_array().unwrap().len(), 3);
        assert!(payload["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["filename"] == "source.md"));
        assert_eq!(payload["edges"].as_array().unwrap().len(), 2);
        assert!(payload["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["from_file_id"] == 2 && edge["to_file_id"] == 1));
        assert!(payload["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["from_file_id"] == 3 && edge["to_file_id"] == 2));

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_relationship_graph_filters_by_direction() {
        let dir = temp_test_dir("relationship-graph-direction");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO files (id, filename, original_name, file_type, status) VALUES
             (1, 'source.md', 'Source', 'md', 'draft'),
             (2, 'output.md', 'Output', 'md', 'draft'),
             (3, 'child.md', 'Child', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO document_links (id, from_file_id, to_file_id, relation_type, created_by_task_id)
             VALUES
             (21, 2, 1, 'derived_from', 11),
             (22, 3, 2, 'followed_up_from', 12)",
        )
        .execute(&db)
        .await
        .unwrap();

        let sources_response = get_relationship_graph(
            State(Arc::clone(&state)),
            Path("output.md".to_string()),
            Query(RelationshipGraphQuery {
                depth: Some(1),
                direction: Some("sources".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(sources_response.status(), StatusCode::OK);
        let sources_body = to_bytes(sources_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let sources_payload: serde_json::Value = serde_json::from_slice(&sources_body).unwrap();
        assert_eq!(sources_payload["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(sources_payload["edges"].as_array().unwrap().len(), 1);
        assert_eq!(sources_payload["edges"][0]["from_file_id"], 2);
        assert_eq!(sources_payload["edges"][0]["to_file_id"], 1);

        let derivatives_response = get_relationship_graph(
            State(state),
            Path("output.md".to_string()),
            Query(RelationshipGraphQuery {
                depth: Some(1),
                direction: Some("derivatives".to_string()),
            }),
        )
        .await
        .into_response();
        assert_eq!(derivatives_response.status(), StatusCode::OK);
        let derivatives_body = to_bytes(derivatives_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let derivatives_payload: serde_json::Value =
            serde_json::from_slice(&derivatives_body).unwrap();
        assert_eq!(derivatives_payload["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(derivatives_payload["edges"].as_array().unwrap().len(), 1);
        assert_eq!(derivatives_payload["edges"][0]["from_file_id"], 3);
        assert_eq!(derivatives_payload["edges"][0]["to_file_id"], 2);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_relationship_graph_rejects_unsupported_depth_and_direction() {
        let dir = temp_test_dir("relationship-graph-invalid-query");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO files (id, filename, original_name, file_type, status)
             VALUES (1, 'output.md', 'Output', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap();

        assert_eq!(
            response_status(
                get_relationship_graph(
                    State(Arc::clone(&state)),
                    Path("output.md".to_string()),
                    Query(RelationshipGraphQuery {
                        depth: Some(2),
                        direction: Some("both".to_string()),
                    }),
                )
                .await
            ),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            response_status(
                get_relationship_graph(
                    State(state),
                    Path("output.md".to_string()),
                    Query(RelationshipGraphQuery {
                        depth: Some(1),
                        direction: Some("sideways".to_string()),
                    }),
                )
                .await
            ),
            StatusCode::BAD_REQUEST
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_research_request_omits_stale_source_ids_but_preserves_source_filenames() {
        let dir = temp_test_dir("research-request-stale-source");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('research.md', 'Research output', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (
                file_id, filename, original_name, status, source_file_ids, source_filenames, file_prefix
             ) VALUES (?, 'research.md', 'Research output', 'completed', '[999]',
                '[\"deleted-source.md\"]', '[Research]')",
        )
        .bind(file_id)
        .execute(&db)
        .await
        .unwrap();

        let response = get_research_request(State(state), Path("research.md".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["source_documents"].as_array().unwrap().len(), 0);
        assert_eq!(payload["source_filenames"][0], "deleted-source.md");

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn get_research_request_falls_back_to_exact_source_filename_when_ids_are_malformed() {
        let dir = temp_test_dir("research-request-source-filename-fallback");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let source_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('source.md', 'Exact Source', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('research.md', 'Research output', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (
                file_id, filename, original_name, status, source_file_ids, source_filenames, file_prefix
             ) VALUES (?, 'research.md', 'Research output', 'completed', 'not-json',
                '[\"source.md\", \"missing.md\"]', '[AI-Research]')",
        )
        .bind(file_id)
        .execute(&db)
        .await
        .unwrap();

        let response = get_research_request(State(state), Path("research.md".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["source_documents"].as_array().unwrap().len(), 1);
        assert_eq!(payload["source_documents"][0]["id"], source_id);
        assert_eq!(payload["source_documents"][0]["filename"], "source.md");
        assert_eq!(payload["source_documents"][0]["title"], "Exact Source");
        assert_eq!(payload["source_filenames"][1], "missing.md");

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn update_tags_replaces_only_user_tags_and_preserves_system_tags() {
        let dir = temp_test_dir("update-tags");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('doc.md', 'Document', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        assign_file_tags(&db, file_id, &["Research"], "task")
            .await
            .unwrap();

        let response = update_tags(
            State(Arc::clone(&state)),
            Path("doc.md".to_string()),
            Json(TagUpdate {
                tags: vec!["Pinned".to_string(), "Client".to_string()],
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let response = update_tags(
            State(state),
            Path("doc.md".to_string()),
            Json(TagUpdate {
                tags: vec!["Client".to_string()],
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let tags = sqlx::query_as::<_, (String, String, String)>(
            "SELECT tags.label, tags.kind, file_tags.source
             FROM tags
             JOIN file_tags ON file_tags.tag_id = tags.id
             WHERE file_tags.file_id = ?
             ORDER BY tags.label",
        )
        .bind(file_id)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            tags,
            vec![
                ("Client".to_string(), "user".to_string(), "user".to_string()),
                (
                    "Research".to_string(),
                    "system".to_string(),
                    "task".to_string()
                ),
            ]
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn update_file_metadata_rejects_invalid_tags_without_changing_title() {
        let dir = temp_test_dir("update-file-metadata-invalid-tags");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('doc.md', 'Original title', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap();

        let response = update_file_metadata(
            State(state),
            Path("doc.md".to_string()),
            Json(crate::models::FileMetadataUpdate {
                title: "Changed title".to_string(),
                tags: (0..25).map(|idx| format!("tag-{idx}")).collect(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let title = sqlx::query_scalar::<_, String>(
            "SELECT original_name FROM files WHERE filename = 'doc.md'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(title, "Original title");

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn list_and_search_files_include_tag_metadata() {
        let dir = temp_test_dir("list-tag-metadata");
        let uploads = dir.join("uploads");
        std_fs::create_dir_all(&uploads).unwrap();
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), uploads.clone());
        let file_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status)
             VALUES ('doc.md', 'Document', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        std_fs::write(uploads.join("doc.md"), "Body").unwrap();
        assign_file_tags(&db, file_id, &["Research"], "task")
            .await
            .unwrap();

        let response = list_files(State(Arc::clone(&state))).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload[0]["tags"][0]["label"], "Research");
        assert_eq!(payload[0]["tags"][0]["source"], "task");

        let response = search_files(
            State(state),
            axum::extract::Query(SearchQuery {
                q: "research".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload[0]["filename"], "doc.md");

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_status_archived_clears_drawer_and_invalid_status_rejected() {
        let dir = temp_test_dir("status-rules");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        sqlx::query("INSERT INTO drawers (name) VALUES ('Shelf')")
            .execute(&db)
            .await
            .unwrap();
        let drawer_id = sqlx::query_scalar::<_, i64>("SELECT id FROM drawers WHERE name = 'Shelf'")
            .fetch_one(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (filename, original_name, file_type, status, drawer_id) VALUES ('published.md', 'Published', 'md', 'published', ?)")
            .bind(drawer_id)
            .execute(&db).await.unwrap();

        assert_eq!(
            response_status(
                update_status(
                    State(Arc::clone(&state)),
                    Path("published.md".to_string()),
                    Json(StatusUpdate {
                        status: "archived".to_string()
                    })
                )
                .await
            ),
            StatusCode::OK
        );
        let row =
            sqlx::query("SELECT status, drawer_id FROM files WHERE filename = 'published.md'")
                .fetch_one(&db)
                .await
                .unwrap();
        let status: String = row.get("status");
        let drawer_id: Option<i64> = row.get("drawer_id");
        assert_eq!(status, "archived");
        assert_eq!(drawer_id, None);

        assert_eq!(
            response_status(
                update_status(
                    State(state),
                    Path("published.md".to_string()),
                    Json(StatusUpdate {
                        status: "unknown".to_string()
                    })
                )
                .await
            ),
            StatusCode::BAD_REQUEST
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }
}
