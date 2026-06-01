use crate::context::{service_error_status, ServerContext};
use crate::presenters::render_loaded_file_content;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use liquid_protocol::{
    FileMetadataUpdate, PushRequest, SearchQuery, StatusUpdate, TagUpdate, TitleUpdate,
};
use research_public_summary::{
    summarize_research_controller_artifacts_for_public_api,
    summarize_research_source_diagnostics_for_public_api,
};
use serde::Deserialize;
use std::sync::Arc;

mod research_public_summary;
pub use research_public_summary::redact_public_diagnostic_text;

#[derive(Debug, Deserialize)]
pub struct RelationshipGraphQuery {
    pub depth: Option<u8>,
    pub direction: Option<String>,
}

pub async fn list_files(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    match state.files.list_files().await {
        Ok(items) => Json(items).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn upload_file(
    State(state): State<Arc<ServerContext>>,
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
            return match state
                .files
                .upload_file(original_name, file_type.to_string(), data.to_vec())
                .await
            {
                Ok(()) => StatusCode::CREATED.into_response(),
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
        }
    }
    StatusCode::BAD_REQUEST.into_response()
}

pub async fn push_content(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<PushRequest>,
) -> impl IntoResponse {
    let status = payload.status.unwrap_or_else(|| "draft".to_string());
    if !liquid_protocol::is_valid_file_status(&status) {
        return StatusCode::BAD_REQUEST;
    }
    match state
        .files
        .push_content(payload.title, payload.content, status)
        .await
    {
        Ok(()) => StatusCode::CREATED,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn get_config(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    Json(state.config.app_config()).into_response()
}

pub async fn get_available_models(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    Json(state.config.available_models().await).into_response()
}

pub async fn update_status(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<StatusUpdate>,
) -> impl IntoResponse {
    if !liquid_protocol::is_valid_file_status(&payload.status) {
        return StatusCode::BAD_REQUEST;
    }
    match state.files.update_status(filename, payload.status).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}

pub async fn update_title(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<TitleUpdate>,
) -> impl IntoResponse {
    match state.files.update_title(filename, payload.title).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}

pub async fn update_file_metadata(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<FileMetadataUpdate>,
) -> impl IntoResponse {
    let title = payload.title.trim();
    if title.is_empty() || title.chars().count() > 256 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match state
        .files
        .update_file_metadata(filename, title.to_string(), payload.tags)
        .await
    {
        Ok(tags) => Json(tags).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn list_tags(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    match state.files.list_tags().await {
        Ok(tags) => Json(tags).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn update_tags(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<TagUpdate>,
) -> impl IntoResponse {
    match state.files.update_tags(filename, payload.tags).await {
        Ok(tags) => Json(tags).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn delete_file(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    match state.files.delete_file(filename).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}
pub async fn get_content(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let loaded = match state.files.load_renderable_content(filename).await {
        Ok(loaded) => loaded,
        Err(error) => return service_error_status(error).into_response(),
    };
    (HeaderMap::new(), Html(render_loaded_file_content(loaded))).into_response()
}

pub async fn get_raw_content(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    match state.files.load_raw_content(filename).await {
        Ok(content) => content.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn get_research_request(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let mut payload = match state.files.load_research_request(filename).await {
        Ok(payload) => payload,
        Err(error) => return service_error_status(error).into_response(),
    };
    let artifacts_summary = summarize_research_controller_artifacts_for_public_api(
        payload.research_controller_artifacts_json.as_deref(),
    );
    let diagnostics_summary = summarize_research_source_diagnostics_for_public_api(
        payload.research_source_diagnostics_json.as_deref(),
    );
    payload.info.research_controller_artifacts_summary = artifacts_summary;
    payload.info.research_source_diagnostics_summary = diagnostics_summary;
    Json(payload.info).into_response()
}

pub async fn get_file_relationships(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    match state.files.load_file_relationships(filename).await {
        Ok(relationships) => Json(relationships).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn get_relationship_graph(
    State(state): State<Arc<ServerContext>>,
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
    match state
        .files
        .load_relationship_graph(filename, direction.to_string())
        .await
    {
        Ok(graph) => Json(graph).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn search_files(
    State(state): State<Arc<ServerContext>>,
    axum::extract::Query(query): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    match state.files.search_files(query.q).await {
        Ok(results) => Json(results).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}
