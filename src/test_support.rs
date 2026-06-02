use crate::application::ports::classic_research_implementation;
use crate::application::server_adapters::build_server_context;
use crate::contracts::TaskUpdateEvent;
use crate::state::AppState;
use axum::{http::StatusCode, response::IntoResponse};
use liquid_runtime::cli_launcher::CliLaunchMode;
use liquid_server::context::ServerContext;
use sqlx::{sqlite::SqlitePool, Row};
use std::collections::HashMap;
use std::fs as std_fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Notify};
use uuid::Uuid;

pub(crate) fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("liquid-{}-{}", name, Uuid::new_v4()));
    std_fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn test_state(db: SqlitePool, uploads_path: PathBuf) -> Arc<AppState> {
    test_state_with_cli_launch_mode(db, uploads_path, CliLaunchMode::Auto)
}

fn test_state_with_historical_phase_engine_config(
    db: SqlitePool,
    uploads_path: PathBuf,
    historical_phase_engine: bool,
) -> Arc<AppState> {
    test_state_with_config(
        db,
        uploads_path,
        CliLaunchMode::Auto,
        historical_phase_engine,
    )
}

pub(crate) fn test_state_with_cli_launch_mode(
    db: SqlitePool,
    uploads_path: PathBuf,
    cli_launch_mode: CliLaunchMode,
) -> Arc<AppState> {
    test_state_with_config(db, uploads_path, cli_launch_mode, false)
}

fn test_state_with_config(
    db: SqlitePool,
    uploads_path: PathBuf,
    cli_launch_mode: CliLaunchMode,
    historical_phase_engine: bool,
) -> Arc<AppState> {
    let (tx, _) = broadcast::channel::<TaskUpdateEvent>(10);
    let data_dir = uploads_path.parent().unwrap_or(&uploads_path).to_path_buf();
    Arc::new(AppState {
        db,
        data_dir,
        uploads_path,
        tx,
        queue_notify: Notify::new(),
        active_tasks: tokio::sync::Mutex::new(HashMap::new()),
        ai_workers: 1,
        local_ai_workers: 1,
        ai_task_timeout_secs: 1200,
        cli_launch_mode,
        research_implementation_id: "classic",
        research_implementation: classic_research_implementation(),
        research_historical_phase_engine: historical_phase_engine,
        benchmark_fixture: None,
    })
}

pub(crate) fn test_state_with_historical_phase_engine(
    db: SqlitePool,
    uploads_path: PathBuf,
) -> Arc<AppState> {
    test_state_with_historical_phase_engine_config(db, uploads_path, true)
}

pub(crate) fn test_state_with_historical_phase_engine_enabled(
    db: SqlitePool,
    uploads_path: PathBuf,
    historical_phase_engine: bool,
) -> Arc<AppState> {
    test_state_with_historical_phase_engine_config(db, uploads_path, historical_phase_engine)
}

pub(crate) fn response_status(response: impl IntoResponse) -> StatusCode {
    response.into_response().status()
}

pub(crate) fn test_server_context(state: Arc<AppState>) -> Arc<ServerContext> {
    build_server_context(state)
}

pub(crate) async fn table_exists(db: &SqlitePool, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table)
    .fetch_one(db)
    .await
    .unwrap()
        > 0
}

pub(crate) async fn schema_snapshot(db: &SqlitePool) -> Vec<String> {
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(db)
    .await
    .unwrap();

    let mut snapshot = Vec::new();
    for table in tables {
        let pragma = format!("PRAGMA table_info({})", table);
        let columns = sqlx::query(&pragma)
            .fetch_all(db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                let name: String = row.get("name");
                let column_type: String = row.get("type");
                let not_null: i64 = row.get("notnull");
                let default_value: Option<String> = row.get("dflt_value");
                let primary_key: i64 = row.get("pk");
                format!(
                    "{}:{}:{}:{}:{}",
                    name,
                    column_type,
                    not_null,
                    default_value.unwrap_or_else(|| "NULL".to_string()),
                    primary_key
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        snapshot.push(format!("{}|{}", table, columns));
    }
    snapshot
}
