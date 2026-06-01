use crate::application::ports::ResearchImplementation;
use crate::contracts::ResearchSourcePackReport;
use crate::contracts::TaskUpdateEvent;
use liquid_runtime::cli_launcher::CliLaunchMode;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, Notify};
use tokio::task::AbortHandle;

#[derive(Debug, Clone, Default)]
pub(crate) struct BenchmarkFixture {
    pub(crate) attempt_outputs: Vec<String>,
    pub(crate) source_pack_report: Option<ResearchSourcePackReport>,
}

impl BenchmarkFixture {
    pub(crate) async fn output_for_task(&self, db: &SqlitePool, task_id: i64) -> Option<String> {
        let iteration = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
            "SELECT research_controller_iteration, quality_current_iteration FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|(controller_iteration, quality_iteration)| {
            controller_iteration
                .filter(|value| *value > 0)
                .or(quality_iteration.filter(|value| *value > 0))
                .unwrap_or(1)
        })
        .unwrap_or(1)
        .max(1) as usize;
        let index = iteration
            .saturating_sub(1)
            .min(self.attempt_outputs.len().saturating_sub(1));
        self.attempt_outputs.get(index).cloned()
    }

    pub(crate) fn source_pack_report(&self) -> Option<ResearchSourcePackReport> {
        self.source_pack_report.clone()
    }
}

pub(crate) struct AppState {
    pub(crate) db: SqlitePool,
    pub(crate) data_dir: PathBuf,
    pub(crate) uploads_path: PathBuf,
    pub(crate) tx: broadcast::Sender<TaskUpdateEvent>,
    pub(crate) queue_notify: Notify,
    pub(crate) active_tasks: Mutex<HashMap<i64, AbortHandle>>,
    pub(crate) ai_workers: usize,
    pub(crate) local_ai_workers: usize,
    pub(crate) ai_task_timeout_secs: u64,
    pub(crate) cli_launch_mode: CliLaunchMode,
    pub(crate) research_implementation_id: &'static str,
    pub(crate) research_implementation: Arc<dyn ResearchImplementation>,
    pub(crate) benchmark_fixture: Option<BenchmarkFixture>,
}
