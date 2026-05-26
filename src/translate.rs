use crate::models::{ActionRequest, FileMetadata};
use crate::state::AppState;
use crate::tasks::run_ai_task;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

pub(crate) async fn translate_file(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<ActionRequest>,
) -> impl IntoResponse {
    let file = sqlx::query_as::<_, FileMetadata>("SELECT * FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
        .ok();
    if let Some(file) = file {
        let system = "You are a professional technical translator. PRESERVE all Markdown formatting. Output ONLY the translated Korean text.".to_string();
        run_ai_task(
            state,
            vec![file.id],
            vec![file.filename],
            file.original_name,
            payload.model,
            system,
            "Translate the following content to Korean:".to_string(),
            "[KO]",
            "md",
            vec![],
            None,
            None,
        )
        .await;
        StatusCode::ACCEPTED.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
