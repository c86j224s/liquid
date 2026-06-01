pub fn build_ko_translation_prompt(content: &str, safe_user_prompt: &str) -> String {
    format!(
        "### Content to Translate:\n{}\n\n### Instructions:\n{}\n\n### Output Requirements:\n- Output Korean-only Markdown.\n- Preserve all Markdown formatting and document order.\n- Preserve code blocks, inline code, API names, identifiers, command names, URLs, and version numbers exactly.\n- Do not summarize, omit sections, add commentary, or include the original English prose except for code/API identifiers.",
        content, safe_user_prompt
    )
}

pub fn build_ko_translation_chunk_prompt(
    chunk: &str,
    chunk_number: usize,
    total_chunks: usize,
    safe_user_prompt: &str,
) -> String {
    format!(
        "### Chunk {chunk_number} of {total_chunks} to Translate:\n{chunk}\n\n### Instructions:\n{}\n\n### Output Requirements:\n- Translate only this chunk.\n- Output Korean-only Markdown for this chunk.\n- Preserve headings, tables, lists, links, code fences, inline code, API names, identifiers, command names, URLs, and version numbers exactly.\n- Do not summarize, omit, reorder, add introductions, add conclusions, or mention chunk numbers.\n- If a sentence continues context from another chunk, translate only the text present here without inventing missing text.",
        safe_user_prompt
    )
}

pub fn ko_chunk_instruction_summary(safe_user_prompt: &str) -> String {
    format!(
        "{} Korean-only Markdown, preserving code/API identifiers and document order.",
        safe_user_prompt
    )
}
