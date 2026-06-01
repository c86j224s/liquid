use crate::context::LoadedFileContent;
use liquid_protocol::{TaskInfo, TaskSummary, TaskUpdateEvent};
use liquid_research_core::normalize_result_url;
use pulldown_cmark::{html, Options, Parser as MarkdownParser};
use url::Url;

use super::files::redact_public_diagnostic_text;

pub fn public_task_summaries(tasks: Vec<TaskInfo>) -> Vec<TaskSummary> {
    tasks.into_iter().map(public_task_summary).collect()
}

pub fn public_task_update_event(event: TaskUpdateEvent) -> TaskUpdateEvent {
    TaskUpdateEvent {
        original_name: public_task_label(&event.original_name),
        ..event
    }
}

fn public_task_summary(task: TaskInfo) -> TaskSummary {
    TaskSummary {
        id: task.id,
        file_id: task.file_id,
        filename: task.filename,
        original_name: public_task_label(&task.original_name),
        status: task.status,
        error_message: task
            .error_message
            .map(|message| truncate_public_task_message(&redact_public_diagnostic_text(&message))),
        created_at: task.created_at,
        deleted_at: task.deleted_at,
        model: task.model,
        source_filenames: task.source_filenames,
        file_prefix: task.file_prefix,
        file_type: task.file_type,
        cleanup_files: task.cleanup_files,
        research_type: task.research_type,
        research_mode: task.research_mode,
        research_format: task.research_format,
        research_topic: task.research_topic,
        prompt_version: task.prompt_version,
        web_search_requested: task.web_search_requested,
        web_search_provider: task.web_search_provider,
        engine_preset_id: task.engine_preset_id,
        engine_preset_name: task.engine_preset_name,
        engine_kind: task.engine_kind,
        resolved_model: task.resolved_model,
        research_intensity: task.research_intensity,
        fallback_used: task.fallback_used,
        fallback_reason: task.fallback_reason,
        quality_current_iteration: task.quality_current_iteration,
        quality_max_iterations: task.quality_max_iterations,
        quality_status: task.quality_status,
        quality_depth: task.quality_depth,
        quality_last_failure: task
            .quality_last_failure
            .map(|message| truncate_public_task_message(&redact_public_diagnostic_text(&message))),
        research_controller_stage: task.research_controller_stage,
        research_controller_iteration: task.research_controller_iteration,
        research_controller_max_iterations: task.research_controller_max_iterations,
    }
}

fn public_task_label(value: &str) -> String {
    let trimmed = value.trim();
    if let Ok(url) = Url::parse(trimmed) {
        if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
            return normalize_result_url(trimmed)
                .unwrap_or_else(|| "<redacted-private-url>".to_string());
        }
    }
    redact_public_diagnostic_text(value)
}

fn truncate_public_task_message(message: &str) -> String {
    const MAX_PUBLIC_TASK_MESSAGE_CHARS: usize = 600;
    let mut output = String::new();
    for (idx, ch) in message.chars().enumerate() {
        if idx >= MAX_PUBLIC_TASK_MESSAGE_CHARS {
            output.push('…');
            return output;
        }
        output.push(ch);
    }
    output
}

pub fn render_loaded_file_content(loaded: LoadedFileContent) -> String {
    let content_html = if loaded.file_type == "md" {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        let parser = MarkdownParser::new_ext(&loaded.content, options);
        let mut output = String::new();
        html::push_html(&mut output, parser);
        if loaded.has_research_request {
            output = wrap_research_verification_appendix(&output);
        }
        output
    } else {
        loaded.content
    };

    if loaded.file_type == "html" {
        return content_html;
    }

    format!(
        r#"<html><head>
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <style>
                * {{ box-sizing: border-box; }}
                body {{
                    color: white;
                    font-family: 'Pretendard', -apple-system, BlinkMacSystemFont, sans-serif;
                    line-height: 1.6;
                    padding: 20px;
                    background: transparent;
                    max-width: 900px;
                    margin: 0 auto;
                    overflow-x: hidden;
                    word-wrap: break-word;
                }}
                a {{ color: #00d2ff; text-decoration: none; font-weight: 600; text-shadow: 0 0 8px rgba(0, 210, 255, 0.4); transition: all 0.3s ease; }}
                a:hover {{ color: #fff; text-shadow: 0 0 15px rgba(0, 210, 255, 0.8); }}
                pre {{
                    background: rgba(0,0,0,0.3);
                    padding: 1rem;
                    border-radius: 12px;
                    overflow-x: auto;
                    border: 1px solid rgba(255,255,255,0.1);
                    max-width: 100%;
                    white-space: pre;
                }}
                code {{ font-family: 'Fira Code', monospace; background: rgba(255,255,255,0.1); padding: 0.2rem 0.4rem; border-radius: 4px; font-size: 0.9em; }}
                pre code {{ display: block; background: transparent; padding: 0; border-radius: 0; white-space: inherit; word-break: normal; overflow-wrap: normal; }}
                img {{ max-width: 100%; height: auto; border-radius: 12px; }}
                h1, h2, h3 {{ border-bottom: 1px solid rgba(255,255,255,0.1); padding-bottom: 0.3em; }}
                blockquote {{ border-left: 4px solid var(--accent-color, #6366f1); padding-left: 1em; color: rgba(255,255,255,0.7); font-style: italic; margin: 1.5em 0; }}
                table {{ display: block; width: 100%; max-width: 100%; overflow-x: auto; border-collapse: collapse; margin: 1.5rem 0; }}
                th, td {{ border: 1px solid rgba(255,255,255,0.16); padding: 0.55rem 0.75rem; text-align: left; vertical-align: top; }}
                th {{ background: rgba(255,255,255,0.12); font-weight: 700; }}
                tr:nth-child(even) td {{ background: rgba(255,255,255,0.04); }}
                .research-verification-appendix {{ margin-top: 3rem; border-top: 1px solid rgba(255,255,255,0.18); padding-top: 1rem; color: rgba(255,255,255,0.78); }}
                .research-verification-appendix > summary {{ cursor: pointer; list-style: none; display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.85rem 1rem; border: 1px solid rgba(255,255,255,0.16); border-radius: 8px; background: rgba(255,255,255,0.08); color: rgba(255,255,255,0.9); font-weight: 700; }}
                .research-verification-appendix > summary::-webkit-details-marker {{ display: none; }}
                .research-verification-appendix > summary::after {{ content: '펼치기'; font-size: 0.82rem; color: rgba(255,255,255,0.58); font-weight: 600; }}
                .research-verification-appendix[open] > summary::after {{ content: '접기'; }}
                .research-verification-body {{ padding-top: 1rem; font-size: 0.94rem; }}
                .research-verification-body h1, .research-verification-body h2, .research-verification-body h3 {{ color: rgba(255,255,255,0.82); }}
                .research-verification-body table {{ font-size: 0.9rem; }}

                @media (max-width: 600px) {{
                    body {{ padding: 15px; }}
                    h1 {{ font-size: 1.5rem; }}
                }}
                </style></head><body>{}</body></html>"#,
        content_html
    )
}

fn wrap_research_verification_appendix(content_html: &str) -> String {
    let Some(start) = find_verification_heading_start(content_html) else {
        return content_html.to_string();
    };
    if content_html[..start]
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .count()
        < 300
    {
        return content_html.to_string();
    }
    let main = content_html[..start].trim_end();
    let appendix = content_html[start..].trim_start();
    format!(
        r#"{main}
<details class="research-verification-appendix">
<summary>검증 부록, 출처, Claim Log</summary>
<div class="research-verification-body">
{appendix}
</div>
</details>"#
    )
}

fn find_verification_heading_start(content_html: &str) -> Option<usize> {
    let markers = [
        "검증 부록",
        "verification appendix",
        "출처 감사",
        "source audit",
        "source cards",
        "claim log",
        "주장 로그",
        "품질 게이트",
        "quality gate",
        "품질 점수",
        "score section",
        "고우선 검증",
        "high-priority verification",
        "conflict map",
        "ambiguity check",
        "resolution check",
        "research quality",
        "연구 품질",
        "품질 검토",
    ];
    let lower = content_html.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(rel_start) = lower[cursor..].find("<h") {
        let start = cursor + rel_start;
        let Some(level) = lower[start + 2..].chars().next() else {
            break;
        };
        if !matches!(level, '1'..='6') {
            cursor = start + 2;
            continue;
        }
        let Some(open_end_rel) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel + 1;
        let close_tag = format!("</h{level}>");
        let Some(close_rel) = lower[open_end..].find(&close_tag) else {
            break;
        };
        let close = open_end + close_rel;
        let heading_text = strip_html_tags(&content_html[open_end..close]).to_ascii_lowercase();
        if markers.iter().any(|marker| heading_text.contains(marker)) {
            return Some(start);
        }
        cursor = close + close_tag.len();
    }
    None
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn wraps_research_verification_sections_as_collapsed_appendix() {
        let html = "<h1>Final Answer</h1><p>본문 ".to_string()
            + &"충분한 독자용 내용 ".repeat(40)
            + "</p><h1>검증 부록</h1><h2>Claim Log</h2><table><tr><td>근거</td></tr></table>";

        let wrapped = render_loaded_file_content(LoadedFileContent {
            file_type: "md".to_string(),
            content: html,
            has_research_request: true,
        });

        assert!(wrapped.contains(r#"<details class="research-verification-appendix">"#));
        assert!(wrapped.contains("<summary>검증 부록, 출처, Claim Log</summary>"));
        assert!(wrapped.contains("<h1>검증 부록</h1>"));
        assert!(wrapped.find("<details").unwrap() < wrapped.find("<h1>검증 부록</h1>").unwrap());
    }

    #[test]
    fn does_not_wrap_short_documents_that_start_with_verification() {
        let html = "<h1>출처 감사</h1><table><tr><td>근거</td></tr></table>";

        let wrapped = render_loaded_file_content(LoadedFileContent {
            file_type: "html".to_string(),
            content: html.to_string(),
            has_research_request: true,
        });

        assert_eq!(wrapped, html);
    }

    #[test]
    fn public_task_summary_redacts_private_original_name_urls() {
        let summaries = public_task_summaries(vec![TaskInfo {
            id: 1,
            file_id: None,
            filename: None,
            original_name: "http://169.254.169.254/latest/meta-data/".to_string(),
            status: "queued".to_string(),
            error_message: None,
            created_at: Utc::now(),
            deleted_at: None,
            model: None,
            system_prompt: None,
            user_prompt: None,
            source_file_ids: None,
            source_filenames: None,
            file_prefix: None,
            file_type: None,
            cleanup_files: None,
            research_type: None,
            research_mode: None,
            research_format: None,
            research_topic: None,
            research_instructions: None,
            prompt_version: None,
            resolved_system_prompt: None,
            resolved_user_prompt: None,
            web_search_requested: None,
            web_search_provider: None,
            engine_preset_id: None,
            engine_preset_name: None,
            engine_kind: None,
            resolved_model: None,
            research_intensity: None,
            fallback_used: None,
            fallback_reason: None,
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            quality_depth: None,
            quality_last_failure: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
            research_controller_artifacts_json: None,
            research_source_diagnostics_json: None,
        }]);

        assert_eq!(summaries[0].original_name, "<redacted-private-url>");
    }

    #[test]
    fn public_task_summary_preserves_public_original_name_urls() {
        let summaries = public_task_summaries(vec![TaskInfo {
            id: 1,
            file_id: None,
            filename: None,
            original_name: "https://example.com/article".to_string(),
            status: "queued".to_string(),
            error_message: None,
            created_at: Utc::now(),
            deleted_at: None,
            model: None,
            system_prompt: None,
            user_prompt: None,
            source_file_ids: None,
            source_filenames: None,
            file_prefix: None,
            file_type: None,
            cleanup_files: None,
            research_type: None,
            research_mode: None,
            research_format: None,
            research_topic: None,
            research_instructions: None,
            prompt_version: None,
            resolved_system_prompt: None,
            resolved_user_prompt: None,
            web_search_requested: None,
            web_search_provider: None,
            engine_preset_id: None,
            engine_preset_name: None,
            engine_kind: None,
            resolved_model: None,
            research_intensity: None,
            fallback_used: None,
            fallback_reason: None,
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            quality_depth: None,
            quality_last_failure: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
            research_controller_artifacts_json: None,
            research_source_diagnostics_json: None,
        }]);

        assert_eq!(summaries[0].original_name, "https://example.com/article");
    }

    #[test]
    fn public_task_update_event_redacts_private_original_name_urls() {
        let event = public_task_update_event(TaskUpdateEvent {
            id: 1,
            status: "queued".to_string(),
            original_name: "http://169.254.169.254/latest/meta-data/".to_string(),
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
        });

        assert_eq!(event.original_name, "<redacted-private-url>");
    }
}
