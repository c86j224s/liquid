use crate::context::{service_error_status, ServerContext};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use liquid_protocol::{EnginePresetCreatePayload, EnginePresetUpdatePayload};
use std::sync::Arc;

pub async fn list_engine_presets(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    match state.engine_presets.list_engine_presets().await {
        Ok(presets) => Json(presets).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn create_engine_preset(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<EnginePresetCreatePayload>,
) -> impl IntoResponse {
    match state.engine_presets.create_engine_preset(payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn update_engine_preset(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
    Json(payload): Json<EnginePresetUpdatePayload>,
) -> impl IntoResponse {
    match state.engine_presets.update_engine_preset(id, payload).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn delete_engine_preset(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match state.engine_presets.delete_engine_preset(id).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn test_engine_preset(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match state.engine_presets.test_engine_preset(id).await {
        Ok((status, message)) => {
            Json(serde_json::json!({ "status": status, "message": message })).into_response()
        }
        Err(error) => service_error_status(error).into_response(),
    }
}
