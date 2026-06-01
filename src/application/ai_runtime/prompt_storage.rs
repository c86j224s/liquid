use super::*;

pub(super) async fn store_resolved_prompts(
    state: &AppState,
    task_id: i64,
    system_prompt: &str,
    user_prompt: &str,
) -> bool {
    let (stored_system_prompt, stored_user_prompt) =
        redact_resolved_prompts_for_storage(system_prompt, user_prompt);
    sqlx::query(
        "UPDATE tasks SET resolved_system_prompt = ?, resolved_user_prompt = ? WHERE id = ?",
    )
    .bind(stored_system_prompt)
    .bind(stored_user_prompt)
    .bind(task_id)
    .execute(&state.db)
    .await
    .map(|result| result.rows_affected() > 0)
    .unwrap_or(false)
}

pub(super) fn redact_resolved_prompts_for_storage(
    system_prompt: &str,
    user_prompt: &str,
) -> (String, String) {
    let stored_user_prompt = redact_resolved_user_prompt_for_storage(user_prompt);
    let stored_system_prompt =
        crate::research_design::redact_html_design_prompt_for_storage(system_prompt);
    if stored_user_prompt == user_prompt {
        return (stored_system_prompt, stored_user_prompt);
    }
    runtime_redact_resolved_prompts_for_storage(
        &stored_system_prompt,
        user_prompt,
        PromptStorageRedactionConfig {
            redacted_source_docs: RESOLVED_PROMPT_STORAGE_REDACTED_SOURCE_DOCS,
            redacted_source_pack: RESOLVED_PROMPT_STORAGE_REDACTED_SOURCE_PACK,
            redacted_system_prompt: RESOLVED_PROMPT_STORAGE_REDACTED_SYSTEM_PROMPT,
        },
    )
}

pub(super) fn redact_resolved_user_prompt_for_storage(user_prompt: &str) -> String {
    runtime_redact_resolved_user_prompt_for_storage(
        user_prompt,
        PromptStorageRedactionConfig {
            redacted_source_docs: RESOLVED_PROMPT_STORAGE_REDACTED_SOURCE_DOCS,
            redacted_source_pack: RESOLVED_PROMPT_STORAGE_REDACTED_SOURCE_PACK,
            redacted_system_prompt: RESOLVED_PROMPT_STORAGE_REDACTED_SYSTEM_PROMPT,
        },
    )
}
