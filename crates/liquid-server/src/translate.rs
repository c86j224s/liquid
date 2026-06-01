use crate::context::{service_error_status, ServerContext};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use liquid_protocol::ActionRequest;
use std::sync::Arc;

pub async fn translate_file(
    State(state): State<Arc<ServerContext>>,
    Path(filename): Path<String>,
    Json(payload): Json<ActionRequest>,
) -> impl IntoResponse {
    match state
        .translation
        .submit_translation(filename, payload.model)
        .await
    {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}
