use super::*;

pub(super) async fn execute_single_file_ko_translation(
    state: &AppState,
    task_id: i64,
    filename: &str,
    model_name: &str,
    source: &str,
    final_system_prompt: &str,
    safe_user_prompt: &str,
    allow_web_search: bool,
    persist_resolved_prompts: bool,
) -> Option<String> {
    let content = read_translation_source_content(state, filename).await?;
    let chunks = split_translation_chunks(&content, KO_CHUNK_TARGET_CHARS);
    if content.chars().count() <= KO_CHUNK_TRIGGER_CHARS || chunks.len() <= 1 {
        let final_user_prompt = build_ko_translation_prompt(&content, safe_user_prompt);
        let resolved_user_prompt = ko_single_file_translation_resolved_prompt_summary(
            filename,
            content.chars().count(),
            safe_user_prompt,
        );
        if persist_resolved_prompts
            && !store_resolved_prompts(state, task_id, final_system_prompt, &resolved_user_prompt)
                .await
        {
            return None;
        }
        return execute_ai_backend(
            state,
            task_id,
            model_name,
            source,
            final_system_prompt,
            &final_user_prompt,
            allow_web_search,
        )
        .await;
    }

    let resolved_user_prompt = format!(
        "Chunked Korean translation for {filename}: {} chunks, source length {} characters. Per chunk prompt: {}",
        chunks.len(),
        content.chars().count(),
        ko_chunk_instruction_summary(safe_user_prompt)
    );
    if persist_resolved_prompts
        && !store_resolved_prompts(state, task_id, final_system_prompt, &resolved_user_prompt).await
    {
        return None;
    }

    let mut translated_chunks = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_prompt =
            build_ko_translation_chunk_prompt(chunk, index + 1, chunks.len(), safe_user_prompt);
        let translated = execute_ai_backend(
            state,
            task_id,
            model_name,
            source,
            final_system_prompt,
            &chunk_prompt,
            allow_web_search,
        )
        .await?;
        if translated.trim().len() < 10 {
            let _ =
                sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
                    .bind(format!(
                        "Translation chunk {} returned too little output",
                        index + 1
                    ))
                    .bind(task_id)
                    .execute(&state.db)
                    .await;
            return None;
        }
        translated_chunks.push(translated.trim().to_string());
    }

    Some(translated_chunks.join("\n\n"))
}

async fn read_translation_source_content(state: &AppState, filename: &str) -> Option<String> {
    let path = state.uploads_path.join(filename);
    let content = fs::read_to_string(path).await.ok()?;
    let clean_content = if filename.ends_with(".html") {
        strip_html(&content)
    } else {
        content
    };
    Some(sanitize_input(&clean_content))
}

pub(super) fn build_ko_translation_prompt(content: &str, safe_user_prompt: &str) -> String {
    runtime_build_ko_translation_prompt(content, safe_user_prompt)
}

pub(super) fn build_ko_translation_chunk_prompt(
    chunk: &str,
    chunk_number: usize,
    total_chunks: usize,
    safe_user_prompt: &str,
) -> String {
    runtime_build_ko_translation_chunk_prompt(chunk, chunk_number, total_chunks, safe_user_prompt)
}

pub(super) fn ko_chunk_instruction_summary(safe_user_prompt: &str) -> String {
    runtime_ko_chunk_instruction_summary(safe_user_prompt)
}

pub(super) fn ko_single_file_translation_resolved_prompt_summary(
    filename: &str,
    source_chars: usize,
    safe_user_prompt: &str,
) -> String {
    format!(
        "Single-file Korean translation for {filename}: source length {source_chars} characters. Prompt summary: {}",
        ko_chunk_instruction_summary(safe_user_prompt)
    )
}

pub(crate) fn split_translation_chunks(content: &str, target_chars: usize) -> Vec<String> {
    let target_chars = target_chars.max(1_000);
    let blocks = markdown_boundary_blocks(content);
    let mut chunks = Vec::new();
    let mut current = String::new();

    for block in blocks {
        if block.chars().count() > target_chars {
            if !current.trim().is_empty() {
                chunks.push(current.trim_end().to_string());
                current.clear();
            }
            chunks.extend(split_oversized_block(&block, target_chars));
            continue;
        }
        let next_len = current.chars().count() + block.chars().count();
        if next_len > target_chars && !current.trim().is_empty() {
            chunks.push(current.trim_end().to_string());
            current.clear();
        }
        current.push_str(&block);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim_end().to_string());
    }

    if chunks.is_empty() {
        vec![content.to_string()]
    } else {
        chunks
    }
}

fn markdown_boundary_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let starts_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        if !in_fence && trimmed.starts_with('#') && !current.trim().is_empty() {
            blocks.push(std::mem::take(&mut current));
        }

        current.push_str(line);

        if starts_fence {
            in_fence = !in_fence;
        }
        if !in_fence && line.trim().is_empty() && !current.trim().is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn split_oversized_block(block: &str, target_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for unit in markdown_safe_split_units(block) {
        if current.chars().count() + unit.chars().count() > target_chars
            && !current.trim().is_empty()
        {
            chunks.push(current.trim_end().to_string());
            current.clear();
        }
        current.push_str(&unit);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim_end().to_string());
    }
    chunks
}

fn markdown_safe_split_units(block: &str) -> Vec<String> {
    let lines = block.split_inclusive('\n').collect::<Vec<_>>();
    let mut units = Vec::new();
    let mut current = String::new();
    let mut region = MarkdownRegion::Normal;

    for line in lines {
        let line_region = markdown_line_region(line, &region);
        let starts_new_region =
            region != MarkdownRegion::Normal && line_region != region && !current.trim().is_empty();
        let starts_table = region != MarkdownRegion::Table
            && line_region == MarkdownRegion::Table
            && !current.trim().is_empty();
        let starts_list = region != MarkdownRegion::List
            && line_region == MarkdownRegion::List
            && !current.trim().is_empty();
        let starts_indented_code = region != MarkdownRegion::IndentedCode
            && line_region == MarkdownRegion::IndentedCode
            && !current.trim().is_empty();
        if starts_new_region || starts_table || starts_list || starts_indented_code {
            units.push(std::mem::take(&mut current));
        }

        current.push_str(line);

        let previous_region = region;
        region = match (region, line_region) {
            (MarkdownRegion::FencedCode, MarkdownRegion::FencedCode) if is_fence_marker(line) => {
                MarkdownRegion::Normal
            }
            (MarkdownRegion::FencedCode, _) => MarkdownRegion::FencedCode,
            (_, MarkdownRegion::FencedCode) => MarkdownRegion::FencedCode,
            (_, next) => next,
        };

        if region == MarkdownRegion::Normal
            && previous_region == MarkdownRegion::FencedCode
            && is_fence_marker(line)
        {
            units.push(std::mem::take(&mut current));
            continue;
        }

        if region == MarkdownRegion::Normal && line.trim().is_empty() && !current.trim().is_empty()
        {
            units.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        units.push(current);
    }
    units
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkdownRegion {
    Normal,
    FencedCode,
    IndentedCode,
    Table,
    List,
}

fn markdown_line_region(line: &str, current_region: &MarkdownRegion) -> MarkdownRegion {
    if matches!(current_region, MarkdownRegion::FencedCode) {
        return MarkdownRegion::FencedCode;
    }
    if is_fence_marker(line) {
        return MarkdownRegion::FencedCode;
    }
    if line.starts_with("    ") || line.starts_with('\t') {
        return MarkdownRegion::IndentedCode;
    }
    if is_table_line(line) {
        return MarkdownRegion::Table;
    }
    if is_list_line(line)
        || matches!(current_region, MarkdownRegion::List) && is_list_continuation(line)
    {
        return MarkdownRegion::List;
    }
    MarkdownRegion::Normal
}

fn is_fence_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return true;
    }
    let Some((prefix, rest)) = trimmed.split_once(". ") else {
        return false;
    };
    !rest.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
}

fn is_list_continuation(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with("  ") || line.starts_with('\t')
}
