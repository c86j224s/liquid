use crate::contracts::{TaskInfo, TaskMetadata};

pub(crate) fn is_retryable_task(task: &TaskInfo) -> bool {
    matches!(task.status.as_str(), "failed" | "interrupted")
        || (task.status == "completed"
            && matches!(
                task.quality_status.as_deref(),
                Some("untrusted" | "low_confidence" | "no_confidence")
            ))
}

pub(crate) fn retry_task_metadata(task: &TaskInfo) -> TaskMetadata {
    TaskMetadata {
        source_file_ids: task.source_file_ids.clone(),
        research_type: task.research_type.clone(),
        research_mode: task.research_mode.clone(),
        research_format: task.research_format.clone(),
        research_topic: task.research_topic.clone(),
        research_instructions: task.research_instructions.clone(),
        prompt_version: task.prompt_version.clone(),
        web_search_requested: task.web_search_requested.clone(),
        web_search_provider: task.web_search_provider.clone(),
        engine_preset_id: task.engine_preset_id,
        engine_preset_name: task.engine_preset_name.clone(),
        engine_kind: task.engine_kind.clone(),
        resolved_model: task.resolved_model.clone(),
        research_intensity: task.research_intensity.clone(),
        fallback_used: task.fallback_used.clone(),
        fallback_reason: task.fallback_reason.clone(),
        quality_max_iterations: task.quality_max_iterations,
        quality_depth: task.quality_depth.clone(),
    }
}

pub(crate) fn merge_retry_engine_metadata(target: &mut TaskMetadata, engine: TaskMetadata) {
    target.engine_preset_id = engine.engine_preset_id;
    target.engine_preset_name = engine.engine_preset_name;
    target.engine_kind = engine.engine_kind;
    target.resolved_model = engine.resolved_model;
    target.research_intensity = engine.research_intensity;
    target.fallback_used = engine.fallback_used;
    target.fallback_reason = engine.fallback_reason;
    target.web_search_requested = engine.web_search_requested;
    target.web_search_provider = engine.web_search_provider;
}
