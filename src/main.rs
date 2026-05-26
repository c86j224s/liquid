use clap::Parser;
use std::{collections::HashMap, sync::Arc};
use tokio::fs;
use tokio::sync::{broadcast, Notify};

mod ai_runtime;
mod app;
mod cli_launcher;
mod config;
mod db;
mod diagnostics;
mod drawers;
mod engine_presets;
mod files;
mod models;
mod pi_runtime;
mod research;
mod research_design;
mod research_quality;
mod research_sources;
mod scraping;
mod state;
mod tasks;
#[cfg(test)]
mod test_support;
mod translate;

use config::{setup_data_dir, Args};
use db::setup_db;
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    let data_dir = setup_data_dir(&args.data_dir)?;
    diagnostics::install_panic_hook(data_dir.join("crash-dumps"));
    let uploads_path = data_dir.join("uploads");
    if !uploads_path.exists() {
        fs::create_dir_all(&uploads_path).await?;
    }

    let db = setup_db(&data_dir).await?;
    let (tx, _) = broadcast::channel(100);
    let ai_workers = args.ai_workers.max(1);
    let local_ai_workers = args.local_ai_workers.max(1);
    let ai_task_timeout_secs = args.ai_task_timeout_secs.max(60);
    let state = Arc::new(AppState {
        db,
        data_dir: data_dir.clone(),
        uploads_path,
        tx,
        queue_notify: Notify::new(),
        active_tasks: tokio::sync::Mutex::new(HashMap::new()),
        ai_workers,
        local_ai_workers,
        ai_task_timeout_secs,
        cli_launch_mode: args.cli_launch_mode,
        benchmark_fixture: None,
    });
    tasks::spawn_ai_queue_workers(Arc::clone(&state), ai_workers, local_ai_workers);

    let app = app::build_router(state);

    let addr_str = format!("{}:{}", args.host, args.port);
    println!(
        "listening on {} with data in {} using {} cloud/CLI worker(s), {} local worker(s)",
        addr_str,
        data_dir.display(),
        ai_workers,
        local_ai_workers
    );
    diagnostics::log_lifecycle_event(
        "startup",
        &format!(
            "listening on {} data_dir={} ai_workers={} local_ai_workers={}",
            addr_str,
            data_dir.display(),
            ai_workers,
            local_ai_workers
        ),
    );
    let listener = tokio::net::TcpListener::bind(addr_str).await?;
    if let Err(err) = axum::serve(listener, app)
        .with_graceful_shutdown(diagnostics::shutdown_signal())
        .await
    {
        diagnostics::log_lifecycle_event("server_error", &err.to_string());
        return Err(Box::<dyn std::error::Error>::from(err));
    }
    diagnostics::log_lifecycle_event("shutdown", "server stopped");
    Ok(())
}
