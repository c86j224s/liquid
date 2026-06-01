use super::*;
use crate::application::ports::ResearchContextPackRequest;

#[derive(Debug, Default)]
pub(super) struct PreparedAiContext {
    pub(super) content: String,
    pub(super) diagnostics: Option<ResearchContextPackingDiagnostics>,
}

pub(super) async fn prepare_ai_context(
    state: &AppState,
    task_id: i64,
    filenames: Vec<String>,
    file_prefix: &str,
    user_prompt: &str,
) -> PreparedAiContext {
    let combined_content = load_raw_ai_context(state, filenames).await;
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return PreparedAiContext {
            content: combined_content,
            diagnostics: None,
        };
    }

    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT research_controller_artifacts_json, research_source_diagnostics_json FROM tasks WHERE id = ?",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some((artifacts_json, diagnostics_json)) = row else {
        return PreparedAiContext {
            content: combined_content.clone(),
            diagnostics: Some(
                state
                    .research_implementation
                    .raw_fallback_context_diagnostics(&combined_content),
            ),
        };
    };
    let artifacts = artifacts_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<ResearchControllerArtifacts>(json).ok());
    let diagnostics = diagnostics_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<ResearchSourceDiagnosticsEnvelope>(json).ok());

    match artifacts {
        Some(artifacts)
            if !artifacts.source_cards.is_empty()
                || !artifacts.claim_log.is_empty()
                || !artifacts.research_debt.is_empty()
                || !artifacts.conflict_map.is_empty() =>
        {
            let (content, packing_diagnostics) = state
                .research_implementation
                .build_research_context_pack(ResearchContextPackRequest {
                    user_prompt,
                    raw_documents: &combined_content,
                    artifacts: &artifacts,
                    diagnostics: diagnostics.as_ref(),
                    source_card_limit: RESEARCH_CONTEXT_SOURCE_CARD_LIMIT,
                    excerpt_chars: RESEARCH_CONTEXT_EXCERPT_CHARS,
                    prompt_safe_ledger_text_chars: PROMPT_SAFE_LEDGER_TEXT_CHARS,
                    prompt_safe_ledger_list_items: PROMPT_SAFE_LEDGER_LIST_ITEMS,
                    historical_narrative_state_prompt_hint: HISTORICAL_NARRATIVE_STATE_PROMPT_HINT,
                    historical_reader_quality_prompt_hint: HISTORICAL_READER_QUALITY_PROMPT_HINT,
                });
            PreparedAiContext {
                content,
                diagnostics: Some(packing_diagnostics),
            }
        }
        _ => PreparedAiContext {
            content: combined_content.clone(),
            diagnostics: Some(
                state
                    .research_implementation
                    .raw_fallback_context_diagnostics(&combined_content),
            ),
        },
    }
}

async fn load_raw_ai_context(state: &AppState, filenames: Vec<String>) -> String {
    let mut combined_content = String::new();
    for fname in filenames {
        let path = state.uploads_path.join(&fname);
        if let Ok(c) = fs::read_to_string(path).await {
            let clean_content = if fname.ends_with(".html") {
                strip_html(&c)
            } else {
                c
            };
            combined_content.push_str(&format!(
                "\n[Document: {}]\n{}\n",
                fname,
                sanitize_input(&clean_content)
            ));
        }
    }
    combined_content
}

#[cfg(test)]
pub(super) fn build_research_context_pack(
    user_prompt: &str,
    raw_documents: &str,
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> (String, ResearchContextPackingDiagnostics) {
    crate::application::ports::classic_research_implementation().build_research_context_pack(
        ResearchContextPackRequest {
            user_prompt,
            raw_documents,
            artifacts,
            diagnostics,
            source_card_limit: RESEARCH_CONTEXT_SOURCE_CARD_LIMIT,
            excerpt_chars: RESEARCH_CONTEXT_EXCERPT_CHARS,
            prompt_safe_ledger_text_chars: PROMPT_SAFE_LEDGER_TEXT_CHARS,
            prompt_safe_ledger_list_items: PROMPT_SAFE_LEDGER_LIST_ITEMS,
            historical_narrative_state_prompt_hint: HISTORICAL_NARRATIVE_STATE_PROMPT_HINT,
            historical_reader_quality_prompt_hint: HISTORICAL_READER_QUALITY_PROMPT_HINT,
        },
    )
}

#[cfg(test)]
pub(super) fn raw_fallback_context_diagnostics(
    raw_documents: &str,
) -> ResearchContextPackingDiagnostics {
    crate::application::ports::classic_research_implementation()
        .raw_fallback_context_diagnostics(raw_documents)
}
