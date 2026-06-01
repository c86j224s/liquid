use crate::context::ServerContext;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use liquid_protocol::ScrapRequest;
use std::sync::Arc;

pub async fn scrap_url(
    State(state): State<Arc<ServerContext>>,
    Json(payload): Json<ScrapRequest>,
) -> impl IntoResponse {
    match state.scraping.submit_scrape(payload).await {
        Ok(task_id) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "task_id": task_id })),
        )
            .into_response(),
        Err(crate::context::ScrapeSubmissionError::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, message).into_response()
        }
        Err(crate::context::ScrapeSubmissionError::Storage) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
