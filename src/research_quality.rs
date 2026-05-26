use crate::models::{
    NarrativeState, ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchDebtItem,
    ResearchSourceCard, ResearchSourceDiagnosticsEnvelope,
};
use scraper::{Html as ParsedHtml, Selector};
use serde_json::{Map, Value};
use std::collections::HashSet;
use url::Url;

const MAX_RESEARCH_ARTIFACT_JSON_BYTES: usize = 24_000;
const MAX_RESEARCH_ARTIFACT_EVENTS: usize = 40;
const MAX_RESEARCH_ARTIFACT_ITEMS: usize = 24;
const MAX_RESEARCH_ARTIFACT_TEXT_ITEMS: usize = 8;
const MAX_RESEARCH_ARTIFACT_ID_CHARS: usize = 64;
const MAX_RESEARCH_ARTIFACT_URL_CHARS: usize = 240;
const MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS: usize = 64;
const MAX_RESEARCH_ARTIFACT_TEXT_CHARS: usize = 240;
const MAX_RESEARCH_ARTIFACT_LONG_TEXT_CHARS: usize = 480;
const MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS: usize = 6;
const MAX_OUTPUT_ARTIFACT_EVENTS: usize = 8;
const MAX_OUTPUT_ARTIFACT_DEBT_ITEMS: usize = 8;
const MAX_OUTPUT_ARTIFACT_WARNINGS: usize = 6;
const MAX_OUTPUT_ARTIFACT_SOURCE_CARDS: usize = 16;
const MAX_OUTPUT_ARTIFACT_CLAIMS: usize = 16;
const MAX_OUTPUT_ARTIFACT_CONFLICTS: usize = 8;
const MAX_OUTPUT_ARTIFACT_SOURCE_CARDS_AGGRESSIVE: usize = 8;
const MAX_OUTPUT_ARTIFACT_CLAIMS_AGGRESSIVE: usize = 8;
const MAX_OUTPUT_ARTIFACT_EVENT_CARDS: usize = 8;
const MAX_OUTPUT_ARTIFACT_EVENT_CARDS_AGGRESSIVE: usize = 6;
const SOURCE_AUDIT_MARKERS: &[&str] = &["source audit", "출처 감사"];
const SOURCE_CARD_MARKERS: &[&str] = &[
    "source cards",
    "source card",
    "출처 카드",
    "소스 카드",
    "출처 감사",
    "source audit",
];
const CLAIM_LOG_MARKERS: &[&str] = &[
    "claim log",
    "claim logs",
    "claim ledger",
    "claims and evidence",
    "claims & evidence",
    "주장 로그",
    "클레임 로그",
    "증거 매트릭스",
    "증거-주장 매트릭스",
    "근거-주장 매트릭스",
    "근거-주장",
];
const FINAL_ANSWER_MARKERS: &[&str] = &[
    "final answer",
    "최종 답변",
    "최종답변",
    "최종 보고서",
    "요약 결론",
    "결론 요약",
    "한 줄 결론",
    "최종 결론",
    "핵심 결론",
];
const QUALITY_GATE_MARKERS: &[&str] = &[
    "quality gate",
    "quality gates",
    "quality check",
    "quality review",
    "품질 게이트",
    "품질 검증",
    "품질 점검",
    "자체 품질 점검",
    "검증 보수 항목",
    "검증 항목",
    "품질 점수",
    "신뢰도 점수",
];
const REPAIR_HINT_BLOCK_LEAK_MARKERS: &[&str] =
    &["repair search hints", "not-yet-adopted evidence"];
const REPAIR_HINT_ROW_LABEL_MARKERS: &[&str] =
    &["query:", "provider:", "class:", "quality:", "snippet:"];
const EVENT_CARD_INTERNAL_LEAK_MARKERS: &[&str] = &[
    "historical event scaffold repair guidance",
    "event scaffold repair guidance",
    "event_cards",
    "source_ids",
    "region_or_front",
    "open_questions",
    "<event_cards>",
    "</event_cards>",
    "<card>",
    "</card>",
    "<source_ids>",
    "</source_ids>",
    "<region_or_front>",
    "</region_or_front>",
    "<open_questions>",
    "</open_questions>",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResearchQualityContext<'a> {
    pub(crate) file_prefix: &'a str,
    pub(crate) file_type: &'a str,
    pub(crate) web_search_requested: bool,
    pub(crate) research_intensity: Option<&'a str>,
    pub(crate) quality_depth: Option<&'a str>,
    pub(crate) research_topic: Option<&'a str>,
    pub(crate) research_instructions: Option<&'a str>,
    pub(crate) evidence_subject: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResearchQualityReport {
    pub(crate) source_url_count: usize,
    pub(crate) audit_url_count: usize,
    pub(crate) matched_topic_terms: usize,
    pub(crate) required_topic_terms: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchFinalizationDiagnostics {
    pub(crate) preserved_reader_prose: bool,
    pub(crate) synthesized_final_answer: bool,
    pub(crate) repaired_final_answer: bool,
    pub(crate) source_audit_row_count: usize,
    pub(crate) claim_log_row_count: usize,
    pub(crate) coverage_miss_count: usize,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResearchFinalizationResult {
    pub(crate) output: String,
    pub(crate) diagnostics: ResearchFinalizationDiagnostics,
}

pub(crate) fn parse_research_artifact_block(
    output: &str,
    file_type: &str,
) -> Result<ResearchControllerArtifacts, String> {
    let markdown_artifact_scan = scan_markdown_research_artifact_blocks(output);
    let html_artifact_scan = scan_html_research_artifact_blocks(output);
    if markdown_artifact_scan.malformed || html_artifact_scan.malformed {
        return Err("malformed machine-readable research artifact JSON block".to_string());
    }
    let markdown_artifact_json_blocks = markdown_artifact_scan.blocks;
    let html_artifact_json_blocks = html_artifact_scan.blocks;
    let artifact_block_count =
        markdown_artifact_json_blocks.len() + html_artifact_json_blocks.len();
    let artifact_json = match artifact_block_count {
        0 => return Err("missing machine-readable research artifact JSON block".to_string()),
        1 => if file_type == "html" {
            html_artifact_json_blocks
                .into_iter()
                .next()
                .or_else(|| markdown_artifact_json_blocks.into_iter().next())
        } else {
            markdown_artifact_json_blocks
                .into_iter()
                .next()
                .or_else(|| html_artifact_json_blocks.into_iter().next())
        }
        .expect("single artifact block must be present"),
        _ => {
            return Err(
                "multiple machine-readable research artifact JSON blocks are not allowed"
                    .to_string(),
            )
        }
    };
    if artifact_json.len() > MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return Err(format!(
            "research artifact JSON exceeds maximum size of {} bytes",
            MAX_RESEARCH_ARTIFACT_JSON_BYTES
        ));
    }
    let mut artifact_value = serde_json::from_str::<Value>(&artifact_json)
        .map_err(|error| format!("invalid research artifact JSON: {error}"))?;
    normalize_research_controller_artifact_value(&mut artifact_value)?;
    let mut artifacts = serde_json::from_value::<ResearchControllerArtifacts>(artifact_value)
        .map_err(|error| format!("invalid research artifact JSON: {error}"))?;
    if artifacts.version == 0 {
        return Err("invalid research artifact JSON: version must be >= 1".to_string());
    }
    normalize_research_controller_artifacts(&mut artifacts);
    Ok(artifacts)
}

pub(crate) fn prompt_safe_research_text(value: &str, limit: usize) -> String {
    serde_json::to_string(&normalize_prompt_text(value, limit))
        .unwrap_or_else(|_| "\"\"".to_string())
}

pub(crate) fn prompt_safe_research_optional_text(value: Option<&str>, limit: usize) -> String {
    value
        .map(|value| prompt_safe_research_text(value, limit))
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn prompt_safe_research_list(
    values: &[String],
    max_items: usize,
    item_limit: usize,
) -> String {
    let normalized = values
        .iter()
        .take(max_items)
        .map(|value| normalize_prompt_text(value, item_limit))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    serde_json::to_string(&normalized).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn render_narrative_state_prompt_block(
    narrative_state: Option<&NarrativeState>,
    max_chars: usize,
) -> Option<String> {
    let state = narrative_state?;
    let mut sections = Vec::new();

    if state.topic_frame.is_some()
        || state.working_thesis.is_some()
        || state.reader_promise.is_some()
    {
        sections.push(format!(
            "<topic_frame>{}</topic_frame>\n<working_thesis>{}</working_thesis>\n<reader_promise>{}</reader_promise>",
            prompt_safe_research_optional_text(state.topic_frame.as_deref(), 160),
            prompt_safe_research_optional_text(state.working_thesis.as_deref(), 160),
            prompt_safe_research_optional_text(state.reader_promise.as_deref(), 160),
        ));
    }
    if !state.timeline.is_empty() {
        sections.push(format!(
            "<timeline>\n{}\n</timeline>",
            state
                .timeline
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|event| {
                    format!(
                        "  <event id={}><label>{}</label><date_anchor>{}</date_anchor><significance>{}</significance><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></event>",
                        prompt_safe_research_text(&event.id, 48),
                        prompt_safe_research_text(&event.label, 160),
                        prompt_safe_research_optional_text(event.date_anchor.as_deref(), 96),
                        prompt_safe_research_optional_text(event.significance.as_deref(), 160),
                        prompt_safe_research_list(
                            &event.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &event.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.event_cards.is_empty() {
        sections.push(format!(
            "<event_cards>\n{}\n</event_cards>",
            state
                .event_cards
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|card| {
                    format!(
                        "  <card><label>{}</label><timeframe>{}</timeframe><actors>{}</actors><region_or_front>{}</region_or_front><trigger>{}</trigger><development>{}</development><outcome>{}</outcome><source_ids>{}</source_ids><confidence>{}</confidence><open_questions>{}</open_questions></card>",
                        prompt_safe_research_text(&card.label, 120),
                        prompt_safe_research_optional_text(card.timeframe.as_deref(), 96),
                        prompt_safe_research_list(
                            &card.actors,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            96,
                        ),
                        prompt_safe_research_optional_text(
                            card.region_or_front.as_deref(),
                            120,
                        ),
                        prompt_safe_research_optional_text(card.trigger.as_deref(), 160),
                        prompt_safe_research_optional_text(card.development.as_deref(), 200),
                        prompt_safe_research_optional_text(card.outcome.as_deref(), 160),
                        prompt_safe_research_list(
                            &card.source_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_optional_text(card.confidence.as_deref(), 32),
                        prompt_safe_research_list(
                            &card.open_questions,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            120,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.actors.is_empty() {
        sections.push(format!(
            "<actors>\n{}\n</actors>",
            state
                .actors
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|actor| {
                    format!(
                        "  <actor id={}><label>{}</label><role>{}</role><relevance>{}</relevance><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></actor>",
                        prompt_safe_research_text(&actor.id, 48),
                        prompt_safe_research_text(&actor.label, 120),
                        prompt_safe_research_optional_text(actor.role.as_deref(), 120),
                        prompt_safe_research_optional_text(actor.relevance.as_deref(), 160),
                        prompt_safe_research_list(
                            &actor.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &actor.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.causal_chain.is_empty() {
        sections.push(format!(
            "<causal_chain>\n{}\n</causal_chain>",
            state
                .causal_chain
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|link| {
                    format!(
                        "  <link id={}><cause>{}</cause><effect>{}</effect><rationale>{}</rationale><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></link>",
                        prompt_safe_research_text(&link.id, 48),
                        prompt_safe_research_text(&link.cause, 160),
                        prompt_safe_research_text(&link.effect, 160),
                        prompt_safe_research_optional_text(link.rationale.as_deref(), 160),
                        prompt_safe_research_list(
                            &link.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &link.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.evidence_layers.is_empty() {
        sections.push(format!(
            "<evidence_layers>\n{}\n</evidence_layers>",
            state
                .evidence_layers
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|layer| {
                    format!(
                        "  <layer id={}><label>{}</label><purpose>{}</purpose><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></layer>",
                        prompt_safe_research_text(&layer.id, 48),
                        prompt_safe_research_text(&layer.label, 120),
                        prompt_safe_research_optional_text(layer.purpose.as_deref(), 160),
                        prompt_safe_research_list(
                            &layer.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &layer.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.interpretive_tensions.is_empty() {
        sections.push(format!(
            "<interpretive_tensions>\n{}\n</interpretive_tensions>",
            state
                .interpretive_tensions
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|tension| {
                    format!(
                        "  <tension id={}><question>{}</question><competing_readings>{}</competing_readings><current_status>{}</current_status><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></tension>",
                        prompt_safe_research_text(&tension.id, 48),
                        prompt_safe_research_text(&tension.question, 160),
                        prompt_safe_research_optional_text(
                            tension.competing_readings.as_deref(),
                            160,
                        ),
                        prompt_safe_research_optional_text(
                            tension.current_status.as_deref(),
                            120,
                        ),
                        prompt_safe_research_list(
                            &tension.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &tension.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.impacts.is_empty() {
        sections.push(format!(
            "<impacts>\n{}\n</impacts>",
            state
                .impacts
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|impact| {
                    format!(
                        "  <impact id={}><label>{}</label><scope>{}</scope><implication>{}</implication><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></impact>",
                        prompt_safe_research_text(&impact.id, 48),
                        prompt_safe_research_text(&impact.label, 120),
                        prompt_safe_research_optional_text(impact.scope.as_deref(), 120),
                        prompt_safe_research_optional_text(impact.implication.as_deref(), 160),
                        prompt_safe_research_list(
                            &impact.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &impact.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.reader_questions.is_empty() {
        sections.push(format!(
            "<reader_questions>\n{}\n</reader_questions>",
            state
                .reader_questions
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|question| {
                    format!(
                        "  <question id={}><prompt>{}</prompt><answer_status>{}</answer_status><answer_plan>{}</answer_plan><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></question>",
                        prompt_safe_research_text(&question.id, 48),
                        prompt_safe_research_text(&question.question, 160),
                        prompt_safe_research_optional_text(
                            question.answer_status.as_deref(),
                            120,
                        ),
                        prompt_safe_research_optional_text(
                            question.answer_plan.as_deref(),
                            160,
                        ),
                        prompt_safe_research_list(
                            &question.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &question.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.section_outline.is_empty() {
        sections.push(format!(
            "<section_outline>\n{}\n</section_outline>",
            state
                .section_outline
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|section| {
                    format!(
                        "  <section id={}><heading>{}</heading><purpose>{}</purpose><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></section>",
                        prompt_safe_research_text(&section.id, 48),
                        prompt_safe_research_text(&section.heading, 120),
                        prompt_safe_research_optional_text(section.purpose.as_deref(), 160),
                        prompt_safe_research_list(
                            &section.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &section.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.transition_plan.is_empty() {
        sections.push(format!(
            "<transition_plan>\n{}\n</transition_plan>",
            state
                .transition_plan
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|transition| {
                    format!(
                        "  <transition id={}><from_section_id>{}</from_section_id><to_section_id>{}</to_section_id><bridge>{}</bridge></transition>",
                        prompt_safe_research_text(&transition.id, 48),
                        prompt_safe_research_optional_text(
                            transition.from_section_id.as_deref(),
                            48,
                        ),
                        prompt_safe_research_optional_text(
                            transition.to_section_id.as_deref(),
                            48,
                        ),
                        prompt_safe_research_text(&transition.bridge, 160),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !state.open_gaps.is_empty() {
        sections.push(format!(
            "<open_gaps>\n{}\n</open_gaps>",
            state
                .open_gaps
                .iter()
                .take(MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS)
                .map(|gap| {
                    format!(
                        "  <gap id={}><gap_type>{}</gap_type><description>{}</description><status>{}</status><expected_claim_log_ids>{}</expected_claim_log_ids><expected_source_card_ids>{}</expected_source_card_ids></gap>",
                        prompt_safe_research_text(&gap.id, 48),
                        prompt_safe_research_text(&gap.gap_type, 96),
                        prompt_safe_research_text(&gap.description, 160),
                        prompt_safe_research_optional_text(gap.status.as_deref(), 96),
                        prompt_safe_research_list(
                            &gap.expected_claim_log_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                        prompt_safe_research_list(
                            &gap.expected_source_card_ids,
                            MAX_RESEARCH_NARRATIVE_PROMPT_ITEMS,
                            48,
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if state.last_iteration_summary.is_some() {
        sections.push(format!(
            "<last_iteration_summary>{}</last_iteration_summary>",
            prompt_safe_research_optional_text(state.last_iteration_summary.as_deref(), 160),
        ));
    }

    if sections.is_empty() {
        return None;
    }

    let block = format!(
        "<narrative_state role=\"outline_only_not_evidence\">\n{}\n</narrative_state>",
        sections.join("\n")
    );
    if block.chars().count() <= max_chars {
        return Some(block);
    }

    let truncation_target = max_chars.saturating_sub(64);
    let mut truncated = block.chars().take(truncation_target).collect::<String>();
    truncated.push_str("\n...[truncated narrative state for prompt budget]\n</narrative_state>");
    Some(truncated)
}

pub(crate) fn finalize_research_output(
    draft: &str,
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
) -> ResearchFinalizationResult {
    let mut finalization = ResearchFinalizationDiagnostics::default();
    let output = if context.file_type == "html" {
        finalize_html_research_output(draft, artifacts, diagnostics, context, &mut finalization)
    } else {
        finalize_markdown_research_output(draft, artifacts, diagnostics, context, &mut finalization)
    };
    ResearchFinalizationResult {
        output,
        diagnostics: finalization,
    }
}

fn finalize_markdown_research_output(
    draft: &str,
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
    finalization: &mut ResearchFinalizationDiagnostics,
) -> String {
    let reader_body = extract_markdown_reader_body(draft);
    let repaired_final_answer = render_markdown_final_answer(
        final_answer_section(&reader_body),
        &reader_body,
        artifacts,
        diagnostics,
        context,
        finalization,
    );
    let main_body = if let Some(section) = final_answer_section(&reader_body) {
        let replaced = reader_body.replacen(section, &repaired_final_answer, 1);
        if replaced.trim().is_empty() {
            repaired_final_answer.clone()
        } else {
            replaced.trim().to_string()
        }
    } else if reader_body.trim().is_empty() {
        repaired_final_answer.clone()
    } else {
        format!("{}\n\n{}", repaired_final_answer, reader_body.trim())
    };
    let main_body = normalize_finalized_markdown_main_body(&main_body);
    let appendix = render_markdown_verification_appendix(artifacts, diagnostics, finalization);
    format!("{}\n\n{}", main_body.trim(), appendix)
}

fn finalize_html_research_output(
    draft: &str,
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
    finalization: &mut ResearchFinalizationDiagnostics,
) -> String {
    let markdown =
        finalize_markdown_research_output(draft, artifacts, diagnostics, context, finalization);
    let visible = strip_research_artifact_blocks(&markdown);
    let final_answer = final_answer_section(&visible)
        .map(section_body_without_heading)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| synthesize_final_answer(artifacts, diagnostics, context));
    let source_audit_rows = render_html_source_audit_rows(artifacts);
    let claim_rows = render_html_claim_log_rows(artifacts);
    let limits = render_html_limits_section(artifacts, diagnostics, finalization);
    let quality_gate = render_html_quality_gate(artifacts);
    let artifact_json = pretty_research_artifact_json(artifacts);
    format!(
        "<!doctype html><html lang=\"ko\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Research Finalization</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif;line-height:1.6;margin:2rem auto;max-width:980px;padding:0 1rem;color:#111827;background:#f8fafc;}}section{{background:#fff;border:1px solid #e5e7eb;border-radius:14px;padding:1.25rem 1.5rem;margin-bottom:1rem;}}table{{width:100%;border-collapse:collapse;font-size:0.95rem;}}th,td{{border:1px solid #d1d5db;padding:0.55rem;vertical-align:top;text-align:left;}}th{{background:#f3f4f6;}}code{{word-break:break-all;}}</style></head><body><section><h2>최종 답변 (Final Answer)</h2><p>{}</p></section><section><h1>검증 부록</h1><h2>출처 감사 (Source Audit)</h2><table><thead><tr><th>ID</th><th>URL</th><th>Source</th><th>Class</th><th>Checked Fact</th><th>Limitation</th><th>Diagnostics</th></tr></thead><tbody>{}</tbody></table><h2>주장 로그 (Claim Log)</h2><table><thead><tr><th>ID</th><th>Claim</th><th>Support</th><th>Confidence</th><th>Uncertainty</th></tr></thead><tbody>{}</tbody></table><h2>한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)</h2>{}<h2>품질 게이트 (Quality Gate)</h2>{}</section><script type=\"application/json\" data-research-artifacts>{}</script></body></html>",
        html_paragraphs(&final_answer),
        source_audit_rows,
        claim_rows,
        limits,
        quality_gate,
        escape_html(&artifact_json),
    )
}

fn render_markdown_final_answer(
    existing_section: Option<&str>,
    reader_body: &str,
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
    finalization: &mut ResearchFinalizationDiagnostics,
) -> String {
    let existing_body = existing_section
        .map(section_body_without_heading)
        .unwrap_or_default();
    let reader_text = if existing_body.is_empty() {
        fallback_reader_text(reader_body)
    } else {
        existing_body.clone()
    };
    let preserve_existing = !reader_text.is_empty();
    let mut final_answer = if preserve_existing {
        finalization.preserved_reader_prose = true;
        reader_text
    } else {
        finalization.synthesized_final_answer = true;
        synthesize_final_answer(artifacts, diagnostics, context)
    };
    final_answer = normalize_reader_markdown_heading_boundaries(&final_answer);
    let repaired_section = format!("## 최종 답변 (Final Answer)\n\n{}", final_answer.trim());
    if final_answer_needs_repair(&repaired_section, context) {
        finalization.repaired_final_answer = true;
        let supplement = synthesize_resolution_repair_paragraph(artifacts, diagnostics, context);
        if !supplement.is_empty() && !final_answer.contains(&supplement) {
            if !final_answer.trim().is_empty() {
                final_answer.push_str("\n\n");
            }
            final_answer.push_str(&supplement);
        }
    }
    final_answer = normalize_reader_markdown_heading_boundaries(&final_answer);
    format!("## 최종 답변 (Final Answer)\n\n{}", final_answer.trim())
}

fn render_markdown_verification_appendix(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    finalization: &mut ResearchFinalizationDiagnostics,
) -> String {
    finalization.source_audit_row_count = artifacts.source_cards.len();
    finalization.claim_log_row_count = artifacts.claim_log.len();
    finalization.coverage_miss_count = diagnostics
        .and_then(|envelope| envelope.source_pack.as_ref())
        .map(|report| report.coverage_misses.len())
        .unwrap_or_default();
    format!(
        "# 검증 부록\n\n## 출처 감사 (Source Audit)\n| ID | URL | Source | Class | Checked Fact | Limitation | Diagnostics |\n| --- | --- | --- | --- | --- | --- | --- |\n{}\n## 주장 로그 (Claim Log)\n| ID | Claim | Support | Confidence | Uncertainty |\n| --- | --- | --- | --- | --- |\n{}\n## 한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)\n{}\n## 품질 게이트 (Quality Gate)\n{}\n[RESEARCH_ARTIFACT_JSON]\n```json\n{}\n```",
        render_markdown_source_audit_rows(artifacts),
        render_markdown_claim_log_rows(artifacts),
        render_markdown_limits_section(artifacts, diagnostics, finalization),
        render_markdown_quality_gate(artifacts),
        pretty_research_artifact_json(artifacts),
    )
}

pub(crate) fn validate_research_artifacts(
    artifacts: &ResearchControllerArtifacts,
    research_intensity: Option<&str>,
    quality_depth: Option<&str>,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    let valid_source_card_ids = artifacts
        .source_cards
        .iter()
        .filter_map(|card| {
            let id = card.id.trim();
            if id.is_empty() {
                return None;
            }
            if valid_http_source_url(&card.url) {
                Some(id.to_string())
            } else {
                failures.push(format!(
                    "source card {} has a non-resolvable URL {}",
                    card.id, card.url
                ));
                None
            }
        })
        .collect::<HashSet<_>>();

    for claim in &artifacts.claim_log {
        if claim.support_source_card_ids.is_empty() && claim.support_urls.is_empty() {
            failures.push(format!(
                "claim {} has no supporting Source Card IDs or source URLs",
                claim.id
            ));
        }
        for source_card_id in &claim.support_source_card_ids {
            if !valid_source_card_ids.contains(source_card_id.trim()) {
                failures.push(format!(
                    "claim {} references missing or invalid Source Card ID {}",
                    claim.id, source_card_id
                ));
            }
        }
        for url in &claim.support_urls {
            if !valid_http_source_url(url) {
                failures.push(format!(
                    "claim {} contains a non-resolvable support URL {}",
                    claim.id, url
                ));
            }
        }
    }

    let unresolved_conflicts = artifacts
        .conflict_map
        .iter()
        .filter(|conflict| {
            !conflict_is_resolved_or_actionably_deferred(conflict, &artifacts.research_debt)
        })
        .count();
    if unresolved_conflicts > 0 {
        failures.push(format!(
            "conflict map contains {unresolved_conflicts} unresolved conflict(s) not promoted to debt"
        ));
    }

    let debt_without_actions = artifacts
        .research_debt
        .iter()
        .filter(|debt| {
            debt.status != "closed"
                && debt.candidate_queries.is_empty()
                && debt.next_check_actions.is_empty()
        })
        .count();
    if debt_without_actions > 0 {
        failures.push(format!(
            "research debt contains {debt_without_actions} open item(s) without candidate queries or next actions"
        ));
    }

    if matches!(research_intensity, Some("high")) && quality_depth == Some("strict") {
        if artifacts.source_cards.is_empty() {
            failures.push(
                "high-intensity strict research must persist at least one Source Card".to_string(),
            );
        }
        if artifacts.claim_log.is_empty() {
            failures.push(
                "high-intensity strict research must persist at least one Claim Log row"
                    .to_string(),
            );
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

pub fn has_visible_final_answer_section(output: &str) -> bool {
    final_answer_section(&strip_research_artifact_blocks(output)).is_some()
}

fn valid_http_source_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .filter(|parsed| matches!(parsed.scheme(), "http" | "https"))
        .is_some()
}

#[derive(Default)]
struct ArtifactBlockScan {
    blocks: Vec<String>,
    ranges: Vec<(usize, usize)>,
    malformed: bool,
}

fn scan_markdown_research_artifact_blocks(output: &str) -> ArtifactBlockScan {
    let mut scan = ArtifactBlockScan::default();
    let mut search_start = 0;
    while let Some(marker_offset) = output[search_start..].find("[RESEARCH_ARTIFACT_JSON]") {
        let marker = search_start + marker_offset;
        let after_marker = &output[marker + "[RESEARCH_ARTIFACT_JSON]".len()..];
        let Some(fence_start) = after_marker.find("```json") else {
            scan.malformed = true;
            scan.ranges.push((marker, output.len()));
            break;
        };
        let fenced = &after_marker[fence_start + "```json".len()..];
        let Some(fence_end) = fenced.find("```") else {
            scan.malformed = true;
            scan.ranges.push((marker, output.len()));
            break;
        };
        let end = marker
            + "[RESEARCH_ARTIFACT_JSON]".len()
            + fence_start
            + "```json".len()
            + fence_end
            + "```".len();
        scan.blocks.push(fenced[..fence_end].trim().to_string());
        scan.ranges.push((marker, end));
        search_start = end;
    }
    scan
}

fn scan_html_research_artifact_blocks(output: &str) -> ArtifactBlockScan {
    let lower = output.to_ascii_lowercase();
    let mut scan = ArtifactBlockScan::default();
    let mut search_start = 0;
    while let Some(script_offset) = lower[search_start..].find("<script") {
        let open_start = search_start + script_offset;
        let Some(open_end_rel) = lower[open_start..].find('>') else {
            scan.malformed = true;
            scan.ranges.push((open_start, output.len()));
            break;
        };
        let open_end = open_end_rel + open_start + 1;
        let opening_tag = &lower[open_start..open_end];
        if !opening_tag_has_research_artifact_attribute(opening_tag) {
            search_start = open_end;
            continue;
        }
        let Some(close_start_rel) = lower[open_end..].find("</script>") else {
            scan.malformed = true;
            scan.ranges.push((open_start, output.len()));
            break;
        };
        let close_start = close_start_rel + open_end;
        let close_end = close_start + "</script>".len();
        scan.blocks
            .push(output[open_end..close_start].trim().to_string());
        scan.ranges.push((open_start, close_end));
        search_start = close_end;
    }
    scan
}

fn opening_tag_has_research_artifact_attribute(opening_tag: &str) -> bool {
    const ATTR: &str = "data-research-artifacts";
    let bytes = opening_tag.as_bytes();
    let attr_bytes = ATTR.as_bytes();
    let mut index = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while index + attr_bytes.len() <= bytes.len() {
        match bytes[index] {
            b'\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                index += 1;
                continue;
            }
            b'"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                index += 1;
                continue;
            }
            _ => {}
        }

        if !in_single_quote
            && !in_double_quote
            && &bytes[index..index + attr_bytes.len()] == attr_bytes
        {
            let left_ok = index == 0 || bytes[index - 1].is_ascii_whitespace();
            let right_ok = bytes.get(index + attr_bytes.len()).is_none_or(|byte| {
                matches!(byte, b'=' | b'/' | b'>') || byte.is_ascii_whitespace()
            });
            if left_ok && right_ok {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn normalize_research_controller_artifact_value(value: &mut Value) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "invalid research artifact JSON: root must be an object".to_string())?;
    normalize_artifact_version(object);
    normalize_quality_gate_defaults(object)?;
    normalize_source_card_aliases(object);
    normalize_artifact_object_id_aliases(object, "claim_log");
    normalize_artifact_object_id_aliases(object, "conflict_map");
    normalize_artifact_object_id_aliases(object, "research_debt");
    normalize_artifact_object_ids(object, "source_cards", "S");
    normalize_source_card_defaults(object);
    normalize_claim_log_aliases(object);
    normalize_artifact_object_ids(object, "claim_log", "C");
    normalize_conflict_map_aliases(object);
    normalize_conflict_map_defaults(object);
    normalize_artifact_object_ids(object, "conflict_map", "X");
    normalize_research_debt_defaults(object);
    normalize_artifact_object_ids(object, "research_debt", "D");
    normalize_narrative_state_value(object);
    Ok(())
}

fn normalize_narrative_state_value(object: &mut Map<String, Value>) {
    let Some(mut narrative_value) = object.remove("narrative_state") else {
        return;
    };
    if narrative_value.is_null() {
        return;
    }
    let Some(narrative_object) = narrative_value.as_object_mut() else {
        push_warning_value(object, "narrative_state_invalid_shape");
        return;
    };

    if matches!(narrative_object.get("version"), None | Some(Value::Null)) {
        narrative_object.insert("version".to_string(), Value::from(1u8));
    }
    merge_narrative_open_gap_alias(narrative_object);
    normalize_narrative_optional_text_field(narrative_object, "topic_frame");
    normalize_narrative_optional_text_field(narrative_object, "working_thesis");
    normalize_narrative_optional_text_field(narrative_object, "reader_promise");
    normalize_narrative_optional_text_field(narrative_object, "last_iteration_summary");
    normalize_narrative_event_cards_value(narrative_object);
    normalize_narrative_item_array(
        narrative_object,
        "timeline",
        "NE",
        &[("label", "timeline event")],
        &["date_anchor", "significance"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "actors",
        "NA",
        &[("label", "actor")],
        &["role", "relevance"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "causal_chain",
        "NC",
        &[("cause", "cause"), ("effect", "effect")],
        &["rationale"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "evidence_layers",
        "NL",
        &[("label", "evidence layer")],
        &["purpose"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "interpretive_tensions",
        "NT",
        &[("question", "interpretive tension")],
        &["competing_readings", "current_status"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "impacts",
        "NI",
        &[("label", "impact")],
        &["scope", "implication"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "reader_questions",
        "NQ",
        &[("question", "reader question")],
        &["answer_status", "answer_plan"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "section_outline",
        "NS",
        &[("heading", "section")],
        &["purpose"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );
    normalize_narrative_item_array(
        narrative_object,
        "transition_plan",
        "NX",
        &[("bridge", "transition")],
        &["from_section_id", "to_section_id"],
        &[],
    );
    normalize_narrative_item_array(
        narrative_object,
        "open_gaps",
        "NG",
        &[("gap_type", "gap"), ("description", "narrative gap")],
        &["status"],
        &["expected_claim_log_ids", "expected_source_card_ids"],
    );

    object.insert("narrative_state".to_string(), narrative_value);
}

fn normalize_narrative_event_cards_value(narrative_object: &mut Map<String, Value>) {
    let items = narrative_object
        .entry("event_cards".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !items.is_array() {
        *items = Value::Array(Vec::new());
    }
    let Value::Array(item_values) = items else {
        return;
    };
    for (index, item) in item_values.iter_mut().enumerate() {
        if !item.is_object() {
            *item = Value::Object(Map::new());
        }
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        normalize_narrative_required_text_field(
            item_object,
            "label",
            &format!("event phase {}", index + 1),
        );
        for field_name in [
            "timeframe",
            "region_or_front",
            "trigger",
            "development",
            "outcome",
            "confidence",
        ] {
            normalize_narrative_item_optional_text_field(item_object, field_name);
        }
        for field_name in ["actors", "source_ids", "open_questions"] {
            normalize_narrative_item_list_field(item_object, field_name);
        }
    }
}

fn merge_narrative_open_gap_alias(narrative_object: &mut Map<String, Value>) {
    let Some(alias_value) = narrative_object.remove("unresolved_structure_gaps") else {
        return;
    };
    let alias_items = alias_value.as_array().cloned().unwrap_or_default();
    let open_gaps = narrative_object
        .entry("open_gaps".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(open_gap_items) = open_gaps else {
        *open_gaps = Value::Array(alias_items);
        return;
    };
    open_gap_items.extend(alias_items);
}

fn normalize_narrative_optional_text_field(narrative_object: &mut Map<String, Value>, field: &str) {
    let Some(value) = narrative_object.get_mut(field) else {
        return;
    };
    if matches!(value, Value::String(_) | Value::Null) {
        return;
    }
    narrative_object.remove(field);
}

fn normalize_narrative_item_array(
    narrative_object: &mut Map<String, Value>,
    field: &str,
    prefix: &str,
    required_text_fields: &[(&str, &str)],
    optional_text_fields: &[&str],
    list_fields: &[&str],
) {
    let items = narrative_object
        .entry(field.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !items.is_array() {
        *items = Value::Array(Vec::new());
    }
    let Value::Array(item_values) = items else {
        return;
    };
    for (index, item) in item_values.iter_mut().enumerate() {
        if !item.is_object() {
            *item = Value::Object(Map::new());
        }
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        normalize_alias_text_field_variants(item_object, &["ID", "Id"], "id");
        for (field_name, fallback_label) in required_text_fields {
            let fallback = format!("{fallback_label} {}", index + 1);
            normalize_narrative_required_text_field(item_object, field_name, &fallback);
        }
        for field_name in optional_text_fields {
            normalize_narrative_item_optional_text_field(item_object, field_name);
        }
        for field_name in list_fields {
            normalize_narrative_item_list_field(item_object, field_name);
        }
    }
    normalize_artifact_object_ids(narrative_object, field, prefix);
}

fn normalize_narrative_required_text_field(
    item_object: &mut Map<String, Value>,
    field: &str,
    fallback: &str,
) {
    let existing = item_object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    item_object.insert(
        field.to_string(),
        Value::String(existing.unwrap_or_else(|| fallback.to_string())),
    );
}

fn normalize_narrative_item_optional_text_field(item_object: &mut Map<String, Value>, field: &str) {
    let Some(value) = item_object.get_mut(field) else {
        return;
    };
    if !matches!(value, Value::String(_) | Value::Null) {
        item_object.remove(field);
    }
}

fn normalize_narrative_item_list_field(item_object: &mut Map<String, Value>, field: &str) {
    let normalized = item_object
        .remove(field)
        .as_ref()
        .and_then(normalize_string_array_value)
        .unwrap_or_default();
    if !normalized.is_empty() {
        item_object.insert(
            field.to_string(),
            Value::Array(normalized.into_iter().map(Value::String).collect()),
        );
    }
}

fn normalize_source_card_aliases(object: &mut Map<String, Value>) {
    let Some(items) = object.get_mut("source_cards").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        normalize_alias_text_field_variants(item_object, &["ID", "Id"], "id");
        normalize_alias_text_field_variants(item_object, &["URL", "Url"], "url");
    }
}

fn normalize_artifact_object_id_aliases(object: &mut Map<String, Value>, field: &str) {
    let Some(items) = object.get_mut(field).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        normalize_alias_text_field_variants(item_object, &["ID", "Id"], "id");
    }
}

fn normalize_artifact_version(object: &mut Map<String, Value>) {
    if matches!(object.get("version"), None | Some(Value::Null)) {
        object.insert("version".to_string(), Value::from(1u8));
    }
}

fn push_warning_value(object: &mut Map<String, Value>, warning: &str) {
    let warnings = object
        .entry("warnings".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(warning_items) = warnings else {
        *warnings = Value::Array(vec![Value::String(warning.to_string())]);
        return;
    };
    if warning_items
        .iter()
        .any(|item| item.as_str().is_some_and(|existing| existing == warning))
    {
        return;
    }
    warning_items.push(Value::String(warning.to_string()));
}

fn normalize_artifact_object_ids(object: &mut Map<String, Value>, field: &str, prefix: &str) {
    let Some(items) = object.get_mut(field).and_then(Value::as_array_mut) else {
        return;
    };
    let mut used_ids = items
        .iter()
        .filter_map(|item| item.as_object())
        .filter_map(|item| item.get("id"))
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let mut next_index = 1usize;
    for item in items.iter_mut() {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        let id = if let Some(existing_id) = item_object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            existing_id.to_string()
        } else {
            loop {
                let candidate = format!("{prefix}{next_index}");
                next_index += 1;
                if used_ids.insert(candidate.clone()) {
                    break candidate;
                }
            }
        };
        item_object.insert("id".to_string(), Value::String(id));
    }
}

fn normalize_conflict_map_defaults(object: &mut Map<String, Value>) {
    let Some(items) = object.get_mut("conflict_map").and_then(Value::as_array_mut) else {
        return;
    };
    for (index, item) in items.iter_mut().enumerate() {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        if item_object
            .get("topic")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|topic| !topic.is_empty())
            .is_none()
        {
            let fallback = item_object
                .get("conflicting_claim_ids")
                .and_then(normalize_string_array_value)
                .and_then(|claim_ids| claim_ids.into_iter().next())
                .map(|claim_id| format!("conflict involving {claim_id}"))
                .unwrap_or_else(|| format!("unspecified conflict {}", index + 1));
            item_object.insert("topic".to_string(), Value::String(fallback));
        }
    }
}

fn normalize_source_card_defaults(object: &mut Map<String, Value>) {
    let Some(items) = object.get_mut("source_cards").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        if item_object
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .is_none()
        {
            item_object.insert(
                "title".to_string(),
                Value::String(derive_source_card_title(item_object)),
            );
        }
    }
}

fn normalize_claim_log_aliases(object: &mut Map<String, Value>) {
    let Some(items) = object.get_mut("claim_log").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        normalize_alias_text_field(item_object, "type", "claim_type");
        let existing_source_card_ids = item_object
            .get("support_source_card_ids")
            .and_then(normalize_string_array_value)
            .unwrap_or_default();
        let existing_urls = item_object
            .get("support_urls")
            .and_then(normalize_string_array_value)
            .unwrap_or_default();
        let (alias_source_card_ids, alias_urls) =
            normalize_support_alias_value(item_object.remove("support"));
        let merged_source_card_ids =
            merge_alias_string_lists(existing_source_card_ids, alias_source_card_ids);
        let merged_urls = merge_alias_string_lists(existing_urls, alias_urls);
        if !merged_source_card_ids.is_empty() {
            item_object.insert(
                "support_source_card_ids".to_string(),
                Value::Array(
                    merged_source_card_ids
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if !merged_urls.is_empty() {
            item_object.insert(
                "support_urls".to_string(),
                Value::Array(merged_urls.into_iter().map(Value::String).collect()),
            );
        }
    }
}

fn normalize_conflict_map_aliases(object: &mut Map<String, Value>) {
    let Some(items) = object.get_mut("conflict_map").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        let existing_source_card_ids = item_object
            .get("source_card_ids")
            .and_then(normalize_string_array_value)
            .unwrap_or_default();
        let alias_source_card_ids = item_object
            .remove("sources")
            .and_then(|value| normalize_string_array_value(&value))
            .unwrap_or_default();
        let merged_source_card_ids =
            merge_alias_string_lists(existing_source_card_ids, alias_source_card_ids);
        if !merged_source_card_ids.is_empty() {
            item_object.insert(
                "source_card_ids".to_string(),
                Value::Array(
                    merged_source_card_ids
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        normalize_alias_text_field(item_object, "status", "resolution_status");
        normalize_alias_text_field(item_object, "resolution", "resolution_note");
    }
}

fn normalize_research_debt_defaults(object: &mut Map<String, Value>) {
    let default_status = inferred_quality_gate_status(object)
        .map(|status| {
            if status == "passed" {
                "closed".to_string()
            } else {
                "open".to_string()
            }
        })
        .unwrap_or_else(|| "open".to_string());
    let default_missing_evidence = quality_gate_failure_summary(object)
        .unwrap_or_else(|| "missing evidence not specified".to_string());
    let Some(items) = object
        .get_mut("research_debt")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for item in items {
        let Some(item_object) = item.as_object_mut() else {
            continue;
        };
        let existing_actions = item_object
            .get("next_check_actions")
            .and_then(normalize_string_array_value)
            .unwrap_or_default();
        let alias_actions = normalize_alias_action_value(item_object.remove("next_action"));
        let merged_actions = if existing_actions.is_empty() {
            alias_actions
        } else {
            existing_actions
        };
        if !merged_actions.is_empty() {
            item_object.insert(
                "next_check_actions".to_string(),
                Value::Array(merged_actions.into_iter().map(Value::String).collect()),
            );
        }
        normalize_required_string_value(item_object, "severity", "medium");
        normalize_required_string_value(item_object, "status", &default_status);
        normalize_required_string_value(item_object, "missing_evidence", &default_missing_evidence);
    }
}

fn derive_source_card_title(item: &Map<String, Value>) -> String {
    if let Some(url) = item
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        if let Ok(parsed) = Url::parse(url) {
            let host = parsed
                .host_str()
                .unwrap_or_default()
                .trim_start_matches("www.");
            if let Some(path_tail) = parsed
                .path_segments()
                .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
                .map(|segment| {
                    segment
                        .trim_matches(|c: char| matches!(c, '-' | '_' | '/' | '.'))
                        .replace(['-', '_'], " ")
                })
                .filter(|segment| !segment.is_empty())
            {
                if !host.is_empty() {
                    return format!("{host} {path_tail}");
                }
            }
            if !host.is_empty() {
                return host.to_string();
            }
        }
        return url.chars().take(MAX_RESEARCH_ARTIFACT_TEXT_CHARS).collect();
    }
    item.get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "untitled source card".to_string())
}

fn normalize_alias_text_field(
    item_object: &mut Map<String, Value>,
    alias_field: &str,
    target_field: &str,
) {
    normalize_alias_text_field_variants(item_object, &[alias_field], target_field);
}

fn normalize_alias_text_field_variants(
    item_object: &mut Map<String, Value>,
    alias_fields: &[&str],
    target_field: &str,
) {
    let existing_value = item_object
        .get(target_field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let alias_value = alias_fields.iter().find_map(|alias_field| {
        item_object
            .remove(*alias_field)
            .and_then(|value| match value {
                Value::String(text) => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
                _ => None,
            })
    });
    if let Some(value) = existing_value.or(alias_value) {
        item_object.insert(target_field.to_string(), Value::String(value));
    }
}

fn normalize_support_alias_value(value: Option<Value>) -> (Vec<String>, Vec<String>) {
    let Some(value) = value else {
        return (Vec::new(), Vec::new());
    };
    let entries = normalize_string_array_value(&value).unwrap_or_default();
    let mut source_card_ids = Vec::new();
    let mut urls = Vec::new();
    for entry in entries {
        if looks_like_support_url(&entry) {
            urls.push(entry);
        } else {
            source_card_ids.push(entry);
        }
    }
    (source_card_ids, urls)
}

fn merge_alias_string_lists(existing: Vec<String>, alias: Vec<String>) -> Vec<String> {
    let mut merged = Vec::with_capacity(existing.len() + alias.len());
    for value in existing.into_iter().chain(alias.into_iter()) {
        if !merged.iter().any(|existing_value| existing_value == &value) {
            merged.push(value);
        }
    }
    merged
}

fn looks_like_support_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("www.")
        || trimmed.starts_with("//")
        || trimmed.contains("://")
}

fn normalize_quality_gate_defaults(object: &mut Map<String, Value>) -> Result<(), String> {
    let Some(gate) = object
        .get_mut("quality_gate")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    normalize_usize_field(gate, "unsupported_claim_count")?;
    normalize_usize_field(gate, "unresolved_conflict_count")?;
    normalize_usize_field(gate, "open_debt_count")?;
    if gate
        .get("failure_messages")
        .map(|value| value.is_null())
        .unwrap_or(false)
    {
        gate.insert("failure_messages".to_string(), Value::Array(Vec::new()));
    }
    if gate
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .is_none()
    {
        let default_status = inferred_quality_gate_status_from_gate(gate);
        gate.insert("status".to_string(), Value::String(default_status));
    }
    Ok(())
}

fn normalize_required_string_value(object: &mut Map<String, Value>, key: &str, default: &str) {
    if object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        object.insert(key.to_string(), Value::String(default.to_string()));
    }
}

fn normalize_usize_field(object: &mut Map<String, Value>, key: &str) -> Result<(), String> {
    match object.get(key) {
        None | Some(Value::Null) => {
            object.insert(key.to_string(), Value::Number(0u64.into()));
            Ok(())
        }
        Some(Value::Number(number)) if number.as_u64().is_some() => Ok(()),
        Some(Value::String(text)) => {
            let parsed = text.trim().parse::<u64>().map_err(|_| {
                format!(
                    "invalid research artifact JSON: quality_gate.{key} must be an unsigned integer when present"
                )
            })?;
            object.insert(key.to_string(), Value::Number(parsed.into()));
            Ok(())
        }
        Some(_) => Err(format!(
            "invalid research artifact JSON: quality_gate.{key} must be an unsigned integer when present"
        )),
    }
}

fn quality_gate_failure_summary(object: &Map<String, Value>) -> Option<String> {
    object
        .get("quality_gate")
        .and_then(Value::as_object)
        .and_then(|gate| gate.get("failure_messages"))
        .and_then(normalize_string_array_value)
        .and_then(|messages| messages.into_iter().find(|message| !message.is_empty()))
}

fn inferred_quality_gate_status(object: &Map<String, Value>) -> Option<String> {
    object
        .get("quality_gate")
        .and_then(Value::as_object)
        .map(inferred_quality_gate_status_from_gate)
}

fn inferred_quality_gate_status_from_gate(gate: &Map<String, Value>) -> String {
    let failure_count = gate
        .get("failure_messages")
        .and_then(normalize_string_array_value)
        .map(|messages| messages.len())
        .unwrap_or(0);
    let unsupported_claim_count = gate
        .get("unsupported_claim_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unresolved_conflict_count = gate
        .get("unresolved_conflict_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let open_debt_count = gate
        .get("open_debt_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if failure_count > 0
        || unsupported_claim_count > 0
        || unresolved_conflict_count > 0
        || open_debt_count > 0
    {
        "failed".to_string()
    } else {
        "passed".to_string()
    }
}

fn normalize_alias_action_value(value: Option<Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Some(Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::trim).map(str::to_string))
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_string_array_value(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Some(Vec::new())
            } else {
                Some(vec![trimmed.to_string()])
            }
        }
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim).map(str::to_string))
                .filter(|item| !item.is_empty())
                .collect(),
        ),
        _ => None,
    }
}

fn normalize_research_controller_artifacts(artifacts: &mut ResearchControllerArtifacts) {
    artifacts.events.truncate(MAX_RESEARCH_ARTIFACT_EVENTS);
    for event in &mut artifacts.events {
        event.stage = normalize_compact_field(&event.stage, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        event.status =
            normalize_compact_field(&event.status, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        event.detail =
            normalize_optional_text_field(event.detail.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
    }

    artifacts.source_cards.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, card) in artifacts.source_cards.iter_mut().enumerate() {
        card.id = normalize_id_field(&card.id);
        if card.id.is_empty() {
            card.id = format!("S{}", index + 1);
        }
        card.url = normalize_url_field(&card.url);
        card.title = normalize_text_field(&card.title, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        card.source_class =
            normalize_compact_field(&card.source_class, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        card.accessed_at = normalize_optional_text_field(
            card.accessed_at.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut card.extracted_facts,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        card.limitation =
            normalize_optional_text_field(card.limitation.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        card.diagnostics_ref = normalize_optional_text_field(
            card.diagnostics_ref.take(),
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        card.confidence = normalize_optional_text_field(
            card.confidence.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
    }

    artifacts.claim_log.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, claim) in artifacts.claim_log.iter_mut().enumerate() {
        claim.id = normalize_id_field(&claim.id);
        if claim.id.is_empty() {
            claim.id = format!("C{}", index + 1);
        }
        claim.claim = normalize_text_field(&claim.claim, MAX_RESEARCH_ARTIFACT_LONG_TEXT_CHARS);
        claim.claim_type = normalize_optional_text_field(
            claim.claim_type.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut claim.support_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_url_list(
            &mut claim.support_urls,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_URL_CHARS,
        );
        claim.confidence = normalize_optional_text_field(
            claim.confidence.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        claim.uncertainty_note = normalize_optional_text_field(
            claim.uncertainty_note.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
    }

    artifacts.conflict_map.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, conflict) in artifacts.conflict_map.iter_mut().enumerate() {
        conflict.id = normalize_id_field(&conflict.id);
        if conflict.id.is_empty() {
            conflict.id = format!("X{}", index + 1);
        }
        conflict.topic = normalize_text_field(&conflict.topic, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut conflict.conflicting_claim_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut conflict.source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        conflict.resolution_status = normalize_optional_text_field(
            conflict.resolution_status.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        conflict.resolution_note = normalize_optional_text_field(
            conflict.resolution_note.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
    }

    artifacts
        .research_debt
        .truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, debt) in artifacts.research_debt.iter_mut().enumerate() {
        debt.id = normalize_id_field(&debt.id);
        if debt.id.is_empty() {
            debt.id = format!("D{}", index + 1);
        }
        debt.severity =
            normalize_compact_field(&debt.severity, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        debt.failed_gate = normalize_optional_text_field(
            debt.failed_gate.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        debt.missing_evidence = normalize_text_field(
            &debt.missing_evidence,
            MAX_RESEARCH_ARTIFACT_LONG_TEXT_CHARS,
        );
        debt.required_source_class = normalize_optional_text_field(
            debt.required_source_class.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut debt.candidate_queries,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut debt.next_check_actions,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        debt.status = normalize_compact_field(&debt.status, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
    }

    if let Some(quality_gate) = artifacts.quality_gate.as_mut() {
        quality_gate.status =
            normalize_compact_field(&quality_gate.status, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        normalize_string_list(
            &mut quality_gate.failure_messages,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
    }

    if let Some(narrative_state) = artifacts.narrative_state.as_mut() {
        normalize_typed_narrative_state(narrative_state);
    }
    if artifacts
        .narrative_state
        .as_ref()
        .is_some_and(narrative_state_has_prompt_like_content)
    {
        push_artifact_warning(
            &mut artifacts.warnings,
            "narrative_state_omitted_prompt_like_content",
        );
        artifacts.narrative_state = None;
    }
    if artifacts
        .narrative_state
        .as_ref()
        .is_some_and(narrative_state_is_effectively_empty)
    {
        artifacts.narrative_state = None;
    }

    normalize_string_list(
        &mut artifacts.warnings,
        MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
        MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
    );
}

fn normalize_typed_narrative_state(state: &mut NarrativeState) {
    if state.version == 0 {
        state.version = 1;
    }
    state.topic_frame =
        normalize_optional_text_field(state.topic_frame.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
    state.working_thesis = normalize_optional_text_field(
        state.working_thesis.take(),
        MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
    );
    state.reader_promise = normalize_optional_text_field(
        state.reader_promise.take(),
        MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
    );
    state.last_iteration_summary = normalize_optional_text_field(
        state.last_iteration_summary.take(),
        MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
    );

    normalize_narrative_event_cards(&mut state.event_cards);
    normalize_narrative_timeline(&mut state.timeline);
    normalize_narrative_actors(&mut state.actors);
    normalize_narrative_causal_chain(&mut state.causal_chain);
    normalize_narrative_evidence_layers(&mut state.evidence_layers);
    normalize_narrative_tensions(&mut state.interpretive_tensions);
    normalize_narrative_impacts(&mut state.impacts);
    normalize_narrative_reader_questions(&mut state.reader_questions);
    normalize_narrative_sections(&mut state.section_outline);
    normalize_narrative_transitions(&mut state.transition_plan);
    normalize_narrative_open_gaps(&mut state.open_gaps);
}

fn push_artifact_warning(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|existing| existing == warning) {
        warnings.push(warning.to_string());
    }
}

fn normalize_id_field(value: &str) -> String {
    normalize_compact_field(value, MAX_RESEARCH_ARTIFACT_ID_CHARS)
}

fn normalize_url_field(value: &str) -> String {
    normalize_compact_field(value, MAX_RESEARCH_ARTIFACT_URL_CHARS)
}

fn normalize_compact_field(value: &str, limit: usize) -> String {
    normalize_text(value, limit).replace('\n', " ")
}

fn normalize_text_field(value: &str, limit: usize) -> String {
    normalize_text(value, limit)
}

fn normalize_optional_text_field(value: Option<String>, limit: usize) -> Option<String> {
    value
        .map(|value| normalize_text(&value, limit))
        .filter(|value| !value.is_empty())
}

fn normalize_string_list(values: &mut Vec<String>, max_items: usize, item_limit: usize) {
    values.truncate(max_items);
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        let normalized_value = normalize_text(&value, item_limit);
        if !normalized_value.is_empty() {
            normalized.push(normalized_value);
        }
    }
    *values = normalized;
}

fn normalize_url_list(values: &mut Vec<String>, max_items: usize, item_limit: usize) {
    values.truncate(max_items);
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        let normalized_value = normalize_compact_field(&value, item_limit);
        if !normalized_value.is_empty() {
            normalized.push(normalized_value);
        }
    }
    *values = normalized;
}

fn normalize_narrative_timeline(items: &mut Vec<crate::models::NarrativeTimelineEvent>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NE", index);
        item.label = normalize_text_field(&item.label, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.date_anchor = normalize_optional_text_field(
            item.date_anchor.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        item.significance = normalize_optional_text_field(
            item.significance.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_event_cards(items: &mut Vec<crate::models::NarrativeEventCard>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for item in items.iter_mut() {
        item.label = normalize_text_field(&item.label, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.timeframe = normalize_optional_text_field(
            item.timeframe.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.actors,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        item.region_or_front = normalize_optional_text_field(
            item.region_or_front.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        item.trigger =
            normalize_optional_text_field(item.trigger.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.development = normalize_optional_text_field(
            item.development.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        item.outcome =
            normalize_optional_text_field(item.outcome.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut item.source_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        item.confidence = normalize_optional_text_field(
            item.confidence.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.open_questions,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
    }
}

fn normalize_narrative_actors(items: &mut Vec<crate::models::NarrativeActor>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NA", index);
        item.label = normalize_text_field(&item.label, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.role =
            normalize_optional_text_field(item.role.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.relevance =
            normalize_optional_text_field(item.relevance.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_causal_chain(items: &mut Vec<crate::models::NarrativeCausalLink>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NC", index);
        item.cause = normalize_text_field(&item.cause, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.effect = normalize_text_field(&item.effect, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.rationale =
            normalize_optional_text_field(item.rationale.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_evidence_layers(items: &mut Vec<crate::models::NarrativeEvidenceLayer>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NL", index);
        item.label = normalize_text_field(&item.label, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.purpose =
            normalize_optional_text_field(item.purpose.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_tensions(items: &mut Vec<crate::models::NarrativeInterpretiveTension>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NT", index);
        item.question = normalize_text_field(&item.question, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.competing_readings = normalize_optional_text_field(
            item.competing_readings.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        item.current_status = normalize_optional_text_field(
            item.current_status.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_impacts(items: &mut Vec<crate::models::NarrativeImpact>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NI", index);
        item.label = normalize_text_field(&item.label, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.scope =
            normalize_optional_text_field(item.scope.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.implication = normalize_optional_text_field(
            item.implication.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_reader_questions(items: &mut Vec<crate::models::NarrativeReaderQuestion>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NQ", index);
        item.question = normalize_text_field(&item.question, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.answer_status = normalize_optional_text_field(
            item.answer_status.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        item.answer_plan = normalize_optional_text_field(
            item.answer_plan.take(),
            MAX_RESEARCH_ARTIFACT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_sections(items: &mut Vec<crate::models::NarrativeSectionOutlineItem>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NS", index);
        item.heading = normalize_text_field(&item.heading, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.purpose =
            normalize_optional_text_field(item.purpose.take(), MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalize_narrative_transitions(items: &mut Vec<crate::models::NarrativeTransition>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NX", index);
        item.from_section_id = normalize_optional_text_field(
            item.from_section_id.take(),
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        item.to_section_id = normalize_optional_text_field(
            item.to_section_id.take(),
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        item.bridge = normalize_text_field(&item.bridge, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
    }
}

fn normalize_narrative_open_gaps(items: &mut Vec<crate::models::NarrativeOpenGap>) {
    items.truncate(MAX_RESEARCH_ARTIFACT_ITEMS);
    for (index, item) in items.iter_mut().enumerate() {
        item.id = normalized_or_generated_id(&item.id, "NG", index);
        item.gap_type =
            normalize_compact_field(&item.gap_type, MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS);
        item.description =
            normalize_text_field(&item.description, MAX_RESEARCH_ARTIFACT_TEXT_CHARS);
        item.status = normalize_optional_text_field(
            item.status.take(),
            MAX_RESEARCH_ARTIFACT_SHORT_TEXT_CHARS,
        );
        normalize_string_list(
            &mut item.expected_claim_log_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
        normalize_string_list(
            &mut item.expected_source_card_ids,
            MAX_RESEARCH_ARTIFACT_TEXT_ITEMS,
            MAX_RESEARCH_ARTIFACT_ID_CHARS,
        );
    }
}

fn normalized_or_generated_id(value: &str, prefix: &str, index: usize) -> String {
    let normalized = normalize_id_field(value);
    if normalized.is_empty() {
        format!("{prefix}{}", index + 1)
    } else {
        normalized
    }
}

fn narrative_state_has_prompt_like_content(state: &NarrativeState) -> bool {
    narrative_state_text_fragments(state)
        .into_iter()
        .any(|value| {
            let lower = value.to_ascii_lowercase();
            [
                "<script",
                "[research_artifact_json]",
                "ignore previous instructions",
                "follow these instructions",
                "system prompt",
                "assistant:",
                "user:",
                "repair iteration",
                "quality gate failed",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
        })
}

fn narrative_state_is_effectively_empty(state: &NarrativeState) -> bool {
    state.topic_frame.is_none()
        && state.working_thesis.is_none()
        && state.reader_promise.is_none()
        && state.event_cards.is_empty()
        && state.timeline.is_empty()
        && state.actors.is_empty()
        && state.causal_chain.is_empty()
        && state.evidence_layers.is_empty()
        && state.interpretive_tensions.is_empty()
        && state.impacts.is_empty()
        && state.reader_questions.is_empty()
        && state.section_outline.is_empty()
        && state.transition_plan.is_empty()
        && state.open_gaps.is_empty()
        && state.last_iteration_summary.is_none()
}

fn narrative_state_text_fragments(state: &NarrativeState) -> Vec<String> {
    let mut fragments = Vec::new();
    fragments.extend(
        [
            state.topic_frame.as_deref(),
            state.working_thesis.as_deref(),
            state.reader_promise.as_deref(),
            state.last_iteration_summary.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string),
    );
    fragments.extend(state.event_cards.iter().flat_map(|item| {
        [
            Some(item.label.as_str()),
            item.timeframe.as_deref(),
            item.region_or_front.as_deref(),
            item.trigger.as_deref(),
            item.development.as_deref(),
            item.outcome.as_deref(),
            item.confidence.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .chain(item.actors.iter().cloned())
        .chain(item.open_questions.iter().cloned())
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.timeline.iter().flat_map(|item| {
        [
            Some(item.label.as_str()),
            item.date_anchor.as_deref(),
            item.significance.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.actors.iter().flat_map(|item| {
        [
            Some(item.label.as_str()),
            item.role.as_deref(),
            item.relevance.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.causal_chain.iter().flat_map(|item| {
        [
            Some(item.cause.as_str()),
            Some(item.effect.as_str()),
            item.rationale.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.evidence_layers.iter().flat_map(|item| {
        [Some(item.label.as_str()), item.purpose.as_deref()]
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect::<Vec<_>>()
    }));
    fragments.extend(state.interpretive_tensions.iter().flat_map(|item| {
        [
            Some(item.question.as_str()),
            item.competing_readings.as_deref(),
            item.current_status.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.impacts.iter().flat_map(|item| {
        [
            Some(item.label.as_str()),
            item.scope.as_deref(),
            item.implication.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.reader_questions.iter().flat_map(|item| {
        [
            Some(item.question.as_str()),
            item.answer_status.as_deref(),
            item.answer_plan.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.section_outline.iter().flat_map(|item| {
        [Some(item.heading.as_str()), item.purpose.as_deref()]
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect::<Vec<_>>()
    }));
    fragments.extend(state.transition_plan.iter().flat_map(|item| {
        [
            item.from_section_id.as_deref(),
            item.to_section_id.as_deref(),
            Some(item.bridge.as_str()),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments.extend(state.open_gaps.iter().flat_map(|item| {
        [
            Some(item.gap_type.as_str()),
            Some(item.description.as_str()),
            item.status.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<Vec<_>>()
    }));
    fragments
}

fn normalize_text(value: &str, limit: usize) -> String {
    let filtered = value
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect::<String>();
    truncate_chars(filtered.trim(), limit)
}

fn normalize_prompt_text(value: &str, limit: usize) -> String {
    normalize_text(value, limit).replace('\r', "")
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated = value.chars().take(limit).collect::<String>();
    truncated.push_str("...[truncated]");
    truncated
}

pub(crate) fn normalize_ai_output(raw: &str, file_type: &str) -> String {
    let trimmed = raw.trim();
    if file_type != "html" {
        return trimmed.to_string();
    }

    if trimmed.to_ascii_lowercase().starts_with("<!doctype html") {
        return trimmed.to_string();
    }

    if let Some(start) = trimmed.to_ascii_lowercase().find("<!doctype html") {
        let candidate = &trimmed[start..];
        if let Some(end) = candidate.to_ascii_lowercase().rfind("</html>") {
            return candidate[..end + "</html>".len()].trim().to_string();
        }
    }

    if let Some(fence_start) = trimmed.find("```html") {
        let after_fence = &trimmed[fence_start + "```html".len()..];
        if let Some(fence_end) = after_fence.find("```") {
            return after_fence[..fence_end].trim().to_string();
        }
    }

    trimmed.to_string()
}

pub(crate) fn validate_research_output(
    output: &str,
    context: &ResearchQualityContext<'_>,
) -> Result<ResearchQualityReport, String> {
    if !matches!(context.file_prefix, "[Research]" | "[AI-Research]") {
        return Ok(ResearchQualityReport {
            source_url_count: 0,
            audit_url_count: 0,
            matched_topic_terms: 0,
            required_topic_terms: 0,
        });
    }

    let mut failures = Vec::new();
    if context.file_type == "html" {
        validate_html_contract(output, &mut failures);
    }

    let parsed_artifacts_result = parse_research_artifact_block(output, context.file_type);
    if let Err(err) = &parsed_artifacts_result {
        if err != "missing machine-readable research artifact JSON block" {
            failures.push(err.clone());
        }
    }
    let parsed_artifacts = parsed_artifacts_result.ok();
    let machine_readable_quality_gate_eligible = parsed_artifacts
        .as_ref()
        .filter(|artifacts| artifacts.quality_gate.is_some())
        .filter(|artifacts| {
            validate_research_artifacts(
                artifacts,
                context.research_intensity,
                context.quality_depth,
            )
            .is_ok()
        })
        .is_some();
    let source_urls = source_urls(output);
    let visible_source_urls = visible_source_urls(output);
    let merged_evidence_urls =
        merged_evidence_urls(output, parsed_artifacts.as_ref(), context.file_type);
    let audit_url_count = audit_source_url_count(output);
    if context.web_search_requested {
        let required_urls =
            required_source_url_count(context.research_intensity, context.quality_depth);
        if visible_source_urls.len() < required_urls {
            failures.push(format!(
                "source URL count {} is below required minimum {}",
                visible_source_urls.len(),
                required_urls
            ));
        }
        if audit_url_count < required_urls {
            failures.push(format!(
                "source audit URL count {} is below required minimum {}",
                audit_url_count, required_urls
            ));
        }
        if matches!(context.research_intensity, Some("high"))
            && context.quality_depth != Some("light")
        {
            let distinct_hosts = distinct_evidence_hosts(&visible_source_urls);
            let required_hosts =
                required_distinct_host_count(context.research_intensity, context.quality_depth);
            if distinct_hosts < required_hosts {
                failures.push(format!(
                    "distinct evidence host count {distinct_hosts} is below required minimum {required_hosts}"
                ));
            }
            let authoritative_urls = authoritative_evidence_url_count(
                &merged_evidence_urls,
                parsed_artifacts.as_ref(),
                context.evidence_subject,
            );
            let required_authoritative_urls =
                required_authoritative_url_count(context.research_intensity, context.quality_depth);
            if authoritative_urls < required_authoritative_urls {
                failures.push(format!(
                    "authoritative evidence URL count {authoritative_urls} is below required minimum {required_authoritative_urls}"
                ));
            }
        }
    }

    let topic_terms = topic_terms(context.research_topic, context.research_instructions);
    let matched_topic_terms = topic_terms
        .iter()
        .filter(|term| output_contains_term(output, term))
        .count();
    let required_topic_terms = required_topic_match_count(topic_terms.len());
    if required_topic_terms > 0 && matched_topic_terms < required_topic_terms {
        failures.push(format!(
            "topic relevance matched {matched_topic_terms}/{}, below required {required_topic_terms}",
            topic_terms.len()
        ));
    }

    let bad_markers = [
        "출처 감사 필요",
        "Source Documents 기반",
        "[필요 데이터]",
        "외부 검색 (n/a)",
        "외부 웹 검색 (n/a)",
    ];
    for marker in bad_markers {
        if output.contains(marker) {
            failures.push(format!("contains unresolved marker: {marker}"));
        }
    }

    if context.quality_depth == Some("strict") {
        validate_controller_contract_markers(
            output,
            machine_readable_quality_gate_eligible,
            &mut failures,
        );
    }

    if matches!(context.research_intensity, Some("high")) && context.quality_depth == Some("strict")
    {
        validate_high_resolution_research(output, &source_urls, &mut failures);
    }

    if context.file_type == "md" && context.quality_depth == Some("strict") {
        validate_markdown_verification_appendix(output, &mut failures);
    }

    if context.quality_depth != Some("light") {
        validate_topic_specific_anchors(output, context, &mut failures);
    }

    validate_historical_development_density(output, context, &mut failures);

    validate_reader_facing_internal_metadata_leaks(output, &mut failures);

    if let Some(artifacts) = parsed_artifacts.as_ref() {
        validate_historical_event_card_development_density(artifacts, context, &mut failures);
        validate_historical_supplementary_source_reliance(
            output,
            artifacts,
            context,
            &mut failures,
        );
        validate_narrative_structure_output(output, artifacts, context, &mut failures);
    }

    if failures.is_empty() {
        Ok(ResearchQualityReport {
            source_url_count: visible_source_urls.len(),
            audit_url_count,
            matched_topic_terms,
            required_topic_terms,
        })
    } else {
        Err(format!(
            "Research quality gate failed: {}",
            failures.join("; ")
        ))
    }
}

pub(crate) fn validate_transient_repair_hint_evidence_provenance(
    output: &str,
    file_type: &str,
    prohibited_urls: &HashSet<String>,
) -> Result<(), String> {
    if prohibited_urls.is_empty() {
        return Ok(());
    }

    let prohibited_urls = prohibited_urls
        .iter()
        .filter(|url| valid_http_source_url(url) && is_evidence_url(url))
        .map(|url| normalize_url_field(url))
        .collect::<HashSet<_>>();
    if prohibited_urls.is_empty() {
        return Ok(());
    }

    let parsed_artifacts = parse_research_artifact_block(output, file_type).ok();
    let mut used_evidence_urls = merged_evidence_urls(output, parsed_artifacts.as_ref(), file_type)
        .into_iter()
        .map(|url| normalize_url_field(&url))
        .collect::<HashSet<_>>();
    if let Some(artifacts) = parsed_artifacts.as_ref() {
        for claim in &artifacts.claim_log {
            for url in &claim.support_urls {
                if valid_http_source_url(url) && is_evidence_url(url) {
                    used_evidence_urls.insert(normalize_url_field(url));
                }
            }
        }
    }

    let prohibited_matches = used_evidence_urls
        .intersection(&prohibited_urls)
        .cloned()
        .collect::<Vec<_>>();
    if prohibited_matches.is_empty() {
        return Ok(());
    }

    Err(format!(
        "repair search hint URLs cannot be cited as adopted evidence before normal adoption or independent fetch: {}",
        prohibited_matches.join(", ")
    ))
}

fn validate_reader_facing_internal_metadata_leaks(output: &str, failures: &mut Vec<String>) {
    let visible_output = strip_research_artifact_blocks(output);
    let final_answer = final_answer_section(&visible_output)
        .map(section_body_without_heading)
        .unwrap_or_default();
    let pre_appendix_visible = visible_output_before_verification_appendix(&visible_output);
    let lower = final_answer.to_ascii_lowercase();
    let pre_appendix_lower = pre_appendix_visible.to_ascii_lowercase();
    for marker in REPAIR_HINT_BLOCK_LEAK_MARKERS {
        if lower.contains(marker) {
            failures.push(format!(
                "reader-facing final answer leaks internal narrative or repair marker: {marker}"
            ));
        }
    }
    for marker in [
        "topic_frame",
        "working_thesis",
        "reader_promise",
        "section_outline",
        "evidence_layers",
        "transition_plan",
        "open_gaps",
        "last_iteration_summary",
        "outline_only_not_evidence",
        "repair_planning",
        "evidence_repair",
    ] {
        if lower.contains(marker) {
            failures.push(format!(
                "reader-facing final answer leaks internal narrative or repair marker: {marker}"
            ));
        }
    }
    for marker in EVENT_CARD_INTERNAL_LEAK_MARKERS {
        if pre_appendix_lower.contains(marker) {
            failures.push(format!(
                "reader-facing final answer leaks internal narrative or repair marker: {marker}"
            ));
        }
    }
    if reader_facing_contains_repair_hint_row(&lower) {
        failures.push(
            "reader-facing final answer leaks internal narrative or repair marker: repair hint row"
                .to_string(),
        );
    }
}

fn visible_output_before_verification_appendix(visible_output: &str) -> String {
    let body_end = first_markdown_heading_with_markers(
        visible_output,
        &["검증 부록", "verification appendix"],
    )
    .map(|heading| heading.start)
    .unwrap_or(visible_output.len());
    visible_output[..body_end].trim().to_string()
}

fn reader_facing_contains_repair_hint_row(lower_final_answer: &str) -> bool {
    if lower_final_answer.lines().any(|line| {
        let label_count = REPAIR_HINT_ROW_LABEL_MARKERS
            .iter()
            .filter(|marker| line.contains(**marker))
            .count();
        label_count >= 2 && (line.contains('|') || line.contains(" - ") || line.contains(" — "))
    }) {
        return true;
    }

    let mut adjacent_labels = HashSet::new();
    for line in lower_final_answer.lines() {
        if let Some(label) = repair_hint_label_at_line_start(line) {
            adjacent_labels.insert(label);
            let has_repair_specific_signal =
                adjacent_labels.contains("query:") || adjacent_labels.contains("snippet:");
            if adjacent_labels.len() >= 3
                || (adjacent_labels.len() >= 2 && has_repair_specific_signal)
            {
                return true;
            }
        } else if !line.trim().is_empty() {
            adjacent_labels.clear();
        }
    }
    false
}

fn repair_hint_label_at_line_start(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    let trimmed = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("• "))
        .unwrap_or(trimmed);
    REPAIR_HINT_ROW_LABEL_MARKERS
        .iter()
        .find(|marker| trimmed.starts_with(**marker))
        .copied()
}

fn validate_narrative_structure_output(
    output: &str,
    artifacts: &ResearchControllerArtifacts,
    context: &ResearchQualityContext<'_>,
    failures: &mut Vec<String>,
) {
    let visible_output = strip_research_artifact_blocks(output);
    let final_answer = final_answer_section(&visible_output)
        .map(section_body_without_heading)
        .unwrap_or_default();
    let lower = final_answer.to_ascii_lowercase();
    for marker in [
        "narrative_state",
        "<narrative_state",
        "<timeline>",
        "<actors>",
        "expected_claim_log_ids",
        "repair iteration",
    ] {
        if lower.contains(marker) {
            failures.push(format!(
                "reader-facing final answer leaks internal narrative or repair marker: {marker}"
            ));
        }
    }

    let Some(state) = artifacts.narrative_state.as_ref() else {
        return;
    };
    if !should_apply_narrative_structure_checks(context, state) {
        return;
    }

    if !state.timeline.is_empty()
        && !final_answer_matches_any(
            &final_answer,
            state
                .timeline
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
        )
        && !contains_any_marker(
            &lower,
            &["먼저", "이후", "당시", "later", "then", "before", "after"],
        )
    {
        failures.push(
            "historical or explanatory final answer lacks visible chronology despite persisted narrative timeline"
                .to_string(),
        );
    }
    if !state.actors.is_empty()
        && !final_answer_matches_any(
            &final_answer,
            state
                .actors
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
        )
    {
        failures.push(
            "historical or explanatory final answer lacks visible actor or institution coverage from narrative state"
                .to_string(),
        );
    }
    if !state.causal_chain.is_empty()
        && !contains_any_marker(
            &lower,
            &["왜", "때문", "caus", "result", "영향", "이어", "낳"],
        )
    {
        failures.push(
            "historical or explanatory final answer lacks visible causal explanation despite persisted narrative causal chain"
                .to_string(),
        );
    }
    if !state.interpretive_tensions.is_empty()
        && !contains_any_marker(
            &lower,
            &["해석", "논쟁", "불확실", "쟁점", "contested", "uncertain"],
        )
    {
        failures.push(
            "interpretive tensions are persisted in narrative state but the final answer does not surface them as evidence-backed resolution or visible uncertainty"
                .to_string(),
        );
    }
    if !state.impacts.is_empty()
        && !contains_any_marker(
            &lower,
            &["영향", "결과", "의미", "함의", "consequence", "impact"],
        )
    {
        failures.push(
            "narrative impacts are persisted but the final answer omits visible consequences or implications"
                .to_string(),
        );
    }
    if !state.reader_questions.is_empty()
        && !contains_any_marker(
            &lower,
            &["질문", "확인", "남는다", "알아야", "question", "unknown"],
        )
    {
        failures.push(
            "reader questions are persisted but the final answer does not answer, scope, or explicitly leave them open"
                .to_string(),
        );
    }
    let unresolved_open_gaps = state
        .open_gaps
        .iter()
        .filter(|gap| !narrative_gap_status_closed(gap.status.as_deref()))
        .collect::<Vec<_>>();
    if !unresolved_open_gaps.is_empty()
        && !unresolved_open_gaps.iter().all(|gap| {
            visible_output.contains(&gap.description)
                || artifacts
                    .research_debt
                    .iter()
                    .any(|debt| debt.missing_evidence.contains(&gap.description))
        })
    {
        failures.push(
            "narrative open gaps remain unresolved but disappear from visible limits or research debt"
                .to_string(),
        );
    }
}

fn should_apply_narrative_structure_checks(
    context: &ResearchQualityContext<'_>,
    state: &NarrativeState,
) -> bool {
    if context.research_intensity != Some("high") || context.quality_depth != Some("strict") {
        return false;
    }
    let topic = context
        .research_topic
        .or(context.evidence_subject)
        .unwrap_or("")
        .to_string();
    if technology_like_topic(&topic) {
        return false;
    }
    let topic = topic.to_ascii_lowercase();
    topic.contains("histor")
        || topic.contains("policy")
        || topic.contains("regulat")
        || topic.contains("explain")
        || topic.contains("배경")
        || topic.contains("맥락")
        || topic.contains("원인")
        || !state.timeline.is_empty()
        || !state.causal_chain.is_empty()
        || !state.open_gaps.is_empty()
}

fn technology_like_topic(topic: &str) -> bool {
    let lower = topic.to_ascii_lowercase();
    if technology_concept_like_topic(&lower) {
        return true;
    }
    let strong_markers = [
        "technology",
        "c++",
        "cpp",
        "scheduler",
        "work-stealing",
        "deque",
        "kernel",
        "runtime",
        "socket",
        "tcp",
        "udp",
        "linux",
        "windows",
        "microsoft",
        "iana",
        "rfc",
        "protocol",
        "specification",
        "specifications",
        "specs",
        "스케줄러",
        "커널",
        "소켓",
        "프로토콜",
        "명세",
    ];
    if strong_markers
        .iter()
        .any(|marker| technology_marker_present(&lower, marker))
    {
        return true;
    }

    let ambiguous_markers = [
        "port",
        "network",
        "cloud",
        "azure",
        "aws",
        "gcp",
        "kubernetes",
        "implementation",
        "implement",
        "engineering",
        "포트",
        "네트워크",
        "클라우드",
        "구현",
        "설계",
    ];
    let pairing_markers = [
        "ephemeral",
        "dynamic",
        "allocation",
        "exhaustion",
        "range",
        "networking",
        "container",
        "containers",
        "nat",
        "vpc",
        "docs",
        "documentation",
        "official",
        "vendor",
        "동적",
        "할당",
        "고갈",
        "범위",
        "문서",
        "공식",
        "벤더",
        "컨테이너",
    ];
    ambiguous_markers
        .iter()
        .any(|marker| technology_marker_present(&lower, marker))
        && pairing_markers
            .iter()
            .any(|marker| technology_marker_present(&lower, marker))
}

fn technology_concept_like_topic(lower_topic: &str) -> bool {
    if [
        "technology_concept",
        "tech_concept",
        "ai_concept",
        "conceptual technology",
    ]
    .iter()
    .any(|marker| technology_marker_present(lower_topic, marker))
    {
        return true;
    }

    if policy_or_regulatory_like_topic(lower_topic) {
        return false;
    }

    let domain_markers = [
        "ai",
        "artificial intelligence",
        "machine learning",
        "deep learning",
        "generative ai",
        "large language model",
        "language model",
        "llm",
        "rag",
        "transformer",
        "neural network",
        "인공지능",
        "머신러닝",
        "딥러닝",
        "생성형 ai",
        "생성형 인공지능",
        "대규모 언어 모델",
        "언어 모델",
        "검색 증강",
        "신경망",
    ];
    let concept_markers = [
        "concept",
        "concepts",
        "definition",
        "definitions",
        "explain",
        "overview",
        "difference",
        "compare",
        "comparison",
        "misconception",
        "limitation",
        "taxonomy",
        "개념",
        "정의",
        "원리",
        "차이",
        "비교",
        "오해",
        "한계",
        "분류",
        "입문",
    ];

    domain_markers
        .iter()
        .any(|marker| technology_marker_present(lower_topic, marker))
        && concept_markers
            .iter()
            .any(|marker| technology_marker_present(lower_topic, marker))
}

fn policy_or_regulatory_like_topic(lower_topic: &str) -> bool {
    [
        "policy",
        "policies",
        "regulation",
        "regulations",
        "regulatory",
        "legal",
        "law",
        "laws",
        "governance",
        "compliance",
        "정책",
        "규제",
        "법률",
        "법제",
        "법적",
        "거버넌스",
        "컴플라이언스",
        "준수",
    ]
    .iter()
    .any(|marker| technology_marker_present(lower_topic, marker))
}

fn technology_marker_present(text: &str, marker: &str) -> bool {
    if marker.is_ascii() && marker.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        contains_ascii_token_with_boundaries(text, marker)
    } else {
        text.contains(marker)
    }
}

fn contains_ascii_token_with_boundaries(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    let mut search_start = 0;
    while let Some(relative_idx) = text[search_start..].find(token) {
        let start = search_start + relative_idx;
        let end = start + token.len();
        if token_match_has_boundaries(text, start, end) {
            return true;
        }
        search_start = start + 1;
    }

    false
}

fn final_answer_matches_any(final_answer: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| {
        let term = term.trim();
        !term.is_empty()
            && final_answer
                .to_ascii_lowercase()
                .contains(&term.to_ascii_lowercase())
    })
}

fn contains_any_marker(text: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| text.contains(&marker.to_ascii_lowercase()))
}

fn validate_html_contract(output: &str, failures: &mut Vec<String>) {
    let trimmed = output.trim_start();
    if !trimmed.to_ascii_lowercase().starts_with("<!doctype html") {
        failures.push("HTML output must start with <!DOCTYPE html>".to_string());
    }
    if !output.to_ascii_lowercase().contains("</html>") {
        failures.push("HTML output must contain closing </html>".to_string());
    }
    if output.contains("```") {
        failures.push("HTML output must not contain Markdown code fences".to_string());
    }
    if !output.to_ascii_lowercase().contains("<title") {
        failures.push("HTML output must contain a <title> element".to_string());
    }
    if !contains_source_audit(output) {
        failures.push("research output must contain a source audit section".to_string());
    }
    validate_runtime_contract(output, failures);
    validate_interactive_html_contract(output, failures);
}

fn contains_source_audit(output: &str) -> bool {
    SOURCE_AUDIT_MARKERS.iter().any(|marker| {
        output
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
    })
}

fn validate_controller_contract_markers(
    output: &str,
    machine_readable_quality_gate_eligible: bool,
    failures: &mut Vec<String>,
) {
    let visible_output = strip_research_artifact_blocks(output);
    let lower = visible_output.to_ascii_lowercase();
    let machine_readable_quality_gate =
        has_verification_appendix_marker(&visible_output) && machine_readable_quality_gate_eligible;
    let missing = [
        ("final answer", FINAL_ANSWER_MARKERS),
        ("source cards", SOURCE_CARD_MARKERS),
        ("claim log", CLAIM_LOG_MARKERS),
        ("quality gate", QUALITY_GATE_MARKERS),
    ]
    .into_iter()
    .filter_map(|(name, variants)| {
        if name == "quality gate" && machine_readable_quality_gate {
            return None;
        }
        if variants
            .iter()
            .any(|variant| lower.contains(&variant.to_ascii_lowercase()))
        {
            None
        } else {
            Some(name)
        }
    })
    .collect::<Vec<_>>();

    if !missing.is_empty() {
        failures.push(format!(
            "controller contract is missing required sections: {}",
            missing.join(", ")
        ));
    }
}

fn has_verification_appendix_marker(output: &str) -> bool {
    first_markdown_heading_with_markers(output, &["검증 부록", "verification appendix"]).is_some()
        || output
            .to_ascii_lowercase()
            .contains("verification appendix")
        || output.contains("검증 부록")
}

fn validate_high_resolution_research(
    output: &str,
    evidence_urls: &[String],
    failures: &mut Vec<String>,
) {
    validate_high_resolution_final_answer(output, failures);

    let Some(claim_section) = claim_log_section(output) else {
        failures.push(
            "high-intensity strict research must contain a resolvable claim log section"
                .to_string(),
        );
        return;
    };

    let claim_rows = evidence_table_row_count(claim_section);
    if claim_rows < 7 {
        failures.push(format!(
            "claim log row count {claim_rows} is below required minimum 7 for high-resolution research"
        ));
    }

    let supported_claim_rows = evidence_supported_claim_row_count(claim_section, output);
    let required_supported_rows = evidence_urls.len().min(5);
    if required_supported_rows > 0 && supported_claim_rows < required_supported_rows {
        failures.push(format!(
            "claim log supported evidence row count {supported_claim_rows} is below required minimum {required_supported_rows}; connect concrete claims directly to source URLs or resolvable Source Card IDs instead of only listing a bibliography"
        ));
    }

    let unresolved_refs = unresolved_source_refs(claim_section, output);
    if !unresolved_refs.is_empty() {
        failures.push(format!(
            "claim log has unresolved source references without matching source-card URL definitions: {}",
            unresolved_refs.join(", ")
        ));
    }
}

fn validate_markdown_verification_appendix(output: &str, failures: &mut Vec<String>) {
    let Some(first_verification) = first_markdown_heading_with_markers(
        output,
        &[
            "source audit",
            "source cards",
            "claim log",
            "claim ledger",
            "claims and evidence",
            "claims & evidence",
            "quality gate",
            "quality check",
            "quality review",
            "resolution check",
            "ambiguity check",
            "conflict map",
            "score section",
            "출처 감사",
            "출처 카드",
            "주장 로그",
            "클레임 로그",
            "품질 게이트",
            "품질 검증",
            "품질 점검",
            "검증 보수 항목",
            "검증 항목",
            "품질 점수",
            "신뢰도 점수",
            "고우선 검증",
        ],
    ) else {
        return;
    };

    let appendix =
        first_markdown_heading_with_markers(output, &["검증 부록", "verification appendix"]);
    let Some(appendix) = appendix else {
        failures.push(
            "strict markdown research must place audit, claim, and quality sections under a final '# 검증 부록' or '# Verification Appendix' heading"
                .to_string(),
        );
        return;
    };

    if first_verification.start < appendix.start {
        failures.push(
            "strict markdown research has verification sections before the final verification appendix"
                .to_string(),
        );
    }
    if appendix.level != 1 {
        failures.push(
            "strict markdown research verification appendix must be a top-level '# 검증 부록' or '# Verification Appendix' heading"
                .to_string(),
        );
    }
    let after_appendix = &output[appendix.end..];
    if after_appendix
        .lines()
        .any(|line| markdown_heading_level(line).is_some_and(|level| level <= appendix.level))
    {
        failures.push(
            "strict markdown research verification appendix must be the final top-level section"
                .to_string(),
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct MarkdownHeading {
    start: usize,
    end: usize,
    level: usize,
}

fn first_markdown_heading_with_markers(output: &str, markers: &[&str]) -> Option<MarkdownHeading> {
    let mut offset = 0;
    for line in output.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        if let Some(level) = markdown_heading_level(line_without_newline) {
            let heading = line_without_newline.trim_start_matches('#').trim();
            let lower_heading = heading.to_ascii_lowercase();
            if markers
                .iter()
                .any(|marker| lower_heading.contains(&marker.to_ascii_lowercase()))
            {
                return Some(MarkdownHeading {
                    start: offset,
                    end: offset + line.len(),
                    level,
                });
            }
        }
        offset += line.len();
    }
    None
}

fn markdown_section_with_markers<'a>(output: &'a str, markers: &[&str]) -> Option<&'a str> {
    let heading = first_markdown_heading_with_markers(output, markers)?;
    let mut next_heading_start = output.len();
    let mut offset = heading.end;
    for line in output[heading.end..].split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        if let Some(level) = markdown_heading_level(line_without_newline) {
            if level <= heading.level {
                next_heading_start = offset;
                break;
            }
        }
        offset += line.len();
    }
    Some(&output[heading.start..next_heading_start])
}

fn extract_markdown_reader_body(output: &str) -> String {
    let stripped = strip_research_artifact_blocks(output);
    let body_end =
        first_markdown_heading_with_markers(&stripped, &["검증 부록", "verification appendix"])
            .map(|heading| heading.start)
            .or_else(|| {
                first_markdown_heading_with_markers(
                    &stripped,
                    &[
                        "source audit",
                        "source cards",
                        "claim log",
                        "quality gate",
                        "conflict map",
                        "출처 감사",
                        "출처 카드",
                        "주장 로그",
                        "품질 게이트",
                    ],
                )
                .map(|heading| heading.start)
            })
            .unwrap_or(stripped.len());
    stripped[..body_end].trim().to_string()
}

fn section_body_without_heading(section: &str) -> String {
    let mut lines = section.lines();
    let _ = lines.next();
    lines.collect::<Vec<_>>().join("\n").trim().to_string()
}

fn fallback_reader_text(reader_body: &str) -> String {
    preserve_markdown_paragraph_breaks(&normalize_reader_markdown_heading_boundaries(reader_body))
}

fn normalize_finalized_markdown_main_body(text: &str) -> String {
    let normalized = text.replace('\r', "");
    let lines = normalized.split('\n').collect::<Vec<_>>();
    let Some(wrapper_index) = lines
        .iter()
        .position(|line| line.trim() == "## 최종 답변 (Final Answer)")
    else {
        return normalized;
    };

    let before = lines[..wrapper_index].join("\n");
    let wrapper = lines[wrapper_index];
    let after = lines[wrapper_index + 1..].join("\n");

    let mut sections = Vec::new();
    if !before.is_empty() {
        sections.push(normalize_reader_markdown_heading_boundaries(&before));
    }
    sections.push(wrapper.to_string());
    if !after.is_empty() {
        sections.push(normalize_reader_markdown_heading_boundaries(&after));
    }
    sections.join("\n")
}

fn normalize_reader_markdown_heading_boundaries(text: &str) -> String {
    let normalized = text.replace('\r', "");
    let mut lines = Vec::new();
    for raw_line in normalized.split('\n') {
        if let Some(index) = inline_markdown_heading_start(raw_line) {
            let before = raw_line[..index].trim_end();
            let heading = raw_line[index..].trim_start();
            if !before.is_empty() {
                lines.push(before.to_string());
                lines.push(String::new());
            }
            lines.push(normalize_reader_heading_line(heading));
        } else {
            lines.push(normalize_reader_heading_line(raw_line));
        }
    }

    let mut output_lines = Vec::with_capacity(lines.len() + 4);
    for line in lines {
        let is_heading = markdown_heading_level(line.trim_start()).is_some();
        if is_heading
            && output_lines
                .last()
                .is_some_and(|last: &String| !last.is_empty())
        {
            output_lines.push(String::new());
        }
        output_lines.push(line);
    }
    output_lines.join("\n")
}

fn inline_markdown_heading_start(line: &str) -> Option<usize> {
    for (index, ch) in line.char_indices() {
        if ch != '#' || line[..index].trim().is_empty() || line[..index].ends_with('#') {
            continue;
        }
        let mut end = index;
        while line[end..].starts_with('#') {
            end += '#'.len_utf8();
            if end >= line.len() {
                break;
            }
        }
        let marker_count = line[index..end].chars().count();
        if (2..=6).contains(&marker_count) && line[end..].starts_with(' ') {
            return Some(index);
        }
    }
    None
}

fn normalize_reader_heading_line(line: &str) -> String {
    let trimmed = line.trim_start();
    if let Some(level) = markdown_heading_level(trimmed) {
        let content = trimmed.trim_start_matches('#').trim();
        return format!("{} {}", "#".repeat(level.max(3)), content);
    }
    line.to_string()
}

fn final_answer_needs_repair(
    final_answer_section: &str,
    context: &ResearchQualityContext<'_>,
) -> bool {
    if context.research_intensity != Some("high") || context.quality_depth != Some("strict") {
        return !has_visible_final_answer_section(final_answer_section);
    }
    let mut failures = Vec::new();
    validate_high_resolution_final_answer(final_answer_section, &mut failures);
    !failures.is_empty()
}

fn synthesize_final_answer(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
) -> String {
    let topic = context
        .research_topic
        .or(context.evidence_subject)
        .unwrap_or("이 조사 주제");
    let narrative_paragraphs = synthesize_narrative_final_answer_paragraphs(artifacts, context);
    if !narrative_paragraphs.is_empty() {
        let mut paragraphs = narrative_paragraphs;
        paragraphs.push(render_debt_summary_sentence(artifacts, diagnostics));
        let repair = synthesize_resolution_repair_paragraph(artifacts, diagnostics, context);
        if !repair.is_empty() {
            paragraphs.push(repair);
        }
        return paragraphs.join("\n\n");
    }
    let supported_claims = artifacts
        .claim_log
        .iter()
        .filter(|claim| !claim.support_source_card_ids.is_empty() || !claim.support_urls.is_empty())
        .take(4)
        .map(|claim| claim.claim.trim().trim_end_matches('.').to_string())
        .collect::<Vec<_>>();
    let summary_sentence = if supported_claims.is_empty() {
        format!(
            "{topic}에 대해 신뢰할 수 있는 공개 근거는 확인했지만, 현재 저장된 주장 로그만으로는 바로 결론을 단정하기보다 근거 범위를 좁혀 설명하는 편이 안전합니다."
        )
    } else {
        format!(
            "{topic}에 대해 현재 남아 있는 검증 가능한 핵심 근거는 {} 입니다.",
            supported_claims.join("; ")
        )
    };
    let conflict_sentence = if artifacts.conflict_map.is_empty() {
        "현재 저장된 conflict map에는 결론을 뒤집을 만큼 큰 충돌이 남아 있지 않지만, 근거 강도 차이는 본문과 부록에서 분리해 읽어야 합니다.".to_string()
    } else {
        format!(
            "동시에 충돌 항목 {}건을 검토했고, 해결되지 않은 항목은 연구 부채로 승격해 과도한 확신을 피했습니다.",
            artifacts.conflict_map.len()
        )
    };
    let debt_sentence = render_debt_summary_sentence(artifacts, diagnostics);
    [
        summary_sentence,
        conflict_sentence,
        debt_sentence,
        synthesize_resolution_repair_paragraph(artifacts, diagnostics, context),
    ]
    .into_iter()
    .filter(|sentence| !sentence.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn synthesize_resolution_repair_paragraph(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    context: &ResearchQualityContext<'_>,
) -> String {
    let open_debt = artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .count();
    let unresolved_conflicts = artifacts
        .conflict_map
        .iter()
        .filter(|conflict| conflict.resolution_status.as_deref() != Some("resolved"))
        .count();
    let coverage_misses = diagnostics
        .and_then(|envelope| envelope.source_pack.as_ref())
        .map(|report| report.coverage_misses.len())
        .unwrap_or_default();
    let actor_labels = dominant_actor_labels(artifacts);
    let subject = context
        .evidence_subject
        .or(context.research_topic)
        .unwrap_or("이 주제");
    let raw_subject = subject;
    let subject = natural_reader_subject(subject);
    if !technology_like_topic(raw_subject) {
        if let Some(narrative_paragraph) =
            synthesize_narrative_repair_paragraph(artifacts, &subject)
        {
            return format!(
                "{} {}",
                narrative_paragraph,
                if coverage_misses > 0 || open_debt > 0 || unresolved_conflicts > 0 {
                    "남아 있는 구조 공백과 후속 확인 지점은 부록의 한계 및 연구 부채에서 숨기지 말고 그대로 보여 주어야 합니다."
                } else {
                    "현재 남은 구조 공백은 크지 않지만, 확인된 사실과 해석의 경계는 계속 분리해 쓰는 편이 안전합니다."
                }
            );
        }
    }
    let caution_phrase = if coverage_misses > 0 || open_debt > 0 || unresolved_conflicts > 0 {
        "다만 당장 단정하기 어려운 지점과 추가 확인이 필요한 항목은 분명히 남아 있습니다."
    } else {
        "현재 공개 근거만으로도 판단의 큰 방향은 비교적 선명합니다."
    };
    format!(
        "{subject}를 설명할 때는 먼저 확인된 사실이 어떤 순서와 맥락에서 이어지는지 분리해 보여 주고, 이어서 {actor_labels}처럼 역할이 다른 주체들이 무엇을 결정하거나 감당하는지 나누어 해석해야 합니다. 또한 지금 확보된 근거가 왜 그런 판단으로 이어지는지, 어디까지는 확인되었고 어디부터는 보수적으로 보아야 하는지를 함께 적어야 독자가 실제 선택에 바로 쓸 수 있습니다. {caution_phrase} 그래서 결론은 단정적인 한 문장보다 실행에 도움이 되는 조건, 한계, 후속 확인 포인트를 함께 제시하는 편이 안전합니다."
    )
}

fn synthesize_narrative_final_answer_paragraphs(
    artifacts: &ResearchControllerArtifacts,
    context: &ResearchQualityContext<'_>,
) -> Vec<String> {
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return Vec::new();
    };
    let subject = natural_reader_subject(
        context
            .evidence_subject
            .or(context.research_topic)
            .unwrap_or("이 주제"),
    );
    let timeline = supported_narrative_labels(
        state.timeline.iter().map(|item| {
            (
                item.label.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let actors = supported_narrative_labels(
        state.actors.iter().map(|item| {
            (
                item.label.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let causes = supported_narrative_labels(
        state.causal_chain.iter().map(|item| {
            (
                item.cause.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let effects = supported_narrative_labels(
        state.causal_chain.iter().map(|item| {
            (
                item.effect.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let impacts = supported_narrative_labels(
        state.impacts.iter().map(|item| {
            (
                item.label.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let tensions = supported_narrative_labels(
        state.interpretive_tensions.iter().map(|item| {
            (
                item.question.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let questions = supported_narrative_labels(
        state.reader_questions.iter().map(|item| {
            (
                item.question.as_str(),
                item.expected_claim_log_ids.as_slice(),
                item.expected_source_card_ids.as_slice(),
            )
        }),
        artifacts,
    );
    let timeline_text = if timeline.is_empty() {
        "확인된 사건 축".to_string()
    } else {
        timeline.join(", ")
    };
    let actor_text = if actors.is_empty() {
        "관련 주체".to_string()
    } else {
        actors.join(", ")
    };
    let cause_text = if causes.is_empty() {
        "앞선 조건 변화".to_string()
    } else {
        causes.join(", ")
    };
    let effect_text = if effects.is_empty() {
        "뒤이은 결과".to_string()
    } else {
        effects.join(", ")
    };
    let tension_text = if tensions.is_empty() {
        "남아 있는 쟁점".to_string()
    } else {
        tensions.join(", ")
    };
    let impact_text = if impacts.is_empty() {
        "실제 영향".to_string()
    } else {
        impacts.join(", ")
    };
    let question_text = if questions.is_empty() {
        "후속 질문".to_string()
    } else {
        questions.join(", ")
    };
    let mut paragraphs = Vec::new();
    if timeline_text != "확인된 사건 축" || actor_text != "관련 주체" {
        paragraphs.push(format!(
            "{subject}를 읽을 때는 먼저 {} 같은 전개 순서를 기준으로 흐름을 잡고, {}처럼 역할이 다른 행위자나 기관이 어느 지점에서 판단을 바꾸거나 책임을 나누는지 따라가는 편이 정확합니다.",
            timeline_text,
            actor_text
        ));
    }
    if cause_text != "앞선 조건 변화" || effect_text != "뒤이은 결과" {
        paragraphs.push(format!(
            "근거가 가리키는 인과선은 {}가 {}로 이어졌다는 설명에 가깝기 때문에, 사실 나열보다 왜 다음 단계가 나왔는지를 함께 풀어 써야 합니다.",
            cause_text,
            effect_text
        ));
    }
    if tension_text != "남아 있는 쟁점"
        || impact_text != "실제 영향"
        || question_text != "후속 질문"
    {
        paragraphs.push(format!(
            "또한 {} 같은 해석상 긴장은 근거가 허용하는 범위까지만 정리하고, {} 같은 영향과 {} 같은 독자 질문은 확인된 주장에 기대어 답하거나 불확실성으로 남겨 두어야 합니다.",
            tension_text,
            impact_text,
            question_text
        ));
    }
    paragraphs
}

fn synthesize_narrative_repair_paragraph(
    artifacts: &ResearchControllerArtifacts,
    subject: &str,
) -> Option<String> {
    let state = artifacts.narrative_state.as_ref()?;
    let section_headings = state
        .section_outline
        .iter()
        .filter(|item| narrative_claim_refs_resolve(&item.expected_claim_log_ids, artifacts))
        .take(3)
        .map(|item| item.heading.trim())
        .filter(|heading| !heading.is_empty())
        .collect::<Vec<_>>();
    let evidence_layers = state
        .evidence_layers
        .iter()
        .filter(|item| narrative_claim_refs_resolve(&item.expected_claim_log_ids, artifacts))
        .take(3)
        .map(|item| item.label.trim())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    let unresolved_open_gaps = state
        .open_gaps
        .iter()
        .filter(|gap| !narrative_gap_status_closed(gap.status.as_deref()))
        .map(|gap| gap.description.trim())
        .filter(|description| meaningful_narrative_open_gap_description(description))
        .filter(|description| !description.is_empty())
        .take(2)
        .collect::<Vec<_>>();
    let has_sections = !section_headings.is_empty();
    let has_layers = !evidence_layers.is_empty();
    let has_open_gaps = !unresolved_open_gaps.is_empty();
    let section_headings_text = if section_headings.is_empty() {
        "핵심 배경, 판단 근거, 한계".to_string()
    } else {
        section_headings.join(" → ")
    };
    let evidence_layers_text = if evidence_layers.is_empty() {
        "확인된 사실".to_string()
    } else {
        evidence_layers.join(", ")
    };
    let open_gap_text = if unresolved_open_gaps.is_empty() {
        "남아 있는 공백은 근거 부족으로 명시하면 됩니다.".to_string()
    } else {
        format!(
            "특히 {} 같은 남은 공백은 해결된 것처럼 덮지 말고 한계나 연구 부채로 남겨 두어야 합니다.",
            unresolved_open_gaps.join(", ")
        )
    };
    if !has_sections && !has_layers && !has_open_gaps {
        return None;
    }
    Some(format!(
        "{subject}의 최종 설명은 {} 순서로 독자가 따라갈 수 있게 재정렬하고, {} 같은 근거 층위를 사실에서 해석과 한계로 이어지게 배치해야 합니다. {}",
        section_headings_text,
        evidence_layers_text,
        open_gap_text,
    ))
}

fn supported_narrative_labels<'a, I>(
    items: I,
    artifacts: &ResearchControllerArtifacts,
) -> Vec<String>
where
    I: Iterator<Item = (&'a str, &'a [String], &'a [String])>,
{
    items
        .filter(|(_, claim_ids, _)| narrative_claim_refs_resolve(claim_ids, artifacts))
        .map(|(label, _, _)| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .take(3)
        .collect()
}

fn narrative_claim_refs_resolve(
    claim_ids: &[String],
    artifacts: &ResearchControllerArtifacts,
) -> bool {
    !claim_ids.is_empty()
        && claim_ids.iter().all(|id| {
            artifacts.claim_log.iter().any(|claim| {
                claim.id.trim() == id.trim()
                    && (!claim.support_source_card_ids.is_empty() || !claim.support_urls.is_empty())
            })
        })
}

fn natural_reader_subject(subject: &str) -> String {
    let normalized = subject
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return "이 주제".to_string();
    }

    let mut cleaned = normalized.clone();
    for marker in [
        "Write a Korean reader-facing research report",
        "Write a reader-facing research report",
        "reader-facing research report",
        "reader-facing",
        "research report",
        "The output should be useful",
        "someone planning",
    ] {
        cleaned = replace_ascii_case_insensitive(&cleaned, marker, " ");
    }
    let cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .trim_matches(|ch: char| ch == ':' || ch == '-' || ch == ',')
        .to_string();

    if cleaned.is_empty() {
        return "이 주제".to_string();
    }

    if subject_needs_generic_reader_fallback(&cleaned) {
        return generic_reader_subject_fallback(&cleaned);
    }

    cleaned.chars().take(96).collect::<String>()
}

fn subject_needs_generic_reader_fallback(subject: &str) -> bool {
    let word_count = subject.split_whitespace().count();
    let ascii_alpha_count = subject
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    let alpha_count = subject.chars().filter(|ch| ch.is_alphabetic()).count();
    let ascii_heavy = alpha_count > 0 && ascii_alpha_count * 100 / alpha_count >= 80;
    let lower = subject.to_ascii_lowercase();

    ascii_heavy
        && (word_count >= 10
            || subject.chars().count() > 72
            || lower.contains("planning to implement")
            || lower.contains("someone planning")
            || lower.contains("cover the ")
            || lower.contains("compare "))
}

fn generic_reader_subject_fallback(subject: &str) -> String {
    let lower = subject.to_ascii_lowercase();
    if [
        "c++",
        "cpp",
        "scheduler",
        "work-stealing",
        "work stealing",
        "deque",
        "memory ordering",
        "parking",
        "wakeup",
        "benchmark",
        "implementation",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "이 기술 구현 주제".to_string();
    }
    if [
        "roman",
        "byzantine",
        "gothic war",
        "historical",
        "history",
        "emperor",
        "ancient",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "이 역사 해설 주제".to_string();
    }
    if [
        "seoul", "namsan", "running", "cafe", "travel", "route", "park",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "이 현장형 추천 주제".to_string();
    }
    "이 조사 주제".to_string()
}

fn replace_ascii_case_insensitive(input: &str, needle: &str, replacement: &str) -> String {
    let lower_input = input.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut start = 0usize;
    let mut output = String::new();

    while let Some(offset) = lower_input[start..].find(&lower_needle) {
        let absolute = start + offset;
        output.push_str(&input[start..absolute]);
        output.push_str(replacement);
        start = absolute + needle.len();
    }
    output.push_str(&input[start..]);
    output
}

fn dominant_actor_labels(artifacts: &ResearchControllerArtifacts) -> String {
    let actors = artifacts
        .source_cards
        .iter()
        .take(3)
        .map(|card| {
            if card.title.trim().is_empty() {
                card.source_class.clone()
            } else {
                card.title.clone()
            }
        })
        .collect::<Vec<_>>();
    if actors.is_empty() {
        "provider, deployer, 규제기관".to_string()
    } else {
        actors.join(", ")
    }
}

fn render_debt_summary_sentence(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> String {
    let open_debts = artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .collect::<Vec<_>>();
    let source_pack_reason = diagnostics
        .and_then(|envelope| envelope.source_pack.as_ref())
        .and_then(|report| report.reason.as_deref());
    if open_debts.is_empty() && source_pack_reason.is_none() {
        return "남은 한계는 부록의 출처 감사와 주장 근거 표에서 설명 가능한 범위이며, 추가 확인 과제는 현재 열려 있지 않습니다.".to_string();
    }
    let debt_labels = open_debts
        .iter()
        .take(3)
        .map(|debt| debt.missing_evidence.trim())
        .collect::<Vec<_>>();
    let joined = if debt_labels.is_empty() {
        "사전 확보 근거의 범위 한계".to_string()
    } else {
        debt_labels.join("; ")
    };
    match source_pack_reason {
        Some(reason) => format!(
            "남은 한계는 {} 이며, 현재 확보된 근거 범위에서는 {} 라는 제한이 남아 있어 추가 확인이 필요한 범위를 분명히 남깁니다.",
            joined,
            finalization_safe_diagnostic_text(reason)
        ),
        None => format!(
            "남은 한계는 {} 이며, 이는 결론을 유지하더라도 confidence 와 추가 확인 범위를 함께 적어야 한다는 뜻입니다.",
            joined
        ),
    }
}

fn visible_narrative_open_gaps(
    artifacts: &ResearchControllerArtifacts,
) -> Vec<&crate::models::NarrativeOpenGap> {
    artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            state
                .open_gaps
                .iter()
                .filter(|gap| {
                    !narrative_gap_status_closed(gap.status.as_deref())
                        && !narrative_open_gap_deferred_to_debt(gap, &artifacts.research_debt)
                        && meaningful_narrative_open_gap_description(&gap.description)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn meaningful_narrative_open_gap_description(description: &str) -> bool {
    let lower = description.trim().to_ascii_lowercase();
    !lower.is_empty()
        && lower != "narrative gap"
        && !lower
            .strip_prefix("narrative gap ")
            .map(|suffix| suffix.chars().all(|ch| ch.is_ascii_digit()))
            .unwrap_or(false)
}

fn narrative_gap_status_closed(status: Option<&str>) -> bool {
    status
        .map(str::trim)
        .is_some_and(|status| matches!(status, "closed" | "resolved" | "converted_to_debt"))
}

fn narrative_open_gap_deferred_to_debt(
    gap: &crate::models::NarrativeOpenGap,
    research_debt: &[ResearchDebtItem],
) -> bool {
    research_debt.iter().any(|debt| {
        debt.status != "closed"
            && (debt.missing_evidence.contains(&gap.id)
                || debt.missing_evidence.contains(&gap.description))
    })
}

fn render_markdown_source_audit_rows(artifacts: &ResearchControllerArtifacts) -> String {
    artifacts
        .source_cards
        .iter()
        .map(|card| {
            let checked_fact = card_checked_fact(card, artifacts);
            format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                escape_markdown_table(&card.id),
                escape_markdown_table(&card.url),
                escape_markdown_table(&card.title),
                escape_markdown_table(&card.source_class),
                escape_markdown_table(&checked_fact),
                escape_markdown_table(card.limitation.as_deref().unwrap_or("-")),
                escape_markdown_table(card.diagnostics_ref.as_deref().unwrap_or("-")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_markdown_claim_log_rows(artifacts: &ResearchControllerArtifacts) -> String {
    artifacts
        .claim_log
        .iter()
        .map(|claim| {
            let support = render_claim_support(claim, artifacts);
            format!(
                "| {} | {} | {} | {} | {} |",
                escape_markdown_table(&claim.id),
                escape_markdown_table(&claim.claim),
                escape_markdown_table(&support),
                escape_markdown_table(claim.confidence.as_deref().unwrap_or("-")),
                escape_markdown_table(claim.uncertainty_note.as_deref().unwrap_or("-")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_markdown_limits_section(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    finalization: &mut ResearchFinalizationDiagnostics,
) -> String {
    let mut lines = Vec::new();
    let (blocking_conflicts, deferred_conflicts) = partition_visible_conflicts(artifacts);
    if blocking_conflicts.is_empty() {
        lines.push("- Blocking unresolved conflicts: none recorded.".to_string());
    } else {
        lines.push("### Unresolved Conflicts".to_string());
        lines.extend(blocking_conflicts.iter().map(|conflict| {
            format!(
                "- {}: {} [{}]",
                conflict.id,
                conflict.topic,
                conflict
                    .resolution_note
                    .as_deref()
                    .unwrap_or("resolution note unavailable")
            )
        }));
    }
    if !deferred_conflicts.is_empty() {
        lines.push("### Deferred / Caveated Conflicts".to_string());
        lines.extend(deferred_conflicts.iter().map(|conflict| {
            format!(
                "- {}: {} [status={} | deferred to research debt | {}]",
                conflict.id,
                conflict.topic,
                conflict.resolution_status.as_deref().unwrap_or("unknown"),
                conflict
                    .resolution_note
                    .as_deref()
                    .unwrap_or("resolution note unavailable")
            )
        }));
    }

    let open_debt = artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .collect::<Vec<_>>();
    if open_debt.is_empty() {
        lines.push("- Open research debt: none.".to_string());
    } else {
        lines.push("### Research Debt".to_string());
        lines.extend(open_debt.iter().map(render_markdown_debt_line));
    }
    let visible_open_gaps = visible_narrative_open_gaps(artifacts);
    if !visible_open_gaps.is_empty() {
        lines.push("### Remaining Explanatory Limits".to_string());
        lines.extend(visible_open_gaps.iter().map(|gap| {
            format!(
                "- {}{}",
                escape_markdown_table(&gap.description),
                gap.status
                    .as_deref()
                    .filter(|status| !status.is_empty())
                    .map(|status| format!(" [status={}]", escape_markdown_table(status)))
                    .unwrap_or_default()
            )
        }));
    }

    if let Some(source_pack) = diagnostics.and_then(|envelope| envelope.source_pack.as_ref()) {
        if let Some(reason) = source_pack.reason.as_deref() {
            lines.push(format!(
                "- Source-pack limitation: {}",
                escape_markdown_table(&finalization_safe_diagnostic_text(reason))
            ));
        }
        let visible_coverage_misses = source_pack
            .coverage_misses
            .iter()
            .filter(|miss| miss.status != "adopted")
            .collect::<Vec<_>>();
        if !visible_coverage_misses.is_empty() {
            lines.push("### Target Host / Source Class Misses".to_string());
            lines.extend(visible_coverage_misses.iter().map(|miss| {
                format!(
                    "- query=`{}` status={} expected_host={} expected_source_class={} provider={} reason={}",
                    escape_inline_code(&miss.query),
                    miss.status,
                    miss.expected_host.as_deref().unwrap_or("unknown"),
                    miss.expected_source_class.as_deref().unwrap_or("unknown"),
                    miss.provider.as_deref().unwrap_or("unknown"),
                    miss.reason.as_deref().unwrap_or("none")
                )
            }));
            finalization.coverage_miss_count = visible_coverage_misses.len();
        }
    }

    lines.join("\n")
}

fn render_markdown_debt_line(debt: &&ResearchDebtItem) -> String {
    format!(
        "- {} [{}]: {} | next={}",
        debt.id,
        debt.status,
        escape_markdown_table(&debt.missing_evidence),
        escape_markdown_table(&debt.next_check_actions.join("; "))
    )
}

fn render_markdown_quality_gate(artifacts: &ResearchControllerArtifacts) -> String {
    let status = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.status.as_str())
        .unwrap_or("unknown");
    let failure_messages = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.failure_messages.join(" | "))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "none".to_string());
    let unsupported = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.unsupported_claim_count)
        .unwrap_or_default();
    let unresolved = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.unresolved_conflict_count)
        .unwrap_or_default();
    let open_debt = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.open_debt_count)
        .unwrap_or_default();
    format!(
        "| Check | Result | Note |\n| --- | --- | --- |\n| deterministic gate status | {} | artifact-backed finalization rendered visible verification sections before validation |\n| failure messages | {} | {} |\n| unsupported claim count | {} | lower is better |\n| unresolved conflict count | {} | unresolved items must stay visible as debt or limits |\n| open debt count | {} | open debt never counts as acceptance |",
        status,
        if failure_messages == "none" { "pass" } else { "review" },
        escape_markdown_table(&failure_messages),
        unsupported,
        unresolved,
        open_debt
    )
}

fn render_html_source_audit_rows(artifacts: &ResearchControllerArtifacts) -> String {
    artifacts
        .source_cards
        .iter()
        .map(|card| {
            format!(
                "<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&card.id),
                escape_html(&card.url),
                escape_html(&card.title),
                escape_html(&card.source_class),
                escape_html(&card_checked_fact(card, artifacts)),
                escape_html(card.limitation.as_deref().unwrap_or("-")),
                escape_html(card.diagnostics_ref.as_deref().unwrap_or("-")),
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_html_claim_log_rows(artifacts: &ResearchControllerArtifacts) -> String {
    artifacts
        .claim_log
        .iter()
        .map(|claim| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&claim.id),
                escape_html(&claim.claim),
                escape_html(&render_claim_support(claim, artifacts)),
                escape_html(claim.confidence.as_deref().unwrap_or("-")),
                escape_html(claim.uncertainty_note.as_deref().unwrap_or("-")),
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_html_limits_section(
    artifacts: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    _finalization: &ResearchFinalizationDiagnostics,
) -> String {
    let mut parts = Vec::new();
    let (blocking_conflicts, deferred_conflicts) = partition_visible_conflicts(artifacts);
    let blocking_conflicts = blocking_conflicts
        .into_iter()
        .map(|conflict| {
            format!(
                "<li><strong>{}</strong>: {} ({})</li>",
                escape_html(&conflict.id),
                escape_html(&conflict.topic),
                escape_html(
                    conflict
                        .resolution_note
                        .as_deref()
                        .unwrap_or("resolution note unavailable")
                )
            )
        })
        .collect::<Vec<_>>();
    if !blocking_conflicts.is_empty() {
        parts.push(format!(
            "<h3>Unresolved Conflicts</h3><ul>{}</ul>",
            blocking_conflicts.join("")
        ));
    }
    let deferred_conflicts = deferred_conflicts
        .into_iter()
        .map(|conflict| {
            format!(
                "<li><strong>{}</strong>: {} (status={} | deferred to research debt | {})</li>",
                escape_html(&conflict.id),
                escape_html(&conflict.topic),
                escape_html(conflict.resolution_status.as_deref().unwrap_or("unknown")),
                escape_html(
                    conflict
                        .resolution_note
                        .as_deref()
                        .unwrap_or("resolution note unavailable")
                )
            )
        })
        .collect::<Vec<_>>();
    if !deferred_conflicts.is_empty() {
        parts.push(format!(
            "<h3>Deferred / Caveated Conflicts</h3><ul>{}</ul>",
            deferred_conflicts.join("")
        ));
    }
    let debts = artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .map(|debt| {
            format!(
                "<li><strong>{}</strong> [{}]: {} | next={}</li>",
                escape_html(&debt.id),
                escape_html(&debt.status),
                escape_html(&debt.missing_evidence),
                escape_html(&debt.next_check_actions.join("; "))
            )
        })
        .collect::<Vec<_>>();
    if !debts.is_empty() {
        parts.push(format!("<h3>Research Debt</h3><ul>{}</ul>", debts.join("")));
    }
    let visible_open_gaps = visible_narrative_open_gaps(artifacts);
    if !visible_open_gaps.is_empty() {
        let gaps = visible_open_gaps
            .iter()
            .map(|gap| {
                format!(
                    "<li>{}{}</li>",
                    escape_html(&gap.description),
                    gap.status
                        .as_deref()
                        .filter(|status| !status.is_empty())
                        .map(|status| format!(" (status={})", escape_html(status)))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();
        parts.push(format!(
            "<h3>Remaining Explanatory Limits</h3><ul>{}</ul>",
            gaps.join("")
        ));
    }
    if let Some(source_pack) = diagnostics.and_then(|envelope| envelope.source_pack.as_ref()) {
        if let Some(reason) = source_pack.reason.as_deref() {
            parts.push(format!(
                "<p><strong>Source-pack limitation:</strong> {}</p>",
                escape_html(&finalization_safe_diagnostic_text(reason))
            ));
        }
        let visible_coverage_misses = source_pack
            .coverage_misses
            .iter()
            .filter(|miss| miss.status != "adopted")
            .collect::<Vec<_>>();
        if !visible_coverage_misses.is_empty() {
            let misses = visible_coverage_misses
                .iter()
                .map(|miss| {
                    format!(
                        "<li>query=<code>{}</code> status={} expected_host={} expected_source_class={} provider={} reason={}</li>",
                        escape_html(&miss.query),
                        escape_html(&miss.status),
                        escape_html(miss.expected_host.as_deref().unwrap_or("unknown")),
                        escape_html(miss.expected_source_class.as_deref().unwrap_or("unknown")),
                        escape_html(miss.provider.as_deref().unwrap_or("unknown")),
                        escape_html(miss.reason.as_deref().unwrap_or("none")),
                    )
                })
                .collect::<Vec<_>>();
            parts.push(format!(
                "<h3>Target Host / Source Class Misses</h3><ul>{}</ul>",
                misses.join("")
            ));
        }
    }
    if parts.is_empty() {
        "<p>No unresolved conflicts or open research debt were recorded.</p>".to_string()
    } else {
        parts.join("")
    }
}

fn partition_visible_conflicts(
    artifacts: &ResearchControllerArtifacts,
) -> (
    Vec<&crate::models::ResearchConflictMapEntry>,
    Vec<&crate::models::ResearchConflictMapEntry>,
) {
    let mut blocking = Vec::new();
    let mut deferred = Vec::new();
    for conflict in &artifacts.conflict_map {
        if conflict.resolution_status.as_deref() == Some("resolved") {
            continue;
        }
        if conflict_is_resolved_or_actionably_deferred(conflict, &artifacts.research_debt) {
            deferred.push(conflict);
        } else {
            blocking.push(conflict);
        }
    }
    (blocking, deferred)
}

fn render_html_quality_gate(artifacts: &ResearchControllerArtifacts) -> String {
    let status = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.status.as_str())
        .unwrap_or("unknown");
    let failures = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.failure_messages.join(" | "))
        .unwrap_or_else(|| "none".to_string());
    let unsupported = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.unsupported_claim_count)
        .unwrap_or_default();
    let unresolved = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.unresolved_conflict_count)
        .unwrap_or_default();
    let debt = artifacts
        .quality_gate
        .as_ref()
        .map(|gate| gate.open_debt_count)
        .unwrap_or_default();
    format!(
        "<table><tbody><tr><th>deterministic gate status</th><td>{}</td></tr><tr><th>failure messages</th><td>{}</td></tr><tr><th>unsupported claim count</th><td>{}</td></tr><tr><th>unresolved conflict count</th><td>{}</td></tr><tr><th>open debt count</th><td>{}</td></tr></tbody></table>",
        escape_html(status),
        escape_html(&failures),
        unsupported,
        unresolved,
        debt
    )
}

fn card_checked_fact(card: &ResearchSourceCard, artifacts: &ResearchControllerArtifacts) -> String {
    artifacts
        .claim_log
        .iter()
        .find(|claim| {
            claim
                .support_source_card_ids
                .iter()
                .any(|id| id == &card.id)
        })
        .map(|claim| claim.claim.clone())
        .or_else(|| card.extracted_facts.first().cloned())
        .unwrap_or_else(|| "verified supporting fact not recorded".to_string())
}

fn render_claim_support(
    claim: &crate::models::ResearchClaimLogEntry,
    artifacts: &ResearchControllerArtifacts,
) -> String {
    let mut parts = claim.support_source_card_ids.clone();
    for url in &claim.support_urls {
        if !parts.iter().any(|existing| existing == url) {
            parts.push(url.clone());
        }
    }
    if parts.is_empty() {
        return "unsupported".to_string();
    }
    let source_map = artifacts
        .source_cards
        .iter()
        .map(|card| (card.id.clone(), card.url.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    parts
        .into_iter()
        .map(|part| {
            source_map
                .get(&part)
                .map(|url| format!("{part} ({url})"))
                .unwrap_or(part)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn output_research_artifact_json(artifacts: &ResearchControllerArtifacts) -> String {
    serde_json::to_string_pretty(artifacts).unwrap_or_else(|_| "{}".to_string())
}

fn pretty_research_artifact_json(artifacts: &ResearchControllerArtifacts) -> String {
    let compact = compact_research_artifacts_for_output(artifacts);
    output_research_artifact_json(&compact)
}

fn compact_research_artifacts_for_output(
    artifacts: &ResearchControllerArtifacts,
) -> ResearchControllerArtifacts {
    let mut compact = artifacts.clone();
    normalize_research_controller_artifacts(&mut compact);

    compact.events = tail_items(&compact.events, MAX_OUTPUT_ARTIFACT_EVENTS);
    compact
        .research_debt
        .retain(|debt| debt.status.trim() != "closed");
    compact
        .research_debt
        .truncate(MAX_OUTPUT_ARTIFACT_DEBT_ITEMS);
    compact.warnings.truncate(MAX_OUTPUT_ARTIFACT_WARNINGS);
    compact.conflict_map.truncate(MAX_OUTPUT_ARTIFACT_CONFLICTS);

    if let Some(state) = compact.narrative_state.as_mut() {
        state.timeline.clear();
        state.actors.clear();
        state.causal_chain.clear();
        state.evidence_layers.clear();
        state.interpretive_tensions.clear();
        state.impacts.clear();
        state.reader_questions.clear();
        state.section_outline.clear();
        state.transition_plan.clear();
        state.open_gaps.truncate(MAX_OUTPUT_ARTIFACT_DEBT_ITEMS);
        compact_event_cards_for_output(&mut state.event_cards, false);
    }

    if artifact_json_len(&compact) <= MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return compact;
    }

    compact.events.clear();
    compact.warnings.truncate(3);
    compact.research_debt.truncate(4);
    compact.conflict_map.truncate(4);
    compact.claim_log.truncate(MAX_OUTPUT_ARTIFACT_CLAIMS);
    compact_claim_log_for_output(&mut compact.claim_log, false);
    prioritize_source_cards_for_claims(&mut compact);
    compact_source_cards_for_output(&mut compact.source_cards, false);
    retain_source_cards_for_claims(&mut compact);
    compact
        .source_cards
        .truncate(MAX_OUTPUT_ARTIFACT_SOURCE_CARDS);
    prune_claims_to_available_sources(&mut compact);
    if let Some(state) = compact.narrative_state.as_mut() {
        compact_event_cards_for_output(&mut state.event_cards, true);
    }

    if artifact_json_len(&compact) <= MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return compact;
    }

    compact.research_debt.clear();
    compact.warnings.clear();
    compact.conflict_map.clear();
    if let Some(state) = compact.narrative_state.as_mut() {
        state.open_gaps.clear();
    }
    compact_claim_log_for_output(&mut compact.claim_log, true);
    prioritize_source_cards_for_claims(&mut compact);
    compact_source_cards_for_output(&mut compact.source_cards, true);
    prune_claims_to_available_sources(&mut compact);
    retain_source_cards_for_claims(&mut compact);
    compact_artifact_ledgers_until_within_limit(&mut compact);
    compact
}

fn compact_source_cards_for_output(
    cards: &mut Vec<crate::models::ResearchSourceCard>,
    aggressive: bool,
) {
    let max_cards = if aggressive {
        MAX_OUTPUT_ARTIFACT_SOURCE_CARDS_AGGRESSIVE
    } else {
        MAX_OUTPUT_ARTIFACT_SOURCE_CARDS
    };
    cards.truncate(max_cards);
    for card in cards.iter_mut() {
        card.title = normalize_text_field(&card.title, if aggressive { 72 } else { 120 });
        card.source_class =
            normalize_compact_field(&card.source_class, if aggressive { 20 } else { 28 });
        card.accessed_at = normalize_optional_text_field(
            card.accessed_at.take(),
            if aggressive { 20 } else { 32 },
        );
        normalize_string_list(
            &mut card.extracted_facts,
            if aggressive { 1 } else { 2 },
            if aggressive { 72 } else { 120 },
        );
        card.limitation = if aggressive {
            None
        } else {
            normalize_optional_text_field(card.limitation.take(), 96)
        };
        card.diagnostics_ref = if aggressive {
            None
        } else {
            normalize_optional_text_field(card.diagnostics_ref.take(), 32)
        };
        card.confidence =
            normalize_optional_text_field(card.confidence.take(), if aggressive { 16 } else { 24 });
    }
}

fn compact_claim_log_for_output(
    claims: &mut Vec<crate::models::ResearchClaimLogEntry>,
    aggressive: bool,
) {
    let max_claims = if aggressive {
        MAX_OUTPUT_ARTIFACT_CLAIMS_AGGRESSIVE
    } else {
        MAX_OUTPUT_ARTIFACT_CLAIMS
    };
    claims.truncate(max_claims);
    for claim in claims.iter_mut() {
        claim.claim = normalize_text_field(&claim.claim, if aggressive { 96 } else { 160 });
        claim.claim_type = normalize_optional_text_field(
            claim.claim_type.take(),
            if aggressive { 16 } else { 24 },
        );
        normalize_string_list(
            &mut claim.support_source_card_ids,
            if aggressive { 1 } else { 2 },
            if aggressive { 24 } else { 40 },
        );
        normalize_url_list(
            &mut claim.support_urls,
            if aggressive { 1 } else { 2 },
            if aggressive { 80 } else { 120 },
        );
        claim.confidence = normalize_optional_text_field(
            claim.confidence.take(),
            if aggressive { 16 } else { 24 },
        );
        claim.uncertainty_note = if aggressive {
            None
        } else {
            normalize_optional_text_field(claim.uncertainty_note.take(), 96)
        };
    }
}

fn compact_event_cards_for_output(
    cards: &mut Vec<crate::models::NarrativeEventCard>,
    aggressive: bool,
) {
    let max_cards = if aggressive {
        MAX_OUTPUT_ARTIFACT_EVENT_CARDS_AGGRESSIVE
    } else {
        MAX_OUTPUT_ARTIFACT_EVENT_CARDS
    };
    retain_event_card_sample_for_output(cards, max_cards);
    for card in cards.iter_mut() {
        card.label = normalize_text_field(&card.label, if aggressive { 48 } else { 72 });
        card.timeframe =
            normalize_optional_text_field(card.timeframe.take(), if aggressive { 24 } else { 40 });
        normalize_string_list(
            &mut card.actors,
            if aggressive { 1 } else { 2 },
            if aggressive { 40 } else { 64 },
        );
        card.region_or_front = normalize_optional_text_field(
            card.region_or_front.take(),
            if aggressive { 40 } else { 64 },
        );
        card.trigger =
            normalize_optional_text_field(card.trigger.take(), if aggressive { 72 } else { 120 });
        card.development = normalize_optional_text_field(
            card.development.take(),
            if aggressive { 96 } else { 144 },
        );
        card.outcome =
            normalize_optional_text_field(card.outcome.take(), if aggressive { 72 } else { 120 });
        normalize_string_list(
            &mut card.source_ids,
            if aggressive { 1 } else { 2 },
            if aggressive { 24 } else { 40 },
        );
        card.confidence =
            normalize_optional_text_field(card.confidence.take(), if aggressive { 16 } else { 24 });
        if aggressive {
            card.open_questions.clear();
        } else {
            normalize_string_list(&mut card.open_questions, 1, 72);
        }
    }
}

fn retain_event_card_sample_for_output(
    cards: &mut Vec<crate::models::NarrativeEventCard>,
    max_items: usize,
) {
    if cards.len() <= max_items {
        return;
    }
    if max_items == 0 {
        cards.clear();
        return;
    }

    let len = cards.len();
    let mut selected = Vec::new();
    push_unique_index(&mut selected, 0);
    if len > 1 {
        push_unique_index(&mut selected, 1);
    }
    push_unique_index(&mut selected, len - 1);

    for marker_set in [
        &[
            "republic",
            "공화정",
            "monarchy abolished",
            "왕정폐지",
            "왕정 폐지",
        ][..],
        &["thermidor", "테르미도르"][..],
        &[
            "european order",
            "유럽 질서",
            "congress of vienna",
            "vienna",
            "빈",
            "balance of power",
            "세력 균형",
            "restoration",
            "복고",
            "post-revolutionary order",
        ][..],
    ] {
        if selected.len() >= max_items {
            break;
        }
        if let Some(index) = cards
            .iter()
            .position(|card| event_card_contains_any_marker(card, marker_set))
        {
            push_unique_index(&mut selected, index);
        }
    }

    let step = (len as f64 / max_items.max(1) as f64).ceil().max(1.0) as usize;
    let mut index = 0;
    while selected.len() < max_items && index < len {
        push_unique_index(&mut selected, index);
        index = index.saturating_add(step);
    }
    let mut index = len.saturating_sub(1);
    while selected.len() < max_items {
        push_unique_index(&mut selected, index);
        if index == 0 {
            break;
        }
        index -= 1;
    }

    selected.sort_unstable();
    selected.truncate(max_items);
    let original = std::mem::take(cards);
    *cards = original
        .into_iter()
        .enumerate()
        .filter_map(|(index, card)| selected.contains(&index).then_some(card))
        .collect();
}

fn push_unique_index(indexes: &mut Vec<usize>, index: usize) {
    if !indexes.contains(&index) {
        indexes.push(index);
    }
}

fn event_card_contains_any_marker(
    card: &crate::models::NarrativeEventCard,
    markers: &[&str],
) -> bool {
    let text = [
        card.label.as_str(),
        card.timeframe.as_deref().unwrap_or_default(),
        card.region_or_front.as_deref().unwrap_or_default(),
        card.trigger.as_deref().unwrap_or_default(),
        card.development.as_deref().unwrap_or_default(),
        card.outcome.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    text_contains_any(&text, markers)
}

fn compact_artifact_ledgers_until_within_limit(artifacts: &mut ResearchControllerArtifacts) {
    if artifact_json_len(artifacts) <= MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return;
    }
    while artifact_json_len(artifacts) > MAX_RESEARCH_ARTIFACT_JSON_BYTES
        && artifacts.claim_log.len() > 1
    {
        artifacts.claim_log.pop();
        prioritize_source_cards_for_claims(artifacts);
        retain_source_cards_for_claims(artifacts);
        prune_claims_to_available_sources(artifacts);
    }
    while artifact_json_len(artifacts) > MAX_RESEARCH_ARTIFACT_JSON_BYTES
        && artifacts.source_cards.len() > 1
    {
        artifacts.source_cards.pop();
        prune_claims_to_available_sources(artifacts);
    }
    while artifact_json_len(artifacts) > MAX_RESEARCH_ARTIFACT_JSON_BYTES
        && artifacts
            .narrative_state
            .as_ref()
            .map(|state| state.event_cards.len() > 1)
            .unwrap_or(false)
    {
        if let Some(state) = artifacts.narrative_state.as_mut() {
            state.event_cards.pop();
        }
    }
    if artifact_json_len(artifacts) <= MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return;
    }
    if let Some(card) = artifacts.source_cards.first_mut() {
        card.extracted_facts.clear();
        card.limitation = None;
        card.diagnostics_ref = None;
        card.title = normalize_text_field(&card.title, 48);
        card.source_class = normalize_compact_field(&card.source_class, 16);
        card.accessed_at = None;
        card.confidence = normalize_optional_text_field(card.confidence.take(), 12);
    }
    for claim in &mut artifacts.claim_log {
        claim.claim = normalize_text_field(&claim.claim, 64);
        claim.support_source_card_ids.truncate(1);
        claim.support_urls.clear();
        claim.claim_type = None;
        claim.confidence = normalize_optional_text_field(claim.confidence.take(), 12);
        claim.uncertainty_note = None;
    }
    prioritize_source_cards_for_claims(artifacts);
    retain_source_cards_for_claims(artifacts);
    prune_claims_to_available_sources(artifacts);
    if artifact_json_len(artifacts) <= MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        return;
    }
    if let Some(card) = artifacts
        .narrative_state
        .as_mut()
        .and_then(|state| state.event_cards.first_mut())
    {
        card.actors.clear();
        card.source_ids.clear();
        card.open_questions.clear();
        card.label = normalize_text_field(&card.label, 32);
        card.timeframe = normalize_optional_text_field(card.timeframe.take(), 20);
        card.region_or_front = normalize_optional_text_field(card.region_or_front.take(), 32);
        card.trigger = normalize_optional_text_field(card.trigger.take(), 48);
        card.development = normalize_optional_text_field(card.development.take(), 72);
        card.outcome = normalize_optional_text_field(card.outcome.take(), 48);
        card.confidence = normalize_optional_text_field(card.confidence.take(), 16);
    }
    if artifact_json_len(artifacts) > MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        artifacts.claim_log.truncate(1);
        prioritize_source_cards_for_claims(artifacts);
        retain_source_cards_for_claims(artifacts);
        prune_claims_to_available_sources(artifacts);
    }
    if artifact_json_len(artifacts) > MAX_RESEARCH_ARTIFACT_JSON_BYTES {
        if let Some(state) = artifacts.narrative_state.as_mut() {
            state.event_cards.clear();
        }
    }
}

fn artifact_json_len(artifacts: &ResearchControllerArtifacts) -> usize {
    output_research_artifact_json(artifacts).len()
}

fn tail_items<T: Clone>(items: &[T], max_items: usize) -> Vec<T> {
    let start = items.len().saturating_sub(max_items);
    items[start..].to_vec()
}

fn prioritize_source_cards_for_claims(artifacts: &mut ResearchControllerArtifacts) {
    let referenced_source_ids = artifacts
        .claim_log
        .iter()
        .flat_map(|claim| claim.support_source_card_ids.iter())
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if referenced_source_ids.is_empty() || artifacts.source_cards.len() < 2 {
        return;
    }

    let mut referenced = Vec::new();
    let mut unreferenced = Vec::new();
    for card in artifacts.source_cards.drain(..) {
        if referenced_source_ids.contains(card.id.trim()) {
            referenced.push(card);
        } else {
            unreferenced.push(card);
        }
    }
    referenced.extend(unreferenced);
    artifacts.source_cards = referenced;
}

fn retain_source_cards_for_claims(artifacts: &mut ResearchControllerArtifacts) {
    let referenced_source_ids = artifacts
        .claim_log
        .iter()
        .flat_map(|claim| claim.support_source_card_ids.iter())
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if referenced_source_ids.is_empty() {
        return;
    }
    artifacts
        .source_cards
        .retain(|card| referenced_source_ids.contains(card.id.trim()));
}

fn prune_claims_to_available_sources(artifacts: &mut ResearchControllerArtifacts) {
    let available_source_ids = artifacts
        .source_cards
        .iter()
        .map(|card| card.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    for claim in &mut artifacts.claim_log {
        claim
            .support_source_card_ids
            .retain(|id| available_source_ids.contains(id.trim()));
    }
    artifacts.claim_log.retain(|claim| {
        !claim.support_source_card_ids.is_empty() || !claim.support_urls.is_empty()
    });
}

fn escape_markdown_table(value: &str) -> String {
    compact_text(value).replace('|', "\\|")
}

fn escape_inline_code(value: &str) -> String {
    compact_text(value).replace('`', "'")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn finalization_safe_diagnostic_text(value: &str) -> String {
    compact_text(value)
        .replace("api_key", "[redacted_key]")
        .replace("client_secret", "[redacted_key]")
        .replace("authorization", "[redacted_header]")
        .replace("raw provider payload", "[redacted_payload]")
        .replace("response body:", "[redacted_payload]:")
}

fn html_paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|paragraph| format!("{}", escape_html(paragraph.trim())))
        .collect::<Vec<_>>()
        .join("</p><p>")
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    trimmed
        .chars()
        .nth(level)
        .is_some_and(|ch| ch.is_whitespace())
        .then_some(level)
}

fn validate_high_resolution_final_answer(output: &str, failures: &mut Vec<String>) {
    let Some(final_answer) = final_answer_section(output) else {
        failures
            .push("high-intensity strict research must contain a Final Answer section".to_string());
        return;
    };

    let final_text = content_text(final_answer);
    let substantive_chars = final_text.chars().filter(|ch| ch.is_alphanumeric()).count();
    if substantive_chars < 450 {
        failures.push(format!(
            "final answer substantive length {substantive_chars} is below required minimum 450 for high-resolution research"
        ));
    }

    let sentence_count = final_text
        .chars()
        .filter(|ch| matches!(ch, '.' | '!' | '?' | '。' | '！' | '？'))
        .count();
    if sentence_count < 4 {
        failures.push(format!(
            "final answer sentence count {sentence_count} is below required minimum 4 for high-resolution research"
        ));
    }

    let dimension_count = final_answer_dimension_count(&final_text);
    if dimension_count < 3 {
        failures.push(format!(
            "final answer explanation coverage count {dimension_count} is below required minimum 3; expand the user-facing answer with concrete sequence/background, key actors or institutions, supporting reasoning, and practical limits or implications"
        ));
    }
}

fn final_answer_section(output: &str) -> Option<&str> {
    if let Some(section) = markdown_section_with_markers(output, FINAL_ANSWER_MARKERS) {
        return Some(section);
    }
    let start = find_first_marker(output, FINAL_ANSWER_MARKERS)?;
    let after_start = &output[start..];
    let end = find_first_marker(
        after_start,
        &[
            "source cards",
            "source audit",
            "claim log",
            "claim logs",
            "claim ledger",
            "claims and evidence",
            "claims & evidence",
            "quality gate",
            "quality check",
            "quality review",
            "출처 카드",
            "출처 감사",
            "주장 로그",
            "클레임 로그",
            "증거 매트릭스",
            "근거-주장 매트릭스",
            "근거-주장",
            "품질 게이트",
            "품질 검증",
            "품질 점검",
            "자체 품질 점검",
            "검증 보수 항목",
            "검증 항목",
        ],
    )
    .unwrap_or(after_start.len());
    Some(&after_start[..end])
}

fn claim_log_section(output: &str) -> Option<&str> {
    if let Some(section) = markdown_section_with_markers(output, CLAIM_LOG_MARKERS) {
        return Some(section);
    }
    let start = find_first_marker(output, CLAIM_LOG_MARKERS)?;
    let after_start = &output[start..];
    let end = find_first_marker(
        after_start,
        &[
            "\n## ",
            "\n### ",
            "\n***",
            "\n---",
            "quality gate",
            "quality check",
            "quality review",
            "품질 게이트",
            "품질 검증",
            "품질 점검",
            "자체 품질 점검",
            "검증 보수 항목",
            "검증 항목",
            "결론",
        ],
    )
    .unwrap_or(after_start.len());
    Some(&after_start[..end])
}

fn content_text(section: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in section.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    for url in extract_http_urls(section) {
        text = text.replace(&url, " ");
    }
    compact_text(&text)
}

fn final_answer_dimension_count(text: &str) -> usize {
    let lower = text.to_ascii_lowercase();
    let dimensions: &[&[&str]] = &[
        &[
            "년",
            "기간",
            "단계",
            "순서",
            "전후",
            "chronology",
            "timeline",
            "before",
            "after",
        ],
        &[
            "행위자",
            "인물",
            "기관",
            "정부",
            "부대",
            "기업",
            "actor",
            "stakeholder",
            "commander",
            "government",
        ],
        &[
            "원인", "배경", "때문", "이유", "동인", "cause", "because", "driver", "context",
        ],
        &[
            "한계",
            "불확실",
            "논쟁",
            "주의",
            "caveat",
            "limit",
            "uncertain",
            "dispute",
        ],
        &[
            "결과",
            "영향",
            "의미",
            "시사점",
            "consequence",
            "impact",
            "implication",
            "result",
        ],
    ];
    dimensions
        .iter()
        .filter(|markers| markers.iter().any(|marker| lower.contains(marker)))
        .count()
}

fn find_first_marker(text: &str, markers: &[&str]) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    markers
        .iter()
        .filter_map(|marker| lower.find(&marker.to_ascii_lowercase()))
        .min()
}

fn evidence_table_row_count(section: &str) -> usize {
    let markdown_rows = section
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('|')
                && trimmed.matches('|').count() >= 3
                && !trimmed.contains(":---")
                && !trimmed.contains("---")
                && !trimmed.contains("주장 (Claim)")
                && !trimmed.to_ascii_lowercase().contains("claim |")
        })
        .count();
    let html_rows = section.to_ascii_lowercase().matches("<tr").count();
    markdown_rows.max(html_rows.saturating_sub(1))
}

fn evidence_supported_claim_row_count(section: &str, output: &str) -> usize {
    let markdown_rows = section
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('|')
                && trimmed.matches('|').count() >= 3
                && !trimmed.contains(":---")
                && !trimmed.contains("---")
                && !trimmed.contains("주장 (Claim)")
                && !trimmed.to_ascii_lowercase().contains("claim |")
                && has_supported_claim_evidence(trimmed, output)
        })
        .count();
    let html_rows = html_table_row_segments(section)
        .into_iter()
        .filter(|row| has_supported_claim_evidence(row, output))
        .count();
    markdown_rows.max(html_rows)
}

fn has_supported_claim_evidence(fragment: &str, output: &str) -> bool {
    source_urls(fragment).iter().any(|url| is_evidence_url(url))
        || extract_source_refs(fragment)
            .iter()
            .any(|source_ref| source_ref_has_url_definition(output, source_ref))
        || extract_defined_source_card_refs(fragment, output)
            .iter()
            .any(|source_ref| source_ref_has_url_definition(output, source_ref))
}

fn conflict_is_resolved_or_actionably_deferred(
    conflict: &crate::models::ResearchConflictMapEntry,
    research_debt: &[crate::models::ResearchDebtItem],
) -> bool {
    if conflict
        .resolution_status
        .as_deref()
        .is_some_and(conflict_status_is_resolved)
    {
        return true;
    }
    conflict.promoted_to_debt == Some(true)
        && conflict_has_matching_actionable_open_debt(conflict, research_debt)
}

fn conflict_status_is_resolved(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    matches!(
        normalized.as_str(),
        "resolved" | "resolved_by_synthesis" | "resolved_by_evidence"
    )
}

pub(crate) fn conflict_has_matching_actionable_open_debt(
    conflict: &crate::models::ResearchConflictMapEntry,
    research_debt: &[crate::models::ResearchDebtItem],
) -> bool {
    research_debt
        .iter()
        .any(|debt| debt_is_actionable_open(debt) && debt_matches_conflict(debt, conflict))
}

fn debt_is_actionable_open(debt: &crate::models::ResearchDebtItem) -> bool {
    debt.status != "closed"
        && (!debt.candidate_queries.is_empty() || !debt.next_check_actions.is_empty())
}

pub(crate) fn debt_matches_conflict(
    debt: &crate::models::ResearchDebtItem,
    conflict: &crate::models::ResearchConflictMapEntry,
) -> bool {
    let debt_text = conflict_matchable_debt_text(debt);
    conflict_match_tokens(conflict)
        .into_iter()
        .any(|token| debt_text_contains_match_token(&debt_text, &token))
        || conflict_debt_has_meaningful_overlap(&debt_text, conflict)
}

fn conflict_debt_has_meaningful_overlap(
    debt_text: &str,
    conflict: &crate::models::ResearchConflictMapEntry,
) -> bool {
    let conflict_text = [
        conflict.topic.as_str(),
        conflict.resolution_note.as_deref().unwrap_or_default(),
    ]
    .join(" ");
    let tokens = significant_match_terms(&conflict_text);
    if tokens.len() < 3 {
        return false;
    }
    let matched = tokens
        .iter()
        .filter(|token| debt_text_contains_match_token(debt_text, token))
        .count();
    matched >= 3
}

fn significant_match_terms(text: &str) -> Vec<String> {
    let stop_words = [
        "needs", "need", "source", "card", "claim", "conflict", "requires", "with", "for", "the",
        "and", "자료", "부족", "확인", "제한", "필요",
    ];
    let mut terms = Vec::new();
    for raw in text.split_whitespace() {
        let term = raw
            .trim_matches(|ch: char| {
                ch.is_ascii_punctuation()
                    || matches!(
                        ch,
                        '“' | '”' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | '.'
                    )
            })
            .to_ascii_lowercase();
        let char_count = term.chars().count();
        if char_count < 3 && !term.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if stop_words.iter().any(|stop_word| *stop_word == term) {
            continue;
        }
        if !terms.iter().any(|existing| existing == &term) {
            terms.push(term);
        }
    }
    terms
}

fn debt_text_contains_match_token(debt_text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    let mut search_start = 0;
    while let Some(relative_idx) = debt_text[search_start..].find(token) {
        let start = search_start + relative_idx;
        let end = start + token.len();
        if token_match_has_boundaries(debt_text, start, end) {
            return true;
        }
        search_start = start + 1;
    }

    false
}

fn token_match_has_boundaries(text: &str, start: usize, end: usize) -> bool {
    boundary_char(text, start, BoundarySide::Left) && boundary_char(text, end, BoundarySide::Right)
}

#[derive(Clone, Copy)]
enum BoundarySide {
    Left,
    Right,
}

fn boundary_char(text: &str, index: usize, side: BoundarySide) -> bool {
    match side {
        BoundarySide::Left => text[..index]
            .chars()
            .next_back()
            .is_none_or(token_boundary_char),
        BoundarySide::Right => text[index..].chars().next().is_none_or(token_boundary_char),
    }
}

fn token_boundary_char(ch: char) -> bool {
    !ch.is_ascii_alphanumeric()
}

fn conflict_matchable_debt_text(debt: &crate::models::ResearchDebtItem) -> String {
    let mut parts = vec![
        debt.id.to_ascii_lowercase(),
        debt.missing_evidence.to_ascii_lowercase(),
    ];
    if let Some(failed_gate) = debt.failed_gate.as_ref() {
        parts.push(failed_gate.to_ascii_lowercase());
    }
    parts.extend(
        debt.candidate_queries
            .iter()
            .map(|query| query.to_ascii_lowercase()),
    );
    parts.extend(
        debt.next_check_actions
            .iter()
            .map(|action| action.to_ascii_lowercase()),
    );
    parts.join(" ")
}

fn conflict_match_tokens(conflict: &crate::models::ResearchConflictMapEntry) -> Vec<String> {
    let mut tokens = Vec::new();
    push_conflict_match_token(&mut tokens, Some(conflict.id.as_str()), 2);
    push_conflict_match_token(&mut tokens, Some(conflict.topic.as_str()), 8);
    push_conflict_match_token(&mut tokens, conflict.resolution_note.as_deref(), 12);
    for claim_id in &conflict.conflicting_claim_ids {
        push_conflict_match_token(&mut tokens, Some(claim_id), 2);
    }
    for source_card_id in &conflict.source_card_ids {
        push_conflict_match_token(&mut tokens, Some(source_card_id), 2);
    }
    tokens
}

fn push_conflict_match_token(tokens: &mut Vec<String>, value: Option<&str>, min_len: usize) {
    let Some(value) = value
        .map(str::trim)
        .filter(|value| value.chars().count() >= min_len)
    else {
        return;
    };
    let lowered = value.to_ascii_lowercase();
    if !tokens.iter().any(|existing| existing == &lowered) {
        tokens.push(lowered);
    }
}

fn html_table_row_segments(text: &str) -> Vec<&str> {
    let lower = text.to_ascii_lowercase();
    let mut rows = Vec::new();
    let mut cursor = 0;
    while let Some(start_rel) = lower[cursor..].find("<tr") {
        let start = cursor + start_rel;
        let Some(end_rel) = lower[start..].find("</tr>") else {
            break;
        };
        let end = start + end_rel + "</tr>".len();
        rows.push(&text[start..end]);
        cursor = end;
    }
    rows
}

fn unresolved_source_refs(section: &str, output: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for source_ref in extract_source_refs(section) {
        if !source_ref_has_url_definition(output, &source_ref) && !refs.contains(&source_ref) {
            refs.push(source_ref);
        }
    }
    refs
}

fn extract_source_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find("Source ") {
        let candidate = &rest[idx..];
        let digits = candidate["Source ".len()..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            let source_ref = format!("Source {digits}");
            if !refs.contains(&source_ref) {
                refs.push(source_ref);
            }
        }
        rest = &candidate["Source ".len()..];
    }
    refs
}

fn source_ref_has_url_definition(output: &str, source_ref: &str) -> bool {
    output
        .lines()
        .filter(|line| line_has_source_ref(line, source_ref))
        .any(|line| {
            extract_http_urls(line)
                .into_iter()
                .any(|url| is_evidence_url(&url))
        })
}

fn line_has_source_ref(line: &str, source_ref: &str) -> bool {
    text_contains_match_token(line, source_ref)
        || extract_source_refs(line)
            .iter()
            .any(|candidate| candidate == source_ref)
}

fn extract_defined_source_card_refs(fragment: &str, output: &str) -> Vec<String> {
    defined_source_card_refs(output)
        .into_iter()
        .filter(|source_ref| text_contains_match_token(fragment, source_ref))
        .collect()
}

fn defined_source_card_refs(output: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in output.lines() {
        if extract_http_urls(line)
            .into_iter()
            .any(|url| is_evidence_url(&url))
        {
            for cell in markdown_table_cells(line).into_iter().take(2) {
                if looks_like_source_card_ref(cell) && !refs.iter().any(|existing| existing == cell)
                {
                    refs.push(cell.to_string());
                }
            }
        }
    }
    refs
}

fn markdown_table_cells(line: &str) -> Vec<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return Vec::new();
    }
    trimmed
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect()
}

fn looks_like_source_card_ref(value: &str) -> bool {
    let upper = value.to_ascii_uppercase();
    let rest = upper
        .strip_prefix("SC-")
        .or_else(|| upper.strip_prefix("SC"))
        .or_else(|| upper.strip_prefix('S'));
    rest.is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn text_contains_match_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let text = text.to_ascii_lowercase();
    let token = token.to_ascii_lowercase();
    let mut search_start = 0;
    while let Some(relative_idx) = text[search_start..].find(&token) {
        let start = search_start + relative_idx;
        let end = start + token.len();
        if token_match_has_boundaries(&text, start, end) {
            return true;
        }
        search_start = start + 1;
    }
    false
}

fn required_source_url_count(intensity: Option<&str>, quality_depth: Option<&str>) -> usize {
    let base: usize = match intensity {
        Some("high") => 3,
        Some("medium") => 2,
        _ => 1,
    };
    match quality_depth {
        Some("light") => base.saturating_sub(1).max(1),
        Some("strict") if intensity == Some("high") => 7,
        _ => base,
    }
}

fn required_distinct_host_count(intensity: Option<&str>, quality_depth: Option<&str>) -> usize {
    match (intensity, quality_depth) {
        (Some("high"), Some("strict")) => 3,
        (Some("high"), _) => 2,
        _ => 1,
    }
}

fn required_authoritative_url_count(intensity: Option<&str>, quality_depth: Option<&str>) -> usize {
    match (intensity, quality_depth) {
        (Some("high"), Some("strict")) => 5,
        (Some("high"), _) => 2,
        _ => 1,
    }
}

fn validate_interactive_html_contract(output: &str, failures: &mut Vec<String>) {
    let document = ParsedHtml::parse_document(output);
    let tab_button_selector = Selector::parse("[data-tab]").unwrap();
    let tab_content_selector = Selector::parse(".tab-content").unwrap();
    let id_selector = Selector::parse("[id]").unwrap();
    let button_selector = Selector::parse("button").unwrap();

    for button in document.select(&button_selector) {
        let class = button.value().attr("class").unwrap_or_default();
        let is_interactive_tab = class.contains("tab")
            || button.value().attr("data-tab").is_some()
            || button.value().attr("@click").is_some();
        if is_interactive_tab {
            let label = button.text().collect::<Vec<_>>().join(" ");
            if compact_text(&label).is_empty() {
                failures.push("interactive tab button must have visible label text".to_string());
            }
        }
    }

    let tab_ids = document
        .select(&tab_button_selector)
        .filter(|node| {
            node.value()
                .attr("class")
                .map(|class| class.contains("tab"))
                .unwrap_or(false)
        })
        .filter_map(|node| node.value().attr("data-tab"))
        .collect::<std::collections::HashSet<_>>();

    if tab_ids.is_empty() {
        return;
    }

    let content_ids = document
        .select(&tab_content_selector)
        .filter_map(|node| node.value().attr("data-tab"))
        .collect::<std::collections::HashSet<_>>();
    let pane_ids = document
        .select(&id_selector)
        .filter(|node| {
            node.value()
                .attr("class")
                .map(|class| class.contains("tab"))
                .unwrap_or(false)
        })
        .filter_map(|node| node.value().attr("id"))
        .collect::<std::collections::HashSet<_>>();

    let missing = tab_ids
        .iter()
        .filter(|tab_id| !content_ids.contains(**tab_id) && !pane_ids.contains(**tab_id))
        .copied()
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        failures.push(format!(
            "interactive tab contract is broken: missing matching tab content or pane for {}",
            missing.join(", ")
        ));
    }
}

fn validate_runtime_contract(output: &str, failures: &mut Vec<String>) {
    let lower = output.to_ascii_lowercase();
    let uses_alpine = lower.contains("x-data")
        || lower.contains("x-show")
        || lower.contains("x-bind:")
        || lower.contains("@click=");
    let loads_alpine = lower.contains("alpinejs") || lower.contains("alpine.js");
    if uses_alpine && !loads_alpine {
        failures.push(
            "HTML uses Alpine-style runtime attributes without loading Alpine or replacing them with plain JavaScript"
                .to_string(),
        );
    }
}

fn compact_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn preserve_markdown_paragraph_breaks(text: &str) -> String {
    let normalized = text.replace('\r', "");
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();
    for line in normalized.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        current.push(line.trim_end().to_string());
    }
    if !current.is_empty() {
        paragraphs.push(current.join("\n"));
    }
    paragraphs.join("\n\n").trim().to_string()
}

fn source_urls(output: &str) -> Vec<String> {
    extract_http_urls(output)
        .into_iter()
        .filter(|url| is_evidence_url(url))
        .collect()
}

fn merged_evidence_urls(
    output: &str,
    artifacts: Option<&ResearchControllerArtifacts>,
    file_type: &str,
) -> Vec<String> {
    let mut urls = visible_source_urls(output);
    if let Some(artifacts) = artifacts {
        for url in referenced_source_card_urls(&artifacts) {
            if !urls.iter().any(|existing| existing == &url) {
                urls.push(url);
            }
        }
    } else if let Ok(artifacts) = parse_research_artifact_block(output, file_type) {
        for url in referenced_source_card_urls(&artifacts) {
            if !urls.iter().any(|existing| existing == &url) {
                urls.push(url);
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

fn authoritative_evidence_url_count(
    urls: &[String],
    artifacts: Option<&ResearchControllerArtifacts>,
    evidence_subject: Option<&str>,
) -> usize {
    let mut authoritative_urls = urls
        .iter()
        .filter(|url| is_authoritative_evidence_url(url, evidence_subject))
        .cloned()
        .collect::<HashSet<_>>();

    if let Some(artifacts) = artifacts {
        for url in referenced_authoritative_source_card_urls(artifacts, evidence_subject) {
            authoritative_urls.insert(url);
        }
    }

    authoritative_urls.len()
}

fn referenced_authoritative_source_card_urls(
    artifacts: &ResearchControllerArtifacts,
    evidence_subject: Option<&str>,
) -> Vec<String> {
    let valid_source_card_ids = artifacts
        .source_cards
        .iter()
        .filter(|card| valid_http_source_url(&card.url) && is_evidence_url(&card.url))
        .map(|card| card.id.trim().to_string())
        .collect::<HashSet<_>>();
    let authoritative_source_card_urls = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            valid_http_source_url(&card.url)
                && is_evidence_url(&card.url)
                && (source_class_is_authoritative(&card.source_class)
                    || is_authoritative_evidence_url(&card.url, evidence_subject))
        })
        .map(|card| (card.id.trim().to_string(), card.url.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut urls = Vec::new();
    for claim in &artifacts.claim_log {
        let ids_are_valid = claim
            .support_source_card_ids
            .iter()
            .all(|id| valid_source_card_ids.contains(id.trim()));
        let urls_are_valid = claim
            .support_urls
            .iter()
            .all(|url| valid_http_source_url(url) && is_evidence_url(url));
        if !ids_are_valid || !urls_are_valid {
            continue;
        }
        for source_card_id in &claim.support_source_card_ids {
            if let Some(url) = authoritative_source_card_urls.get(source_card_id.trim()) {
                if !urls.iter().any(|existing| existing == url) {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls
}

fn source_class_is_authoritative(source_class: &str) -> bool {
    let lower = source_class.to_ascii_lowercase();
    lower.contains("official")
        || lower.contains("primary")
        || lower.contains("government")
        || lower.contains("public_institution")
        || lower.contains("project_docs")
        || lower.contains("vendor")
        || source_class.contains("공식")
        || source_class.contains("제조사")
        || source_class.contains("1차")
}

fn visible_source_urls(output: &str) -> Vec<String> {
    source_urls(&strip_research_artifact_blocks(output))
}

pub fn strip_research_artifact_blocks(output: &str) -> String {
    let mut stripped = output.to_string();
    let mut ranges = scan_markdown_research_artifact_blocks(&stripped).ranges;
    ranges.extend(scan_html_research_artifact_blocks(&stripped).ranges);
    let ranges = merge_artifact_block_ranges(ranges);
    for (start, end) in ranges.into_iter().rev() {
        stripped.replace_range(start..end, "");
    }
    stripped
}

fn merge_artifact_block_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_by_key(|(start, end)| (*start, *end));
    let mut merged = Vec::with_capacity(ranges.len());
    let mut current = ranges[0];
    for (start, end) in ranges.into_iter().skip(1) {
        if start <= current.1 {
            current.1 = current.1.max(end);
        } else {
            merged.push(current);
            current = (start, end);
        }
    }
    merged.push(current);
    merged
}

fn referenced_source_card_urls(artifacts: &ResearchControllerArtifacts) -> Vec<String> {
    let valid_source_card_urls = artifacts
        .source_cards
        .iter()
        .filter(|card| valid_http_source_url(&card.url) && is_evidence_url(&card.url))
        .map(|card| (card.id.trim().to_string(), card.url.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut urls = Vec::new();
    for claim in &artifacts.claim_log {
        let has_support =
            !claim.support_source_card_ids.is_empty() || !claim.support_urls.is_empty();
        if !has_support {
            continue;
        }
        let ids_are_valid = claim
            .support_source_card_ids
            .iter()
            .all(|id| valid_source_card_urls.contains_key(id.trim()));
        let urls_are_valid = claim
            .support_urls
            .iter()
            .all(|url| valid_http_source_url(url) && is_evidence_url(url));
        if !ids_are_valid || !urls_are_valid {
            continue;
        }
        for source_card_id in &claim.support_source_card_ids {
            if let Some(url) = valid_source_card_urls.get(source_card_id.trim()) {
                if !urls.iter().any(|existing| existing == url) {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls
}

fn audit_source_url_count(output: &str) -> usize {
    let audit_start = output
        .find("출처 감사")
        .or_else(|| output.to_ascii_lowercase().find("source audit"));
    audit_start
        .map(|idx| {
            extract_http_urls(&output[idx..])
                .into_iter()
                .filter(|url| is_evidence_url(url))
                .count()
        })
        .unwrap_or(0)
}

fn extract_http_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for scheme in ["http://", "https://"] {
        let mut rest = text;
        while let Some(start) = rest.find(scheme) {
            let after = &rest[start..];
            let end = after
                .find(|ch: char| {
                    ch.is_whitespace()
                        || matches!(ch, '"' | '\'' | '<' | '>' | ')' | '(' | ']' | '[' | ',')
                })
                .unwrap_or(after.len());
            urls.push(
                after[..end]
                    .trim_end_matches(|ch: char| matches!(ch, '.' | ';' | ':' | '&'))
                    .to_string(),
            );
            rest = &after[end..];
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

fn is_evidence_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ![
        "cdn.tailwindcss.com",
        "tailwindcss.com",
        "w3.org",
        "schema.org",
        "example.com",
        "example.org",
        "example.net",
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "fonts.googleapis.com",
        "fonts.gstatic.com",
        "unpkg.com",
        "cdn.jsdelivr.net",
    ]
    .iter()
    .any(|blocked| lower.contains(blocked))
}

fn distinct_evidence_hosts(urls: &[String]) -> usize {
    let mut hosts = std::collections::HashSet::new();
    for url in urls {
        if let Some(host) = normalized_host(url) {
            hosts.insert(host);
        }
    }
    hosts.len()
}

fn is_authoritative_evidence_url(url: &str, evidence_subject: Option<&str>) -> bool {
    if is_subject_primary_source_url(url, evidence_subject) {
        return true;
    }
    let Some(host) = normalized_host(url) else {
        return false;
    };
    host.ends_with(".edu")
        || host.ends_with(".ac.uk")
        || is_official_government_or_military_host(&host)
        || is_public_institution_host(&host)
        || is_official_product_or_project_docs_host(&host)
        || host_matches_domain_boundary(&host, "cambridge.org")
        || host_matches_domain_boundary(&host, "academic.oup.com")
        || host_matches_domain_boundary(&host, "jstor.org")
        || host_matches_domain_boundary(&host, "degruyter.com")
        || host_matches_domain_boundary(&host, "britannica.com")
        || host_matches_domain_boundary(&host, "worldhistory.org")
        || host_matches_domain_boundary(&host, "livius.org")
        || host_matches_domain_boundary(&host, "cppreference.com")
        || host_matches_domain_boundary(&host, "uxlfoundation.github.io")
        || host_matches_domain_boundary(&host, "oneapi-spec.uxlfoundation.org")
        || host_matches_domain_boundary(&host, "taskflow.github.io")
        || host == "wikipedia.org"
        || host.ends_with(".wikipedia.org")
}

fn is_subject_primary_source_url(url: &str, evidence_subject: Option<&str>) -> bool {
    let Some(subject) = evidence_subject else {
        return false;
    };
    let Some((subject_owner, subject_repo)) = extract_owner_repo_from_subject(subject) else {
        return false;
    };
    let Some((url_owner, url_repo)) = extract_github_repo_from_url(url) else {
        return false;
    };
    subject_owner.eq_ignore_ascii_case(&url_owner) && subject_repo.eq_ignore_ascii_case(&url_repo)
}

fn extract_owner_repo_from_subject(subject: &str) -> Option<(String, String)> {
    subject.split_whitespace().find_map(|token| {
        let cleaned = token.trim_matches(|c: char| {
            matches!(c, '`' | '"' | '\'' | ',' | '.' | ')' | '(' | '[' | ']')
        });
        split_owner_repo(cleaned)
    })
}

fn extract_github_repo_from_url(url: &str) -> Option<(String, String)> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let mut segments = parsed.path_segments()?;
    match host.as_str() {
        "github.com" | "raw.githubusercontent.com" => {
            let owner = segments.next()?;
            let repo = segments.next()?;
            split_owner_repo(&format!("{owner}/{repo}"))
        }
        _ => None,
    }
}

fn split_owner_repo(candidate: &str) -> Option<(String, String)> {
    let trimmed = candidate.trim_start_matches('/').trim_end_matches('/');
    let (owner, repo) = trimmed.split_once('/')?;
    if owner.is_empty()
        || repo.is_empty()
        || repo.contains('/')
        || !owner.chars().all(is_repo_char)
        || !repo.chars().all(is_repo_char)
        || !owner.chars().any(|c| c.is_ascii_alphabetic())
        || !repo.chars().any(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn is_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn is_official_government_or_military_host(host: &str) -> bool {
    host.ends_with(".gov")
        || host.ends_with(".mil")
        || host.ends_with(".gov.uk")
        || host.ends_with(".gov.au")
        || host.ends_with(".govt.nz")
        || host.ends_with(".gc.ca")
        || host.ends_with(".go.kr")
        || host.ends_with(".go.jp")
        || host.ends_with(".gouv.fr")
        || host.ends_with(".bund.de")
        || host.ends_with(".admin.ch")
        || host.ends_with(".europa.eu")
}

fn is_public_institution_host(host: &str) -> bool {
    let exact_hosts = [
        "un.org",
        "worldbank.org",
        "imf.org",
        "oecd.org",
        "iea.org",
        "who.int",
        "wto.org",
        "nato.int",
        "usafacts.org",
    ];
    exact_hosts
        .iter()
        .any(|known| host == *known || host.ends_with(&format!(".{known}")))
}

fn is_official_product_or_project_docs_host(host: &str) -> bool {
    [
        "apple.com",
        "developer.apple.com",
        "support.apple.com",
        "lenovo.com",
        "support.lenovo.com",
        "nvidia.com",
        "developer.nvidia.com",
        "docs.nvidia.com",
        "pytorch.org",
        "rust-lang.org",
        "rfc-editor.org",
        "datatracker.ietf.org",
        "ietf.org",
        "iana.org",
        "kernel.org",
        "docs.kernel.org",
        "learn.microsoft.com",
        "azure.microsoft.com",
        "docs.aws.amazon.com",
        "cloud.google.com",
        "kubernetes.io",
    ]
    .iter()
    .any(|domain| host_matches_domain_boundary(host, domain))
}

fn host_matches_domain_boundary(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn normalized_host(raw_url: &str) -> Option<String> {
    Url::parse(raw_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
}

fn topic_terms(topic: Option<&str>, instructions: Option<&str>) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for source in [topic, instructions].into_iter().flatten() {
        for term in tokenize_terms(source) {
            if seen.insert(term.clone()) {
                terms.push(term);
            }
            if terms.len() >= 16 {
                return terms;
            }
        }
    }
    terms
}

fn tokenize_terms(text: &str) -> Vec<String> {
    let stopwords = [
        "대한",
        "대해",
        "각각",
        "전체",
        "정리",
        "조사",
        "보고서",
        "개요",
        "비교",
        "분석",
        "포함",
        "결과",
        "의미",
        "한계",
        "배경",
        "전황",
        "시간",
        "흐름",
        "다시",
        "한번",
        "깊게",
        "내용",
        "자료",
        "with",
        "from",
        "that",
        "this",
        "into",
        "about",
    ];
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ('가'..='힣').contains(&ch) {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            push_term(&mut tokens, &current, &stopwords);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_term(&mut tokens, &current, &stopwords);
    }
    tokens
}

fn push_term(tokens: &mut Vec<String>, token: &str, stopwords: &[&str]) {
    let char_count = token.chars().count();
    if char_count < 2 || stopwords.contains(&token) {
        return;
    }
    tokens.push(token.to_string());
}

fn required_topic_match_count(term_count: usize) -> usize {
    if term_count == 0 {
        0
    } else {
        ((term_count as f32) * 0.35).ceil() as usize
    }
}

fn output_contains_term(output: &str, term: &str) -> bool {
    output.to_ascii_lowercase().contains(term)
}

fn validate_topic_specific_anchors(
    output: &str,
    context: &ResearchQualityContext<'_>,
    failures: &mut Vec<String>,
) {
    let topic = context.research_topic.unwrap_or_default();
    if topic.contains("유스티니아누스") && (topic.contains("고트") || topic.contains("고토"))
    {
        let false_claim_patterns = [
            "노르만계 출신 장군 나르세스",
            "노르만계 장군 나르세스",
            "나르세스는 노르만",
            "평화 조약(Pragmatic Sanction)",
            "실질적인 평화 조약",
            "Pragmatic Sanction을 통해 마무리",
        ];
        for pattern in false_claim_patterns {
            if output.contains(pattern) {
                failures.push(format!(
                    "contains known false Justinian Gothic War claim pattern: {pattern}"
                ));
            }
        }

        let anchors = [
            ["시칠리아", "sicily"],
            ["나폴리", "naples"],
            ["로마", "rome"],
            ["라벤나", "ravenna"],
            ["토틸라", "totila"],
            ["타기나이", "taginae"],
            ["락타리우스", "lactarius"],
            ["국사조칙", "pragmatic sanction"],
        ];
        let matched = anchors
            .iter()
            .filter(|variants| {
                variants
                    .iter()
                    .any(|variant| output_contains_term(output, variant))
            })
            .count();
        if matched < 6 {
            failures.push(format!(
                "Justinian Gothic War anchor coverage {matched}/{} is below required 6",
                anchors.len()
            ));
        }
    }
}

fn validate_historical_supplementary_source_reliance(
    output: &str,
    artifacts: &ResearchControllerArtifacts,
    context: &ResearchQualityContext<'_>,
    failures: &mut Vec<String>,
) {
    let topic = context.research_topic.unwrap_or_default();
    if !history_like_topic(topic) {
        return;
    }
    let final_answer = final_answer_section(output)
        .map(section_body_without_heading)
        .unwrap_or_default();
    let has_visible_caveat = final_answer_has_historical_caveat_markers(&final_answer);
    let has_uncaveated_weak_claim = artifacts.claim_log.iter().any(|claim| {
        claim_uses_weak_historical_support(claim, artifacts)
            && !claim_has_strong_historical_corroboration(claim, artifacts, context)
    });
    if has_uncaveated_weak_claim && !has_visible_caveat {
        failures.push(
            "historical final answer relies on weak or contested supplementary sources without visible caveats or stronger corroborating evidence"
                .to_string(),
        );
    }
}

fn validate_historical_development_density(
    output: &str,
    context: &ResearchQualityContext<'_>,
    failures: &mut Vec<String>,
) {
    if !should_apply_historical_development_density_gate(context) {
        return;
    }

    let visible_output = strip_research_artifact_blocks(output);
    let final_answer = final_answer_section(&visible_output)
        .map(section_body_without_heading)
        .unwrap_or_else(|| visible_output.trim().to_string());
    let development_body = historical_development_body(&final_answer);
    let development_sentences = split_historical_reader_sentences(&development_body);
    let lower = final_answer.to_ascii_lowercase();
    let development_lower = development_body.to_ascii_lowercase();

    let chronology_signal_count = usize::from(contains_historical_year_marker(&final_answer))
        + usize::from(contains_any_marker(
            &lower,
            &[
                "먼저",
                "이후",
                "그 뒤",
                "다음",
                "후반",
                "전반",
                "초기",
                "중기",
                "말기",
                "국면",
                "단계",
                "전환점",
                "phase",
                "turning point",
            ],
        ))
        + usize::from(contains_any_marker(
            &lower,
            &[
                "개전", "침공", "진격", "후퇴", "포위", "공세", "방어", "campaign", "front",
            ],
        ));
    let has_actor_or_front_region =
        historical_development_actor_or_front_region_signal(&development_lower);
    let has_settlement_or_outcome_sequence =
        historical_development_outcome_signal(&development_lower);
    let has_cause_effect_sequence =
        historical_development_has_cause_to_next_phase_progression(&development_sentences);
    let distinct_phase_bucket_count = development_sentences
        .iter()
        .filter_map(|sentence| historical_development_phase_bucket(&sentence.to_ascii_lowercase()))
        .collect::<HashSet<_>>()
        .len();
    let concrete_phase_sentence_count = development_sentences
        .iter()
        .filter(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            (contains_historical_year_marker(sentence)
                || historical_development_phase_bucket(&lower).is_some())
                && historical_development_event_signal(&lower)
        })
        .count();
    let concrete_development_chars = development_sentences
        .iter()
        .filter(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            ((contains_historical_year_marker(sentence)
                || historical_development_phase_bucket(&lower).is_some())
                && historical_development_event_signal(&lower))
                || historical_development_outcome_signal(&lower)
        })
        .map(|sentence| sentence.chars().count())
        .sum::<usize>();

    if chronology_signal_count < 2
        || distinct_phase_bucket_count < 2
        || concrete_phase_sentence_count < 2
        || concrete_development_chars < 120
        || !has_actor_or_front_region
        || !has_settlement_or_outcome_sequence
        || !has_cause_effect_sequence
    {
        failures.push(
            "historical development density is below required minimum for a strict event/war report: visible chronology phases, actors/fronts or regions, treaty/settlement or outcome sequence, and cause-effect progression must appear before significance prose"
                .to_string(),
        );
    }
}

fn validate_historical_event_card_development_density(
    artifacts: &ResearchControllerArtifacts,
    context: &ResearchQualityContext<'_>,
    failures: &mut Vec<String>,
) {
    if !should_apply_historical_development_density_gate(context) {
        return;
    }
    let diagnostics = artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            historical_event_card_missing_diagnostics_for_context(&state.event_cards, context)
        })
        .unwrap_or_else(|| historical_event_card_missing_diagnostics_for_context(&[], context));
    if diagnostics.is_empty() {
        return;
    }

    failures.push(format!(
        "historical event scaffold is too shallow for a strict event/process report: {}",
        diagnostics.join(", ")
    ));
}

fn historical_event_card_missing_diagnostics_for_context(
    cards: &[crate::models::NarrativeEventCard],
    context: &ResearchQualityContext<'_>,
) -> Vec<&'static str> {
    let mut missing = historical_event_card_missing_diagnostics(cards);
    if broad_historical_event_or_process_topic(context) && cards.len() < 6 {
        missing.insert(
            0,
            "broad historical event/process topics still need at least 6 distinct phase cards",
        );
    }
    for diagnostic in historical_event_card_scope_anchor_diagnostics(cards, context) {
        if !missing.contains(&diagnostic) {
            missing.push(diagnostic);
        }
    }
    missing
}

fn historical_event_card_scope_anchor_diagnostics(
    cards: &[crate::models::NarrativeEventCard],
    context: &ResearchQualityContext<'_>,
) -> Vec<&'static str> {
    if !broad_historical_event_or_process_topic(context) {
        return Vec::new();
    }

    let request_text = historical_event_card_request_text(context);
    let card_text = historical_event_card_scope_text(cards);
    let mut missing = Vec::new();

    if text_contains_any(
        &request_text,
        &[
            "republic",
            "공화정",
            "왕정폐지",
            "왕정 폐지",
            "monarchy abolished",
        ],
    ) && !text_contains_any(
        &card_text,
        &[
            "republic",
            "공화정",
            "왕정폐지",
            "왕정 폐지",
            "왕정 폐지",
            "monarchy abolished",
        ],
    ) {
        missing.push("requested republican transition is still missing from phase cards");
    }

    if text_contains_any(&request_text, &["thermidor", "테르미도르"])
        && !text_contains_any(&card_text, &["thermidor", "테르미도르"])
    {
        missing
            .push("requested thermidor or later reaction phase is still missing from phase cards");
    }

    if text_contains_any(
        &request_text,
        &[
            "european order",
            "유럽 질서",
            "international order",
            "regional order",
            "restoration settlement",
            "restoration",
            "postwar settlement",
            "전후 질서",
            "복고 체제",
        ],
    ) && !text_contains_any(
        &card_text,
        &[
            "european order",
            "유럽 질서",
            "international order",
            "regional order",
            "balance of power",
            "세력 균형",
            "restoration",
            "복고",
            "외교 질서",
            "전후 질서",
        ],
    ) {
        missing.push("requested later settlement or wider-order impact phase is still missing from phase cards");
    }

    missing
}

pub(crate) fn historical_event_card_missing_diagnostics(
    cards: &[crate::models::NarrativeEventCard],
) -> Vec<&'static str> {
    let detailed_development_count = cards
        .iter()
        .filter(|card| historical_event_card_has_development_detail(card))
        .count();
    let actor_rich_count = cards
        .iter()
        .filter(|card| historical_event_card_has_actor_context(card))
        .count();
    let region_context_count = cards
        .iter()
        .filter(|card| historical_event_card_has_region_context(card))
        .count();
    let trigger_count = cards
        .iter()
        .filter(|card| historical_event_card_has_concrete_trigger(card))
        .count();
    let outcome_count = cards
        .iter()
        .filter(|card| historical_event_card_has_outcome(card))
        .count();
    let progression_count = cards
        .windows(2)
        .filter(|window| {
            historical_event_card_has_outcome(&window[0])
                && historical_event_card_has_concrete_trigger(&window[1])
        })
        .count();

    let mut missing = Vec::new();
    if cards.len() < 2 {
        missing.push("multiple phase cards are still missing");
    }
    if trigger_count < cards.len() {
        missing.push("some phase cards still omit a concrete trigger or cause");
    }
    if actor_rich_count < cards.len() {
        missing.push("some phase cards still omit main actors or institutions");
    }
    if region_context_count < cards.len() {
        missing.push("some phase cards still omit front or place context");
    }
    if detailed_development_count < cards.len() {
        missing.push("some phase cards still omit visible development detail");
    }
    if outcome_count < cards.len() {
        missing.push("some phase cards still omit phase outcome or next-step consequence");
    }
    if progression_count < cards.len().saturating_sub(1) {
        missing.push("cause-to-next-phase progression is still missing between phases");
    }
    missing
}

fn broad_historical_event_or_process_topic(context: &ResearchQualityContext<'_>) -> bool {
    if !should_apply_historical_development_density_gate(context) {
        return false;
    }

    let topic_and_subject = [
        context.research_topic.unwrap_or_default(),
        context.evidence_subject.unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let combined = [
        context.research_topic.unwrap_or_default(),
        context.research_instructions.unwrap_or_default(),
        context.evidence_subject.unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();

    if [
        "전쟁",
        "혁명",
        "반란",
        "봉기",
        "공방",
        "포위",
        "침공",
        "원정",
        "캠페인",
        "과정",
    ]
    .iter()
    .any(|marker| combined.contains(marker))
    {
        return true;
    }

    [
        "campaign",
        "war",
        "wars",
        "revolution",
        "revolt",
        "rebellion",
        "uprising",
        "siege",
        "invasion",
        "process",
    ]
    .iter()
    .any(|marker| contains_ascii_word_like(&topic_and_subject, marker))
}

fn contains_ascii_word_like(text: &str, needle: &str) -> bool {
    debug_assert!(needle.is_ascii());
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
        return false;
    }

    bytes
        .windows(needle_bytes.len())
        .enumerate()
        .any(|(idx, window)| {
            window == needle_bytes
                && is_ascii_word_start_boundary(bytes, idx)
                && is_ascii_word_boundary(bytes, idx + needle_bytes.len())
        })
}

fn is_ascii_word_start_boundary(bytes: &[u8], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    !bytes[idx - 1].is_ascii_alphanumeric()
}

fn is_ascii_word_boundary(bytes: &[u8], idx: usize) -> bool {
    if idx >= bytes.len() {
        return true;
    }
    !bytes[idx].is_ascii_alphanumeric()
}

fn historical_event_card_request_text(context: &ResearchQualityContext<'_>) -> String {
    [
        context.research_topic.unwrap_or_default(),
        context.research_instructions.unwrap_or_default(),
        context.evidence_subject.unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase()
}

fn historical_event_card_scope_text(cards: &[crate::models::NarrativeEventCard]) -> String {
    cards
        .iter()
        .flat_map(|card| {
            [
                Some(card.label.as_str()),
                card.timeframe.as_deref(),
                card.region_or_front.as_deref(),
                card.trigger.as_deref(),
                card.development.as_deref(),
                card.outcome.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(card.actors.iter().map(String::as_str))
            .map(str::to_owned)
            .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn text_contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn historical_event_card_has_concrete_trigger(card: &crate::models::NarrativeEventCard) -> bool {
    card.trigger
        .as_deref()
        .is_some_and(|text| text.trim().chars().count() >= 8)
}

fn historical_event_card_has_actor_context(card: &crate::models::NarrativeEventCard) -> bool {
    card.actors
        .iter()
        .any(|actor| actor.trim().chars().count() >= 2)
}

fn historical_event_card_has_region_context(card: &crate::models::NarrativeEventCard) -> bool {
    card.region_or_front
        .as_deref()
        .is_some_and(|text| text.trim().chars().count() >= 2)
}

fn historical_event_card_has_development_detail(card: &crate::models::NarrativeEventCard) -> bool {
    card.development
        .as_deref()
        .is_some_and(|text| text.trim().chars().count() >= 60)
}

fn historical_event_card_has_outcome(card: &crate::models::NarrativeEventCard) -> bool {
    card.outcome
        .as_deref()
        .is_some_and(|text| text.trim().chars().count() >= 12)
}

fn historical_development_body(final_answer: &str) -> String {
    let sentences = split_historical_reader_sentences(final_answer);
    if sentences.is_empty() {
        return final_answer.trim().to_string();
    }

    let mut kept = Vec::new();
    for sentence in sentences {
        let lower = sentence.to_ascii_lowercase();
        let significance_only = historical_development_significance_signal(&lower)
            && !historical_development_event_signal(&lower)
            && historical_development_phase_bucket(&lower).is_none()
            && !historical_development_outcome_signal(&lower);
        if significance_only && !kept.is_empty() {
            break;
        }
        kept.push(sentence);
    }

    if kept.is_empty() {
        final_answer.trim().to_string()
    } else {
        kept.join(" ").trim().to_string()
    }
}

fn split_historical_reader_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    sentences
}

fn historical_development_phase_bucket(lower: &str) -> Option<&'static str> {
    if contains_any_marker(
        lower,
        &[
            "개전", "발발", "시작", "초기", "전반", "처음", "1789", "1740", "outbreak", "began",
            "initial", "early",
        ],
    ) {
        return Some("opening");
    }
    if contains_any_marker(
        lower,
        &[
            "이후",
            "그 뒤",
            "다음",
            "이어",
            "확대",
            "전환",
            "중기",
            "국면",
            "later",
            "then",
            "next",
            "subsequent",
            "expanded",
            "shifted",
        ],
    ) {
        return Some("middle");
    }
    if contains_any_marker(
        lower,
        &[
            "후반",
            "말기",
            "종전",
            "결국",
            "마침내",
            "종결",
            "협상",
            "조약",
            "강화",
            "정전",
            "반동",
            "폐지",
            "수립",
            "재편",
            "ended",
            "eventually",
            "final",
            "settlement",
            "treaty",
            "armistice",
            "abolished",
            "established",
            "reorganized",
        ],
    ) {
        return Some("resolution");
    }
    None
}

fn historical_development_event_signal(lower: &str) -> bool {
    contains_any_marker(
        lower,
        &[
            "침공",
            "점령",
            "봉기",
            "폐지",
            "수립",
            "선포",
            "소집",
            "공세",
            "후퇴",
            "포위",
            "협상",
            "서명",
            "체포",
            "축출",
            "동원",
            "반동",
            "진격",
            "invad",
            "capture",
            "storm",
            "abolish",
            "establish",
            "proclaim",
            "summon",
            "march",
            "negotia",
            "sign",
            "execute",
            "overthrow",
            "mobiliz",
            "reorgan",
        ],
    )
}

fn historical_development_actor_or_front_region_signal(lower: &str) -> bool {
    contains_any_marker(
        lower,
        &[
            "행위자",
            "세력",
            "동맹",
            "군",
            "왕조",
            "제국",
            "정부",
            "front",
            "region",
            "전선",
            "지역",
            "전역",
            "국경",
            "영토",
            "의회",
            "왕실",
            "국민의회",
            "군중",
            "assembly",
            "parliament",
            "convention",
            "monarchy",
            "government",
            "army",
            "troops",
        ],
    )
}

fn historical_development_outcome_signal(lower: &str) -> bool {
    contains_any_marker(
        lower,
        &[
            "조약",
            "강화",
            "협정",
            "정전",
            "화의",
            "종전",
            "settlement",
            "treaty",
            "peace",
            "armistice",
            "outcome",
            "aftermath",
            "귀결",
            "종결",
            "체제 재편",
            "왕정 폐지",
            "공화정 수립",
            "체제 전환",
            "정권 재편",
            "반동",
            "regime change",
            "republic",
            "abolition",
            "reorganization",
        ],
    )
}

fn historical_development_significance_signal(lower: &str) -> bool {
    contains_any_marker(
        lower,
        &[
            "영향",
            "의의",
            "의미",
            "significance",
            "impact",
            "legacy",
            "importance",
        ],
    )
}

fn historical_development_cause_signal(lower: &str) -> bool {
    contains_any_marker(
        lower,
        &[
            "원인", "배경", "이유", "동인", "왜", "때문", "because", "cause", "driver",
        ],
    )
}

fn historical_development_has_cause_to_next_phase_progression(sentences: &[String]) -> bool {
    let mut seen_cause = false;
    for sentence in sentences {
        let lower = sentence.to_ascii_lowercase();
        let has_cause = historical_development_cause_signal(&lower);
        let has_progression = (historical_development_phase_bucket(&lower).is_some()
            && historical_development_event_signal(&lower))
            || historical_development_outcome_signal(&lower);
        if (seen_cause || has_cause) && has_progression {
            return true;
        }
        if has_cause {
            seen_cause = true;
        }
    }
    false
}

fn history_like_topic(topic: &str) -> bool {
    let lower = topic.to_ascii_lowercase();
    [
        "historical",
        "history",
        "roman",
        "byzantine",
        "emperor",
        "emperors",
        "ancient",
        "dynasty",
        "war",
        "battle",
        "conflict",
        "succession",
        "treaty",
        "rebellion",
        "gothic war",
        "palmyrene",
        "gallic",
        "로마",
        "황제",
        "제국",
        "고대",
        "역사",
        "전쟁",
        "전투",
        "왕위계승",
        "조약",
        "반란",
        "왕조",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn has_explicit_historical_context_for_development_gate(topic: &str) -> bool {
    if has_named_historical_war_title(topic) || has_named_historical_event_title(topic) {
        return true;
    }
    let lower = topic.to_ascii_lowercase();

    [
        "roman",
        "byzantine",
        "emperor",
        "emperors",
        "ancient",
        "medieval",
        "dynasty",
        "dynastic",
        "empire",
        "historiography",
        "succession war",
        "gothic war",
        "역사",
        "로마",
        "황제",
        "제국",
        "고대",
        "중세",
        "왕조",
        "사료",
        "왕위계승전쟁",
        "계승전쟁",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn has_named_historical_war_title(topic: &str) -> bool {
    let lower = topic.to_ascii_lowercase();
    if ((lower.contains("war of ") || lower.contains("war of the "))
        && lower.contains("succession"))
        || lower.contains("world war")
        || lower.contains("years' war")
        || lower.contains("years war")
    {
        return true;
    }

    let normalized = topic
        .chars()
        .map(|ch| match ch {
            '\'' | '’' | '-' => ch,
            _ if ch.is_alphanumeric() || ch.is_whitespace() => ch,
            _ => ' ',
        })
        .collect::<String>();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(2) {
        let left = window[0];
        let right = window[1].to_ascii_lowercase();
        if right == "war" || right == "wars" {
            let left_lower = left.to_ascii_lowercase();
            if looks_like_historical_war_name_token(&left_lower) {
                return true;
            }
        }
    }
    false
}

fn has_named_historical_event_title(topic: &str) -> bool {
    let lower = topic.to_ascii_lowercase();
    if has_named_korean_historical_event_title(&lower) {
        return true;
    }

    let normalized = topic
        .chars()
        .map(|ch| match ch {
            '\'' | '’' | '-' => ch,
            _ if ch.is_alphanumeric() || ch.is_whitespace() => ch,
            _ => ' ',
        })
        .collect::<String>();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(2) {
        let left = window[0];
        let right = window[1].to_ascii_lowercase();
        if matches!(
            right.as_str(),
            "revolution" | "revolt" | "rebellion" | "uprising"
        ) {
            let left_lower = left.to_ascii_lowercase();
            if looks_like_historical_event_name_token(left, &left_lower) {
                return true;
            }
        }
    }
    false
}

fn has_named_korean_historical_event_title(lower: &str) -> bool {
    let has_event_marker = ["혁명", "반란", "봉기", "공방", "포위", "내전", "쿠데타"]
        .iter()
        .any(|marker| lower.contains(marker));
    if lower.contains("혁명")
        && has_modern_korean_revolution_modifier(lower)
        && !has_historical_korean_revolution_context(lower)
    {
        return false;
    }
    let has_industrial_revolution_marker =
        lower.contains("산업혁명") || (lower.contains("산업") && lower.contains("혁명"));
    if has_industrial_revolution_marker {
        if has_modern_korean_revolution_modifier(lower)
            && !has_historical_korean_revolution_context(lower)
        {
            return false;
        }
        return true;
    }
    let has_historical_qualifier = [
        "프랑스",
        "러시아",
        "미국",
        "영국",
        "중국",
        "아이티",
        "쿠바",
        "이란",
        "멕시코",
        "신해",
        "명예",
        "독립",
        "한니발",
        "포트 아서",
        "여순",
    ]
    .iter()
    .any(|marker| lower.contains(marker));

    has_event_marker && has_historical_qualifier
}

fn has_modern_korean_revolution_modifier(lower: &str) -> bool {
    [
        "ai",
        "인공지능",
        "생성형",
        "디지털",
        "소프트웨어",
        "플랫폼",
        "스타트업",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn has_historical_korean_revolution_context(lower: &str) -> bool {
    [
        "18세기",
        "19세기",
        "근대",
        "제1차",
        "제2차",
        "1차 산업",
        "2차 산업",
        "독립혁명",
        "독립 혁명",
        "프랑스혁명",
        "프랑스 혁명",
        "러시아혁명",
        "러시아 혁명",
        "신해혁명",
        "신해 혁명",
        "아이티혁명",
        "아이티 혁명",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn looks_like_historical_war_name_token(lower: &str) -> bool {
    if lower.is_empty() {
        return false;
    }
    if matches!(
        lower,
        "on" | "over" | "against" | "for" | "with" | "between"
    ) {
        return false;
    }
    if is_ascii_roman_numeral(lower) {
        return true;
    }
    lower.ends_with("ian")
        || lower.ends_with("onic")
        || lower.ends_with("ean")
        || lower.ends_with("lands")
        || matches!(
            lower,
            "vietnam" | "punic" | "crimean" | "korean" | "american" | "iraq" | "falklands" | "boer"
        )
}

fn looks_like_historical_event_name_token(original: &str, lower: &str) -> bool {
    if lower.is_empty() {
        return false;
    }
    if original.chars().all(|ch| !ch.is_ascii_lowercase()) {
        return false;
    }
    lower.ends_with("ian")
        || lower.ends_with("ese")
        || lower.ends_with("ish")
        || matches!(
            lower,
            "french"
                | "american"
                | "english"
                | "roman"
                | "ottoman"
                | "irish"
                | "greek"
                | "glorious"
                | "taiping"
                | "boxer"
                | "xinhai"
        )
}

fn is_ascii_roman_numeral(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    fn value(ch: char) -> Option<u32> {
        match ch {
            'i' => Some(1),
            'v' => Some(5),
            'x' => Some(10),
            'l' => Some(50),
            'c' => Some(100),
            'd' => Some(500),
            'm' => Some(1000),
            _ => None,
        }
    }

    let mut total = 0u32;
    let mut previous = 0u32;
    for ch in token.chars().rev() {
        let Some(current) = value(ch) else {
            return false;
        };
        if current < previous {
            total = total.saturating_sub(current);
        } else {
            total = total.saturating_add(current);
            previous = current;
        }
    }

    total > 0 && to_ascii_roman(total).as_deref() == Some(token)
}

fn to_ascii_roman(mut value: u32) -> Option<String> {
    if value == 0 || value > 3999 {
        return None;
    }

    let numerals = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];

    let mut result = String::new();
    for (number, numeral) in numerals {
        while value >= number {
            result.push_str(numeral);
            value -= number;
        }
    }
    Some(result)
}

fn should_apply_historical_development_density_gate(context: &ResearchQualityContext<'_>) -> bool {
    if context.research_intensity != Some("high") || context.quality_depth != Some("strict") {
        return false;
    }

    let subject_original = [
        context.research_topic.unwrap_or_default(),
        context.evidence_subject.unwrap_or_default(),
    ]
    .join(" ");
    if !has_explicit_historical_context_for_development_gate(&subject_original) {
        return false;
    }

    let combined_original = [
        context.research_topic.unwrap_or_default(),
        context.research_instructions.unwrap_or_default(),
        context.evidence_subject.unwrap_or_default(),
    ]
    .join(" ");
    let combined = combined_original.to_ascii_lowercase();

    let development_request_count = [
        "배경",
        "전개",
        "영향",
        "의의",
        "경과",
        "과정",
        "결과",
        "의미",
        "background",
        "development",
        "impact",
        "significance",
        "aftermath",
        "consequence",
    ]
    .iter()
    .filter(|marker| combined.contains(**marker))
    .count();
    let event_or_war_topic_korean = [
        "전쟁",
        "혁명",
        "왕위계승",
        "반란",
        "봉기",
        "공방",
        "포위",
        "내전",
        "쿠데타",
        "사건",
    ]
    .iter()
    .any(|marker| combined.contains(marker));
    let event_or_war_topic_ascii = [
        "conflict",
        "conflicts",
        "war",
        "wars",
        "battle",
        "battles",
        "campaign",
        "campaigns",
        "revolution",
        "revolutions",
        "revolt",
        "revolts",
        "succession",
        "rebellion",
        "rebellions",
        "uprising",
    ]
    .iter()
    .any(|marker| contains_ascii_word_like(&combined, marker));

    (event_or_war_topic_korean || event_or_war_topic_ascii) && development_request_count >= 2
}

fn contains_historical_year_marker(text: &str) -> bool {
    let chars = text.chars().collect::<Vec<_>>();
    for window in chars.windows(5) {
        if window[..4].iter().all(|ch| ch.is_ascii_digit())
            && matches!(window[4], '년' | '-' | '–' | '—' | '.')
        {
            return true;
        }
    }
    text.contains("BC")
        || text.contains("AD")
        || text.contains("BCE")
        || text.contains("CE")
        || text.contains("세기")
}

fn weak_historical_supplementary_source_card(card: &ResearchSourceCard) -> bool {
    weak_historical_supplementary_source_match(&card.title, &card.url)
}

fn weak_historical_supplementary_support_url(url: &str) -> bool {
    weak_historical_supplementary_source_match("", url)
}

fn weak_historical_supplementary_source_match(title: &str, url: &str) -> bool {
    let title = title.to_ascii_lowercase();
    let url = url.to_ascii_lowercase();
    title.contains("historia augusta")
        || url.contains("sha-")
        || url.contains("historia-augusta")
        || url.contains("sourcebooks.fordham.edu/ancient/")
}

fn strong_historical_corroborating_source_card(card: &ResearchSourceCard) -> bool {
    !weak_historical_supplementary_source_card(card)
        && (source_class_is_authoritative(&card.source_class)
            || is_authoritative_evidence_url(&card.url, None))
}

fn claim_uses_weak_historical_support(
    claim: &ResearchClaimLogEntry,
    artifacts: &ResearchControllerArtifacts,
) -> bool {
    claim.support_source_card_ids.iter().any(|source_card_id| {
        artifacts
            .source_cards
            .iter()
            .find(|card| card.id.trim() == source_card_id.trim())
            .is_some_and(weak_historical_supplementary_source_card)
    }) || claim
        .support_urls
        .iter()
        .any(|url| weak_historical_supplementary_support_url(url))
}

fn claim_has_strong_historical_corroboration(
    claim: &ResearchClaimLogEntry,
    artifacts: &ResearchControllerArtifacts,
    context: &ResearchQualityContext<'_>,
) -> bool {
    claim.support_source_card_ids.iter().any(|source_card_id| {
        artifacts
            .source_cards
            .iter()
            .find(|card| card.id.trim() == source_card_id.trim())
            .is_some_and(strong_historical_corroborating_source_card)
    }) || claim.support_urls.iter().any(|url| {
        !weak_historical_supplementary_support_url(url)
            && is_authoritative_evidence_url(url, context.evidence_subject)
    })
}

fn final_answer_has_historical_caveat_markers(final_answer: &str) -> bool {
    let lower = final_answer.to_ascii_lowercase();
    [
        "사료의 한계",
        "자료의 한계",
        "기록의 한계",
        "후대 서술",
        "문제 많은 사료",
        "논쟁",
        "이견",
        "불확실",
        "단정하기 어렵",
        "과장 가능성",
        "해석이 갈린",
        "견해가 갈린",
        "contested",
        "uncertain",
        "problematic source",
        "late tradition",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dense_final_answer() -> &'static str {
        "최종 답변: 이 조사의 핵심은 단일 결론을 빠르게 내리는 것이 아니라, 기간과 단계의 변화, 행위자별 이해관계, 원인과 배경, 한계와 불확실성, 결과와 영향을 함께 분리해서 읽는 데 있다. 먼저 시간 순서에서는 초기 조건이 형성된 뒤 주요 결정이 이어지고, 이후 실행 단계에서 관찰 가능한 결과가 누적되므로 전후 관계를 분리해야 한다. 다음으로 행위자와 기관은 같은 사건을 서로 다른 목적과 제약 속에서 해석하기 때문에, 정부, 기업, 현장 참여자, 외부 관찰자의 관점을 나누어 보아야 한다. 원인 측면에서는 제도적 배경, 경제적 동인, 기술적 조건, 사회적 기대가 함께 작동했으며 어느 하나만으로 전체 흐름을 설명하기 어렵다. 다만 공개 자료의 한계와 출처별 편향, 통계의 갱신 시점 차이가 있어 단정적 표현은 피해야 한다. 결과적으로 이 사안은 단기 효과보다 중장기 영향과 시사점을 함께 보아야 하며, 확인된 사실과 남은 불확실성을 구분할 때 사용자가 실제 판단에 쓸 수 있는 해상도가 확보된다. 따라서 최종 산출물은 짧은 요약이 아니라 핵심 주장마다 출처, 근거의 강도, 반대 가능성, 후속 확인 지점을 함께 제시해야 하며, 그래야 조사 요청자가 정보의 밀도와 신뢰도를 동시에 평가할 수 있다. 이런 구조는 표면적 요약을 줄이고 실제 검토 가능한 분석을 남긴다."
    }

    fn html_with_audit(urls: &[&str]) -> String {
        let rows = urls
            .iter()
            .map(|url| format!("<tr><td>{url}</td><td>확인 주장</td></tr>"))
            .collect::<String>();
        let claim_rows = urls
            .iter()
            .enumerate()
            .map(|(idx, url)| format!("| 주장 {} | {url} | 높음 |\n", idx + 1))
            .collect::<String>();
        format!(
            "<!DOCTYPE html><html><head><title>유스티니아누스 고토 수복</title></head><body><h1>유스티니아누스 고토 수복 전쟁</h1><section>Final Answer\n{}</section><section>Source Cards</section><section>Claim Log\n| Claim | Source URL | Confidence |\n| --- | --- | --- |\n{claim_rows}</section><section>Quality Gate</section><section>출처 감사<table>{rows}</table></section></body></html>",
            dense_final_answer()
        )
    }

    #[test]
    fn normalizes_markdown_fenced_html_output() {
        let raw = "Here is the report:\n```html\n<!DOCTYPE html><html><head><title>T</title></head><body>출처 감사 https://example.com</body></html>\n```";

        let normalized = normalize_ai_output(raw, "html");

        assert!(normalized.starts_with("<!DOCTYPE html>"));
        assert!(!normalized.contains("```"));
    }

    #[test]
    fn accepts_research_html_with_audit_urls_and_topic_terms() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + "<p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고토 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_high_intensity_research_with_only_weak_evidence_urls() {
        let output = html_with_audit(&[
            "https://namu.wiki/w/example-a",
            "https://thewiki.kr/w/example-b",
            "https://blog.example.com/example-c",
            "https://blog.example.com/example-d",
            "https://blog.example.com/example-e",
            "https://blog.example.com/example-f",
            "https://blog.example.com/example-g",
        ]);
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고토 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("authoritative evidence URL count"));
    }

    #[test]
    fn rejects_high_strict_research_with_bibliography_but_thin_claim_log() {
        let output = r#"
## 유스티니아누스 대제의 로마 고토 수복 전쟁 개요 보고서

### 1. 최종 보고서 (Final Answer)

유스티니아누스 대제의 로마 고토 수복 전쟁은 535년에서 554년 사이 이탈리아를 동로마 제국의 영토로 통합하려는 군사적, 정치적 캠페인이었습니다. 전쟁은 시칠리아에서 시작되어 나폴리와 로마로 이어졌고, 벨리사리우스와 나르세스가 주요 지휘관이었습니다. 라벤나 점령 이후에도 토틸라가 저항을 되살렸으나, 타기나이 전투와 락타리우스 산 전투 이후 고트족 저항은 약화되었습니다. 국사조칙은 이탈리아 통치 재편의 법적 장치였습니다.

### 2. 출처 감사 (Source Audit)

| URL | 확인된 주장 |
| :--- | :--- |
| https://www.worldhistory.org/Belisarius/ | 벨리사리우스의 역할 |
| https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554) | 전쟁 연대기 |
| https://www.britannica.com/biography/Justinian-I | 유스티니아누스와 국사조칙 |
| https://www.britannica.com/topic/Ostrogoth | 전쟁 배경 |
| https://www.worldhistory.org/Justinian_I/ | 유스티니아누스 개요 |
| https://www.britannica.com/biography/Narses-Byzantine-general | 나르세스의 역할 |
| https://www.worldhistory.org/Totila/ | 토틸라의 저항 |

### 3. 주장 로그 (Claim Log)

| 주장 (Claim) | 지원 출처 (Source ID/URL) | 신뢰도 |
| :--- | :--- | :--- |
| 전쟁은 535년부터 554년 사이에 발생했다. | Source 2 | 높음 |
| 전쟁의 배경은 고트족 왕국의 불안정성이다. | Source 4 | 높음 |
| 주요 지휘관은 벨리사리우스와 나르세스이다. | Source 1, Source 6 | 높음 |
| 토틸라는 고트족 저항을 부활시켰다. | Source 7 | 높음 |
| 국사조칙은 제국의 법적 정당성을 부여했다. | Source 3 | 높음 |
| 전쟁은 이탈리아 통제권 확보에 결정적이었다. | Source 5 | 높음 |

### 4. 품질 게이트

출처 감사, 주장 로그, 자체 품질 점검을 완료했습니다.
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();
        assert!(err.contains("claim log row count 6 is below required minimum 7"));
        assert!(
            err.contains("claim log supported evidence row count 0 is below required minimum 5")
        );
        assert!(err.contains("unresolved source references"));
    }

    #[test]
    fn rejects_high_strict_research_with_full_tables_but_short_final_answer() {
        let output = r#"
## 최종 답변 (Final Answer)
확인된 출처가 충분하므로 전체적으로 신뢰할 수 있습니다.

## 출처 감사 (Source Audit)
| URL | 확인 |
| --- | --- |
| https://www.congress.gov/crs-product/R48123 | 확인 |
| https://www.defense.gov/News/Releases/Release/Article/4150048/ | 확인 |
| https://www.usfj.mil/About-USFJ/ | 확인 |
| https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/ | 확인 |
| https://www.usfk.mil/About/USFK/ | 확인 |
| https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/ | 확인 |
| https://www.nato.int/cps/en/natohq/topics_67655.htm | 확인 |

## 주장 로그 (Claim Log)
| Claim | Source URL | Confidence |
| --- | --- | --- |
| 주장 1 | https://www.congress.gov/crs-product/R48123 | 높음 |
| 주장 2 | https://www.defense.gov/News/Releases/Release/Article/4150048/ | 높음 |
| 주장 3 | https://www.usfj.mil/About-USFJ/ | 높음 |
| 주장 4 | https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/ | 높음 |
| 주장 5 | https://www.usfk.mil/About/USFK/ | 높음 |
| 주장 6 | https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/ | 높음 |
| 주장 7 | https://www.nato.int/cps/en/natohq/topics_67655.htm | 높음 |

## 품질 게이트 (Quality Gate)
출처 감사, 주장 로그, 자체 품질 점검을 완료했습니다.
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("해외 미군의 배치 및 전력 분석"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();
        assert!(err.contains("final answer substantive length"));
        assert!(err.contains("final answer sentence count"));
        assert!(err.contains("final answer explanation coverage count"));
    }

    #[test]
    fn accepts_high_strict_claim_log_with_resolvable_source_card_ids() {
        let urls = [
            "https://www.congress.gov/crs-product/R48123",
            "https://www.defense.gov/News/Releases/Release/Article/4150048/",
            "https://www.usfj.mil/About-USFJ/",
            "https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/",
            "https://www.usfk.mil/About/USFK/",
            "https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/",
            "https://www.nato.int/cps/en/natohq/topics_67655.htm",
        ];
        let source_cards = urls
            .iter()
            .enumerate()
            .map(|(idx, url)| format!("Source {}: {url} - 확인된 출처 카드\n", idx + 1))
            .collect::<String>();
        let claim_rows = (1..=7)
            .map(|idx| format!("| 해외 미군 배치 분석 주장 {idx} | Source {idx} | 높음 |\n"))
            .collect::<String>();
        let audit_rows = urls
            .iter()
            .map(|url| format!("| {url} | 해외 미군 배치 분석 확인 |\n"))
            .collect::<String>();
        let output = format!(
            "## 최종 답변 (Final Answer)\n{}\n\n# 검증 부록\n\n## Source Cards\n{}\n## 주장 로그 (Claim Log)\n| Claim | Source ID | Confidence |\n| --- | --- | --- |\n{}\n## 출처 감사 (Source Audit)\n| URL | 확인 |\n| --- | --- |\n{}\n## 품질 게이트 (Quality Gate)\n자체 품질 점검 완료.",
            dense_final_answer(),
            source_cards,
            claim_rows,
            audit_rows
        );
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("해외 미군의 배치 및 전력 분석"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_strict_markdown_with_interleaved_verification_sections() {
        let urls = [
            "https://www.congress.gov/crs-product/R48123",
            "https://www.defense.gov/News/Releases/Release/Article/4150048/",
            "https://www.usfj.mil/About-USFJ/",
            "https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/",
            "https://www.usfk.mil/About/USFK/",
            "https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/",
            "https://www.nato.int/cps/en/natohq/topics_67655.htm",
        ];
        let claim_rows = urls
            .iter()
            .enumerate()
            .map(|(idx, url)| format!("| 해외 미군 배치 분석 주장 {} | {url} | 높음 |\n", idx + 1))
            .collect::<String>();
        let audit_rows = urls
            .iter()
            .map(|url| format!("| {url} | 해외 미군 배치 분석 확인 |\n"))
            .collect::<String>();
        let output = format!(
            "## 최종 답변 (Final Answer)\n{}\n\n## 출처 감사 (Source Audit)\n| URL | 확인 |\n| --- | --- |\n{}\n## 주장 로그 (Claim Log)\n| Claim | Source URL | Confidence |\n| --- | --- | --- |\n{}\n# 검증 부록\n## 품질 게이트 (Quality Gate)\n자체 품질 점검 완료.",
            dense_final_answer(),
            audit_rows,
            claim_rows
        );
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("해외 미군의 배치 및 전력 분석"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("verification sections before the final verification appendix"));
    }

    #[test]
    fn rejects_strict_markdown_without_top_level_verification_appendix() {
        let urls = [
            "https://www.congress.gov/crs-product/R48123",
            "https://www.defense.gov/News/Releases/Release/Article/4150048/",
            "https://www.usfj.mil/About-USFJ/",
            "https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/",
            "https://www.usfk.mil/About/USFK/",
            "https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/",
            "https://www.nato.int/cps/en/natohq/topics_67655.htm",
        ];
        let claim_rows = urls
            .iter()
            .enumerate()
            .map(|(idx, url)| format!("| 해외 미군 배치 분석 주장 {} | {url} | 높음 |\n", idx + 1))
            .collect::<String>();
        let audit_rows = urls
            .iter()
            .map(|url| format!("| {url} | 해외 미군 배치 분석 확인 |\n"))
            .collect::<String>();
        let output = format!(
            "## 최종 답변 (Final Answer)\n{}\n\n## 검증 부록\n## 출처 감사 (Source Audit)\n| URL | 확인 |\n| --- | --- |\n{}\n## 주장 로그 (Claim Log)\n| Claim | Source URL | Confidence |\n| --- | --- | --- |\n{}\n## 품질 게이트 (Quality Gate)\n자체 품질 점검 완료.",
            dense_final_answer(),
            audit_rows,
            claim_rows
        );
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("해외 미군의 배치 및 전력 분석"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("verification appendix must be a top-level"));
    }

    #[test]
    fn source_ref_matching_uses_exact_source_id_boundaries() {
        assert!(line_has_source_ref(
            "Source 1: https://www.congress.gov/crs-product/R48123",
            "Source 1"
        ));
        assert!(!line_has_source_ref(
            "Source 10: https://www.congress.gov/crs-product/R48123",
            "Source 1"
        ));
    }

    #[test]
    fn accepts_compact_html_claim_log_rows_on_one_line() {
        let urls = [
            "https://www.congress.gov/crs-product/R48123",
            "https://www.defense.gov/News/Releases/Release/Article/4150048/",
            "https://www.usfj.mil/About-USFJ/",
            "https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/",
            "https://www.usfk.mil/About/USFK/",
            "https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/",
            "https://www.nato.int/cps/en/natohq/topics_67655.htm",
        ];
        let claim_rows = urls
            .iter()
            .enumerate()
            .map(|(idx, url)| {
                format!(
                    "<tr><td>해외 미군 배치 분석 주장 {}</td><td>{url}</td><td>높음</td></tr>",
                    idx + 1
                )
            })
            .collect::<String>();
        let audit_rows = urls
            .iter()
            .map(|url| format!("<tr><td>{url}</td><td>확인</td></tr>"))
            .collect::<String>();
        let output = format!(
            "<!DOCTYPE html><html><head><title>해외 미군 배치 분석</title></head><body><section>Final Answer {}</section><section>Source Cards</section><section>Claim Log<table><tr><th>Claim</th><th>Source URL</th><th>Confidence</th></tr>{claim_rows}</table></section><section>Source Audit<table>{audit_rows}</table></section><section>Quality Gate 자체 품질 점검 완료</section></body></html>",
            dense_final_answer()
        );
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("해외 미군의 배치 및 전력 분석"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_placeholder_urls_as_evidence() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://example.com/archaeology/justinian",
        ]) + "<p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("source URL count 6 is below required minimum 7"));
    }

    #[test]
    fn rejects_research_html_without_audit_urls() {
        let output = "<!DOCTYPE html><html><head><title>T</title></head><body><h1>유스티니아누스 고토</h1><section>출처 감사 Wikipedia: Gothic War</section></body></html>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고토 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();
        assert!(err.contains("source URL count"));
        assert!(err.contains("source audit URL count"));
    }

    #[test]
    fn rejects_strict_research_without_controller_contract_sections() {
        let output = "<!DOCTYPE html><html><head><title>T</title></head><body><h1>유스티니아누스 고토</h1><section>출처 감사 https://www.britannica.com/biography/Justinian-I https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554) https://www.worldhistory.org/Justinian_I/ https://www.britannica.com/biography/Narses-Byzantine-general https://www.worldhistory.org/Totila/ https://www.britannica.com/topic/Ostrogoth https://www.worldhistory.org/Belisarius/</section><p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p></body></html>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();
        assert!(err.contains("controller contract is missing required sections"));
        assert!(err.contains("claim log"));
        assert!(err.contains("quality gate"));
    }

    #[test]
    fn accepts_korean_controller_contract_section_names() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + "<section>요약 결론</section><section>근거-주장 매트릭스</section><section>자체 품질 점검</section><p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn accepts_korean_final_answer_alias_conclusion_summary() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + "<section>결론 요약</section><section>근거-주장 매트릭스</section><section>자체 품질 점검</section><p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn accepts_korean_controller_contract_audit_aliases() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + "<section>최종 답변</section><section>증거 매트릭스</section><section>검증 보수 항목</section><p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn counts_official_and_public_institution_urls_as_authoritative() {
        let authoritative_urls = [
            "https://www.congress.gov/crs-product/R48123",
            "https://www.defense.gov/News/Releases/Release/Article/4150048/",
            "https://www.usfj.mil/About-USFJ/",
            "https://www.centcom.mil/ABOUT-US/POSTURE-STATEMENT/",
            "https://www.usfk.mil/About/USFK/",
            "https://usafacts.org/articles/where-are-us-military-members-stationed-and-why/",
            "https://www.nato.int/cps/en/natohq/topics_67655.htm",
        ];
        for url in authoritative_urls {
            assert!(
                is_authoritative_evidence_url(url, None),
                "expected authoritative URL: {url}"
            );
        }
        assert!(!is_authoritative_evidence_url(
            "https://blog.example.com/overseas-posture",
            None
        ));
    }

    #[test]
    fn counts_official_vendor_and_project_docs_as_authoritative() {
        let authoritative_urls = [
            "https://developer.apple.com/documentation/metal",
            "https://support.lenovo.com/us/en/solutions/ht516908",
            "https://docs.nvidia.com/cuda/cuda-installation-guide-linux/",
            "https://pytorch.org/get-started/locally/",
            "https://www.rust-lang.org/tools/install",
        ];

        for url in authoritative_urls {
            assert!(
                is_authoritative_evidence_url(url, None),
                "expected authoritative URL: {url}"
            );
        }

        assert!(!is_authoritative_evidence_url(
            "https://apple.com.attacker.example/fake-docs",
            None
        ));
    }

    #[test]
    fn counts_network_standards_and_cloud_vendor_docs_as_authoritative() {
        let authoritative_urls = [
            "https://www.rfc-editor.org/rfc/rfc6056",
            "https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml",
            "https://docs.kernel.org/networking/ip-sysctl.html",
            "https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/default-dynamic-port-range-tcpip-chang",
            "https://azure.microsoft.com/en-us/products/virtual-network",
            "https://docs.aws.amazon.com/vpc/latest/userguide/nat-gateway-working-with.html",
            "https://cloud.google.com/nat/docs/ports-and-addresses",
        ];

        for url in authoritative_urls {
            assert!(
                is_authoritative_evidence_url(url, None),
                "expected authoritative URL: {url}"
            );
        }

        assert!(!is_authoritative_evidence_url(
            "https://blog.example.com/ephemeral-port-explainer",
            None
        ));
    }

    #[test]
    fn counts_matching_repository_urls_as_subject_primary_evidence() {
        let subject = Some("Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심");
        assert!(is_authoritative_evidence_url(
            "https://github.com/sbluemin/fleet-harness",
            subject
        ));
        assert!(is_authoritative_evidence_url(
            "https://raw.githubusercontent.com/sbluemin/fleet-harness/main/CHANGELOG.md",
            subject
        ));
        assert!(!is_authoritative_evidence_url(
            "https://github.com/other/fleet-harness",
            subject
        ));
        assert!(!is_authoritative_evidence_url(
            "https://github.com/sbluemin/fleet-harness",
            None
        ));
    }

    #[test]
    fn accepts_high_research_with_matching_repository_primary_sources() {
        let output = "## 최종 답변\nSbluemin/fleet-harness의 작동 구조는 저장소 문서와 릴리스 문서를 통해 확인된다.\n\n## 출처 감사 (Source Audit)\n| URL | 확인 |\n| --- | --- |\n| https://github.com/sbluemin/fleet-harness | 저장소 |\n| https://raw.githubusercontent.com/sbluemin/fleet-harness/main/README.ko.md | README |\n| https://raw.githubusercontent.com/sbluemin/fleet-harness/main/CHANGELOG.md | 변경 기록 |\n\n## 주장 로그 (Claim Log)\n| Claim | Source URL | Confidence |\n| --- | --- | --- |\n| 저장소 구조 확인 | https://github.com/sbluemin/fleet-harness | 높음 |\n| README 확인 | https://raw.githubusercontent.com/sbluemin/fleet-harness/main/README.ko.md | 높음 |";
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("standard"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: Some("Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심"),
        };

        let result = validate_research_output(output, &context);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn merged_evidence_urls_include_only_supported_claim_backed_source_cards() {
        let output = r#"
## 최종 답변 (Final Answer)
노트북 비교 요약.

## 출처 감사 (Source Audit)
| URL | 확인 |
| --- | --- |
| https://reviews.samplecorp.com/laptops | secondary overview |

# Verification Appendix
## Claims and Evidence
| Claim | Evidence | Confidence |
| --- | --- | --- |
| Apple thermals | Source 1 | high |

## Quality Check
검토 완료.

[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {"id":"S1","url":"https://developer.apple.com/documentation/apple-silicon","title":"Apple Silicon","source_class":"official_or_primary"},
    {"id":"S2","url":"https://docs.nvidia.com/cuda/","title":"CUDA Docs","source_class":"official_or_primary"},
    {"id":"S3","url":"https://support.lenovo.com/us/en/solutions/laptop","title":"Lenovo Support","source_class":"official_or_primary"}
  ],
  "claim_log": [
    {"id":"C1","claim":"Apple claim","support_source_card_ids":["S1","S2"]},
    {"id":"C2","claim":"Broken claim","support_source_card_ids":["MISSING"]}
  ],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;

        let merged = merged_evidence_urls(output, None, "md");

        assert!(merged.contains(&"https://reviews.samplecorp.com/laptops".to_string()));
        assert!(
            merged.contains(&"https://developer.apple.com/documentation/apple-silicon".to_string())
        );
        assert!(merged.contains(&"https://docs.nvidia.com/cuda/".to_string()));
        assert!(!merged.contains(&"https://support.lenovo.com/us/en/solutions/laptop".to_string()));
    }

    #[test]
    fn rejects_multiple_markdown_research_artifact_json_blocks() {
        let output = r#"
[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
```
[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
```
"#;

        let err = parse_research_artifact_block(output, "md").unwrap_err();

        assert_eq!(
            err,
            "multiple machine-readable research artifact JSON blocks are not allowed"
        );
    }

    #[test]
    fn rejects_valid_markdown_artifact_block_with_malformed_extra_marker() {
        let output = r#"
[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
```
[RESEARCH_ARTIFACT_JSON]
"#;

        let err = parse_research_artifact_block(output, "md").unwrap_err();

        assert_eq!(
            err,
            "malformed machine-readable research artifact JSON block"
        );
    }

    #[test]
    fn ignores_data_research_artifacts_marker_inside_script_body() {
        let output = r#"
<script type="application/json">
data-research-artifacts
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
</script>
"#;

        let err = parse_research_artifact_block(output, "html").unwrap_err();

        assert_eq!(err, "missing machine-readable research artifact JSON block");
    }

    #[test]
    fn ignores_data_research_artifacts_marker_inside_other_attribute_value() {
        let output = r#"
<script type="application/json" data-note="data-research-artifacts">
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
</script>
"#;

        let err = parse_research_artifact_block(output, "html").unwrap_err();

        assert_eq!(err, "missing machine-readable research artifact JSON block");
    }

    #[test]
    fn ignores_data_research_artifacts_marker_inside_whitespace_padded_attribute_value() {
        let output = r#"
<script type="application/json" data-note=" data-research-artifacts ">
{"version":1,"source_cards":[],"claim_log":[],"conflict_map":[],"research_debt":[]}
</script>
"#;

        let err = parse_research_artifact_block(output, "html").unwrap_err();

        assert_eq!(err, "missing machine-readable research artifact JSON block");
    }

    #[test]
    fn counts_visible_source_card_ids_as_supported_claim_rows() {
        let output = r#"
## 최종 답변 (Final Answer)
노트북 비교 요약.

# 검증 부록
## Source Cards
| ID | URL | Title |
| --- | --- | --- |
| SC-01 | https://www.apple.com/macbook-pro/specs/ | Apple specs |
| SC-02 | https://www.asus.com/us/laptops/for-creators/proart/proart-p16-h7606/ | ASUS specs |

## 주장 로그 (Claim Log)
| ID | 주장 | 근거 |
| --- | --- | --- |
| CL-01 | MacBook memory ceiling | SC-01 |
| CL-02 | ProArt GPU option | SC-02 |
"#;
        let claim_section = claim_log_section(output).expect("claim section");

        assert_eq!(evidence_supported_claim_row_count(claim_section, output), 2);
    }

    #[test]
    fn accepts_korean_one_line_conclusion_as_final_answer_marker() {
        let output = "## 한 줄 결론\n이 보고서는 사용자의 판단에 필요한 사실, 한계, 추천을 먼저 제시한다.\n\n# 검증 부록\n## Source Cards";

        assert!(final_answer_section(output).is_some());
        assert!(has_visible_final_answer_section(output));
        let mut failures = Vec::new();
        validate_controller_contract_markers(output, false, &mut failures);

        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("final answer")),
            "{failures:?}"
        );
    }

    #[test]
    fn accepts_korean_final_conclusion_as_final_answer_marker() {
        let output =
            "## 최종 결론\n이 보고서는 사용자의 판단에 필요한 사실, 한계, 추천을 먼저 제시한다.\n\n# 검증 부록\n## Source Cards";

        assert!(final_answer_section(output).is_some());
        assert!(has_visible_final_answer_section(output));
        let mut failures = Vec::new();
        validate_controller_contract_markers(output, false, &mut failures);

        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("final answer")),
            "{failures:?}"
        );
    }

    #[test]
    fn accepts_korean_core_conclusion_as_final_answer_marker() {
        let output =
            "## 핵심 결론\n이 보고서는 사용자의 판단에 필요한 사실, 한계, 추천을 먼저 제시한다.\n\n# 검증 부록\n## Source Cards";

        assert!(final_answer_section(output).is_some());
        assert!(has_visible_final_answer_section(output));
        let mut failures = Vec::new();
        validate_controller_contract_markers(output, false, &mut failures);

        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("final answer")),
            "{failures:?}"
        );
    }

    #[test]
    fn claim_log_section_prefers_visible_heading_over_prose_reference() {
        let source_card_rows = (1..=7)
            .map(|idx| format!("| S{idx} | https://example{idx}.net/source | Source {idx} |\n"))
            .collect::<String>();
        let claim_rows = (1..=7)
            .map(|idx| format!("| Claim {idx} | S{idx} | 높음 |\n"))
            .collect::<String>();
        let output = format!(
            "## 최종 답변 (Final Answer)\n{}\n\n## 보강 메모\n이전 시도에서는 Claim Log와 출처 감사의 연결이 약했다.\n\n# 검증 부록\n## Source Cards\n| ID | URL | Title |\n| --- | --- | --- |\n{}## 주장 로그 (Claim Log)\n| Claim | Source ID | Confidence |\n| --- | --- | --- |\n{}## 품질 게이트 (Quality Gate)\n자체 품질 점검 완료.\n",
            dense_final_answer(),
            source_card_rows,
            claim_rows,
        );
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn counts_claim_backed_official_source_cards_as_authoritative() {
        let artifacts = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![
                crate::models::ResearchSourceCard {
                    id: "SC-01".to_string(),
                    url: "https://www.asus.com/us/laptops/for-creators/proart/proart-p16-h7606/"
                        .to_string(),
                    title: "ASUS specs".to_string(),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-02".to_string(),
                    url: "https://www.notebookcheck.net/laptop-review.html".to_string(),
                    title: "Review".to_string(),
                    source_class: "secondary".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
            ],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "official vendor spec".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SC-01".to_string(), "SC-02".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let urls = vec!["https://www.notebookcheck.net/laptop-review.html".to_string()];

        assert_eq!(
            authoritative_evidence_url_count(&urls, Some(&artifacts), None),
            1
        );
    }

    #[test]
    fn counts_claim_backed_cpp_reference_and_academic_source_cards_as_authoritative() {
        let artifacts = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![
                crate::models::ResearchSourceCard {
                    id: "SC-CPPREF".to_string(),
                    url: "https://en.cppreference.com/w/cpp/atomic/memory_order".to_string(),
                    title: "std::memory_order".to_string(),
                    source_class: "reference".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-TBB".to_string(),
                    url: "https://uxlfoundation.github.io/oneTBB/main/tbb_userguide/How_Task_Scheduler_Works.html".to_string(),
                    title: "How Task Scheduler Works".to_string(),
                    source_class: "official documentation".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-ACADEMIC".to_string(),
                    url: "https://www.cs.wm.edu/~dcschmidt/PDF/work-stealing-dequeue.pdf"
                        .to_string(),
                    title: "Dynamic Circular Work-Stealing Deque".to_string(),
                    source_class: "academic/institutional".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
            ],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-CPP".to_string(),
                claim: "work-stealing scheduler implementation details".to_string(),
                claim_type: None,
                support_source_card_ids: vec![
                    "SC-CPPREF".to_string(),
                    "SC-TBB".to_string(),
                    "SC-ACADEMIC".to_string(),
                ],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        assert_eq!(
            authoritative_evidence_url_count(
                &[],
                Some(&artifacts),
                Some("modern C++ work-stealing scheduler implementation guide")
            ),
            3
        );
    }

    #[test]
    fn does_not_count_non_authoritative_urls_from_broad_reference_or_academic_source_classes() {
        let artifacts = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![
                crate::models::ResearchSourceCard {
                    id: "SC-REF".to_string(),
                    url: "https://blog.example.com/memory-order-notes".to_string(),
                    title: "Memory ordering notes".to_string(),
                    source_class: "reference".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-ACADEMIC".to_string(),
                    url: "https://example.org/paper-summary/work-stealing".to_string(),
                    title: "Paper summary".to_string(),
                    source_class: "academic/institutional".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
            ],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "claimed reference support".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SC-REF".to_string(), "SC-ACADEMIC".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        assert_eq!(
            authoritative_evidence_url_count(
                &[],
                Some(&artifacts),
                Some("modern C++ work-stealing scheduler implementation guide")
            ),
            0
        );
    }

    #[test]
    fn counts_claim_backed_network_and_cloud_source_cards_as_authoritative() {
        let artifacts = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![
                crate::models::ResearchSourceCard {
                    id: "SC-RFC".to_string(),
                    url: "https://www.rfc-editor.org/rfc/rfc6056".to_string(),
                    title: "RFC 6056".to_string(),
                    source_class: "reference".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-IANA".to_string(),
                    url: "https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml".to_string(),
                    title: "IANA Port Registry".to_string(),
                    source_class: "reference".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-KERNEL".to_string(),
                    url: "https://docs.kernel.org/networking/ip-sysctl.html".to_string(),
                    title: "Linux ip-sysctl".to_string(),
                    source_class: "official documentation".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-MS".to_string(),
                    url: "https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/default-dynamic-port-range-tcpip-chang".to_string(),
                    title: "Windows dynamic port range".to_string(),
                    source_class: "vendor documentation".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "SC-CLOUD".to_string(),
                    url: "https://cloud.google.com/nat/docs/ports-and-addresses".to_string(),
                    title: "Cloud NAT ports".to_string(),
                    source_class: "official documentation".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
            ],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "ephemeral port defaults vary by standards, OS, and cloud layer".to_string(),
                claim_type: None,
                support_source_card_ids: vec![
                    "SC-RFC".to_string(),
                    "SC-IANA".to_string(),
                    "SC-KERNEL".to_string(),
                    "SC-MS".to_string(),
                    "SC-CLOUD".to_string(),
                ],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        assert_eq!(
            authoritative_evidence_url_count(
                &[],
                Some(&artifacts),
                Some(
                    "Explain ephemeral port allocation and port exhaustion across Linux kernel defaults, Windows dynamic port ranges, and cloud container networking behavior.",
                )
            ),
            5
        );
    }

    #[test]
    fn hidden_artifact_json_urls_do_not_satisfy_visible_source_or_host_gates() {
        let visible_rows = (1..=7)
            .map(|idx| {
                format!("| 리뷰 {idx} | https://reviews.samplecorp.com/laptops/{idx} | 높음 |")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let output = format!(
            "## 최종 답변 (Final Answer)\n노트북 비교 요약.\n\n## 출처 감사 (Source Audit)\n| URL | 확인 |\n| --- | --- |\n{audit_rows}\n\n## 주장 로그 (Claim Log)\n| Claim | Source URL | Confidence |\n| --- | --- | --- |\n{claim_rows}\n\n## 품질 게이트 (Quality Gate)\n자체 품질 점검 완료.\n\n# Verification Appendix\n[RESEARCH_ARTIFACT_JSON]\n```json\n{{\n  \"version\": 1,\n  \"source_cards\": [\n    {{\"id\":\"S1\",\"url\":\"https://developer.apple.com/documentation/apple-silicon\",\"title\":\"Apple\",\"source_class\":\"official_or_primary\"}},\n    {{\"id\":\"S2\",\"url\":\"https://docs.nvidia.com/cuda/\",\"title\":\"NVIDIA\",\"source_class\":\"official_or_primary\"}},\n    {{\"id\":\"S3\",\"url\":\"https://support.lenovo.com/us/en/solutions/ht516908\",\"title\":\"Lenovo\",\"source_class\":\"official_or_primary\"}},\n    {{\"id\":\"S4\",\"url\":\"https://pytorch.org/get-started/locally/\",\"title\":\"PyTorch\",\"source_class\":\"official_or_primary\"}},\n    {{\"id\":\"S5\",\"url\":\"https://www.rust-lang.org/tools/install\",\"title\":\"Rust\",\"source_class\":\"official_or_primary\"}}\n  ],\n  \"claim_log\": [\n    {{\"id\":\"C1\",\"claim\":\"vendor-backed claim\",\"support_source_card_ids\":[\"S1\",\"S2\",\"S3\",\"S4\",\"S5\"]}}\n  ],\n  \"conflict_map\": [],\n  \"research_debt\": [],\n  \"quality_gate\": {{\"status\":\"passed\",\"failure_messages\":[],\"unsupported_claim_count\":0,\"unresolved_conflict_count\":0,\"open_debt_count\":0}}\n}}\n```\n",
            audit_rows = (1..=7)
                .map(|idx| format!("| https://reviews.samplecorp.com/laptops/{idx} | visible |"))
                .collect::<Vec<_>>()
                .join("\n"),
            claim_rows = visible_rows,
        );
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("developer laptop comparison"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();

        assert!(err.contains("distinct evidence host count 1 is below required minimum 3"));
    }

    #[test]
    fn strip_research_artifact_blocks_removes_all_markdown_artifact_blocks() {
        let output = r#"
## 최종 답변 (Final Answer)
Visible answer.

[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"warnings":["claim log"],"source_cards":[{"id":"S1","url":"https://hidden.example.com/one","title":"Hidden One","source_class":"official_or_primary"}],"claim_log":[],"conflict_map":[],"research_debt":[]}
```

[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"warnings":["quality gate"],"source_cards":[{"id":"S2","url":"https://hidden.example.com/two","title":"Hidden Two","source_class":"official_or_primary"}],"claim_log":[],"conflict_map":[],"research_debt":[]}
```
"#;

        let stripped = strip_research_artifact_blocks(output);

        assert!(!stripped.contains("[RESEARCH_ARTIFACT_JSON]"));
        assert!(!stripped.contains("https://hidden.example.com/one"));
        assert!(!stripped.contains("https://hidden.example.com/two"));
        assert!(!stripped.to_ascii_lowercase().contains("claim log"));
        assert!(!stripped.to_ascii_lowercase().contains("quality gate"));
    }

    #[test]
    fn strip_research_artifact_blocks_handles_overlapping_markdown_and_html_ranges() {
        let output = r#"
## 최종 답변 (Final Answer)
Visible answer.

<script type="application/json" data-research-artifacts>
[RESEARCH_ARTIFACT_JSON]
```json
{"version":1,"source_cards":[{"id":"S1","url":"https://hidden.example.com/one","title":"Hidden One","source_class":"official_or_primary"}],"claim_log":[],"conflict_map":[],"research_debt":[]}
```
</script>
"#;

        let stripped = strip_research_artifact_blocks(output);

        assert!(stripped.contains("Visible answer."));
        assert!(!stripped.contains("<script"));
        assert!(!stripped.contains("[RESEARCH_ARTIFACT_JSON]"));
        assert!(!stripped.contains("https://hidden.example.com/one"));
    }

    #[test]
    fn strip_research_artifact_blocks_ignores_marker_after_unrelated_script() {
        let output = r#"
## 최종 답변 (Final Answer)
Visible answer.

<script type="application/json">
{"safe":"content"}
</script>
data-research-artifacts
Visible footer.
"#;

        let stripped = strip_research_artifact_blocks(output);

        assert!(stripped.contains("Visible answer."));
        assert!(stripped.contains(r#"<script type="application/json">"#));
        assert!(stripped.contains(r#"{"safe":"content"}"#));
        assert!(stripped.contains("data-research-artifacts"));
        assert!(stripped.contains("Visible footer."));
    }

    #[test]
    fn accepts_reasonable_claim_log_and_quality_gate_aliases() {
        let output = r#"
## 최종 답변 (Final Answer)
간단한 조사 결과입니다.

# Verification Appendix
## Source Cards
- Source 1: https://example.net/source
## Claims and Evidence
| Claim | Source URL | Confidence |
| --- | --- | --- |
| 핵심 주장 | https://example.net/source | 높음 |
## Quality Check
자체 점검 완료.
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(output, &context);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn accepts_machine_readable_quality_gate_under_verification_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)
간단한 조사 결과입니다.

# 검증 부록
## 출처 감사
- https://example.net/source
## 주장 로그 (Claim Log)
| Claim | Source URL | Confidence |
| --- | --- | --- |
| 핵심 주장 | https://example.net/source | 높음 |
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [{"id":"S1","url":"https://example.net/source","title":"Example","source_class":"official_or_primary"}],
  "claim_log": [{"id":"C1","claim":"핵심 주장","support_source_card_ids":["S1"]}],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(output, &context);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_missing_visible_quality_gate_when_artifact_block_is_invalid() {
        let output = r#"
## 최종 답변 (Final Answer)
간단한 조사 결과입니다.

# 검증 부록
## 출처 감사
- https://example.net/source
## 주장 로그 (Claim Log)
| Claim | Source URL | Confidence |
| --- | --- | --- |
| 핵심 주장 | https://example.net/source | 높음 |
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [{"ID":"S1","title":"Example","source_class":"official_or_primary"}],
  "claim_log": [{"id":"C1","claim":"핵심 주장","support_source_card_ids":["S1"]}],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("quality gate"));
    }

    #[test]
    fn rejects_machine_readable_quality_gate_when_appendix_marker_exists_only_inside_artifact() {
        let output = r#"
## 최종 답변 (Final Answer)
간단한 조사 결과입니다.

## 출처 감사
- https://example.net/source
## 주장 로그 (Claim Log)
| Claim | Source URL | Confidence |
| --- | --- | --- |
| 핵심 주장 | https://example.net/source | 높음 |
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "warnings": ["verification appendix present"],
  "source_cards": [{"id":"S1","url":"https://example.net/source","title":"Example","source_class":"official_or_primary"}],
  "claim_log": [{"id":"C1","claim":"핵심 주장","support_source_card_ids":["S1"]}],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("quality gate"));
    }

    #[test]
    fn rejects_hidden_artifact_json_section_markers_as_visible_controller_sections() {
        let output = r#"
## 최종 답변 (Final Answer)
간단한 조사 결과입니다.

# 검증 부록
## 출처 감사
- https://example.net/source
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "warnings": [
    "source cards claim log quality gate"
  ],
  "source_cards": [{"id":"S1","url":"https://example.net/source","title":"Example","source_class":"official_or_primary"}],
  "claim_log": [{"id":"C1","claim":"핵심 주장","support_source_card_ids":["S1"]}],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: None,
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("claim log"));
    }

    #[test]
    fn rejects_unresolved_markers() {
        let output = format!(
            "{}<p>출처 감사 필요</p>",
            html_with_audit(&["https://example.com/a"])
        );
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("low"),
            quality_depth: Some("standard"),
            research_topic: Some("유스티니아누스 고토"),
            research_instructions: None,
            evidence_subject: None,
        };

        assert!(validate_research_output(&output, &context)
            .unwrap_err()
            .contains("unresolved marker"));
    }

    #[test]
    fn topic_terms_preserve_user_order_for_relevance_checks() {
        let terms = topic_terms(
            Some("[RQ-TEST] 유스티니아누스 대제의 로마-고트 수복 전쟁 개요"),
            Some("벨리사리우스 나르세스 토틸라 로마 라벤나 535-554"),
        );

        assert!(
            terms
                .iter()
                .position(|term| term == "유스티니아누스")
                .unwrap()
                < terms
                    .iter()
                    .position(|term| term == "벨리사리우스")
                    .unwrap()
        );
        assert!(terms.iter().any(|term| term == "토틸라"));
    }

    #[test]
    fn rejects_justinian_gothic_war_output_missing_core_chronology_anchors() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]);
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("Justinian Gothic War anchor coverage"));
    }

    #[test]
    fn rejects_broken_interactive_tab_contract() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + r#"
            <button class="tab-btn" data-tab="belisarius">Belisarius</button>
            <div class="tab-content">missing data-tab</div>
            <p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>
        "#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("interactive tab contract is broken"));
    }

    #[test]
    fn accepts_tab_buttons_with_matching_id_panes() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + r#"
            <button class="tab-button" data-tab="belisarius">Belisarius</button>
            <div id="belisarius" class="tab-pane">본문</div>
            <p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>
        "#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let result = validate_research_output(&output, &context);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_alpine_runtime_attrs_without_runtime() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + r#"
            <div x-data="{ activeTab: 'belisarius' }">
                <button @click="activeTab = 'belisarius'">Belisarius</button>
                <div x-show="activeTab === 'belisarius'">본문</div>
            </div>
            <p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>
        "#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("Alpine-style runtime attributes"));
    }

    #[test]
    fn rejects_empty_interactive_tab_button_labels() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
            "https://www.worldhistory.org/Totila/",
            "https://www.britannica.com/topic/Ostrogoth",
            "https://www.worldhistory.org/Belisarius/",
        ]) + r#"
            <button class="tab-button" data-tab="belisarius"></button>
            <div id="belisarius" class="tab-pane">본문</div>
            <p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙</p>
        "#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("interactive tab button must have visible label text"));
    }

    #[test]
    fn rejects_known_false_justinian_gothic_war_claim_patterns() {
        let output = html_with_audit(&[
            "https://www.britannica.com/biography/Justinian-I",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
            "https://www.worldhistory.org/Justinian_I/",
        ]) + "<p>시칠리아 나폴리 로마 라벤나 토틸라 타기나이 락타리우스 국사조칙 노르만계 출신 장군 나르세스</p>";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("유스티니아누스 대제의 로마 고트 수복 전쟁 개요"),
            research_instructions: None,
            evidence_subject: None,
        };

        let err = validate_research_output(&output, &context).unwrap_err();
        assert!(err.contains("known false Justinian Gothic War claim"));
    }

    #[test]
    fn parses_markdown_research_artifact_json_block() {
        let output = r#"
## 최종 답변
본문

# 검증 부록
## Research Artifact JSON
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [{"id":"S1","url":"https://example.com","title":"Example","source_class":"official_or_primary","extracted_facts":["fact"],"confidence":"high"}],
  "claim_log": [{"id":"C1","claim":"supported","support_source_card_ids":["S1"]}],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();
        assert_eq!(artifacts.source_cards[0].id, "S1");
        assert_eq!(artifacts.claim_log[0].support_source_card_ids, vec!["S1"]);
    }

    #[test]
    fn rejects_invalid_research_artifact_json_block() {
        let output = "[RESEARCH_ARTIFACT_JSON]\n```json\n{not valid json}\n```";
        let err = parse_research_artifact_block(output, "md").unwrap_err();

        assert!(err.contains("invalid research artifact JSON"));
    }

    #[test]
    fn rejects_oversized_research_artifact_json_block() {
        let oversized_claim = "x".repeat(MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        let output = format!(
            "[RESEARCH_ARTIFACT_JSON]\n```json\n{{\"version\":1,\"claim_log\":[{{\"id\":\"C1\",\"claim\":\"{oversized_claim}\"}}]}}\n```"
        );

        let err = parse_research_artifact_block(&output, "md").unwrap_err();

        assert!(err.contains("exceeds maximum size"));
    }

    #[test]
    fn normalizes_research_artifact_lengths_and_item_counts() {
        let source_cards = (0..(MAX_RESEARCH_ARTIFACT_ITEMS + 5))
            .map(|idx| {
                format!(
                    "{{\"id\":\"S{idx}-{}\",\"url\":\"https://example.com/{idx}/{}\",\"title\":\"{}\",\"source_class\":\"official_or_primary\",\"extracted_facts\":[\"{}\",\"{}\"]}}",
                    "x".repeat(32),
                    "y".repeat(96),
                    "t".repeat(96),
                    "f".repeat(96),
                    "g".repeat(96),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let output = format!(
            "[RESEARCH_ARTIFACT_JSON]\n```json\n{{\"version\":1,\"source_cards\":[{source_cards}],\"claim_log\":[],\"conflict_map\":[],\"research_debt\":[],\"warnings\":[\"{}\",\"{}\"]}}\n```",
            "w".repeat(400),
            "z".repeat(400)
        );

        let artifacts = parse_research_artifact_block(&output, "md").unwrap();

        assert_eq!(artifacts.source_cards.len(), MAX_RESEARCH_ARTIFACT_ITEMS);
        assert!(
            artifacts.source_cards[0].id.chars().count() <= MAX_RESEARCH_ARTIFACT_ID_CHARS + 14
        );
        assert!(
            artifacts.source_cards[0].url.chars().count() <= MAX_RESEARCH_ARTIFACT_URL_CHARS + 14
        );
        assert!(
            artifacts.source_cards[0].title.chars().count()
                <= MAX_RESEARCH_ARTIFACT_TEXT_CHARS + 14
        );
        assert!(artifacts.source_cards[0].extracted_facts.len() <= 2);
        assert!(
            artifacts.source_cards[0].extracted_facts[0].chars().count()
                <= MAX_RESEARCH_ARTIFACT_TEXT_CHARS + 14
        );
        assert_eq!(artifacts.warnings.len(), 2);
        assert!(artifacts.warnings[0].chars().count() <= MAX_RESEARCH_ARTIFACT_TEXT_CHARS + 14);
    }

    #[test]
    fn parses_live_smoke_artifacts_without_ids_and_with_next_action_alias() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence",
      "title": "Example Evidence",
      "source_class": "official_or_primary",
      "extracted_facts": ["fact"]
    }
  ],
  "claim_log": [
    {
      "claim": "live smoke claim",
      "support_source_card_ids": ["S1"],
      "support_urls": ["https://example.com/evidence"]
    }
  ],
  "conflict_map": [
    {
      "topic": "live smoke conflict",
      "conflicting_claim_ids": ["C1"],
      "source_card_ids": ["S1"],
      "resolution_status": "resolved"
    }
  ],
  "research_debt": [
    {
      "severity": "medium",
      "failed_gate": "quality_gate",
      "missing_evidence": "Need one more current source",
      "candidate_queries": ["live smoke benchmark"],
      "next_action": "Search for a fresher primary source",
      "status": "open"
    }
  ],
  "quality_gate": {
    "status": "failed",
    "failure_messages": [],
    "unsupported_claim_count": 0,
    "unresolved_conflict_count": 0,
    "open_debt_count": 1
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(artifacts.claim_log[0].id, "C1");
        assert_eq!(artifacts.conflict_map[0].id, "X1");
        assert_eq!(artifacts.research_debt[0].id, "D1");
        assert_eq!(
            artifacts.research_debt[0].next_check_actions,
            vec!["Search for a fresher primary source"]
        );
    }

    #[test]
    fn parses_research_artifact_json_without_version_field() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence",
      "title": "Example Evidence",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "live smoke claim",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(artifacts.version, 1);
        assert_eq!(artifacts.source_cards.len(), 1);
        assert_eq!(artifacts.claim_log.len(), 1);
    }

    #[test]
    fn parses_live_smoke_artifacts_with_uppercase_source_card_id_and_url_aliases() {
        let output = r#"
# 검증 부록
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "ID": "S1",
      "URL": "https://developer.apple.com/documentation/metal",
      "source_class": "official_or_primary",
      "extracted_facts": ["fact"]
    }
  ],
  "claim_log": [
    {
      "ID": "C1",
      "claim": "live smoke claim",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [
    {
      "ID": "X1",
      "topic": "live smoke conflict",
      "conflicting_claim_ids": ["C1"],
      "source_card_ids": ["S1"],
      "resolution_status": "resolved"
    }
  ],
  "research_debt": [
    {
      "ID": "D1",
      "failed_gate": "quality_gate",
      "missing_evidence": "Need one more current source",
      "candidate_queries": ["live smoke benchmark"],
      "next_check_actions": ["Search for a fresher primary source"],
      "status": "open"
    }
  ],
  "quality_gate": {
    "status": "failed",
    "failure_messages": [],
    "unsupported_claim_count": 0,
    "unresolved_conflict_count": 0,
    "open_debt_count": 1
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(artifacts.source_cards[0].id, "S1");
        assert_eq!(
            artifacts.source_cards[0].url,
            "https://developer.apple.com/documentation/metal"
        );
        assert_eq!(artifacts.source_cards[0].title, "developer.apple.com metal");
        assert_eq!(artifacts.claim_log[0].id, "C1");
        assert_eq!(artifacts.conflict_map[0].id, "X1");
        assert_eq!(artifacts.research_debt[0].id, "D1");
    }

    #[test]
    fn normalizes_live_claim_and_conflict_aliases_into_typed_fields() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence-1",
      "title": "Evidence 1",
      "source_class": "official_or_primary"
    },
    {
      "id": "S2",
      "url": "https://example.com/evidence-2",
      "title": "Evidence 2",
      "source_class": "secondary"
    }
  ],
  "claim_log": [
    {
      "claim": "live smoke claim",
      "type": "factual",
      "support": ["S1", "https://example.com/evidence-2"]
    }
  ],
  "conflict_map": [
    {
      "conflicting_claim_ids": ["C1"],
      "sources": ["S1", "S2"],
      "status": "resolved",
      "resolution": "Cross-checked against source cards."
    }
  ],
  "research_debt": [],
  "quality_gate": {
    "status": "passed",
    "failure_messages": [],
    "unsupported_claim_count": 0,
    "unresolved_conflict_count": 0,
    "open_debt_count": 0
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(
            artifacts.claim_log[0].claim_type.as_deref(),
            Some("factual")
        );
        assert_eq!(artifacts.claim_log[0].support_source_card_ids, vec!["S1"]);
        assert_eq!(
            artifacts.claim_log[0].support_urls,
            vec!["https://example.com/evidence-2"]
        );
        assert_eq!(artifacts.conflict_map[0].source_card_ids, vec!["S1", "S2"]);
        assert_eq!(
            artifacts.conflict_map[0].resolution_status.as_deref(),
            Some("resolved")
        );
        assert_eq!(
            artifacts.conflict_map[0].resolution_note.as_deref(),
            Some("Cross-checked against source cards.")
        );
    }

    #[test]
    fn validates_invalid_claim_support_values_after_alias_normalization() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence-1",
      "title": "Evidence 1",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "claim": "live smoke claim",
      "support": ["MISSING", "ftp://example.com/not-http"]
    }
  ],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {
    "status": "failed",
    "failure_messages": ["missing support"],
    "unsupported_claim_count": 1,
    "unresolved_conflict_count": 0,
    "open_debt_count": 0
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();
        let err =
            validate_research_artifacts(&artifacts, Some("high"), Some("strict")).unwrap_err();

        assert!(err.iter().any(|message| {
            message.contains("claim C1 references missing or invalid Source Card ID MISSING")
        }));
        assert!(err.iter().any(|message| {
            message.contains(
                "claim C1 contains a non-resolvable support URL ftp://example.com/not-http",
            )
        }));
    }

    #[test]
    fn rejects_research_artifact_json_with_explicit_zero_version() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 0,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence",
      "title": "Example Evidence",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```
"#;

        let err = parse_research_artifact_block(output, "md").unwrap_err();

        assert_eq!(err, "invalid research artifact JSON: version must be >= 1");
    }

    #[test]
    fn parses_live_smoke_artifacts_with_missing_topic_and_status_fields() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/evidence",
      "title": "Example Evidence",
      "source_class": "official_or_primary",
      "extracted_facts": ["fact"]
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "live smoke claim",
      "support_source_card_ids": ["S1"],
      "support_urls": ["https://example.com/evidence"]
    }
  ],
  "conflict_map": [
    {
      "conflicting_claim_ids": ["C1"],
      "source_card_ids": ["S1"]
    }
  ],
  "research_debt": [
    {
      "failed_gate": "quality_gate",
      "candidate_queries": ["live smoke benchmark"]
    }
  ],
  "quality_gate": {
    "failure_messages": ["Need one more current source"],
    "open_debt_count": 1
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(artifacts.conflict_map[0].topic, "conflict involving C1");
        assert_eq!(artifacts.research_debt[0].status, "open");
        assert_eq!(artifacts.research_debt[0].severity, "medium");
        assert_eq!(
            artifacts.research_debt[0].missing_evidence,
            "Need one more current source"
        );
        assert_eq!(artifacts.quality_gate.as_ref().unwrap().status, "failed");
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .unwrap()
                .unsupported_claim_count,
            0
        );
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .unwrap()
                .unresolved_conflict_count,
            0
        );
    }

    #[test]
    fn derives_missing_source_card_titles_without_rejecting_artifact_block() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://docs.python.org/3/library/pathlib.html",
      "source_class": "official_or_primary",
      "extracted_facts": ["fact"]
    },
    {
      "id": "S2",
      "url": "https://example.com/secondary",
      "source_class": "secondary",
      "extracted_facts": ["fallback"]
    }
  ],
  "claim_log": [
    {
      "claim": "live smoke claim",
      "support_source_card_ids": ["S1", "S2"]
    }
  ],
  "conflict_map": [],
  "research_debt": [],
  "quality_gate": {
    "status": "passed",
    "failure_messages": [],
    "unsupported_claim_count": 0,
    "unresolved_conflict_count": 0,
    "open_debt_count": 0
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(
            artifacts.source_cards[0].title,
            "docs.python.org pathlib.html"
        );
        assert_eq!(artifacts.source_cards[1].title, "example.com secondary");
        assert_eq!(artifacts.claim_log[0].id, "C1");
        assert_eq!(
            artifacts.claim_log[0].support_source_card_ids,
            vec!["S1", "S2"]
        );
    }

    #[test]
    fn rejects_invalid_source_card_urls_during_artifact_validation() {
        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![
                crate::models::ResearchSourceCard {
                    id: "S1".to_string(),
                    url: "".to_string(),
                    title: "Missing URL".to_string(),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: None,
                    extracted_facts: vec!["fact".to_string()],
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
                crate::models::ResearchSourceCard {
                    id: "S2".to_string(),
                    url: "ftp://example.com/file".to_string(),
                    title: "Bad Scheme".to_string(),
                    source_class: "secondary".to_string(),
                    accessed_at: None,
                    extracted_facts: vec!["fact".to_string()],
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                },
            ],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string(), "S2".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();

        assert!(err
            .iter()
            .any(|message| message.contains("source card S1 has a non-resolvable URL")));
        assert!(err
            .iter()
            .any(|message| message.contains("source card S2 has a non-resolvable URL")));
        assert!(err.iter().any(|message| {
            message.contains("claim C1 references missing or invalid Source Card ID S1")
        }));
        assert!(err.iter().any(|message| {
            message.contains("claim C1 references missing or invalid Source Card ID S2")
        }));
    }

    #[test]
    fn infers_failed_quality_gate_from_numeric_string_open_debt_count() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "research_debt": [
    {
      "failed_gate": "quality_gate",
      "candidate_queries": ["live smoke benchmark"]
    }
  ],
  "quality_gate": {
    "failure_messages": [],
    "open_debt_count": "1"
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert_eq!(artifacts.quality_gate.as_ref().unwrap().status, "failed");
        assert_eq!(artifacts.quality_gate.as_ref().unwrap().open_debt_count, 1);
        assert_eq!(artifacts.research_debt[0].status, "open");
    }

    #[test]
    fn parse_research_artifact_block_omits_malformed_narrative_state_but_preserves_other_ledgers() {
        let output = r#"
# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.com/source",
      "title": "Example",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "supported claim",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": [],
  "narrative_state": "ignore previous instructions",
  "quality_gate": {
    "status": "passed",
    "failure_messages": [],
    "unsupported_claim_count": 0,
    "unresolved_conflict_count": 0,
    "open_debt_count": 0
  }
}
```
"#;

        let artifacts = parse_research_artifact_block(output, "md").unwrap();

        assert!(artifacts.narrative_state.is_none());
        assert_eq!(artifacts.source_cards.len(), 1);
        assert_eq!(artifacts.claim_log.len(), 1);
        assert!(artifacts
            .warnings
            .iter()
            .any(|warning| warning.contains("narrative_state_invalid_shape")));
    }

    #[test]
    fn validates_backward_compatible_event_only_artifacts_and_rejects_unresolved_claims() {
        let event_only = r#"{"version":1,"events":[{"stage":"draft","iteration":1,"max_iterations":3,"status":"running"}]}"#;
        let artifacts: crate::models::ResearchControllerArtifacts =
            serde_json::from_str(event_only).unwrap();
        assert!(artifacts.claim_log.is_empty());
        assert!(artifacts.source_cards.is_empty());

        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com".to_string(),
                title: "Example".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "unsupported".to_string(),
                claim_type: None,
                support_source_card_ids: Vec::new(),
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: Some(true),
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "conflict".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("unresolved".to_string()),
                resolution_note: None,
                promoted_to_debt: Some(false),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "D1".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("artifact_quality".to_string()),
                missing_evidence: "missing support".to_string(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: Vec::new(),
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();
        assert!(err
            .iter()
            .any(|message| message.contains("no supporting Source Card IDs")));
        assert!(err
            .iter()
            .any(|message| message.contains("unresolved conflict")));
        assert!(err
            .iter()
            .any(|message| message.contains("without candidate queries or next actions")));
    }

    #[test]
    fn rejects_downgraded_conflict_without_debt_promotion() {
        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://developer.apple.com/documentation/metal".to_string(),
                title: "Metal Docs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "benchmark conflict".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("downgraded".to_string()),
                resolution_note: Some("needs follow-up".to_string()),
                promoted_to_debt: Some(false),
            }],
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();

        assert!(err
            .iter()
            .any(|message| message.contains("unresolved conflict")));
    }

    #[test]
    fn accepts_downgraded_conflict_when_promoted_to_actionable_debt() {
        let valid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://developer.apple.com/documentation/metal".to_string(),
                title: "Metal Docs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "thermal throttling discrepancy".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("downgraded".to_string()),
                resolution_note: Some("requires vendor reconciliation".to_string()),
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                missing_evidence: "Conflict X1 for claim C1 needs one more primary thermal source"
                    .to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec![
                    "benchmark conflict Apple thermals official documentation".to_string()
                ],
                next_check_actions: vec![
                    "Resolve benchmark conflict for claim C1 with source card S1".to_string(),
                ],
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let result = validate_research_artifacts(&valid, Some("high"), Some("strict"));

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn accepts_promoted_conflict_when_debt_matches_topic_terms() {
        let valid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "SC-01".to_string(),
                url: "https://www.apple.com/macbook-pro/specs/".to_string(),
                title: "Apple Specs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: Vec::new(),
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SC-01".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "CM-02".to_string(),
                topic: "MacBook Pro M5 Max 16형의 독립 열 측정 자료 부족".to_string(),
                conflicting_claim_ids: Vec::new(),
                source_card_ids: vec!["SC-01".to_string()],
                resolution_status: Some("promoted_to_debt".to_string()),
                resolution_note: Some("16형 정량 열/소음 리뷰가 제한적".to_string()),
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "RD-01".to_string(),
                severity: "medium".to_string(),
                failed_gate: None,
                missing_evidence:
                    "MacBook Pro 16 M5 Max의 장시간 LLM 부하 정량 열/소음/전력 측정 부족"
                        .to_string(),
                required_source_class: None,
                candidate_queries: vec![
                    "MacBook Pro 16 M5 Max llama.cpp thermal throttling review".to_string(),
                ],
                next_check_actions: vec!["16형 M5 Max 장시간 리뷰 추가 확인".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let result = validate_research_artifacts(&valid, Some("high"), Some("strict"));

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_resolved_with_scope_conflict_status_without_actionable_debt() {
        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "SC-01".to_string(),
                url: "https://www.apple.com/macbook-pro/specs/".to_string(),
                title: "Apple Specs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: Vec::new(),
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SC-01".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "CF2".to_string(),
                topic: "memory capacity versus CUDA compatibility".to_string(),
                conflicting_claim_ids: Vec::new(),
                source_card_ids: vec!["SC-01".to_string()],
                resolution_status: Some("resolved_with_scope".to_string()),
                resolution_note: Some("Scope split in final recommendation".to_string()),
                promoted_to_debt: Some(false),
            }],
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();

        assert!(err
            .iter()
            .any(|message| message.contains("unresolved conflict")));
    }

    #[test]
    fn accepts_resolved_by_synthesis_conflict_status_without_actionable_debt() {
        let valid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "SC-01".to_string(),
                url: "https://www.britannica.com/event/French-Revolution".to_string(),
                title: "French Revolution".to_string(),
                source_class: "authoritative secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "CL-01".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SC-01".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "CF1".to_string(),
                topic: "multi-causal interpretation".to_string(),
                conflicting_claim_ids: Vec::new(),
                source_card_ids: vec!["SC-01".to_string()],
                resolution_status: Some("resolved_by_synthesis".to_string()),
                resolution_note: Some(
                    "Resolved by presenting a multi-causal synthesis.".to_string(),
                ),
                promoted_to_debt: Some(false),
            }],
            research_debt: Vec::new(),
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        assert!(validate_research_artifacts(&valid, Some("high"), Some("strict")).is_ok());
    }

    #[test]
    fn rejects_promoted_conflict_with_unrelated_actionable_debt() {
        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://developer.apple.com/documentation/metal".to_string(),
                title: "Metal Docs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "benchmark thermals disagreement".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("downgraded".to_string()),
                resolution_note: Some("Needs thermal follow-up".to_string()),
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                missing_evidence: "Need one more battery benchmark".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["battery life official documentation".to_string()],
                next_check_actions: vec!["Verify battery life claim".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();

        assert!(err
            .iter()
            .any(|message| message.contains("unresolved conflict")));
    }

    #[test]
    fn rejects_promoted_conflict_when_debt_only_mentions_prefix_sharing_ids() {
        let invalid = crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::models::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://developer.apple.com/documentation/metal".to_string(),
                title: "Metal Docs".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![crate::models::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: None,
                uncertainty_note: None,
                needs_verification: None,
            }],
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "benchmark conflict".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("downgraded".to_string()),
                resolution_note: Some("needs follow-up".to_string()),
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                missing_evidence:
                    "Conflict X10 for claim C10 needs one more primary thermal source".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["Apple thermals official documentation for S10".to_string()],
                next_check_actions: vec!["Resolve claim C10 with source card S10".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        let err = validate_research_artifacts(&invalid, Some("high"), Some("strict")).unwrap_err();

        assert!(err
            .iter()
            .any(|message| message.contains("unresolved conflict")));
    }

    fn sample_finalization_artifacts() -> crate::models::ResearchControllerArtifacts {
        crate::models::ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: (1..=7)
                .map(|idx| crate::models::ResearchSourceCard {
                    id: format!("S{idx}"),
                    url: format!("https://example{idx}.gov/report/{idx}"),
                    title: format!("Official Source {idx}"),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: Some("2026-05-15".to_string()),
                    extracted_facts: vec![format!("verified fact {idx}")],
                    limitation: Some("scope limits".to_string()),
                    diagnostics_ref: None,
                    confidence: Some("high".to_string()),
                })
                .collect(),
            claim_log: (1..=7)
                .map(|idx| crate::models::ResearchClaimLogEntry {
                    id: format!("C{idx}"),
                    claim: format!("검증된 주장 {idx}"),
                    claim_type: Some("verified_fact".to_string()),
                    support_source_card_ids: vec![format!("S{idx}")],
                    support_urls: Vec::new(),
                    confidence: Some("high".to_string()),
                    uncertainty_note: Some("none".to_string()),
                    needs_verification: Some(false),
                })
                .collect(),
            conflict_map: vec![crate::models::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "timeline ambiguity".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("resolved".to_string()),
                resolution_note: Some("resolved by primary source".to_string()),
                promoted_to_debt: Some(false),
            }],
            research_debt: vec![crate::models::ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: None,
                missing_evidence: "follow-up monitoring".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["official follow-up query".to_string()],
                next_check_actions: vec!["monitor delegated update".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            quality_gate: Some(crate::models::ResearchQualityGateArtifact {
                status: "passed".to_string(),
                failure_messages: Vec::new(),
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 1,
            }),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn finalization_preserves_reader_prose_and_renders_visible_markdown_appendix() {
        let artifacts = sample_finalization_artifacts();
        let diagnostics = crate::models::ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("policy comparison".to_string()),
            source_pack: Some(crate::models::ResearchSourcePackReport {
                subject: Some("policy comparison".to_string()),
                status: "partial".to_string(),
                reason: Some("provider blocked one query".to_string()),
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: vec![crate::models::ResearchSourceCoverageMiss {
                    expected_host: Some("nist.gov".to_string()),
                    expected_source_class: Some("official_or_primary".to_string()),
                    query: "policy comparison nist.gov official guidance".to_string(),
                    provider: Some("naver".to_string()),
                    status: "missed".to_string(),
                    reason: Some("Target host nist.gov was not recovered.".to_string()),
                }],
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };
        let draft = "## 최종 답변 (Final Answer)\n\n기존 본문 고유 문장은 유지되어야 합니다. 이 비교에서는 정부 지침과 기업 운영 방식이 어떤 순서와 배경 속에서 달라지는지 먼저 구분해 읽는 것이 중요합니다. 같은 정책 문구라도 집행 주체와 책임 범위가 다르면 실제 적용 방식이 달라질 수 있으므로 역할별 차이를 나누어 설명해야 합니다. 지금 확인된 근거로 판단 가능한 범위와 추가 확인이 필요한 지점을 함께 적어야 독자가 실제 선택에 바로 활용할 수 있습니다. 마지막 문장은 추가 확인 범위를 좁혀 제시합니다.\n";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("policy comparison"),
            research_instructions: None,
            evidence_subject: Some("policy comparison"),
        };

        let finalized = finalize_research_output(draft, &artifacts, Some(&diagnostics), &context);

        assert!(finalized
            .output
            .contains("기존 본문 고유 문장은 유지되어야 합니다."));
        assert!(finalized.output.contains("## 출처 감사 (Source Audit)"));
        assert!(finalized
            .output
            .contains("| S1 | https://example1.gov/report/1 |"));
        assert!(finalized.output.contains("## 주장 로그 (Claim Log)"));
        assert!(finalized.output.contains("## 한계, 충돌, 연구 부채"));
        assert!(finalized.output.contains("expected_host=nist.gov"));
        assert!(validate_research_output(&finalized.output, &context).is_ok());
    }

    #[test]
    fn finalization_compacts_machine_artifact_but_keeps_event_cards_parseable() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.events = (1..=40)
            .map(|idx| crate::models::ResearchControllerEvent {
                stage: format!("stage-{idx}"),
                iteration: idx,
                max_iterations: 40,
                status: "running".to_string(),
                detail: Some("controller detail ".repeat(20)),
            })
            .collect();
        artifacts.research_debt = (1..=18)
            .map(|idx| crate::models::ResearchDebtItem {
                id: format!("D{idx}"),
                severity: "high".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                missing_evidence: "historical development density repair item ".repeat(10),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["historical phase evidence query ".repeat(8)],
                next_check_actions: vec!["verify phase card against source cards ".repeat(8)],
                status: "open".to_string(),
            })
            .collect();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("Second Punic War".to_string()),
            working_thesis: Some("Rome won through multi-front endurance.".to_string()),
            event_cards: vec![
                crate::models::NarrativeEventCard {
                    label: "Saguntum crisis".to_string(),
                    timeframe: Some("219-218 BCE".to_string()),
                    actors: vec!["Hannibal".to_string(), "Roman Senate".to_string()],
                    region_or_front: Some("Iberia".to_string()),
                    trigger: Some("Saguntum dispute escalated the treaty conflict.".to_string()),
                    development: Some(
                        "Hannibal's siege converted a local ally dispute into a Roman ultimatum."
                            .to_string(),
                    ),
                    outcome: Some(
                        "The failed settlement carried the conflict into open war.".to_string(),
                    ),
                    source_ids: vec!["S1".to_string()],
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                },
                crate::models::NarrativeEventCard {
                    label: "Alpine crossing".to_string(),
                    timeframe: Some("218 BCE".to_string()),
                    actors: vec!["Hannibal".to_string(), "Gallic allies".to_string()],
                    region_or_front: Some("Alps and northern Italy".to_string()),
                    trigger: Some(
                        "Roman sea power made a direct maritime attack risky.".to_string(),
                    ),
                    development: Some(
                        "The army crossed Gaul and the Alps to shift the theater into Italy."
                            .to_string(),
                    ),
                    outcome: Some(
                        "The crossing opened the Italian campaign and forced Rome to react."
                            .to_string(),
                    ),
                    source_ids: vec!["S2".to_string()],
                    confidence: Some("medium_high".to_string()),
                    open_questions: Vec::new(),
                },
            ],
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);

        assert!(json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(json.contains("\"event_cards\""));
        assert!(!json.contains("\"stage-1\""));
        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();
        assert_eq!(
            parsed
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len()),
            Some(2)
        );
    }

    #[test]
    fn finalization_compacts_verbose_event_cards_to_stay_within_parser_limit() {
        let mut artifacts = sample_finalization_artifacts();
        let long_detail = "전개 세부 설명 ".repeat(24);
        let long_trigger = "직접 계기 설명 ".repeat(16);
        let long_outcome = "국면 결과 설명 ".repeat(16);
        let long_question = "남은 쟁점 설명 ".repeat(10);
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("historical process".to_string()),
            working_thesis: Some("phase-by-phase escalation".to_string()),
            event_cards: (1..=24)
                .map(|idx| crate::models::NarrativeEventCard {
                    label: format!("국면 {idx} {}", "세부 구분".repeat(10)),
                    timeframe: Some(format!("{idx}단계 {}", "시기".repeat(8))),
                    actors: vec![
                        format!("행위자 {idx} {}", "설명".repeat(8)),
                        format!("기관 {idx} {}", "설명".repeat(8)),
                        format!("동맹 {idx} {}", "설명".repeat(8)),
                    ],
                    region_or_front: Some(format!("지역 {idx} {}", "전선".repeat(10))),
                    trigger: Some(format!("{idx} {long_trigger}")),
                    development: Some(format!("{idx} {long_detail}")),
                    outcome: Some(format!("{idx} {long_outcome}")),
                    source_ids: vec![
                        format!("S{idx}"),
                        format!("S{}{}", idx, "X".repeat(20)),
                        format!("S{}{}", idx, "Y".repeat(20)),
                    ],
                    confidence: Some("medium_high".to_string()),
                    open_questions: vec![
                        format!("{idx} {long_question}"),
                        format!("{idx} {}", "추가 검증".repeat(10)),
                    ],
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);

        assert!(json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(json.contains("\"event_cards\""));

        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();
        let event_card_count = parsed
            .narrative_state
            .as_ref()
            .map(|state| state.event_cards.len())
            .unwrap_or(0);

        assert!(event_card_count > 0);
        assert!(event_card_count < 24);
    }

    #[test]
    fn finalization_preserves_late_historical_event_cards_when_compacting_output() {
        let mut artifacts = sample_finalization_artifacts();
        let labels = [
            "Old Regime crisis",
            "Estates-General to National Assembly",
            "Popular revolution and legal rupture",
            "Constitutional monarchy strain",
            "Local conflict consolidation",
            "Fiscal emergency aftermath",
            "War and republican rupture",
            "Thermidor",
            "Directory settlement",
            "European order transformed",
        ];
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("French Revolution".to_string()),
            working_thesis: Some(
                "The revolution moved through distinct political phases.".to_string(),
            ),
            event_cards: labels
                .iter()
                .enumerate()
                .map(|(idx, label)| crate::models::NarrativeEventCard {
                    label: (*label).to_string(),
                    timeframe: Some(format!("phase {}", idx + 1)),
                    actors: vec![format!("actor {}", idx + 1)],
                    region_or_front: Some("France and Europe".to_string()),
                    trigger: Some(format!("trigger {}", idx + 1)),
                    development: Some(format!("development {}", idx + 1)),
                    outcome: Some(format!("outcome {}", idx + 1)),
                    source_ids: vec![format!("S{}", idx + 1)],
                    confidence: Some("medium".to_string()),
                    open_questions: Vec::new(),
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);
        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();
        let retained_labels: Vec<_> = parsed
            .narrative_state
            .as_ref()
            .unwrap()
            .event_cards
            .iter()
            .map(|card| card.label.as_str())
            .collect();

        assert_eq!(retained_labels.len(), MAX_OUTPUT_ARTIFACT_EVENT_CARDS);
        assert!(retained_labels.contains(&"Old Regime crisis"));
        assert!(retained_labels.contains(&"Thermidor"));
        assert!(retained_labels.contains(&"Directory settlement"));
        assert!(retained_labels.contains(&"European order transformed"));
    }

    #[test]
    fn finalization_preserves_requested_historical_anchors_when_aggressively_compacting_output() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.source_cards = (1..=18)
            .map(|idx| crate::models::ResearchSourceCard {
                id: format!("S{idx}"),
                url: format!("https://example{idx}.org/{}", "long-path/".repeat(8)),
                title: format!("Verbose source {idx} {}", "detail ".repeat(20)),
                source_class: "authoritative secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec![format!("fact {idx} {}", "detail ".repeat(20))],
                limitation: Some("overview".to_string()),
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            })
            .collect();
        artifacts.claim_log = (1..=18)
            .map(|idx| crate::models::ResearchClaimLogEntry {
                id: format!("C{idx}"),
                claim: format!("claim {idx} {}", "supported detail ".repeat(16)),
                claim_type: Some("historical_process".to_string()),
                support_source_card_ids: vec![format!("S{idx}")],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: None,
            })
            .collect();
        let labels = [
            "Old Regime fiscal and social crisis",
            "Estates-General and National Assembly",
            "Parisian revolt and Bastille",
            "Great Fear and abolition of feudalism",
            "Declaration and constitutional reconstruction",
            "War and fall of monarchy",
            "National Convention and republic",
            "Civil war, faction, and Terror",
            "Levee en masse and military transformation",
            "Thermidorian Reaction",
            "Napoleonic consolidation and legal legacy",
            "Directory and military dependence",
            "Congress of Vienna and post-revolutionary order",
        ];
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("French Revolution".to_string()),
            working_thesis: Some("Anchored phase map".to_string()),
            event_cards: labels
                .iter()
                .enumerate()
                .map(|(idx, label)| crate::models::NarrativeEventCard {
                    label: (*label).to_string(),
                    timeframe: Some(format!("phase {}", idx + 1)),
                    actors: vec![format!("actor {}", idx + 1)],
                    region_or_front: Some("France and Europe".to_string()),
                    trigger: Some(format!("trigger {} {}", idx + 1, "detail ".repeat(10))),
                    development: Some(format!(
                        "development {} {}",
                        idx + 1,
                        "phase detail ".repeat(20)
                    )),
                    outcome: Some(format!("outcome {} {}", idx + 1, "result ".repeat(10))),
                    source_ids: vec![format!("S{}", idx + 1)],
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);
        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();
        let retained_labels: Vec<_> = parsed
            .narrative_state
            .as_ref()
            .unwrap()
            .event_cards
            .iter()
            .map(|card| card.label.as_str())
            .collect();

        assert!(json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(retained_labels.len() >= 6);
        assert!(retained_labels
            .iter()
            .any(|label| label.contains("republic")));
        assert!(retained_labels
            .iter()
            .any(|label| label.contains("Thermidorian")));
        assert!(retained_labels.iter().any(|label| label.contains("Vienna")));
    }

    #[test]
    fn finalization_compacts_verbose_source_cards_and_claims_to_stay_within_parser_limit() {
        let mut artifacts = sample_finalization_artifacts();
        let long_url_tail = "detail-path/".repeat(12);
        artifacts.source_cards = (1..=24)
            .map(|idx| crate::models::ResearchSourceCard {
                id: format!("S{idx}"),
                url: format!("https://example{idx}.org/{long_url_tail}{idx}"),
                title: format!("Verbose source card title {idx} {}", "설명".repeat(24)),
                source_class: "official_or_primary".to_string(),
                accessed_at: Some(format!("2026-05-{:02}T12:34:56Z", (idx % 28) + 1)),
                extracted_facts: vec![
                    format!("fact {idx} {}", "추출 세부".repeat(20)),
                    format!("fact {idx} {}", "맥락 세부".repeat(20)),
                    format!("fact {idx} {}", "보강 세부".repeat(20)),
                ],
                limitation: Some(format!("limitation {idx} {}", "주의사항".repeat(18))),
                diagnostics_ref: Some(format!("diag-{idx}-{}", "x".repeat(40))),
                confidence: Some("medium_high".to_string()),
            })
            .collect();
        artifacts.claim_log = (1..=24)
            .map(|idx| crate::models::ResearchClaimLogEntry {
                id: format!("C{idx}"),
                claim: format!("verbose claim {idx} {}", "근거 연결 설명".repeat(30)),
                claim_type: Some("historical_process".to_string()),
                support_source_card_ids: vec![
                    format!("S{idx}"),
                    format!("S{}", ((idx + 1 - 1) % 24) + 1),
                    format!("S{}", ((idx + 2 - 1) % 24) + 1),
                ],
                support_urls: vec![
                    format!(
                        "https://support.example.org/{idx}/{}",
                        "reference/".repeat(16)
                    ),
                    format!("https://docs.example.org/{idx}/{}", "evidence/".repeat(16)),
                ],
                confidence: Some("medium".to_string()),
                uncertainty_note: Some(format!("uncertainty {idx} {}", "추가 검증".repeat(18))),
                needs_verification: None,
            })
            .collect();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("historical process".to_string()),
            working_thesis: Some("long causal chain".to_string()),
            event_cards: (1..=12)
                .map(|idx| crate::models::NarrativeEventCard {
                    label: format!("국면 {idx} {}", "세부 구분".repeat(8)),
                    timeframe: Some(format!("{idx}단계 {}", "시기".repeat(6))),
                    actors: vec![format!("행위자 {idx} {}", "설명".repeat(8))],
                    region_or_front: Some(format!("지역 {idx} {}", "전선".repeat(8))),
                    trigger: Some(format!("계기 {idx} {}", "원인".repeat(10))),
                    development: Some(format!("전개 {idx} {}", "세부 설명".repeat(16))),
                    outcome: Some(format!("결과 {idx} {}", "영향".repeat(10))),
                    source_ids: vec![format!("S{idx}")],
                    confidence: Some("medium".to_string()),
                    open_questions: vec![format!("쟁점 {idx} {}", "후속 확인".repeat(8))],
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);

        assert!(json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(json.contains("\"source_cards\""));
        assert!(json.contains("\"claim_log\""));
        assert!(json.contains("\"event_cards\""));

        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();

        assert!(!parsed.source_cards.is_empty());
        assert!(!parsed.claim_log.is_empty());
        assert!(parsed
            .narrative_state
            .as_ref()
            .map(|state| !state.event_cards.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn finalization_measures_the_same_artifact_json_representation_that_it_emits() {
        let mut artifacts = sample_finalization_artifacts();
        let repeated_url_segment = "p/".repeat(8);
        artifacts.source_cards = (1..=24)
            .map(|idx| crate::models::ResearchSourceCard {
                id: format!("S{idx}"),
                url: format!("https://example{idx}.org/{repeated_url_segment}{idx}"),
                title: format!("Boundary source {idx}"),
                source_class: "official_or_primary".to_string(),
                accessed_at: Some(format!("2026-05-{:02}T12:34:56Z", (idx % 28) + 1)),
                extracted_facts: vec![format!("fact {idx} a"), format!("fact {idx} b")],
                limitation: Some(format!("limit {idx}")),
                diagnostics_ref: Some(format!("diag-{idx}")),
                confidence: Some("medium_high".to_string()),
            })
            .collect();
        artifacts.claim_log = (1..=24)
            .map(|idx| crate::models::ResearchClaimLogEntry {
                id: format!("C{idx}"),
                claim: format!("claim {idx} {}", "e ".repeat(4)),
                claim_type: Some("historical_process".to_string()),
                support_source_card_ids: vec![format!("S{idx}"), format!("S{}", idx.max(2) - 1)],
                support_urls: vec![
                    format!("https://support.example.org/{idx}/{}", "e/".repeat(8)),
                    format!("https://docs.example.org/{idx}/{}", "d/".repeat(8)),
                ],
                confidence: Some("medium".to_string()),
                uncertainty_note: Some(format!("note {idx} {}", "u ".repeat(4))),
                needs_verification: None,
            })
            .collect();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            topic_frame: Some("historical process".to_string()),
            working_thesis: Some("boundary compaction".to_string()),
            event_cards: (1..=16)
                .map(|idx| crate::models::NarrativeEventCard {
                    label: format!("국면 {idx}"),
                    timeframe: Some(format!("{idx}단계")),
                    actors: vec![format!("행위자 {idx}")],
                    region_or_front: Some(format!("지역 {idx}")),
                    trigger: Some(format!("계기 {idx} {}", "t ".repeat(3))),
                    development: Some(format!("전개 {idx} {}", "d ".repeat(4))),
                    outcome: Some(format!("결과 {idx} {}", "o ".repeat(3))),
                    source_ids: vec![format!("S{idx}")],
                    confidence: Some("medium".to_string()),
                    open_questions: vec![format!("쟁점 {idx} {}", "q ".repeat(2))],
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let raw_minified_len = serde_json::to_string(&artifacts).unwrap().len();
        let raw_pretty_len = output_research_artifact_json(&artifacts).len();

        assert!(raw_minified_len <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(raw_pretty_len > MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert_eq!(artifact_json_len(&artifacts), raw_pretty_len);

        let compact = compact_research_artifacts_for_output(&artifacts);
        let minified_len = serde_json::to_string(&compact).unwrap().len();
        let pretty_json = output_research_artifact_json(&compact);

        assert!(minified_len <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(pretty_json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);
        assert!(pretty_json.len() >= minified_len);

        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{pretty_json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();

        assert!(!parsed.source_cards.is_empty());
        assert!(!parsed.claim_log.is_empty());
        assert!(parsed
            .narrative_state
            .as_ref()
            .map(|state| !state.event_cards.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn finalization_prioritizes_claim_referenced_late_source_cards_during_compaction() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.source_cards = (1..=24)
            .map(|idx| crate::models::ResearchSourceCard {
                id: format!("S{idx}"),
                url: format!("https://example{idx}.org/source/{}", "detail/".repeat(10)),
                title: format!("Verbose source title {idx} {}", "설명".repeat(16)),
                source_class: "official_or_primary".to_string(),
                accessed_at: Some(format!("2026-05-{:02}T12:00:00Z", (idx % 28) + 1)),
                extracted_facts: vec![
                    format!("fact {idx} {}", "세부".repeat(18)),
                    format!("fact {idx} {}", "맥락".repeat(18)),
                ],
                limitation: Some(format!("limit {idx} {}", "주의".repeat(16))),
                diagnostics_ref: Some(format!("diag-{idx}-{}", "x".repeat(24))),
                confidence: Some("medium".to_string()),
            })
            .collect();
        artifacts.claim_log = (17..=24)
            .map(|idx| crate::models::ResearchClaimLogEntry {
                id: format!("C{idx}"),
                claim: format!("retained claim {idx} {}", "근거 연결".repeat(20)),
                claim_type: Some("historical_process".to_string()),
                support_source_card_ids: vec![format!("S{idx}")],
                support_urls: vec![format!(
                    "https://support.example.org/{idx}/{}",
                    "evidence/".repeat(10)
                )],
                confidence: Some("medium".to_string()),
                uncertainty_note: Some(format!("note {idx} {}", "추가 확인".repeat(12))),
                needs_verification: None,
            })
            .collect();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            event_cards: (1..=8)
                .map(|idx| crate::models::NarrativeEventCard {
                    label: format!("국면 {idx} {}", "세부 구분".repeat(6)),
                    timeframe: Some(format!("{idx}단계 {}", "시기".repeat(4))),
                    actors: vec![format!("행위자 {idx}")],
                    region_or_front: Some(format!("지역 {idx}")),
                    trigger: Some(format!("계기 {idx} {}", "원인".repeat(8))),
                    development: Some(format!("전개 {idx} {}", "세부 설명".repeat(12))),
                    outcome: Some(format!("결과 {idx} {}", "영향".repeat(8))),
                    source_ids: vec![format!("S{idx}")],
                    confidence: Some("medium".to_string()),
                    open_questions: vec![format!("쟁점 {idx}")],
                })
                .collect(),
            ..crate::models::NarrativeState::default()
        });

        let json = pretty_research_artifact_json(&artifacts);
        assert!(json.len() <= MAX_RESEARCH_ARTIFACT_JSON_BYTES);

        let output = format!("[RESEARCH_ARTIFACT_JSON]\n```json\n{json}\n```");
        let parsed = parse_research_artifact_block(&output, "md").unwrap();
        let parsed_source_ids = parsed
            .source_cards
            .iter()
            .map(|card| card.id.clone())
            .collect::<HashSet<_>>();

        assert!(!parsed.claim_log.is_empty());
        assert!(parsed.claim_log.iter().all(|claim| claim
            .support_source_card_ids
            .iter()
            .all(|id| parsed_source_ids.contains(id))));
        assert!(parsed_source_ids.contains("S17"));
    }

    #[test]
    fn finalization_separates_reader_facing_markdown_headings_with_blank_lines() {
        let normalized = normalize_reader_markdown_heading_boundaries(
            "이 판단은 현재 확인된 사료 범위에서는 비교적 안전합니다.## 배경\n혁명의 구조적 원인을 먼저 정리합니다.\n## 전개\n주요 국면별 사건을 나눠 설명합니다.\n",
        );
        assert_eq!(
            normalized,
            "이 판단은 현재 확인된 사료 범위에서는 비교적 안전합니다.\n\n### 배경\n혁명의 구조적 원인을 먼저 정리합니다.\n\n### 전개\n주요 국면별 사건을 나눠 설명합니다.\n"
        );

        let artifacts = sample_finalization_artifacts();
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("historical explanation"),
            research_instructions: None,
            evidence_subject: Some("historical explanation"),
        };
        let draft = "## 최종 답변 (Final Answer)\n\n이 판단은 현재 확인된 사료 범위에서는 비교적 안전합니다.## 배경\n혁명의 구조적 원인을 먼저 정리합니다.\n## 전개\n주요 국면별 사건을 나눠 설명합니다.\n";

        let finalized = finalize_research_output(draft, &artifacts, None, &context);
        assert!(finalized
            .output
            .starts_with("## 최종 답변 (Final Answer)\n\n"));
        assert!(finalized.output.contains("비교적 안전합니다.\n\n### 배경"));
        assert!(finalized
            .output
            .contains("\n\n### 전개\n주요 국면별 사건을 나눠 설명합니다."));
        assert!(!finalized.output.contains("안전합니다.## 배경"));
        assert!(!finalized.output.contains("안전합니다.\n## 배경"));
        assert!(!finalized.output.contains("안전합니다.## 전개"));
        assert!(!finalized.output.starts_with("### 최종 답변 (Final Answer)"));
    }

    #[test]
    fn finalization_does_not_split_csharp_prose_as_markdown_heading() {
        let normalized = normalize_reader_markdown_heading_boundaries("C# 11 introduced changes.");
        assert_eq!(normalized, "C# 11 introduced changes.");

        let artifacts = sample_finalization_artifacts();
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("technology implementation guide"),
            research_instructions: None,
            evidence_subject: Some("technology implementation guide"),
        };
        let draft = "## 최종 답변 (Final Answer)\n\nC# 11 introduced changes.\n문단 끝입니다.## 배경\n세부 구현 설명입니다.\n";

        let finalized = finalize_research_output(draft, &artifacts, None, &context);
        assert!(finalized
            .output
            .starts_with("## 최종 답변 (Final Answer)\n\nC# 11 introduced changes."));
        assert!(finalized.output.contains("문단 끝입니다.\n\n### 배경"));
        assert!(finalized.output.contains("C# 11 introduced changes."));
        assert!(!finalized.output.contains("C\n\n# 11"));
    }

    #[test]
    fn finalization_preserves_final_answer_wrapper_after_leading_preamble() {
        let artifacts = sample_finalization_artifacts();
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("historical explanation"),
            research_instructions: None,
            evidence_subject: Some("historical explanation"),
        };
        let draft = "# 문서 제목\n\n간단한 서문입니다.\n\n## 최종 답변 (Final Answer)\n\nC# 11 introduced changes.\n문단 끝입니다.## 배경\n세부 구현 설명입니다.\n";

        let finalized = finalize_research_output(draft, &artifacts, None, &context);
        assert!(finalized
            .output
            .contains("\n\n## 최종 답변 (Final Answer)\n\n"));
        assert!(finalized.output.contains("C# 11 introduced changes."));
        assert!(finalized.output.contains("문단 끝입니다.\n\n### 배경"));
        assert!(!finalized
            .output
            .contains("\n\n### 최종 답변 (Final Answer)"));
        assert!(!finalized.output.contains("C\n\n# 11"));
    }

    #[test]
    fn finalization_synthesizes_fallback_and_redacts_sensitive_diagnostics() {
        let artifacts = sample_finalization_artifacts();
        let diagnostics = crate::models::ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("product comparison".to_string()),
            source_pack: Some(crate::models::ResearchSourcePackReport {
                subject: Some("product comparison".to_string()),
                status: "partial".to_string(),
                reason: Some("api_key leaked response body: raw provider payload".to_string()),
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: Some("product comparison"),
            research_instructions: None,
            evidence_subject: Some("product comparison"),
        };

        let finalized = finalize_research_output(
            "본문만 있고 최종 답변 표시는 없습니다.",
            &artifacts,
            Some(&diagnostics),
            &context,
        );

        assert!(finalized.output.contains("## 최종 답변 (Final Answer)"));
        assert!(finalized.output.contains("[redacted_key]"));
        assert!(!finalized.output.contains("raw provider payload"));
        assert!(!finalized.output.contains("api_key"));
    }

    #[test]
    fn fallback_reader_text_preserves_markdown_paragraph_breaks() {
        let preserved = fallback_reader_text(
            "첫 문단 첫 줄.\n두 번째 줄.\n\n- 불릿 하나\n- 불릿 둘\n\n마지막 문단.",
        );

        assert!(preserved.contains("첫 문단 첫 줄.\n두 번째 줄."));
        assert!(preserved.contains("\n\n- 불릿 하나\n- 불릿 둘\n\n"));
        assert!(!preserved.contains("첫 문단 첫 줄. 두 번째 줄. - 불릿 하나"));
    }

    #[test]
    fn finalization_filters_adopted_coverage_expectations_from_visible_miss_section() {
        let artifacts = sample_finalization_artifacts();
        let diagnostics = crate::models::ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("product comparison".to_string()),
            source_pack: Some(crate::models::ResearchSourcePackReport {
                subject: Some("product comparison".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: vec![
                    crate::models::ResearchSourceCoverageMiss {
                        expected_host: Some("apple.com".to_string()),
                        expected_source_class: Some("official_or_primary".to_string()),
                        query: "apple.com official technical specifications".to_string(),
                        provider: Some("naver".to_string()),
                        status: "adopted".to_string(),
                        reason: Some("Target host apple.com was recovered.".to_string()),
                    },
                    crate::models::ResearchSourceCoverageMiss {
                        expected_host: Some("nist.gov".to_string()),
                        expected_source_class: Some("official_or_primary".to_string()),
                        query: "nist.gov official guidance".to_string(),
                        provider: Some("naver".to_string()),
                        status: "missed".to_string(),
                        reason: Some("Target host nist.gov was not recovered.".to_string()),
                    },
                ],
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("product comparison"),
            research_instructions: None,
            evidence_subject: Some("product comparison"),
        };
        let finalized = finalize_research_output(
            "## 최종 답변 (Final Answer)\n\n충분한 길이의 비교 결론입니다. 정책 제약과 기술 제약이 어떤 상황에서 갈리는지 먼저 정리하고, 실제 선택에 영향을 주는 운영 조건과 확인된 근거를 함께 설명합니다. 현재 자료로 단정할 수 있는 범위와 보수적으로 봐야 할 한계를 분리해 적어야 독자가 비교 기준을 바로 적용할 수 있습니다. 마지막 문장은 다음 확인 범위를 제시합니다.\n",
            &artifacts,
            Some(&diagnostics),
            &context,
        );

        assert!(finalized.output.contains("expected_host=nist.gov"));
        assert!(!finalized.output.contains("expected_host=apple.com"));
    }

    #[test]
    fn finalization_repair_paragraph_avoids_prompt_echo_and_internal_validation_labels() {
        let artifacts = sample_finalization_artifacts();
        let diagnostics = crate::models::ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("남산 아침 러닝과 카페 동선".to_string()),
            source_pack: Some(crate::models::ResearchSourcePackReport {
                subject: Some("남산 아침 러닝과 카페 동선".to_string()),
                status: "partial".to_string(),
                reason: Some("thin but relevant local evidence".to_string()),
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 3,
                adopted_source_count: 2,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: vec![crate::models::ResearchSourceCoverageMiss {
                    expected_host: Some("namsanpark.seoul.go.kr".to_string()),
                    expected_source_class: Some("official_or_primary".to_string()),
                    query: "남산공원 공식".to_string(),
                    provider: Some("naver".to_string()),
                    status: "missed".to_string(),
                    reason: Some("Target host was not recovered.".to_string()),
                }],
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates. The output should be useful for actually deciding where to run and where to go afterward."),
            research_instructions: None,
            evidence_subject: Some("Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward."),
        };

        let finalized = finalize_research_output(
            "## 최종 답변 (Final Answer)\n\n짧은 결론입니다.\n",
            &artifacts,
            Some(&diagnostics),
            &context,
        );
        let final_answer = final_answer_section(&strip_research_artifact_blocks(&finalized.output))
            .map(section_body_without_heading)
            .expect("final answer");
        let lower = final_answer.to_ascii_lowercase();

        assert!(!lower.contains("someone planning"));
        assert!(!lower.contains("source pack"));
        assert!(!lower.contains("open debt"));
        assert!(!lower.contains("target-host"));
        assert!(!lower.contains("target host"));
        assert!(!lower.contains("source-class"));
        assert!(!lower.contains("source class"));
        assert!(!lower.contains("repair search hints"));
        assert!(!lower.contains("not-yet-adopted evidence"));
        assert!(!lower.contains("query:"));
        assert!(!lower.contains("provider:"));
        assert!(!lower.contains("snippet:"));
        assert!(!lower.contains("chronology"));
        assert!(!lower.contains("actor"));
        assert!(!lower.contains("cause"));
        assert!(!lower.contains("consequence"));
    }

    #[test]
    fn finalization_surfaces_remaining_narrative_open_gaps_without_leaking_internal_labels() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            timeline: vec![crate::models::NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "배경 형성".to_string(),
                date_anchor: Some("2019".to_string()),
                significance: Some("정책 변화의 출발점".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            actors: vec![crate::models::NarrativeActor {
                id: "NA1".to_string(),
                label: "감독 기관".to_string(),
                role: Some("집행".to_string()),
                relevance: Some("책임 분기".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "impact".to_string(),
                description: "지역별 운영 영향의 정량 비교는 추가 확인이 필요합니다.".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..crate::models::NarrativeState::default()
        });
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("policy explanation"),
            research_instructions: None,
            evidence_subject: Some("policy explanation"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);
        let visible = strip_research_artifact_blocks(&finalized.output);
        let final_answer = final_answer_section(&visible)
            .map(section_body_without_heading)
            .expect("final answer");
        let lower = final_answer.to_ascii_lowercase();

        assert!(visible.contains("### Remaining Explanatory Limits"));
        assert!(visible.contains("지역별 운영 영향의 정량 비교는 추가 확인이 필요합니다."));
        assert!(final_answer.contains("배경 형성"));
        assert!(final_answer.contains("감독 기관"));
        assert!(!lower.contains("narrative_state"));
        assert!(!lower.contains("<narrative_state"));
        assert!(!visible.contains("NG1"));
    }

    #[test]
    fn finalization_suppresses_generic_narrative_gap_labels_for_technology_topics() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "gap".to_string(),
                description: "narrative gap 1".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: Vec::new(),
            }],
            ..crate::models::NarrativeState::default()
        });
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("AI 개념과 LLM, RAG, agent의 차이와 오해"),
            research_instructions: None,
            evidence_subject: Some("AI 개념과 LLM, RAG, agent의 차이와 오해"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);
        let visible = strip_research_artifact_blocks(&finalized.output);
        let lower = visible.to_ascii_lowercase();

        assert!(!lower.contains("narrative gap"));
        assert!(!visible.contains("### Remaining Explanatory Limits"));
    }

    #[test]
    fn finalization_does_not_render_narrative_labels_without_supported_claim_refs() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            timeline: vec![crate::models::NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "검증되지 않은 전환점".to_string(),
                date_anchor: Some("2019".to_string()),
                significance: Some("bogus".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            actors: vec![crate::models::NarrativeActor {
                id: "NA1".to_string(),
                label: "근거 없는 주도 세력".to_string(),
                role: Some("bogus".to_string()),
                relevance: Some("bogus".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            causal_chain: vec![crate::models::NarrativeCausalLink {
                id: "NL1".to_string(),
                cause: "근거 없는 원인".to_string(),
                effect: "근거 없는 결과".to_string(),
                rationale: Some("bogus".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..crate::models::NarrativeState::default()
        });
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("historical explanation"),
            research_instructions: None,
            evidence_subject: Some("historical explanation"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);
        let final_answer = final_answer_section(&strip_research_artifact_blocks(&finalized.output))
            .map(section_body_without_heading)
            .expect("final answer");

        assert!(!final_answer.contains("검증되지 않은 전환점"));
        assert!(!final_answer.contains("근거 없는 주도 세력"));
        assert!(!final_answer.contains("근거 없는 원인"));
        assert!(!final_answer.contains("근거 없는 결과"));
    }

    #[test]
    fn finalization_does_not_render_source_only_section_or_evidence_labels() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.narrative_state = Some(crate::models::NarrativeState {
            version: 1,
            section_outline: vec![crate::models::NarrativeSectionOutlineItem {
                id: "NS1".to_string(),
                heading: "근거 없는 비교 축".to_string(),
                purpose: Some("bogus".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            evidence_layers: vec![crate::models::NarrativeEvidenceLayer {
                id: "NL1".to_string(),
                label: "근거 없는 사료 층위".to_string(),
                purpose: Some("bogus".to_string()),
                expected_claim_log_ids: Vec::new(),
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..crate::models::NarrativeState::default()
        });
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("historical explanation"),
            research_instructions: None,
            evidence_subject: Some("historical explanation"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);
        let final_answer = final_answer_section(&strip_research_artifact_blocks(&finalized.output))
            .map(section_body_without_heading)
            .expect("final answer");

        assert!(!final_answer.contains("근거 없는 비교 축"));
        assert!(!final_answer.contains("근거 없는 사료 층위"));
    }

    #[test]
    fn finalization_repair_uses_generic_korean_subject_for_long_english_cpp_prompt() {
        let artifacts = sample_finalization_artifacts();
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("an experienced C++ developer planning to implement a work-stealing scheduler. Cover the core scheduling model, Chase-Lev work-stealing deque design, worker/local queue vs global injection queue, memory ordering, blocking and parking/wakeup strategy, cancellation/shutdown, instrumentation, and benchmark strategy."),
            research_instructions: None,
            evidence_subject: Some("an experienced C++ developer planning to implement a work-stealing scheduler. Cover the core scheduling model, Chase-Lev work-stealing deque design, worker/local queue vs global injection queue, memory ordering, blocking and parking/wakeup strategy, cancellation/shutdown, instrumentation, and benchmark strategy."),
        };

        let finalized = finalize_research_output(
            "## 최종 답변 (Final Answer)\n\n짧은 결론입니다.\n",
            &artifacts,
            None,
            &context,
        );
        let final_answer = final_answer_section(&strip_research_artifact_blocks(&finalized.output))
            .map(section_body_without_heading)
            .expect("final answer");
        let lower = final_answer.to_ascii_lowercase();

        assert!(final_answer.contains("이 기술 구현 주제"));
        assert!(!lower.contains("an experienced c++ developer"));
        assert!(!lower.contains("planning to implement"));
        assert!(!lower.contains("worker/local queue"));
        assert!(!lower.contains("memory ordering"));
    }

    #[test]
    fn finalization_renders_caveated_promoted_conflicts_under_deferred_heading() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.conflict_map = vec![crate::models::ResearchConflictMapEntry {
            id: "K3".to_string(),
            topic: "NIST AI RMF enforcement scope ambiguity".to_string(),
            conflicting_claim_ids: vec!["C1".to_string()],
            source_card_ids: vec!["S1".to_string()],
            resolution_status: Some("resolved_with_caveat".to_string()),
            resolution_note: Some("Needs one more official clarification".to_string()),
            promoted_to_debt: Some(true),
        }];
        artifacts.research_debt = vec![crate::models::ResearchDebtItem {
            id: "D1".to_string(),
            severity: "medium".to_string(),
            failed_gate: None,
            missing_evidence:
                "Conflict K3 needs one more official clarification for claim C1 and source card S1"
                    .to_string(),
            required_source_class: Some("official_or_primary".to_string()),
            candidate_queries: vec!["NIST AI RMF official clarification".to_string()],
            next_check_actions: vec!["Keep K3 visible as deferred debt".to_string()],
            status: "open".to_string(),
        }];
        artifacts.quality_gate = Some(crate::models::ResearchQualityGateArtifact {
            status: "passed".to_string(),
            failure_messages: Vec::new(),
            unsupported_claim_count: 0,
            unresolved_conflict_count: 0,
            open_debt_count: 1,
        });
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("policy comparison"),
            research_instructions: None,
            evidence_subject: Some("policy comparison"),
        };

        let finalized = finalize_research_output(
            "## 최종 답변 (Final Answer)\n\n충분한 길이의 정책 비교 결론입니다. 정책 변화가 어떤 맥락과 순서로 이어졌는지 먼저 설명하고, 기관별 역할 차이와 현재 확인 가능한 판단 범위를 분리해 적습니다. 남아 있는 불확실성과 후속 확인 포인트를 함께 제시해야 독자가 실제 대응 방향을 정할 수 있습니다. 마지막 문장은 추가 확인 범위를 제시합니다.\n",
            &artifacts,
            None,
            &context,
        );

        assert!(finalized
            .output
            .contains("### Deferred / Caveated Conflicts"));
        assert!(finalized.output.contains("status=resolved_with_caveat"));
        assert!(!finalized.output.contains("### Unresolved Conflicts"));
        assert!(finalized.output.contains("### Research Debt"));
    }

    #[test]
    fn finalization_renders_html_sections_and_hidden_artifact_script() {
        let artifacts = sample_finalization_artifacts();
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("medium"),
            quality_depth: Some("strict"),
            research_topic: Some("html replay"),
            research_instructions: None,
            evidence_subject: Some("html replay"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);

        assert!(finalized
            .output
            .contains("<h2>최종 답변 (Final Answer)</h2>"));
        assert!(finalized
            .output
            .contains("<h2>출처 감사 (Source Audit)</h2>"));
        assert!(finalized.output.contains("data-research-artifacts"));
    }

    #[test]
    fn finalization_renders_html_deferred_conflicts_without_unresolved_heading() {
        let mut artifacts = sample_finalization_artifacts();
        artifacts.conflict_map = vec![crate::models::ResearchConflictMapEntry {
            id: "K3".to_string(),
            topic: "NIST AI RMF enforcement scope ambiguity".to_string(),
            conflicting_claim_ids: vec!["C1".to_string()],
            source_card_ids: vec!["S1".to_string()],
            resolution_status: Some("resolved_with_caveat".to_string()),
            resolution_note: Some("Needs one more official clarification".to_string()),
            promoted_to_debt: Some(true),
        }];
        artifacts.research_debt = vec![crate::models::ResearchDebtItem {
            id: "D1".to_string(),
            severity: "medium".to_string(),
            failed_gate: None,
            missing_evidence:
                "Conflict K3 needs one more official clarification for claim C1 and source card S1"
                    .to_string(),
            required_source_class: Some("official_or_primary".to_string()),
            candidate_queries: vec!["NIST AI RMF official clarification".to_string()],
            next_check_actions: vec!["Keep K3 visible as deferred debt".to_string()],
            status: "open".to_string(),
        }];
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "html",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("policy comparison"),
            research_instructions: None,
            evidence_subject: Some("policy comparison"),
        };

        let finalized = finalize_research_output("", &artifacts, None, &context);

        assert!(finalized
            .output
            .contains("<h3>Deferred / Caveated Conflicts</h3>"));
        assert!(finalized.output.contains("resolved_with_caveat"));
        assert!(!finalized.output.contains("<h3>Unresolved Conflicts</h3>"));
    }

    #[test]
    fn strict_explanatory_validation_fails_when_narrative_open_gap_disappears_from_visible_output()
    {
        let output = r#"
## 최종 답변 (Final Answer)

이 설명은 정책 변화의 배경과 현재 결론만 간단히 적고 추가 공백은 언급하지 않습니다. 감독 기관과 사업자 역할을 구분한다고 말하지만, 실제로 어떤 영향이 남는지는 쓰지 않았습니다. 현재 공개 근거가 가리키는 방향만 요약하고 남은 한계를 숨긴 채 마무리합니다. 마지막 문장은 후속 확인 범위를 적지 않습니다.

# 검증 부록
## 출처 감사 (Source Audit)
| ID | URL | Source | Class | Checked Fact | Limitation | Diagnostics |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | https://example1.gov/report/1 | Official Source 1 | official_or_primary | fact 1 | scope limits | - |
| S2 | https://example2.gov/report/2 | Official Source 2 | official_or_primary | fact 2 | scope limits | - |
| S3 | https://example3.gov/report/3 | Official Source 3 | official_or_primary | fact 3 | scope limits | - |
| S4 | https://example4.gov/report/4 | Official Source 4 | official_or_primary | fact 4 | scope limits | - |
| S5 | https://example5.gov/report/5 | Official Source 5 | official_or_primary | fact 5 | scope limits | - |
| S6 | https://example6.gov/report/6 | Official Source 6 | official_or_primary | fact 6 | scope limits | - |
| S7 | https://example7.gov/report/7 | Official Source 7 | official_or_primary | fact 7 | scope limits | - |
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | 검증된 주장 1 | S1 | high | none |
| C2 | 검증된 주장 2 | S2 | high | none |
| C3 | 검증된 주장 3 | S3 | high | none |
| C4 | 검증된 주장 4 | S4 | high | none |
| C5 | 검증된 주장 5 | S5 | high | none |
| C6 | 검증된 주장 6 | S6 | high | none |
| C7 | 검증된 주장 7 | S7 | high | none |
## 한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)
- Blocking unresolved conflicts: none recorded.
- Open research debt: none.
## 품질 게이트 (Quality Gate)
| Check | Result | Note |
| --- | --- | --- |
| deterministic gate status | passed | artifact-backed finalization rendered visible verification sections before validation |
| failure messages | pass | none |
| unsupported claim count | 0 | lower is better |
| unresolved conflict count | 0 | unresolved items must stay visible as debt or limits |
| open debt count | 0 | open debt never counts as acceptance |
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {"id":"S1","url":"https://example1.gov/report/1","title":"Official Source 1","source_class":"official_or_primary"},
    {"id":"S2","url":"https://example2.gov/report/2","title":"Official Source 2","source_class":"official_or_primary"},
    {"id":"S3","url":"https://example3.gov/report/3","title":"Official Source 3","source_class":"official_or_primary"},
    {"id":"S4","url":"https://example4.gov/report/4","title":"Official Source 4","source_class":"official_or_primary"},
    {"id":"S5","url":"https://example5.gov/report/5","title":"Official Source 5","source_class":"official_or_primary"},
    {"id":"S6","url":"https://example6.gov/report/6","title":"Official Source 6","source_class":"official_or_primary"},
    {"id":"S7","url":"https://example7.gov/report/7","title":"Official Source 7","source_class":"official_or_primary"}
  ],
  "claim_log": [
    {"id":"C1","claim":"검증된 주장 1","support_source_card_ids":["S1"],"confidence":"high"},
    {"id":"C2","claim":"검증된 주장 2","support_source_card_ids":["S2"],"confidence":"high"},
    {"id":"C3","claim":"검증된 주장 3","support_source_card_ids":["S3"],"confidence":"high"},
    {"id":"C4","claim":"검증된 주장 4","support_source_card_ids":["S4"],"confidence":"high"},
    {"id":"C5","claim":"검증된 주장 5","support_source_card_ids":["S5"],"confidence":"high"},
    {"id":"C6","claim":"검증된 주장 6","support_source_card_ids":["S6"],"confidence":"high"},
    {"id":"C7","claim":"검증된 주장 7","support_source_card_ids":["S7"],"confidence":"high"}
  ],
  "conflict_map": [],
  "research_debt": [],
  "narrative_state": {
    "version": 1,
    "timeline": [
      {
        "id": "NE1",
        "label": "배경 형성",
        "expected_claim_log_ids": ["C1"],
        "expected_source_card_ids": ["S1"]
      }
    ],
    "actors": [
      {
        "id": "NA1",
        "label": "감독 기관",
        "expected_claim_log_ids": ["C1"],
        "expected_source_card_ids": ["S1"]
      }
    ],
    "open_gaps": [
      {
        "id": "NG1",
        "gap_type": "impact",
        "description": "지역별 운영 영향의 정량 비교는 추가 확인이 필요합니다.",
        "status": "open",
        "expected_claim_log_ids": ["C1"],
        "expected_source_card_ids": ["S1"]
      }
    ]
  },
  "quality_gate": {"status":"passed","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("policy explanation"),
            research_instructions: None,
            evidence_subject: Some("policy explanation"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("narrative open gaps remain unresolved"));
    }

    #[test]
    fn rejects_uncaveated_historical_answer_that_relies_only_on_weak_supplementary_sources() {
        let output = r#"
## 최종 답변 (Final Answer)

아우렐리아누스는 제3세기 위기의 모든 문제를 혼자 해결했고, 그의 성격과 정책 의도도 분명하게 복원할 수 있다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp",
      "title": "Historia Augusta: Aurelian",
      "source_class": "secondary",
      "extracted_facts": ["late antique biography"]
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Aurelian's motives are fully recoverable",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("historical explanation of the Roman emperor Aurelian"),
            research_instructions: None,
            evidence_subject: Some("historical explanation of the Roman emperor Aurelian"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err
            .contains("historical final answer relies on weak or contested supplementary sources"));
    }

    #[test]
    fn accepts_historical_answer_when_weak_supplementary_source_is_corroborated() {
        let output = r#"
## 최종 답변 (Final Answer)

아우렐리아누스의 재통합 성과는 강한 근거가 있지만, 후기 전승의 세부 묘사는 더 신중하게 읽어야 한다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp",
      "title": "Historia Augusta: Aurelian",
      "source_class": "secondary",
      "extracted_facts": ["late antique biography"]
    },
    {
      "id": "S2",
      "url": "https://www.britannica.com/biography/Aurelian",
      "title": "Aurelian | Roman emperor",
      "source_class": "official_or_primary",
      "extracted_facts": ["modern reference overview"]
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Aurelian reunified the empire",
      "support_source_card_ids": ["S1", "S2"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("historical explanation of the Roman emperor Aurelian"),
            research_instructions: None,
            evidence_subject: Some("historical explanation of the Roman emperor Aurelian"),
        };

        assert!(validate_research_output(output, &context).is_ok());
    }

    #[test]
    fn rejects_thin_strict_historical_event_answer_without_treaty_or_phase_sequence() {
        let output = r#"
## 최종 답변 (Final Answer)

오스트리아 왕위계승전쟁은 합스부르크 계승 문제와 유럽 세력 균형 경쟁에서 비롯되었다. 여러 강대국이 이 문제에 개입하면서 전쟁은 넓어졌고, 결과적으로 유럽 외교 질서에 큰 의미를 남겼다. 프로이센의 위상이 커졌다는 점은 중요하지만, 여기서는 전반적 의의만 간단히 정리한다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/War-of-the-Austrian-Succession",
      "title": "War of the Austrian Succession",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The War of the Austrian Succession reshaped European power politics",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의"),
            research_instructions: None,
            evidence_subject: Some("오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn dense_strict_historical_event_answer_does_not_trigger_development_density_gate() {
        let output = r#"
## 최종 답변 (Final Answer)

1740년 프로이센의 실레지아 침공으로 전쟁이 시작된 뒤, 이후 보헤미아 전선과 이탈리아 전선으로 국면이 갈라졌다. 마리아 테레지아, 프리드리히 2세, 프랑스와 영국은 서로 다른 동맹 계산 속에서 개전 배경을 확대했고, 각 지역 군대와 정부는 전쟁 목표를 다르게 조정했다. 전쟁 후반에는 각 전역의 소모전과 외교 교착이 겹치며 종전 협상이 힘을 얻었고, 1748년 아헨 조약에서 승계권 인정과 영토 조정이라는 종결 결과가 정리되었다. 이런 전개는 계승 분쟁이 어떻게 유럽 세력균형 재편과 프로이센 부상이라는 장기적 영향으로 이어졌는지 보여 준다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/War-of-the-Austrian-Succession",
      "title": "War of the Austrian Succession",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The War of the Austrian Succession unfolded across multiple fronts and ended in the Treaty of Aix-la-Chapelle",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의"),
            research_instructions: None,
            evidence_subject: Some("오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn english_strict_historical_event_title_order_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The conflict began because succession rules were disputed. The impact was large for European politics.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/War-of-the-Austrian-Succession",
      "title": "War of the Austrian Succession",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The War of the Austrian Succession had broad political consequences",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "War of the Austrian Succession background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "War of the Austrian Succession background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn english_named_war_title_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The war began because tensions were already high. Its impact changed later international politics.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/World-War-I",
      "title": "World War I",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "World War I had major geopolitical consequences",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("World War I background, development, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some("World War I background, development, impact, and significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn english_previously_missed_named_war_title_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The war started because political tensions had grown. Its impact changed the region afterward.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/Vietnam-War",
      "title": "Vietnam War",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The Vietnam War had major regional and global effects",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Vietnam War background, development, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some("Vietnam War background, development, impact, and significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn capitalized_named_war_title_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The war began because the dispute escalated. Its impact changed later regional politics.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/Falklands-War",
      "title": "Falklands War",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The Falklands War had major regional and diplomatic effects",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Falklands War background, development, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some(
                "Falklands War background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn metaphorical_war_phrase_does_not_trigger_development_density_gate() {
        let output = r#"
## Final Answer

The war on bugs began because the release schedule was compressed. The impact was slower delivery and more team fatigue.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://martinfowler.com/articles/continuousIntegration.html",
      "title": "Continuous Integration",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Software teams can suffer schedule pressure and defect churn",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("war on bugs background, impact, and team significance"),
            research_instructions: None,
            evidence_subject: Some("war on bugs background, impact, and team significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn title_cased_business_price_war_does_not_trigger_development_density_gate() {
        let output = r#"
## Final Answer

The price war started because competitors cut margins. Its impact changed customer expectations and near-term profitability.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://hbr.org/2018/08/how-to-respond-to-a-price-war",
      "title": "How to Respond to a Price War",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Price wars can reduce profitability and reset customer expectations",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Price War background, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some("Price War background, impact, and significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn ambiguous_company_civil_war_topic_does_not_trigger_development_density_gate() {
        let output = r#"
## Final Answer

The company civil war began because leadership factions diverged. Its impact changed internal morale and short-term execution.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://hbr.org/2020/01/when-leadership-teams-split-into-factions",
      "title": "When leadership teams split into factions",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Leadership factionalism can reduce morale and execution quality",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("company civil war background, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some("company civil war background, impact, and significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn explicit_historical_civil_war_topic_still_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The war began because elite rivalry escalated. Its impact changed later political order.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/Roman-civil-wars",
      "title": "Roman civil wars",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Roman civil wars reshaped Roman political order",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "Roman civil war history background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "Roman civil war history background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn french_revolution_topic_triggers_development_density_gate() {
        let output = r#"
## Final Answer

The revolution began because social tensions escalated. Its impact changed later political life.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution reshaped political order in France and Europe",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn thin_french_revolution_summary_with_phase_labels_still_fails_development_density_gate() {
        let output = r#"
## Final Answer

초기 국면에는 재정 위기와 대표 요구가 쌓였다. 중기 국면에는 급진 세력과 정부의 충돌이 커졌다. 말기 국면에는 체제가 흔들렸고 결과적으로 의의가 커졌다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution transformed French political order",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn richer_french_revolution_phase_by_phase_answer_does_not_trigger_development_density_gate() {
        let output = r#"
## Final Answer

국가 재정 위기와 대표제 갈등 때문에 1789년 삼부회 소집과 바스티유 점령이 이어지며 혁명은 공개 국면에 들어섰다. 이후 국민의회와 왕실, 파리 군중은 입헌군주제와 전쟁 동원을 둘러싸고 충돌했고, 1792년 왕정 폐지와 공화정 수립으로 다음 단계가 시작되었다. 그 뒤 자코뱅 정부와 지방 반란, 대외 전선의 압박이 총동원과 공포정치로 이어졌고, 1794년 테르미도르 반동 이후에는 총재정부가 정권 재편을 시도했다. 이런 전개는 사회적 불만과 국가 재정 위기가 어떻게 급진화, 전시 체제, 체제 전환의 연쇄로 이어졌는지 보여 준다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution moved from 1789 mobilization to republican restructuring by 1794",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn rich_visible_history_answer_still_fails_when_event_scaffold_is_shallow() {
        let output = r#"
## Final Answer

국가 재정 위기와 대표제 갈등 때문에 1789년 삼부회 소집과 바스티유 점령이 이어지며 혁명은 공개 국면에 들어섰다. 이후 국민의회와 왕실, 파리 군중은 입헌군주제와 전쟁 동원을 둘러싸고 충돌했고, 1792년 왕정 폐지와 공화정 수립으로 다음 단계가 시작되었다. 그 뒤 자코뱅 정부와 지방 반란, 대외 전선의 압박이 총동원과 공포정치로 이어졌고, 1794년 테르미도르 반동 이후에는 총재정부가 정권 재편을 시도했다. 이런 전개는 사회적 불만과 국가 재정 위기가 어떻게 급진화, 전시 체제, 체제 전환의 연쇄로 이어졌는지 보여 준다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution moved from 1789 mobilization to republican restructuring by 1794",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": [],
  "narrative_state": {
    "version": 1,
    "event_cards": [
      {
        "label": "초기 국면",
        "timeframe": "1789",
        "development": "봉기가 일어났다."
      }
    ]
  }
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("historical event scaffold is too shallow"));
    }

    #[test]
    fn rich_visible_history_answer_does_not_trigger_event_scaffold_gate_when_cards_are_richer() {
        let output = r#"
## Final Answer

국가 재정 위기와 대표제 갈등 때문에 1789년 삼부회 소집과 바스티유 점령이 이어지며 혁명은 공개 국면에 들어섰다. 이후 국민의회와 왕실, 파리 군중은 입헌군주제와 전쟁 동원을 둘러싸고 충돌했고, 1792년 왕정 폐지와 공화정 수립으로 다음 단계가 시작되었다. 그 뒤 자코뱅 정부와 지방 반란, 대외 전선의 압박이 총동원과 공포정치로 이어졌고, 1794년 테르미도르 반동 이후에는 총재정부가 정권 재편을 시도했다. 이런 전개는 사회적 불만과 국가 재정 위기가 어떻게 급진화, 전시 체제, 체제 전환의 연쇄로 이어졌는지 보여 준다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution moved from 1789 mobilization to republican restructuring by 1794",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": [],
  "narrative_state": {
    "version": 1,
      "event_cards": [
        {
          "label": "개시 국면",
          "timeframe": "1789",
          "actors": ["삼부회", "파리 군중"],
        "region_or_front": "파리와 베르사유",
        "trigger": "재정 위기와 대표 요구 충돌",
          "development": "삼부회 소집 이후 국민의회 구성과 바스티유 점령이 이어지며 공개 혁명 국면이 열렸고, 파리 거리 정치와 대표제 논쟁이 왕권을 압박했다.",
          "outcome": "왕권과 대표제의 충돌이 제도 개편 단계로 넘어갔다."
        },
        {
          "label": "제도 재편 국면",
          "timeframe": "1789-1791",
          "actors": ["국민의회", "왕실"],
          "region_or_front": "파리와 베르사유",
          "trigger": "왕권 제한과 대표제 재설계를 둘러싼 충돌",
          "development": "인권선언과 헌정 개편이 추진되었지만 왕실 도주와 정치 불신이 누적되면서 입헌군주제 타협이 흔들렸고, 교회 재편과 재산권 논쟁도 갈등을 넓혔다.",
          "outcome": "전쟁과 왕실 위기 속에서 공화정 전환 국면의 계기가 마련되었다."
        },
        {
          "label": "전쟁과 왕정 붕괴 국면",
          "timeframe": "1791-1792",
          "actors": ["입법의회", "루이 16세", "파리 민중"],
          "region_or_front": "파리, 튈르리, 대외 전선",
          "trigger": "바렌 도주 이후 왕실 불신과 오스트리아 전쟁 압력",
          "development": "대외 전쟁과 패전 공포가 왕실의 배신 의혹을 키웠고, 파리 민중과 혁명 세력은 튈르리 궁 공격으로 군주정을 무너뜨렸다.",
          "outcome": "국민공회 소집과 왕정 폐지가 공화정 수립의 직접 조건이 되었다."
        },
        {
          "label": "공화정 수립 국면",
          "timeframe": "1792-1793",
          "actors": ["국민공회", "지롱드파", "산악파"],
          "region_or_front": "파리와 국민공회",
          "trigger": "군주정 붕괴 뒤 새 주권 형태를 확정해야 하는 정치적 압박",
          "development": "국민공회는 왕정을 폐지하고 공화정을 선포했지만, 루이 16세 재판과 처형을 둘러싼 갈등이 혁명 내부의 분열을 심화시켰다.",
          "outcome": "대외 전쟁 확대와 내전 압력이 비상정부와 공포정치의 조건을 만들었다."
        },
        {
          "label": "급진화 국면",
          "timeframe": "1792-1794",
          "actors": ["국민공회", "자코뱅 정부"],
        "region_or_front": "파리와 대외 전선",
        "trigger": "전쟁 압력과 왕실 불신",
        "development": "왕정 폐지와 공화정 수립 이후 총동원과 공포정치가 이어졌고, 지방 반란과 대외 전선의 압박이 체제 급진화를 밀어 올렸다.",
        "outcome": "테르미도르 반동과 총재정부 재편으로 다음 정치 질서가 열렸다."
      },
      {
        "label": "테르미도르와 총재정부 국면",
        "timeframe": "1794-1799",
        "actors": ["국민공회", "반자코뱅 세력", "총재정부"],
        "region_or_front": "파리와 프랑스 국내 정치",
        "trigger": "공포정치 피로와 로베스피에르 권력 집중에 대한 공포",
        "development": "로베스피에르 실각 뒤 공포정치 장치가 약화되었고, 총재정부는 급진 민주주의와 왕당파 복귀를 동시에 막으려 했지만 군대 의존이 커졌다.",
        "outcome": "정치 불안과 군사화가 브뤼메르 쿠데타와 나폴레옹 부상의 조건이 되었다."
      }
    ]
  }
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical event scaffold is too shallow"));
    }

    #[test]
    fn broad_historical_event_topics_still_fail_when_only_two_rich_phase_cards_are_present() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    crate::models::NarrativeEventCard {
                        label: "개시 국면".to_string(),
                        timeframe: Some("1789".to_string()),
                        actors: vec!["삼부회".to_string(), "파리 군중".to_string()],
                        region_or_front: Some("파리와 베르사유".to_string()),
                        trigger: Some("재정 위기와 대표 요구 충돌".to_string()),
                        development: Some(
                            "삼부회 소집과 국민의회 구성, 바스티유 점령이 이어지며 혁명의 공개 국면이 열렸다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "왕권과 대표제의 충돌이 제도 재편과 대외 위기 국면으로 넘어갔다."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "급진화 국면".to_string(),
                        timeframe: Some("1792-1794".to_string()),
                        actors: vec!["국민공회".to_string(), "자코뱅 정부".to_string()],
                        region_or_front: Some("파리와 대외 전선".to_string()),
                        trigger: Some("전쟁 압력과 왕실 불신".to_string()),
                        development: Some(
                            "왕정 폐지와 공화정 수립 이후 총동원과 공포정치가 이어졌고 대외 전선 압박이 체제 급진화를 밀어 올렸다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "테르미도르 반동과 총재정부 재편으로 다음 정치 질서가 열렸다."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains(
            "broad historical event/process topics still need at least 6 distinct phase cards"
        ));
    }

    #[test]
    fn focused_battle_topics_with_three_rich_phase_cards_clear_event_scaffold_gate() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    crate::models::NarrativeEventCard {
                        label: "사군툼 위기".to_string(),
                        timeframe: Some("219-218 BCE".to_string()),
                        actors: vec!["한니발".to_string(), "로마 원로원".to_string()],
                        region_or_front: Some("이베리아".to_string()),
                        trigger: Some("동맹 도시 분쟁과 조약 해석 충돌".to_string()),
                        development: Some(
                            "사군툼 포위와 로마의 항의가 지역 분쟁을 전면전 직전 국면으로 바꾸었다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "외교 결렬이 알프스 원정과 이탈리아 전선 개시의 직접 계기가 되었다."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "이탈리아 전환 국면".to_string(),
                        timeframe: Some("218-216 BCE".to_string()),
                        actors: vec!["한니발".to_string(), "로마 집정관들".to_string()],
                        region_or_front: Some("알프스와 북부 이탈리아".to_string()),
                        trigger: Some("해상 우세를 피하려는 전략 전환".to_string()),
                        development: Some(
                            "알프스 돌파와 연속 승전으로 전쟁 중심이 이탈리아 본토로 이동했고 로마의 동원 체계가 압박을 받았다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "칸나에 이후에도 로마가 붕괴하지 않으면서 장기 소모전 국면으로 넘어갔다."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "역전과 종결 국면".to_string(),
                        timeframe: Some("212-201 BCE".to_string()),
                        actors: vec!["스키피오".to_string(), "카르타고 원로회".to_string()],
                        region_or_front: Some("이베리아, 시칠리아, 북아프리카".to_string()),
                        trigger: Some("로마의 재정비와 다전선 압박 전략".to_string()),
                        development: Some(
                            "로마는 이베리아와 시칠리아에서 주도권을 되찾고 북아프리카 침공으로 한니발을 본국으로 되돌리며 전쟁 축을 바꾸었다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "자마 전투와 강화 조건이 전쟁을 마무리하고 지중해 세력 균형을 로마 쪽으로 기울였다."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "Battle of Cannae background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "Battle of Cannae background, development, impact, and significance",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert!(failures.is_empty());
    }

    #[test]
    fn broad_historical_event_topics_fail_when_requested_late_scope_anchors_are_missing() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    crate::models::NarrativeEventCard {
                        label: "Old Regime crisis".to_string(),
                        timeframe: Some("1788-1789".to_string()),
                        actors: vec!["Louis XVI".to_string(), "Estates-General".to_string()],
                        region_or_front: Some("Versailles".to_string()),
                        trigger: Some("Fiscal breakdown and political deadlock".to_string()),
                        development: Some(
                            "Royal insolvency and representative conflict forced the crown to summon the Estates-General."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The representative dispute opened an early revolutionary phase in Paris and Versailles."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "National Assembly phase".to_string(),
                        timeframe: Some("June 1789".to_string()),
                        actors: vec!["Third Estate".to_string(), "National Assembly".to_string()],
                        region_or_front: Some("Versailles".to_string()),
                        trigger: Some("Representation dispute and the Tennis Court Oath".to_string()),
                        development: Some(
                            "The Third Estate declared itself the National Assembly and challenged royal authority."
                                .to_string(),
                        ),
                        outcome: Some(
                            "Constitutional confrontation intensified and pushed the crisis toward street intervention."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Popular intervention".to_string(),
                        timeframe: Some("July 1789".to_string()),
                        actors: vec!["Paris crowd".to_string(), "Royal troops".to_string()],
                        region_or_front: Some("Paris".to_string()),
                        trigger: Some("Troop concentration and fear of repression".to_string()),
                        development: Some(
                            "The Bastille crisis and municipal reorganization turned political deadlock into urban mobilization."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The crown retreated and revolutionary legitimacy widened beyond Versailles."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Abolition and rights".to_string(),
                        timeframe: Some("August 1789".to_string()),
                        actors: vec!["National Assembly".to_string()],
                        region_or_front: Some("Paris and Versailles".to_string()),
                        trigger: Some("Peasant unrest and pressure for reform".to_string()),
                        development: Some(
                            "The Assembly abolished feudal privileges and adopted the Declaration of the Rights of Man."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The early revolutionary settlement reframed legitimacy but remained within the first constitutional phase."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, republic, Thermidor, and European order impact",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, republic, Thermidor, and European order impact",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert_eq!(failures.len(), 1);
        assert!(failures[0]
            .contains("requested republican transition is still missing from phase cards"));
        assert!(failures[0].contains(
            "requested thermidor or later reaction phase is still missing from phase cards"
        ));
        assert!(failures[0].contains(
            "requested later settlement or wider-order impact phase is still missing from phase cards"
        ));
    }

    #[test]
    fn broad_historical_event_topics_pass_when_requested_late_scope_anchors_are_covered() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    crate::models::NarrativeEventCard {
                        label: "Old Regime crisis".to_string(),
                        timeframe: Some("1788-1789".to_string()),
                        actors: vec!["Louis XVI".to_string(), "Estates-General".to_string()],
                        region_or_front: Some("Versailles".to_string()),
                        trigger: Some("Fiscal breakdown and political deadlock".to_string()),
                        development: Some(
                            "Royal insolvency and representative conflict forced the crown to summon the Estates-General."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The representative dispute opened an early revolutionary phase in Paris and Versailles."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Popular mobilization and constitutional rupture".to_string(),
                        timeframe: Some("1789-1791".to_string()),
                        actors: vec![
                            "National Assembly".to_string(),
                            "Paris crowds".to_string(),
                            "Louis XVI".to_string(),
                        ],
                        region_or_front: Some("Paris and Versailles".to_string()),
                        trigger: Some(
                            "Urban mobilization and disputes over constitutional sovereignty"
                                .to_string(),
                        ),
                        development: Some(
                            "Popular pressure and assembly reforms turned the fiscal crisis into a constitutional rupture, while the monarchy's wavering response kept distrust alive."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The attempted constitutional settlement remained unstable and prepared the ground for a sharper monarchy crisis."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Republican transition".to_string(),
                        timeframe: Some("1792-1793".to_string()),
                        actors: vec!["National Convention".to_string(), "Paris sections".to_string()],
                        region_or_front: Some("Paris".to_string()),
                        trigger: Some("War pressure and collapse of trust in the monarchy".to_string()),
                        development: Some(
                            "The monarchy was abolished and the republic was declared as war and insurrection transformed the revolution."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The republican turn created the conditions for emergency government and sharper factional conflict."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Emergency government and Terror".to_string(),
                        timeframe: Some("1793-1794".to_string()),
                        actors: vec![
                            "Committee of Public Safety".to_string(),
                            "Jacobins".to_string(),
                            "Vendée rebels".to_string(),
                        ],
                        region_or_front: Some("Paris, Vendée, and coalition fronts".to_string()),
                        trigger: Some(
                            "Foreign war, civil war, scarcity, and factional conflict".to_string(),
                        ),
                        development: Some(
                            "Emergency institutions, mass mobilization, surveillance, and revolutionary tribunals concentrated authority while war and internal revolt intensified."
                                .to_string(),
                        ),
                        outcome: Some(
                            "Military recovery strengthened the republic but made the politics of Terror harder to justify once crisis pressure eased."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "Thermidorian reaction".to_string(),
                        timeframe: Some("1794-1795".to_string()),
                        actors: vec!["Convention deputies".to_string(), "Jacobin leadership".to_string()],
                        region_or_front: Some("Paris".to_string()),
                        trigger: Some("Backlash against emergency rule and concentrated power".to_string()),
                        development: Some(
                            "Thermidor broke the Jacobin phase and reorganized political authority after the fall of Robespierre."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The reaction redirected the revolution toward a new constitutional and diplomatic settlement."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::models::NarrativeEventCard {
                        label: "European order impact".to_string(),
                        timeframe: Some("1790s-1815".to_string()),
                        actors: vec!["European monarchies".to_string(), "French regimes".to_string()],
                        region_or_front: Some("Europe".to_string()),
                        trigger: Some("Revolutionary war and regime transformation".to_string()),
                        development: Some(
                            "Successive French regimes and coalition wars reshaped diplomacy, mobilization, and the wider European order."
                                .to_string(),
                        ),
                        outcome: Some(
                            "The restoration settlement and European balance of power were recast by the revolution's long aftereffects."
                                .to_string(),
                        ),
                        source_ids: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, republic, Thermidor, and European order impact",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, republic, Thermidor, and European order impact",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert!(failures.is_empty());
    }

    #[test]
    fn historical_event_card_diagnostics_require_per_phase_cause_actor_place_and_outcome() {
        let diagnostics = historical_event_card_missing_diagnostics(&[
            crate::models::NarrativeEventCard {
                label: "초기 국면".to_string(),
                timeframe: Some("1789".to_string()),
                actors: vec!["국민의회".to_string()],
                region_or_front: None,
                trigger: Some("재정 위기".to_string()),
                development: Some("봉기가 일어났다.".to_string()),
                outcome: Some("체제가 흔들렸다.".to_string()),
                source_ids: Vec::new(),
                confidence: None,
                open_questions: Vec::new(),
            },
            crate::models::NarrativeEventCard {
                label: "중기 국면".to_string(),
                timeframe: Some("1792".to_string()),
                actors: Vec::new(),
                region_or_front: Some("파리".to_string()),
                trigger: None,
                development: Some("전개가 심화되었다.".to_string()),
                outcome: None,
                source_ids: Vec::new(),
                confidence: None,
                open_questions: Vec::new(),
            },
        ]);

        assert!(diagnostics.contains(&"some phase cards still omit a concrete trigger or cause"));
        assert!(diagnostics.contains(&"some phase cards still omit main actors or institutions"));
        assert!(diagnostics.contains(&"some phase cards still omit front or place context"));
        assert!(diagnostics.contains(&"some phase cards still omit visible development detail"));
        assert!(diagnostics
            .contains(&"some phase cards still omit phase outcome or next-step consequence"));
        assert!(diagnostics
            .contains(&"cause-to-next-phase progression is still missing between phases"));
    }

    #[test]
    fn strict_historical_event_topics_require_event_cards_when_narrative_state_is_missing() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: None,
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("historical event scaffold is too shallow"));
    }

    #[test]
    fn strict_korean_historical_event_topics_require_event_cards_when_missing() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: None,
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "프랑스 혁명의 배경과 전개, 공화정 수립과 테르미도르 반동, 유럽 질서에 미친 영향과 의의",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "프랑스 혁명의 배경과 전개, 공화정 수립과 테르미도르 반동, 유럽 질서에 미친 영향과 의의",
            ),
        };
        let mut failures = Vec::new();

        assert!(should_apply_historical_development_density_gate(&context));
        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("historical event scaffold is too shallow"));
        assert!(failures[0]
            .contains("requested republican transition is still missing from phase cards"));
        assert!(failures[0].contains(
            "requested thermidor or later reaction phase is still missing from phase cards"
        ));
        assert!(failures[0].contains(
            "requested later settlement or wider-order impact phase is still missing from phase cards"
        ));
    }

    #[test]
    fn strict_historical_event_topics_require_event_cards_when_event_cards_are_empty() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState::default()),
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("historical event scaffold is too shallow"));
    }

    #[test]
    fn non_historical_topics_do_not_require_event_cards() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: None,
            ..ResearchControllerArtifacts::default()
        };
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "modern C++ work-stealing scheduler implementation guide with benchmark strategy",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "modern C++ work-stealing scheduler implementation guide with benchmark strategy",
            ),
        };
        let mut failures = Vec::new();

        validate_historical_event_card_development_density(&artifacts, &context, &mut failures);

        assert!(failures.is_empty());
    }

    #[test]
    fn broad_historical_event_scope_does_not_trigger_on_awards_substring() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Academy Awards background, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some("Academy Awards background, impact, and significance"),
        };

        assert!(!broad_historical_event_or_process_topic(&context));
    }

    #[test]
    fn historical_development_density_gate_does_not_trigger_on_awards_substring() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Roman architecture awards background, impact, and significance"),
            research_instructions: None,
            evidence_subject: Some(
                "Roman architecture awards background, impact, and significance",
            ),
        };

        assert!(!should_apply_historical_development_density_gate(&context));
    }

    #[test]
    fn broad_historical_event_scope_does_not_trigger_on_generic_research_process_instructions() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("Enlightenment political thought and long-term significance"),
            research_instructions: Some(
                "Use a careful research process with source audit and step-by-step verification.",
            ),
            evidence_subject: Some("Enlightenment political thought and long-term significance"),
        };

        assert!(!broad_historical_event_or_process_topic(&context));
    }

    #[test]
    fn cause_markers_inside_concrete_phase_sentences_still_satisfy_development_progression() {
        let output = r#"
## Final Answer

국가 재정 위기와 대표제 갈등 때문에 1789년 삼부회 소집과 바스티유 점령이 이어지며 혁명은 공개 국면에 들어섰다. 이후 왕실의 저항과 전쟁 압력 때문에 국민의회와 파리 군중은 충돌했고, 1792년 왕정 폐지와 공화정 수립으로 다음 단계가 시작되었다. 그 뒤 대외 전선의 압박과 지방 반란 때문에 자코뱅 정부는 총동원과 공포정치로 나아갔고, 1794년 테르미도르 반동 이후에는 총재정부가 정권 재편을 시도했다. 이런 전개는 재정 위기와 정치적 충돌이 어떻게 급진화, 전시 체제, 체제 전환의 연쇄로 이어졌는지 보여 준다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "The French Revolution moved through successive phases from 1789 mobilization to later regime restructuring",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn ai_revolution_topic_does_not_trigger_development_density_gate_even_with_historical_mode_text(
    ) {
        let output = r#"
## Final Answer

The AI revolution began because foundation models became cheaper to deploy. Its impact changed product strategy and hiring plans.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.mckinsey.com/capabilities/quantumblack/our-insights/the-state-of-ai",
      "title": "The state of AI",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "AI adoption is changing product and labor decisions",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("AI revolution background, impact, and significance"),
            research_instructions: Some(
                "Use a historical lens with chronology, actors, causes, consequences, and scope limits.",
            ),
            evidence_subject: Some("AI revolution background, impact, and significance"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn history_of_ai_revolution_topic_does_not_trigger_development_density_gate() {
        let output = r#"
## Final Answer

The AI revolution changed software adoption, labor expectations, and product roadmaps over the last few years.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.mckinsey.com/capabilities/quantumblack/our-insights/the-state-of-ai",
      "title": "The state of AI",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "AI adoption is changing product and labor decisions",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "history of the AI revolution background, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "history of the AI revolution background, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn korean_ai_industrial_revolution_topic_does_not_trigger_historical_event_gate() {
        let output = r#"
## 최종 답변 (Final Answer)

AI 산업 혁명이라는 표현은 생성형 AI와 자동화가 산업 구조, 업무 방식, 투자 우선순위에 미치는 영향을 비유적으로 가리킨다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.mckinsey.com/capabilities/quantumblack/our-insights/the-state-of-ai",
      "title": "The state of AI",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "AI adoption is changing product and labor decisions",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("AI 산업 혁명의 배경과 영향과 의의"),
            research_instructions: Some("산업 구조 변화와 기술 채택 관점으로 설명"),
            evidence_subject: Some("AI 산업 혁명의 배경과 영향과 의의"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
        assert!(!err.contains("historical event scaffold is too shallow"));
    }

    #[test]
    fn korean_country_qualified_ai_revolution_topic_does_not_trigger_historical_event_gate() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("미국 AI 혁명의 배경과 영향과 의의"),
            research_instructions: None,
            evidence_subject: Some("미국 AI 혁명의 배경과 영향과 의의"),
        };

        assert!(!should_apply_historical_development_density_gate(&context));
    }

    #[test]
    fn korean_historical_independence_revolution_topic_triggers_historical_event_gate() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("미국 독립혁명의 배경과 전개, 영향과 의의"),
            research_instructions: None,
            evidence_subject: Some("미국 독립혁명의 배경과 전개, 영향과 의의"),
        };

        assert!(should_apply_historical_development_density_gate(&context));
    }

    #[test]
    fn korean_british_industrial_revolution_topic_triggers_historical_event_gate() {
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("영국 산업혁명의 배경과 전개, 유럽 사회에 미친 영향"),
            research_instructions: None,
            evidence_subject: Some("영국 산업혁명의 배경과 전개, 유럽 사회에 미친 영향"),
        };

        assert!(should_apply_historical_development_density_gate(&context));
    }

    #[test]
    fn non_historical_strict_conflict_topic_does_not_trigger_development_density_gate() {
        let output = r#"
## 최종 답변 (Final Answer)

Workplace conflict often begins with role ambiguity and poor communication. The result can be lower morale and weaker organizational trust.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.cipd.org/uk/knowledge/guides/conflict-at-work-factsheet/",
      "title": "Conflict at work",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Workplace conflict can reduce trust and morale",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(
                "workplace conflict background, impact, and organizational meaning",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "workplace conflict background, impact, and organizational meaning",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(!err.contains("historical development density is below required minimum"));
    }

    #[test]
    fn rejects_historical_answer_when_only_unrelated_claim_has_strong_corroboration() {
        let output = r#"
## 최종 답변 (Final Answer)

아우렐리아누스의 의도와 성격은 분명하게 복원할 수 있다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp",
      "title": "Historia Augusta: Aurelian",
      "source_class": "secondary",
      "extracted_facts": ["late antique biography"]
    },
    {
      "id": "S2",
      "url": "https://www.britannica.com/biography/Aurelian",
      "title": "Aurelian | Roman emperor",
      "source_class": "official_or_primary",
      "extracted_facts": ["modern reference overview"]
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Aurelian's motives are fully recoverable",
      "support_source_card_ids": ["S1"]
    },
    {
      "id": "C2",
      "claim": "Aurelian reunified the empire",
      "support_source_card_ids": ["S2"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("historical explanation of the Roman emperor Aurelian"),
            research_instructions: None,
            evidence_subject: Some("historical explanation of the Roman emperor Aurelian"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err
            .contains("historical final answer relies on weak or contested supplementary sources"));
    }

    #[test]
    fn rejects_uncaveated_historical_answer_with_direct_weak_support_url() {
        let output = r#"
## 최종 답변 (Final Answer)

아우렐리아누스의 세부 동기와 발언은 신뢰성 문제 없이 그대로 받아들일 수 있다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp",
      "title": "Historia Augusta: Aurelian",
      "source_class": "secondary",
      "extracted_facts": ["late antique biography"]
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Aurelian's exact motives are certain",
      "support_urls": ["https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("historical explanation of the Roman emperor Aurelian"),
            research_instructions: None,
            evidence_subject: Some("historical explanation of the Roman emperor Aurelian"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err
            .contains("historical final answer relies on weak or contested supplementary sources"));
    }

    #[test]
    fn accepts_historical_answer_with_direct_weak_support_url_when_final_answer_is_visibly_caveated(
    ) {
        let output = r#"
## 최종 답변 (Final Answer)

이 historical explanation은 Roman emperor Aurelian의 동기와 통치 맥락을 다루지만, 후대 전승의 성격상 사료의 한계와 과장 가능성을 감안해 단정하지 않는 편이 안전하다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Aurelian's exact motives are certain",
      "support_urls": ["https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("historical explanation of the Roman emperor Aurelian"),
            research_instructions: None,
            evidence_subject: Some("historical explanation of the Roman emperor Aurelian"),
        };

        assert!(validate_research_output(output, &context).is_ok());
    }

    #[test]
    fn rejects_final_answer_that_copies_repair_search_hint_labels_before_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)

Repair Search Hints (not-yet-adopted evidence; verify before use): query: "남산공원 공식" | provider: "naver" | class: "official_or_primary" | quality: "high" | snippet: "운영 시간 안내"

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 시간은 계절과 구간에 따라 다를 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("남산 아침 러닝과 카페 동선"),
            research_instructions: None,
            evidence_subject: Some("남산 아침 러닝과 카페 동선"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(
            err.contains("repair search hints")
                || err.contains("not-yet-adopted evidence")
                || err.contains("repair hint row")
        );
    }

    #[test]
    fn rejects_final_answer_that_copies_multiline_repair_hint_fields_before_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)

query: "남산공원 공식"
provider: "naver"
snippet: "운영 시간 안내"

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 시간은 계절과 구간에 따라 다를 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("남산 아침 러닝과 카페 동선"),
            research_instructions: None,
            evidence_subject: Some("남산 아침 러닝과 카페 동선"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(err.contains("repair hint row"));
    }

    #[test]
    fn accepts_final_answer_that_uses_cited_hint_content_without_repair_labels() {
        let output = r#"
## 최종 답변 (Final Answer)

남산공원 공식 안내를 기준으로 보면 운영 시간과 동선 정보는 계절과 구간에 따라 달라질 수 있으므로, 실제 방문 전 공원 공지와 교통 안내를 함께 확인하는 편이 안전하다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 시간과 기본 동선 정보는 공식 안내로 확인할 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("남산 아침 러닝과 카페 동선"),
            research_instructions: None,
            evidence_subject: Some("남산 아침 러닝과 카페 동선"),
        };

        assert!(validate_research_output(output, &context).is_ok());
    }

    #[test]
    fn accepts_final_answer_with_standalone_reader_facing_quality_or_query_words() {
        let output = r#"
## 최종 답변 (Final Answer)

이 비교의 query 범위는 운영 시간과 동선 확인에 맞추고, provider 선택보다는 실제 현장 공지의 품질(quality)과 분류(class) 기준을 독자가 이해하기 쉽게 풀어 설명하는 편이 낫다. 마지막 snippet은 현장 변동 가능성을 짧게 덧붙이는 수준이면 충분하다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 정보는 공식 안내로 확인할 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("남산 아침 러닝과 카페 동선"),
            research_instructions: None,
            evidence_subject: Some("남산 아침 러닝과 카페 동선"),
        };

        assert!(validate_research_output(output, &context).is_ok());
    }

    #[test]
    fn accepts_final_answer_with_adjacent_quality_and_class_list_without_repair_specific_fields() {
        let output = r#"
## 최종 답변 (Final Answer)

남산공원 공식 안내를 기준으로 보면 운영 시간과 동선 정보는 계절과 구간에 따라 달라질 수 있으므로, 실제 방문 전 공원 공지와 교통 안내를 함께 확인하는 편이 안전하다.

quality: 현장 공지는 계절 변수에 따라 달라질 수 있다.
class: 공식 안내와 후기성 정보는 신뢰도 역할이 다르다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 정보는 공식 안내로 확인할 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("남산 아침 러닝과 카페 동선"),
            research_instructions: None,
            evidence_subject: Some("남산 아침 러닝과 카페 동선"),
        };

        assert!(validate_research_output(output, &context).is_ok());
    }

    #[test]
    fn rejects_final_answer_that_leaks_internal_narrative_placeholder_labels() {
        let output = r#"
## 최종 답변 (Final Answer)

topic_frame: 구현 판단의 큰 틀
section_outline: 핵심 모델 -> 예외 처리 -> 운영 한계
evidence_layers: 공식 문서 / 보조 문서

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://learn.microsoft.com/en-us/windows/win32/api/winsock2/",
      "title": "Winsock",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "포트 동작은 OS 문서로 확인해야 한다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some("technology implementation guide"),
            research_instructions: None,
            evidence_subject: Some("technology implementation guide"),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(err.contains("topic_frame") || err.contains("section_outline"));
    }

    #[test]
    fn rejects_final_answer_that_leaks_event_card_scaffold_markers() {
        let output = r#"
## 최종 답변 (Final Answer)

<event_cards>
  <card><label>초기 국면</label><region_or_front>북부 전선</region_or_front><source_ids>S1</source_ids><open_questions>보급 경로</open_questions></card>
</event_cards>

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://www.britannica.com/event/French-Revolution",
      "title": "French Revolution",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "혁명 전개는 단계별로 봐야 한다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(
            err.contains("event_cards")
                || err.contains("region_or_front")
                || err.contains("source_ids")
                || err.contains("open_questions")
        );
    }

    #[test]
    fn allows_event_card_artifact_json_inside_verification_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건의 전개를 단계별로 설명하고, 사실과 남은 불확실성을 본문과 부록에서 분리한다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": [],
  "narrative_state": {
    "version": 1,
    "event_cards": [
      {
        "label": "초기 국면",
        "region_or_front": "북부 전선",
        "source_ids": ["S1"],
        "open_questions": ["보급 경로"]
      }
    ]
  }
}
```
"#;
        let mut failures = Vec::new();

        validate_reader_facing_internal_metadata_leaks(output, &mut failures);

        assert!(failures.is_empty());
    }

    #[test]
    fn rejects_event_card_scaffold_markers_in_non_final_answer_visible_section_before_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

## 작업 메모

event_cards
<event_cards>
  <card><label>초기 국면</label><region_or_front>북부 전선</region_or_front><source_ids>S1</source_ids></card>
</event_cards>

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(
            err.contains("event_cards")
                || err.contains("region_or_front")
                || err.contains("source_ids")
        );
    }

    #[test]
    fn rejects_event_card_repair_guidance_header_before_appendix() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

## 작업 메모

Historical event scaffold repair guidance:
- 각 국면의 전개를 다시 채운다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(
            err.contains("reader-facing final answer leaks internal narrative or repair marker")
        );
        assert!(
            err.contains("historical event scaffold repair guidance")
                || err.contains("event scaffold repair guidance")
        );
    }

    #[test]
    fn rejects_malformed_artifact_marker_before_event_scaffold_repair_header() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

[RESEARCH_ARTIFACT_JSON]
Historical event scaffold repair guidance:
- 각 국면의 전개를 다시 채운다.

# Verification Appendix
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("malformed machine-readable research artifact JSON block"));
    }

    #[test]
    fn rejects_malformed_artifact_marker_before_event_card_scaffold_text() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

[RESEARCH_ARTIFACT_JSON]
<event_cards>
  <card><label>초기 국면</label><region_or_front>북부 전선</region_or_front></card>
</event_cards>

# Verification Appendix
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("malformed machine-readable research artifact JSON block"));
    }

    #[test]
    fn rejects_invalid_artifact_json_before_event_scaffold_repair_header() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "warnings": ["Historical event scaffold repair guidance"],
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": [],
```

# Verification Appendix
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("invalid research artifact JSON"));
    }

    #[test]
    fn rejects_multiple_artifact_blocks_before_event_card_scaffold_text() {
        let output = r#"
## 최종 답변 (Final Answer)

이 보고서는 사건 전개를 단계별로 설명한다.

[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "warnings": ["<event_cards><card><source_ids>S1</source_ids></card></event_cards>"],
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```

[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```

# Verification Appendix
"#;
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: false,
            research_intensity: Some("medium"),
            quality_depth: Some("medium"),
            research_topic: Some(
                "French Revolution background, development, impact, and significance",
            ),
            research_instructions: None,
            evidence_subject: Some(
                "French Revolution background, development, impact, and significance",
            ),
        };

        let err = validate_research_output(output, &context).unwrap_err();

        assert!(err.contains("multiple machine-readable research artifact JSON blocks"));
    }

    #[test]
    fn technology_topics_do_not_apply_narrative_structure_checks() {
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some("technology implementation guide for ephemeral port behavior"),
            research_instructions: None,
            evidence_subject: Some("technology implementation guide for ephemeral port behavior"),
        };
        let state = NarrativeState {
            version: 1,
            timeline: vec![crate::models::NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "임시 타임라인".to_string(),
                date_anchor: None,
                significance: None,
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            actors: vec![crate::models::NarrativeActor {
                id: "NA1".to_string(),
                label: "임시 행위자".to_string(),
                role: None,
                relevance: None,
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            causal_chain: vec![crate::models::NarrativeCausalLink {
                id: "NC1".to_string(),
                cause: "임시 원인".to_string(),
                effect: "임시 결과".to_string(),
                rationale: None,
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "transition".to_string(),
                description: "임시 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn technology_concept_topics_do_not_apply_narrative_structure_checks() {
        let subject = "AI 개념과 머신러닝, 딥러닝, 생성형 AI, LLM, RAG, agent의 차이와 오해, 한계";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "concept".to_string(),
                description: "개념 비교 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(technology_like_topic(subject));
        assert!(!should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn ai_policy_overview_does_not_bypass_narrative_structure_checks_as_technology_concept() {
        let subject = "AI regulation overview and governance policy explanation";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "policy".to_string(),
                description: "regulatory timeline gap".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_llm_method_topic_still_triggers_technology_concept_detection() {
        let subject = "LLM 작동 방법과 원리";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "concept".to_string(),
                description: "작동 원리 설명 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(technology_like_topic(subject));
        assert!(!should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn non_technology_port_substrings_do_not_trigger_technology_topic_detection() {
        let subject =
            "Explain why important support and transportation choices matter for customer trust.";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "transition".to_string(),
                description: "임시 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn historical_port_arthur_topic_does_not_trigger_technology_topic_detection() {
        let subject =
            "Port Arthur siege history: background, development, impact, and significance";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            timeline: vec![crate::models::NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "포트 아서 공방".to_string(),
                date_anchor: None,
                significance: None,
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn local_port_city_topic_does_not_trigger_technology_topic_detection() {
        let subject = "Best port city breakfast route and cafe stops for a short local trip";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "comparison".to_string(),
                description: "후보 비교 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_port_arthur_history_does_not_trigger_technology_topic_detection() {
        let subject = "포트 아서 공방의 배경과 전개";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            timeline: vec![crate::models::NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "공방 전개".to_string(),
                date_anchor: None,
                significance: None,
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_local_cloud_route_topic_does_not_trigger_technology_topic_detection() {
        let subject = "클라우드 전망이 보이는 포트 시티 산책 동선과 카페 추천";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "comparison".to_string(),
                description: "후보 비교 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_technical_port_protocol_topic_still_triggers_technology_detection() {
        let subject = "동적 포트 할당과 네트워크 프로토콜 동작을 Linux와 Windows 기준으로 설명";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "transition".to_string(),
                description: "임시 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(technology_like_topic(subject));
        assert!(!should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn policy_implementation_history_topic_does_not_trigger_technology_topic_detection() {
        let subject = "policy implementation history and institutional impact";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "transition".to_string(),
                description: "제도 변화 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn local_travel_route_design_topic_does_not_trigger_technology_topic_detection() {
        let subject = "travel route design for a quiet morning port city walk";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "comparison".to_string(),
                description: "동선 비교 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_policy_implementation_topic_does_not_trigger_technology_topic_detection() {
        let subject = "정책 구현의 배경과 영향";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "transition".to_string(),
                description: "정책 전개 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn korean_local_itinerary_design_topic_does_not_trigger_technology_topic_detection() {
        let subject = "여행 일정 설계와 포트 시티 동선 추천";
        let context = ResearchQualityContext {
            file_prefix: "[AI-Research]",
            file_type: "md",
            web_search_requested: true,
            research_intensity: Some("high"),
            quality_depth: Some("strict"),
            research_topic: Some(subject),
            research_instructions: None,
            evidence_subject: Some(subject),
        };
        let state = NarrativeState {
            open_gaps: vec![crate::models::NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: "comparison".to_string(),
                description: "일정 비교 공백".to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: vec!["C1".to_string()],
                expected_source_card_ids: vec!["S1".to_string()],
            }],
            ..NarrativeState::default()
        };

        assert!(!technology_like_topic(subject));
        assert!(should_apply_narrative_structure_checks(&context, &state));
    }

    #[test]
    fn rejects_transient_repair_hint_url_as_adopted_evidence_provenance() {
        let output = r#"
## 최종 답변 (Final Answer)

남산공원 공식 안내를 참고하면 운영 시간과 접근 동선은 현장 공지 기준으로 다시 확인하는 편이 안전하다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 정보는 공식 안내로 확인할 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let prohibited = HashSet::from([String::from(
            "https://parks.seoul.go.kr/template/sub/namsan.do",
        )]);

        let err = validate_transient_repair_hint_evidence_provenance(output, "md", &prohibited)
            .unwrap_err();

        assert!(err.contains("repair search hint URLs cannot be cited as adopted evidence"));
        assert!(err.contains("https://parks.seoul.go.kr/template/sub/namsan.do"));
    }
}
