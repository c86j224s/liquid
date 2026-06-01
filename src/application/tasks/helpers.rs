use crate::contracts::{TaskInfo, TaskUpdateEvent};

pub(super) type TaskProgressSnapshot = (
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

pub(super) fn task_update_event(
    id: i64,
    status: impl Into<String>,
    original_name: impl Into<String>,
    task: Option<&TaskInfo>,
) -> TaskUpdateEvent {
    TaskUpdateEvent {
        id,
        status: status.into(),
        original_name: original_name.into(),
        quality_current_iteration: task.and_then(|task| task.quality_current_iteration),
        quality_max_iterations: task.and_then(|task| task.quality_max_iterations),
        quality_status: task.and_then(|task| task.quality_status.clone()),
        research_controller_stage: task.and_then(|task| task.research_controller_stage.clone()),
        research_controller_iteration: task.and_then(|task| task.research_controller_iteration),
        research_controller_max_iterations: task
            .and_then(|task| task.research_controller_max_iterations),
    }
}

pub(super) fn task_update_event_from_progress(
    id: i64,
    status: impl Into<String>,
    original_name: impl Into<String>,
    progress: Option<&TaskProgressSnapshot>,
) -> TaskUpdateEvent {
    TaskUpdateEvent {
        id,
        status: status.into(),
        original_name: original_name.into(),
        quality_current_iteration: progress.and_then(|(current, _, _, _, _, _)| *current),
        quality_max_iterations: progress.and_then(|(_, max, _, _, _, _)| *max),
        quality_status: progress.and_then(|(_, _, status, _, _, _)| status.clone()),
        research_controller_stage: progress.and_then(|(_, _, _, stage, _, _)| stage.clone()),
        research_controller_iteration: progress.and_then(|(_, _, _, _, iteration, _)| *iteration),
        research_controller_max_iterations: progress.and_then(|(_, _, _, _, _, max)| *max),
    }
}

pub(super) fn scrape_task_identity_name(scraped_title: &str, original_url: &str) -> String {
    let trimmed = scraped_title.trim();
    if trimmed.is_empty() {
        original_url.trim().to_string()
    } else {
        trimmed.to_string()
    }
}
