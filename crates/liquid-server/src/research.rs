use crate::context::{service_error_status, ServerContext};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use liquid_protocol::{MultiResearchRequest, TopicResearchRequest};
use std::sync::Arc;

pub async fn multi_research_file(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<MultiResearchRequest>,
) -> impl IntoResponse {
    match state.research.submit_multi_research(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn topic_research_file(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<TopicResearchRequest>,
) -> impl IntoResponse {
    match state.research.submit_topic_research(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}
