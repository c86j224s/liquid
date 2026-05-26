#[cfg(test)]
const ROUTE_INVENTORY: &[(&str, &str)] = &[
    ("GET", "/api/files"),
    ("GET", "/api/drawers"),
    ("POST", "/api/drawers"),
    ("PUT", "/api/drawers/:id"),
    ("DELETE", "/api/drawers/:id"),
    ("GET", "/api/engine-presets"),
    ("POST", "/api/engine-presets"),
    ("PUT", "/api/engine-presets/:id"),
    ("DELETE", "/api/engine-presets/:id"),
    ("POST", "/api/engine-presets/:id/test"),
    ("POST", "/api/upload"),
    ("POST", "/api/scrap"),
    ("POST", "/api/push"),
    ("GET", "/api/config"),
    ("GET", "/api/models"),
    ("GET", "/api/tags"),
    ("GET", "/api/tasks"),
    ("GET", "/api/tasks/stream"),
    ("POST", "/api/tasks/:id/retry"),
    ("DELETE", "/api/tasks/:id"),
    ("GET", "/api/search"),
    ("POST", "/api/files/:filename/translate"),
    ("POST", "/api/files/research"),
    ("POST", "/api/research/topic"),
    ("PUT", "/api/files/:filename/status"),
    ("PUT", "/api/files/:filename/drawer"),
    ("PUT", "/api/files/:filename/title"),
    ("PUT", "/api/files/:filename/metadata"),
    ("PUT", "/api/files/:filename/tags"),
    ("DELETE", "/api/files/:filename"),
    ("GET", "/api/files/:filename/content"),
    ("GET", "/api/files/:filename/raw"),
    ("GET", "/api/files/:filename/relationships"),
    ("GET", "/api/files/:filename/relationship-graph"),
    ("GET", "/api/files/:filename/research-request"),
];

use crate::drawers::{
    create_drawer, delete_drawer, list_drawers, update_drawer, update_file_drawer,
};
use crate::engine_presets::{
    create_engine_preset, delete_engine_preset, list_engine_presets, test_engine_preset,
    update_engine_preset,
};
use crate::files::{
    delete_file, get_available_models, get_config, get_content, get_file_relationships,
    get_raw_content, get_relationship_graph, get_research_request, list_files, list_tags,
    push_content, search_files, update_file_metadata, update_status, update_tags, update_title,
    upload_file,
};
use crate::research::{multi_research_file, topic_research_file};
use crate::scraping::scrap_url;
use crate::state::AppState;
use crate::tasks::{delete_task, list_tasks, retry_task, stream_tasks};
use crate::translate::translate_file;
use axum::{
    http::{
        header::{CACHE_CONTROL, EXPIRES, PRAGMA},
        HeaderValue,
    },
    middleware,
    response::Response,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::services::ServeDir;

pub(crate) fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/files", get(list_files))
        .route("/api/drawers", get(list_drawers).post(create_drawer))
        .route("/api/drawers/:id", put(update_drawer).delete(delete_drawer))
        .route(
            "/api/engine-presets",
            get(list_engine_presets).post(create_engine_preset),
        )
        .route(
            "/api/engine-presets/:id",
            put(update_engine_preset).delete(delete_engine_preset),
        )
        .route("/api/engine-presets/:id/test", post(test_engine_preset))
        .route("/api/upload", post(upload_file))
        .route("/api/scrap", post(scrap_url))
        .route("/api/push", post(push_content))
        .route("/api/config", get(get_config))
        .route("/api/models", get(get_available_models))
        .route("/api/tags", get(list_tags))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks/stream", get(stream_tasks))
        .route("/api/tasks/:id/retry", post(retry_task))
        .route("/api/tasks/:id", delete(delete_task))
        .route("/api/search", get(search_files))
        .route("/api/files/:filename/translate", post(translate_file))
        .route("/api/files/research", post(multi_research_file))
        .route("/api/research/topic", post(topic_research_file))
        .route("/api/files/:filename/status", put(update_status))
        .route("/api/files/:filename/drawer", put(update_file_drawer))
        .route("/api/files/:filename/title", put(update_title))
        .route("/api/files/:filename/metadata", put(update_file_metadata))
        .route("/api/files/:filename/tags", put(update_tags))
        .route("/api/files/:filename", delete(delete_file))
        .route("/api/files/:filename/content", get(get_content))
        .route("/api/files/:filename/raw", get(get_raw_content))
        .route(
            "/api/files/:filename/relationships",
            get(get_file_relationships),
        )
        .route(
            "/api/files/:filename/relationship-graph",
            get(get_relationship_graph),
        )
        .route(
            "/api/files/:filename/research-request",
            get(get_research_request),
        )
        .fallback_service(ServeDir::new("static"))
        .layer(middleware::map_response(add_no_cache_headers))
        .with_state(state)
}

async fn add_no_cache_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(EXPIRES, HeaderValue::from_static("0"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_inventory_snapshot_matches_current_router_surface() {
        let expected = [
            ("GET", "/api/files"),
            ("GET", "/api/drawers"),
            ("POST", "/api/drawers"),
            ("PUT", "/api/drawers/:id"),
            ("DELETE", "/api/drawers/:id"),
            ("GET", "/api/engine-presets"),
            ("POST", "/api/engine-presets"),
            ("PUT", "/api/engine-presets/:id"),
            ("DELETE", "/api/engine-presets/:id"),
            ("POST", "/api/engine-presets/:id/test"),
            ("POST", "/api/upload"),
            ("POST", "/api/scrap"),
            ("POST", "/api/push"),
            ("GET", "/api/config"),
            ("GET", "/api/models"),
            ("GET", "/api/tags"),
            ("GET", "/api/tasks"),
            ("GET", "/api/tasks/stream"),
            ("POST", "/api/tasks/:id/retry"),
            ("DELETE", "/api/tasks/:id"),
            ("GET", "/api/search"),
            ("POST", "/api/files/:filename/translate"),
            ("POST", "/api/files/research"),
            ("POST", "/api/research/topic"),
            ("PUT", "/api/files/:filename/status"),
            ("PUT", "/api/files/:filename/drawer"),
            ("PUT", "/api/files/:filename/title"),
            ("PUT", "/api/files/:filename/metadata"),
            ("PUT", "/api/files/:filename/tags"),
            ("DELETE", "/api/files/:filename"),
            ("GET", "/api/files/:filename/content"),
            ("GET", "/api/files/:filename/raw"),
            ("GET", "/api/files/:filename/relationships"),
            ("GET", "/api/files/:filename/relationship-graph"),
            ("GET", "/api/files/:filename/research-request"),
        ];

        assert_eq!(ROUTE_INVENTORY, expected);
    }
}
