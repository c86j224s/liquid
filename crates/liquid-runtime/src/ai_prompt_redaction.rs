#[derive(Debug, Clone, Copy)]
pub struct PromptStorageRedactionConfig<'a> {
    pub redacted_source_docs: &'a str,
    pub redacted_source_pack: &'a str,
    pub redacted_system_prompt: &'a str,
}

pub fn redact_resolved_prompts_for_storage(
    system_prompt: &str,
    user_prompt: &str,
    config: PromptStorageRedactionConfig<'_>,
) -> (String, String) {
    let stored_user_prompt = redact_resolved_user_prompt_for_storage(user_prompt, config);
    if stored_user_prompt == user_prompt {
        return (system_prompt.to_string(), stored_user_prompt);
    }
    (
        config.redacted_system_prompt.to_string(),
        stored_user_prompt,
    )
}

pub fn redact_resolved_user_prompt_for_storage(
    user_prompt: &str,
    config: PromptStorageRedactionConfig<'_>,
) -> String {
    let source_pack_markers = [
        "### Pre-Collected Evidence Bundle:",
        "[PRE-COLLECTED SOURCE PACK]",
    ];
    let source_docs_marker = "### SOURCE DOCUMENTS";
    let reminder_marker = "### Final Pre-Collected Evidence Reminder:";
    let source_pack_start = source_pack_markers
        .iter()
        .filter_map(|marker| user_prompt.find(marker).map(|idx| (idx, *marker)))
        .min_by_key(|(idx, _)| *idx);
    let source_docs_start = user_prompt.find(source_docs_marker);
    let reminder_start = user_prompt.find(reminder_marker);
    let contains_redactable_source_material =
        source_pack_start.is_some() || source_docs_start.is_some() || reminder_start.is_some();
    if !contains_redactable_source_material {
        return user_prompt.to_string();
    }

    let redaction_start = [
        source_pack_start.map(|(idx, _)| idx),
        source_docs_start,
        reminder_start,
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(user_prompt.len());
    let safe_prefix = user_prompt[..redaction_start].trim();
    let mut sections = Vec::new();
    if let Some(user_request) = extract_markdown_heading_block(safe_prefix, "### User Request:") {
        sections.push(format!("### User Request:\n{}", user_request.trim()));
    } else if !safe_prefix.is_empty() {
        sections.push(safe_prefix.to_string());
    }
    if source_docs_start.is_some() {
        sections.push(format!(
            "### SOURCE DOCUMENTS (REDACTED FOR STORAGE):\n{}",
            config.redacted_source_docs
        ));
    }
    if let Some((_, marker)) = source_pack_start {
        sections.push(format!("{}\n{}", marker, config.redacted_source_pack));
    }
    if reminder_start.is_some() {
        sections.push(
            "### Final Pre-Collected Evidence Reminder:\n[redacted storage-safe reminder only]"
                .to_string(),
        );
    }
    sections
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn extract_markdown_heading_block(text: &str, heading: &str) -> Option<String> {
    let start = text.find(heading)?;
    let after = &text[start + heading.len()..];
    let end = after.find("\n### ").unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}
