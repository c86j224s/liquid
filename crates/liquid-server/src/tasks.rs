use crate::context::{service_error_status, ServerContext};
use crate::presenters::{public_task_summaries, public_task_update_event};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    Json,
};
use futures::stream::Stream;
use liquid_protocol::{RetryTaskPayload, TaskUpdateEvent};
use std::{convert::Infallible, sync::Arc};

pub async fn list_tasks(State(state): State<Arc<ServerContext>>) -> impl IntoResponse {
    match state.tasks.list_tasks().await {
        Ok(tasks) => Json(public_task_summaries(tasks)).into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn stream_tasks(
    State(state): State<Arc<ServerContext>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.tasks.subscribe_updates();
    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            if let Some(data) = serialize_public_task_update_event(event) {
                yield Ok(Event::default().data(data));
            }
        }
    };
    Sse::new(stream)
}

fn serialize_public_task_update_event(event: TaskUpdateEvent) -> Option<String> {
    serde_json::to_string(&public_task_update_event(event)).ok()
}

pub async fn retry_task(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
    payload: Option<Json<RetryTaskPayload>>,
) -> impl IntoResponse {
    let payload = payload.map(|Json(payload)| payload).unwrap_or_default();
    match state.tasks.retry_task(id, payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => service_error_status(error).into_response(),
    }
}

pub async fn delete_task(
    State(state): State<Arc<ServerContext>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match state.tasks.delete_task(id).await {
        Ok(()) => StatusCode::OK,
        Err(error) => service_error_status(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_stream_serialization_redacts_private_original_name_urls() {
        let payload = serialize_public_task_update_event(TaskUpdateEvent {
            id: 7,
            status: "queued".to_string(),
            original_name: "http://169.254.169.254/latest/meta-data/".to_string(),
            quality_current_iteration: Some(1),
            quality_max_iterations: Some(3),
            quality_status: Some("running".to_string()),
            research_controller_stage: Some("collect".to_string()),
            research_controller_iteration: Some(1),
            research_controller_max_iterations: Some(3),
        })
        .expect("serialized event");

        let value: serde_json::Value = serde_json::from_str(&payload).expect("json");
        assert_eq!(value["original_name"], "<redacted-private-url>");
        assert_eq!(value["id"], 7);
        assert_eq!(value["status"], "queued");
    }
}
