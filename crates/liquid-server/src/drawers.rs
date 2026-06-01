use crate::context::{service_error_status, ServerContext};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use liquid_protocol::{DrawerAssignment, DrawerPayload};
use std::sync::Arc;

pub async fn list_drawers(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    match state.drawers.list_drawers().await {
        Ok(drawers) => Json(drawers).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn create_drawer(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<DrawerPayload>,
) -> impl IntoResponse {
    match state.drawers.create_drawer(payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn update_drawer(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
    Json(payload): Json<DrawerPayload>,
) -> impl IntoResponse {
    match state.drawers.update_drawer(id, payload).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn delete_drawer(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match state.drawers.delete_drawer(id).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}

pub async fn update_file_drawer(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<DrawerAssignment>,
) -> impl IntoResponse {
    match state.drawers.update_file_drawer(filename, payload).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}
