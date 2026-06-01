pub(crate) const TASK_CANCELLED_MESSAGE: &str = "Task was cancelled";

pub(super) const DELETE_CANCELLABLE_TASK_STATUSES: &[&str] = &[
    "queued",
    "processing",
    "translating",
    "researching",
    "scraping",
];
pub(crate) const DELETE_CANCELLABLE_TASK_STATUS_SQL_LIST: &str =
    "'queued', 'processing', 'translating', 'researching', 'scraping'";

pub(super) const ABORT_RECOVERY_TASK_STATUSES: &[&str] =
    &["processing", "translating", "researching", "scraping"];

pub(crate) fn is_delete_cancellable_task_status(status: &str) -> bool {
    DELETE_CANCELLABLE_TASK_STATUSES.contains(&status)
}

pub(super) fn is_abort_recovery_task_status(status: &str) -> bool {
    ABORT_RECOVERY_TASK_STATUSES.contains(&status)
}

pub(super) fn lifecycle_target_status(file_prefix: &str) -> &'static str {
    match file_prefix {
        "[KO]" => "translating",
        "[Research]" | "[AI-Research]" => "researching",
        "[Scrape]" | "[Scrape+KO]" => "scraping",
        _ => "processing",
    }
}
