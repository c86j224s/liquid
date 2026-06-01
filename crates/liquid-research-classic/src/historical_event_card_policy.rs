use liquid_protocol::{
    NarrativeCausalSpineStep, NarrativeEventCard, NarrativeInterpretiveLayer,
    ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchDebtItem, ResearchSourceCard,
};
use liquid_research_core::normalize_absolute_public_evidence_url;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const DEVELOPMENT_THIN_CHAR_THRESHOLD: usize = 80;
const PHASE_FIELD_THIN_CHAR_THRESHOLD: usize = 8;
const MAX_PROMPT_CLAIMS: usize = 24;
const MAX_PROMPT_SOURCES: usize = 16;
const MAX_TEXT_CHARS: usize = 700;
const MAX_SHORT_TEXT_CHARS: usize = 180;
const MAX_ARRAY_ITEMS: usize = 8;
const MAX_NESTED_ITEMS: usize = 6;
const MAX_RESEARCH_DEBT_ITEMS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalEventCardSelection {
    pub index: usize,
    pub label: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoricalEventCardMergeReport {
    pub accepted_fields: Vec<String>,
    pub rejected_fields: Vec<String>,
    pub debts: Vec<ResearchDebtItem>,
    pub warnings: Vec<String>,
}

impl HistoricalEventCardMergeReport {
    pub fn accepted(&self) -> bool {
        !self.accepted_fields.is_empty()
    }
}

#[derive(Debug, Clone)]
struct EvidenceIndex {
    claim_ids: HashSet<String>,
    supported_claim_ids: HashSet<String>,
    source_ids: HashSet<String>,
    claim_support_sources: HashMap<String, Vec<String>>,
    claim_texts: HashMap<String, String>,
}

pub fn historical_event_card_enrichment_applies(
    file_prefix: &str,
    research_intensity: Option<&str>,
    quality_depth: Option<&str>,
    research_topic: Option<&str>,
    research_instructions: Option<&str>,
    evidence_subject: Option<&str>,
    artifacts: &ResearchControllerArtifacts,
) -> bool {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return false;
    }
    if research_intensity != Some("high") || quality_depth != Some("strict") {
        return false;
    }
    if artifacts.source_cards.is_empty() || artifacts.claim_log.is_empty() {
        return false;
    }
    let subject = [
        research_topic.unwrap_or_default(),
        research_instructions.unwrap_or_default(),
        evidence_subject.unwrap_or_default(),
    ]
    .join(" ");
    historical_subject_like(&subject)
}

pub fn select_weak_historical_event_cards(
    artifacts: &ResearchControllerArtifacts,
    max_cards: usize,
) -> Vec<HistoricalEventCardSelection> {
    if max_cards == 0 {
        return Vec::new();
    }
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return Vec::new();
    };
    let evidence = EvidenceIndex::new(artifacts);
    state
        .event_cards
        .iter()
        .enumerate()
        .filter_map(|(index, card)| {
            let reasons = historical_event_card_weakness_reasons(card, &evidence);
            if reasons.is_empty() {
                None
            } else {
                Some(HistoricalEventCardSelection {
                    index,
                    label: card.label.clone(),
                    reasons,
                })
            }
        })
        .take(max_cards)
        .collect()
}

pub fn select_evidence_ready_weak_historical_event_cards(
    artifacts: &ResearchControllerArtifacts,
    max_cards: usize,
) -> Vec<HistoricalEventCardSelection> {
    if max_cards == 0 {
        return Vec::new();
    }
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return Vec::new();
    };
    let evidence = EvidenceIndex::new(artifacts);
    state
        .event_cards
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            !crate::historical_phase_label_is_placeholder(&card.label)
                && ids_have_supported_claim(&card.claim_log_ids, &evidence)
                && !broad_generic_phase_support(card, &evidence)
        })
        .filter_map(|(index, card)| {
            let reasons = historical_event_card_weakness_reasons(card, &evidence)
                .into_iter()
                .filter(|reason| {
                    !reason.contains("missing claim_log_ids")
                        && !reason.contains("supported Claim Log")
                        && !reason.contains("broad whole-topic")
                        && !reason.contains("missing source_ids")
                })
                .collect::<Vec<_>>();
            if reasons.is_empty() {
                None
            } else {
                Some(HistoricalEventCardSelection {
                    index,
                    label: card.label.clone(),
                    reasons,
                })
            }
        })
        .take(max_cards)
        .collect()
}

pub fn historical_enrichment_readiness_debt(
    reason: &str,
    missing_evidence: &str,
) -> ResearchDebtItem {
    ResearchDebtItem {
        id: format!("narrative-enrichment-readiness-{reason}"),
        severity: "high".to_string(),
        failed_gate: Some("narrative_enrichment_readiness".to_string()),
        missing_evidence: missing_evidence.to_string(),
        required_source_class: None,
        candidate_queries: Vec::new(),
        next_check_actions: vec![
            "Repair Source Cards and phase-specific supported Claim Log rows before event-card enrichment."
                .to_string(),
        ],
        status: "open".to_string(),
    }
}

pub fn build_historical_event_card_enrichment_prompt(
    artifacts: &ResearchControllerArtifacts,
    selection: &HistoricalEventCardSelection,
    subject: &str,
) -> String {
    let card_json = artifacts
        .narrative_state
        .as_ref()
        .and_then(|state| state.event_cards.get(selection.index))
        .and_then(|card| serde_json::to_string_pretty(card).ok())
        .unwrap_or_else(|| "{}".to_string());
    let evidence = EvidenceIndex::new(artifacts);
    let trusted_claims = prompt_claim_rows(&artifacts.claim_log, &evidence);
    let trusted_sources = prompt_source_rows(&artifacts.source_cards, &evidence);
    let claims = compact_claim_rows(&trusted_claims);
    let sources = compact_source_rows(&trusted_sources);
    let reasons = selection.reasons.join("; ");
    format!(
        "You are enriching one weak historical event card inside a larger research artifact.\n\
Return JSON only. Do not write a report. Do not add new sources. Do not invent Claim Log IDs. Do not invent Source Card IDs. Use only the Claim Log rows and Source Cards listed below.\n\
If a field cannot be grounded in the provided Claim Log rows, leave it out and add specific research_debt for the missing phase, actor, place/front, transition, source layer, or interpretive gap.\n\
For every causal_spine item include step_type, description, epistemic_status, reasoning, limits, claim_log_ids, source_ids.\n\
For every interpretive_layers item include layer_type, interpretation, epistemic_status, reasoning, limits, claim_log_ids, source_ids.\n\
Each causal_spine or interpretive_layers item must cite at least one existing Claim Log ID. Source IDs are supplemental.\n\n\
Subject: {subject}\n\
Selected card index: {index}\n\
Selected card label: {label}\n\
Weakness reasons: {reasons}\n\n\
Current event card JSON:\n{card_json}\n\n\
Allowed Claim Log rows:\n{claims}\n\n\
Allowed Source Cards:\n{sources}\n\n\
Expected JSON shape:\n{{\n  \"label\": \"...\",\n  \"timeframe\": \"...\",\n  \"actors\": [\"...\"],\n  \"region_or_front\": \"...\",\n  \"trigger\": \"...\",\n  \"development\": \"...\",\n  \"outcome\": \"...\",\n  \"claim_log_ids\": [\"C1\"],\n  \"source_ids\": [\"S1\"],\n  \"causal_spine\": [{{\"step_type\":\"forcing_factor\",\"description\":\"...\",\"epistemic_status\":\"interpretation\",\"reasoning\":\"...\",\"limits\":[\"...\"],\"claim_log_ids\":[\"C1\"],\"source_ids\":[\"S1\"]}}],\n  \"interpretive_layers\": [{{\"layer_type\":\"diplomacy\",\"interpretation\":\"...\",\"epistemic_status\":\"interpretation\",\"reasoning\":\"...\",\"limits\":[\"...\"],\"claim_log_ids\":[\"C1\"],\"source_ids\":[\"S1\"]}}],\n  \"open_questions\": [\"...\"],\n  \"research_debt\": [{{\"missing_evidence\":\"...\",\"candidate_queries\":[\"...\"],\"next_check_actions\":[\"...\"]}}]\n}}",
        subject = sanitize_text(subject, MAX_SHORT_TEXT_CHARS),
        index = selection.index,
        label = sanitize_text(&selection.label, MAX_SHORT_TEXT_CHARS),
        reasons = sanitize_text(&reasons, MAX_TEXT_CHARS),
        card_json = sanitize_text(&card_json, 2400),
        claims = claims,
        sources = sources,
    )
}

pub fn merge_historical_event_card_enrichment_json(
    artifacts: &mut ResearchControllerArtifacts,
    card_index: usize,
    raw_json: &str,
) -> HistoricalEventCardMergeReport {
    let mut report = HistoricalEventCardMergeReport::default();
    let Some(state) = artifacts.narrative_state.as_mut() else {
        report.debts.push(enrichment_debt(
            card_index,
            "missing-narrative-state",
            "event-card enrichment could not run because narrative_state is missing",
            Vec::new(),
            vec!["Re-run strict research with narrative_state.event_cards present.".to_string()],
        ));
        return report;
    };
    if card_index >= state.event_cards.len() {
        report.debts.push(enrichment_debt(
            card_index,
            "missing-card",
            "event-card enrichment could not match the selected card index",
            Vec::new(),
            vec![
                "Re-run enrichment with a stable event-card index from current artifacts."
                    .to_string(),
            ],
        ));
        return report;
    }

    let parse_result = parse_enrichment_object(raw_json);
    let Ok(value) = parse_result else {
        let message = parse_result
            .err()
            .unwrap_or_else(|| "invalid JSON".to_string());
        report.rejected_fields.push("json".to_string());
        report.debts.push(enrichment_debt(
            card_index,
            "invalid-json",
            &format!("event-card enrichment returned unparseable JSON: {message}"),
            Vec::new(),
            vec!["Return a single JSON object for event-card enrichment.".to_string()],
        ));
        return report;
    };

    let evidence = EvidenceIndex::from_parts(&artifacts.claim_log, &artifacts.source_cards);
    let card = &mut state.event_cards[card_index];
    let valid_claim_ids = valid_claim_ids_from_value(
        &value,
        "claim_log_ids",
        &evidence,
        &mut report,
        card_index,
        "event-card",
    );
    let valid_source_ids = valid_source_ids_from_value(
        &value,
        "source_ids",
        &evidence,
        &mut report,
        card_index,
        "event-card",
    );
    let field_grounded = !valid_claim_ids.is_empty();

    if field_grounded {
        merge_id_list(&mut card.claim_log_ids, valid_claim_ids.clone());
        report.accepted_fields.push("claim_log_ids".to_string());
        let mut supplemental_sources = valid_source_ids.clone();
        for claim_id in &valid_claim_ids {
            if let Some(sources) = evidence.claim_support_sources.get(claim_id) {
                supplemental_sources.extend(sources.iter().cloned());
            }
        }
        merge_id_list(
            &mut card.source_ids,
            filter_known_ids(supplemental_sources, &evidence.source_ids),
        );
        if !valid_source_ids.is_empty() {
            report.accepted_fields.push("source_ids".to_string());
        }
        accept_string_field(card, &value, "label", &mut report, card_index, false);
        accept_string_field(card, &value, "timeframe", &mut report, card_index, false);
        accept_string_array_field(card, &value, "actors", &mut report, card_index);
        accept_string_field(
            card,
            &value,
            "region_or_front",
            &mut report,
            card_index,
            false,
        );
        accept_string_field(card, &value, "trigger", &mut report, card_index, true);
        accept_string_field(card, &value, "development", &mut report, card_index, true);
        accept_string_field(card, &value, "outcome", &mut report, card_index, true);
    } else if value.get("claim_log_ids").is_some()
        || [
            "label",
            "timeframe",
            "actors",
            "region_or_front",
            "trigger",
            "development",
            "outcome",
        ]
        .iter()
        .any(|field| value.get(field).is_some())
    {
        report.rejected_fields.push("event-card-fields".to_string());
        report.debts.push(enrichment_debt(
            card_index,
            "missing-valid-claim-refs",
            "event-card enrichment supplied phase fields without any existing supported Claim Log ID",
            Vec::new(),
            vec!["Add or repair a phase-specific Claim Log row before enriching this event card.".to_string()],
        ));
    }

    merge_causal_spine(card, &value, &evidence, &mut report, card_index);
    merge_interpretive_layers(card, &value, &evidence, &mut report, card_index);
    merge_open_questions(card, &value, &mut report, card_index);
    merge_model_research_debt(&value, &mut report, card_index, card.label.as_str());

    report
}

pub fn historical_event_card_weakness_reasons_for_card(
    artifacts: &ResearchControllerArtifacts,
    card: &NarrativeEventCard,
) -> Vec<String> {
    let evidence = EvidenceIndex::new(artifacts);
    historical_event_card_weakness_reasons(card, &evidence)
}

fn historical_subject_like(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    let markers = [
        "history",
        "historical",
        "war",
        "battle",
        "revolution",
        "empire",
        "dynasty",
        "colonial",
        "siege",
        "treaty",
        "ancient",
        "medieval",
        "modern era",
        "한니발",
        "포에니",
        "전쟁",
        "전투",
        "혁명",
        "역사",
        "제국",
        "왕조",
        "식민",
        "조약",
        "외교",
        "근대",
        "고대",
        "시대",
    ];
    markers
        .iter()
        .any(|marker| lower.contains(marker) || subject.contains(marker))
}

fn historical_event_card_weakness_reasons(
    card: &NarrativeEventCard,
    evidence: &EvidenceIndex,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if thin_optional_text(card.timeframe.as_deref(), PHASE_FIELD_THIN_CHAR_THRESHOLD) {
        reasons.push("missing or thin timeframe".to_string());
    }
    if card.actors.is_empty()
        || card
            .actors
            .iter()
            .all(|actor| sanitize_text(actor, MAX_SHORT_TEXT_CHARS).chars().count() < 2)
    {
        reasons.push("missing actors".to_string());
    }
    if thin_optional_text(
        card.region_or_front.as_deref(),
        PHASE_FIELD_THIN_CHAR_THRESHOLD,
    ) {
        reasons.push("missing or thin region/front".to_string());
    }
    if thin_optional_text(card.trigger.as_deref(), PHASE_FIELD_THIN_CHAR_THRESHOLD) {
        reasons.push("missing or thin trigger".to_string());
    }
    if thin_optional_text(card.development.as_deref(), DEVELOPMENT_THIN_CHAR_THRESHOLD) {
        reasons.push("missing or thin development".to_string());
    }
    if thin_optional_text(card.outcome.as_deref(), PHASE_FIELD_THIN_CHAR_THRESHOLD) {
        reasons.push("missing or thin outcome".to_string());
    }
    if card.claim_log_ids.is_empty() {
        reasons.push("missing claim_log_ids".to_string());
    } else if !card
        .claim_log_ids
        .iter()
        .any(|claim_id| evidence.supported_claim_ids.contains(claim_id))
    {
        reasons.push("claim_log_ids do not resolve to supported Claim Log rows".to_string());
    }
    if !card.source_ids.is_empty()
        && !card
            .source_ids
            .iter()
            .any(|source_id| evidence.source_ids.contains(source_id))
    {
        reasons.push("source_ids do not resolve to existing Source Cards".to_string());
    }
    if card.source_ids.is_empty() {
        reasons.push("missing source_ids".to_string());
    }
    if card.causal_spine.is_empty() {
        reasons.push("empty causal_spine".to_string());
    } else if card
        .causal_spine
        .iter()
        .any(|step| !ids_have_supported_claim(&step.claim_log_ids, evidence))
    {
        reasons.push("causal_spine items lack supported claim_log_ids".to_string());
    }
    if card.interpretive_layers.is_empty() {
        reasons.push("empty interpretive_layers".to_string());
    } else if card
        .interpretive_layers
        .iter()
        .any(|layer| !ids_have_supported_claim(&layer.claim_log_ids, evidence))
    {
        reasons.push("interpretive_layers items lack supported claim_log_ids".to_string());
    }
    if broad_generic_phase_support(card, evidence) {
        reasons.push("phase card appears to rely on broad whole-topic claims instead of phase-specific support".to_string());
    }
    reasons
}

fn ids_have_supported_claim(ids: &[String], evidence: &EvidenceIndex) -> bool {
    ids.iter()
        .any(|id| evidence.supported_claim_ids.contains(id))
}

fn broad_generic_phase_support(card: &NarrativeEventCard, evidence: &EvidenceIndex) -> bool {
    let card_anchor_text = [
        card.label.as_str(),
        card.timeframe.as_deref().unwrap_or_default(),
        card.region_or_front.as_deref().unwrap_or_default(),
        card.trigger.as_deref().unwrap_or_default(),
        card.development.as_deref().unwrap_or_default(),
        card.outcome.as_deref().unwrap_or_default(),
    ]
    .join(" ");
    let card_tokens = anchor_tokens(&card_anchor_text);
    if card_tokens.len() < 3 || card.claim_log_ids.is_empty() {
        return false;
    }
    card.claim_log_ids.iter().all(|claim_id| {
        evidence
            .claim_ids
            .get(claim_id)
            .and_then(|_| evidence.claim_support_sources.get(claim_id))
            .is_some_and(|_| {
                let claim_text = evidence_claim_text(evidence, claim_id);
                let claim_tokens = anchor_tokens(&claim_text);
                !card_tokens.iter().any(|token| claim_tokens.contains(token))
            })
    })
}

fn evidence_claim_text(evidence: &EvidenceIndex, claim_id: &str) -> String {
    evidence
        .claim_texts
        .get(claim_id)
        .cloned()
        .unwrap_or_default()
}

fn parse_enrichment_object(raw_json: &str) -> Result<Value, String> {
    let stripped = strip_json_fence(raw_json).trim().to_string();
    let value: Value = serde_json::from_str(&stripped).map_err(|err| err.to_string())?;
    if value.is_object() {
        Ok(value)
    } else {
        Err("enrichment output must be a JSON object".to_string())
    }
}

fn strip_json_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or_default();
    if !first.trim_start().starts_with("```") {
        return trimmed.to_string();
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    if let Some(end) = body.rfind("```") {
        body[..end].trim().to_string()
    } else {
        body.trim().to_string()
    }
}

fn valid_claim_ids_from_value(
    value: &Value,
    field: &str,
    evidence: &EvidenceIndex,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    context: &str,
) -> Vec<String> {
    let ids = string_array(value.get(field), MAX_ARRAY_ITEMS);
    let valid = filter_known_ids(ids.clone(), &evidence.supported_claim_ids);
    let invalid = ids
        .into_iter()
        .filter(|id| !evidence.supported_claim_ids.contains(id))
        .collect::<Vec<_>>();
    if !invalid.is_empty() {
        report.rejected_fields.push(format!("{context}.{field}"));
        report.debts.push(enrichment_debt(
            card_index,
            &format!("invalid-claim-refs-{context}"),
            &format!(
                "event-card enrichment referenced unknown or unsupported Claim Log IDs in {context}: {}",
                invalid.join(", ")
            ),
            Vec::new(),
            vec!["Use only existing supported Claim Log IDs for event-card enrichment.".to_string()],
        ));
    }
    valid
}

fn valid_source_ids_from_value(
    value: &Value,
    field: &str,
    evidence: &EvidenceIndex,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    context: &str,
) -> Vec<String> {
    let ids = string_array(value.get(field), MAX_ARRAY_ITEMS);
    let valid = filter_known_ids(ids.clone(), &evidence.source_ids);
    let invalid = ids
        .into_iter()
        .filter(|id| !evidence.source_ids.contains(id))
        .collect::<Vec<_>>();
    if !invalid.is_empty() {
        report.rejected_fields.push(format!("{context}.{field}"));
        report.debts.push(enrichment_debt(
            card_index,
            &format!("invalid-source-refs-{context}"),
            &format!(
                "event-card enrichment referenced unknown Source Card IDs in {context}: {}",
                invalid.join(", ")
            ),
            Vec::new(),
            vec!["Use only existing Source Card IDs for event-card enrichment.".to_string()],
        ));
    }
    valid
}

fn accept_string_field(
    card: &mut NarrativeEventCard,
    value: &Value,
    field: &str,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    prefer_richer: bool,
) {
    let Some(incoming) = value.get(field).and_then(Value::as_str) else {
        return;
    };
    let incoming = sanitize_text(incoming, MAX_TEXT_CHARS);
    if !safe_enrichment_text(&incoming) {
        reject_unsafe_enrichment_text(report, card_index, field);
        return;
    }
    if !useful_text(&incoming, PHASE_FIELD_THIN_CHAR_THRESHOLD) {
        report.rejected_fields.push(field.to_string());
        return;
    }
    let target = match field {
        "label" => Some(&mut card.label),
        "timeframe" => option_string_slot(&mut card.timeframe),
        "region_or_front" => option_string_slot(&mut card.region_or_front),
        "trigger" => option_string_slot(&mut card.trigger),
        "development" => option_string_slot(&mut card.development),
        "outcome" => option_string_slot(&mut card.outcome),
        _ => None,
    };
    let Some(target) = target else {
        return;
    };
    if should_replace_text(target, &incoming, prefer_richer) {
        *target = incoming;
        report.accepted_fields.push(field.to_string());
    }
}

fn option_string_slot(value: &mut Option<String>) -> Option<&mut String> {
    if value.is_none() {
        *value = Some(String::new());
    }
    value.as_mut()
}

fn accept_string_array_field(
    card: &mut NarrativeEventCard,
    value: &Value,
    field: &str,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
) {
    if field != "actors" {
        return;
    }
    let actors = filter_safe_enrichment_strings(
        string_array(value.get(field), MAX_ARRAY_ITEMS),
        report,
        card_index,
        field,
    )
    .into_iter()
    .filter(|actor| useful_text(actor, 2))
    .collect::<Vec<_>>();
    if actors.is_empty() {
        return;
    }
    let before = card.actors.len();
    merge_id_list(&mut card.actors, actors);
    if card.actors.len() > before {
        report.accepted_fields.push(field.to_string());
    }
}

fn merge_causal_spine(
    card: &mut NarrativeEventCard,
    value: &Value,
    evidence: &EvidenceIndex,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
) {
    let Some(items) = value.get("causal_spine").and_then(Value::as_array) else {
        return;
    };
    let mut accepted = Vec::new();
    for (idx, item) in items.iter().take(MAX_NESTED_ITEMS).enumerate() {
        let context = format!("causal_spine[{idx}]");
        let claim_log_ids = valid_claim_ids_from_value(
            item,
            "claim_log_ids",
            evidence,
            report,
            card_index,
            &context,
        );
        if claim_log_ids.is_empty() {
            report.rejected_fields.push(context.clone());
            report.debts.push(enrichment_debt(
                card_index,
                &format!("missing-claim-ref-causal-{idx}"),
                &format!("event-card enrichment causal_spine item {idx} lacked any existing supported Claim Log ID"),
                Vec::new(),
                vec!["Ground each causal_spine item in at least one supported phase-specific Claim Log row.".to_string()],
            ));
            continue;
        }
        let description = string_field(item, "description", MAX_TEXT_CHARS);
        let reasoning = string_field(item, "reasoning", MAX_TEXT_CHARS);
        if !useful_text(&description, 20) || !useful_text(&reasoning, 20) {
            report.rejected_fields.push(context);
            continue;
        }
        let step_type = string_field(item, "step_type", MAX_SHORT_TEXT_CHARS);
        let epistemic_status =
            optional_string_field(item, "epistemic_status", MAX_SHORT_TEXT_CHARS);
        if !safe_enrichment_text(&description)
            || !safe_enrichment_text(&reasoning)
            || !safe_enrichment_text(&step_type)
            || epistemic_status
                .as_deref()
                .is_some_and(|status| !safe_enrichment_text(status))
        {
            report.rejected_fields.push(context.clone());
            reject_unsafe_enrichment_text(report, card_index, &context);
            continue;
        }
        let limits = filter_safe_enrichment_strings(
            string_array(item.get("limits"), MAX_ARRAY_ITEMS),
            report,
            card_index,
            &format!("{context}.limits"),
        );
        let mut source_ids = valid_source_ids_from_value(
            item,
            "source_ids",
            evidence,
            report,
            card_index,
            "causal_spine",
        );
        for claim_id in &claim_log_ids {
            if let Some(sources) = evidence.claim_support_sources.get(claim_id) {
                source_ids.extend(sources.iter().cloned());
            }
        }
        accepted.push(NarrativeCausalSpineStep {
            step_type,
            description,
            epistemic_status,
            reasoning: Some(reasoning),
            limits,
            claim_log_ids,
            source_ids: filter_known_ids(source_ids, &evidence.source_ids),
        });
    }
    if !accepted.is_empty() && accepted.len() >= card.causal_spine.len() {
        card.causal_spine = accepted;
        report.accepted_fields.push("causal_spine".to_string());
    }
}

fn merge_interpretive_layers(
    card: &mut NarrativeEventCard,
    value: &Value,
    evidence: &EvidenceIndex,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
) {
    let Some(items) = value.get("interpretive_layers").and_then(Value::as_array) else {
        return;
    };
    let mut accepted = Vec::new();
    for (idx, item) in items.iter().take(MAX_NESTED_ITEMS).enumerate() {
        let context = format!("interpretive_layers[{idx}]");
        let claim_log_ids = valid_claim_ids_from_value(
            item,
            "claim_log_ids",
            evidence,
            report,
            card_index,
            &context,
        );
        if claim_log_ids.is_empty() {
            report.rejected_fields.push(context.clone());
            report.debts.push(enrichment_debt(
                card_index,
                &format!("missing-claim-ref-layer-{idx}"),
                &format!("event-card enrichment interpretive_layers item {idx} lacked any existing supported Claim Log ID"),
                Vec::new(),
                vec!["Ground each interpretive layer in at least one supported phase-specific Claim Log row.".to_string()],
            ));
            continue;
        }
        let interpretation = string_field(item, "interpretation", MAX_TEXT_CHARS);
        let reasoning = string_field(item, "reasoning", MAX_TEXT_CHARS);
        if !useful_text(&interpretation, 20) || !useful_text(&reasoning, 20) {
            report.rejected_fields.push(context);
            continue;
        }
        let layer_type = string_field(item, "layer_type", MAX_SHORT_TEXT_CHARS);
        let epistemic_status =
            optional_string_field(item, "epistemic_status", MAX_SHORT_TEXT_CHARS);
        if !safe_enrichment_text(&interpretation)
            || !safe_enrichment_text(&reasoning)
            || !safe_enrichment_text(&layer_type)
            || epistemic_status
                .as_deref()
                .is_some_and(|status| !safe_enrichment_text(status))
        {
            report.rejected_fields.push(context.clone());
            reject_unsafe_enrichment_text(report, card_index, &context);
            continue;
        }
        let limits = filter_safe_enrichment_strings(
            string_array(item.get("limits"), MAX_ARRAY_ITEMS),
            report,
            card_index,
            &format!("{context}.limits"),
        );
        let mut source_ids = valid_source_ids_from_value(
            item,
            "source_ids",
            evidence,
            report,
            card_index,
            "interpretive_layers",
        );
        for claim_id in &claim_log_ids {
            if let Some(sources) = evidence.claim_support_sources.get(claim_id) {
                source_ids.extend(sources.iter().cloned());
            }
        }
        accepted.push(NarrativeInterpretiveLayer {
            layer_type,
            interpretation,
            epistemic_status,
            reasoning: Some(reasoning),
            limits,
            claim_log_ids,
            source_ids: filter_known_ids(source_ids, &evidence.source_ids),
        });
    }
    if !accepted.is_empty() && accepted.len() >= card.interpretive_layers.len() {
        card.interpretive_layers = accepted;
        report
            .accepted_fields
            .push("interpretive_layers".to_string());
    }
}

fn merge_open_questions(
    card: &mut NarrativeEventCard,
    value: &Value,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
) {
    let questions = filter_safe_enrichment_strings(
        string_array(value.get("open_questions"), MAX_ARRAY_ITEMS),
        report,
        card_index,
        "open_questions",
    )
    .into_iter()
    .filter(|question| useful_text(question, 12))
    .collect::<Vec<_>>();
    if questions.is_empty() {
        return;
    }
    let before = card.open_questions.len();
    merge_id_list(&mut card.open_questions, questions);
    if card.open_questions.len() > before {
        report.accepted_fields.push("open_questions".to_string());
    }
}

fn merge_model_research_debt(
    value: &Value,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    label: &str,
) {
    let Some(items) = value.get("research_debt").and_then(Value::as_array) else {
        return;
    };
    for (idx, item) in items.iter().take(MAX_RESEARCH_DEBT_ITEMS).enumerate() {
        let mut missing = string_field(item, "missing_evidence", MAX_TEXT_CHARS);
        if !safe_enrichment_text(&missing) {
            report
                .rejected_fields
                .push(format!("research_debt[{idx}].missing_evidence"));
            missing = String::new();
        }
        if !useful_text(&missing, 12) {
            missing = format!(
                "event-card enrichment still lacks specific evidence for phase '{}': debt item {}",
                sanitize_text(label, 80),
                idx + 1
            );
        }
        let candidate_queries = filter_safe_enrichment_strings(
            string_array(item.get("candidate_queries"), MAX_ARRAY_ITEMS),
            report,
            card_index,
            &format!("research_debt[{idx}].candidate_queries"),
        );
        let next_check_actions = filter_safe_enrichment_strings(
            string_array(item.get("next_check_actions"), MAX_ARRAY_ITEMS),
            report,
            card_index,
            &format!("research_debt[{idx}].next_check_actions"),
        );
        report.debts.push(enrichment_debt(
            card_index,
            &format!("model-debt-{idx}"),
            &missing,
            candidate_queries,
            next_check_actions,
        ));
    }
}

fn string_field(value: &Value, field: &str, limit: usize) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(|value| sanitize_text(value, limit))
        .unwrap_or_default()
}

fn optional_string_field(value: &Value, field: &str, limit: usize) -> Option<String> {
    let value = string_field(value, field, limit);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn string_array(value: Option<&Value>, limit: usize) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(Value::as_str)
        .map(|item| sanitize_text(item, MAX_SHORT_TEXT_CHARS))
        .filter(|item| !item.is_empty())
        .filter(|item| seen.insert(item.clone()))
        .take(limit)
        .collect()
}

fn filter_safe_enrichment_strings(
    items: Vec<String>,
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    context: &str,
) -> Vec<String> {
    let mut rejected = false;
    let safe = items
        .into_iter()
        .filter(|item| {
            let is_safe = safe_enrichment_text(item);
            if !is_safe {
                rejected = true;
            }
            is_safe
        })
        .collect();
    if rejected {
        reject_unsafe_enrichment_text(report, card_index, context);
    }
    safe
}

fn safe_enrichment_text(value: &str) -> bool {
    unsafe_enrichment_text_reason(value).is_none()
}

fn unsafe_enrichment_text_reason(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    let unsafe_markers = [
        "http://",
        "https://",
        "://",
        "www.",
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "169.254.169.254",
        "metadata.google.internal",
        "instance-data.ec2.internal",
        "computemetadata",
        "/latest/meta-data",
        "resolved prompt",
        "resolved_system_prompt",
        "resolved_user_prompt",
        "system prompt",
        "user prompt",
        "source pack",
        "provider payload",
        "controller artifact",
        "research_controller",
        "source diagnostics",
        "raw diagnostics",
        "raw prompt",
        "quality gate",
        "validator",
        "diagnostic",
        "payload",
        "research_artifact_json",
        "[research_artifact_json]",
        "<system-reminder",
        "BEGIN PRIVATE",
    ];
    if unsafe_markers
        .iter()
        .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
    {
        return Some("unsafe prompt/provider/URL marker");
    }
    contains_private_host_literal(value).then_some("private/internal host literal")
}

fn contains_private_host_literal(value: &str) -> bool {
    if contains_contextual_bare_private_host_reference(value) {
        return true;
    }
    if contains_blocked_bracketed_host(value) {
        return true;
    }
    value
        .split(|ch: char| !host_candidate_char(ch))
        .filter(|token| !token.is_empty())
        .any(|token| {
            host_token_is_blocked(token)
                || token
                    .split(':')
                    .any(|part| part.contains('.') && host_token_is_blocked(part))
        })
}

fn contains_contextual_bare_private_host_reference(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let host_labels = ["host", "hostname", "source", "url", "origin", "endpoint"];
    let bare_hosts = ["internal", "metadata"];

    host_labels.iter().any(|label| {
        bare_hosts.iter().any(|host| {
            contextual_bare_host_value_match(&lower, label, ':', host)
                || contextual_bare_host_value_match(&lower, label, '=', host)
        })
    })
}

fn contextual_bare_host_value_match(
    haystack: &str,
    label: &str,
    separator: char,
    host: &str,
) -> bool {
    for label_form in [
        label.to_string(),
        format!("\"{label}\""),
        format!("'{label}'"),
    ] {
        let mut search_offset = 0usize;
        while let Some(rel_idx) = haystack[search_offset..].find(&label_form) {
            let idx = search_offset + rel_idx;
            if !contextual_label_start_ok(haystack, idx)
                || !contextual_label_end_ok(haystack, idx + label_form.len())
            {
                search_offset = idx + 1;
                continue;
            }
            let after_label = &haystack[idx + label_form.len()..];
            let after_label_trimmed = after_label.trim_start();
            if !after_label_trimmed.starts_with(separator) {
                search_offset = idx + 1;
                continue;
            }
            let after_separator = &after_label_trimmed[separator.len_utf8()..];
            let trimmed = after_separator.trim_start();
            let had_space_after_separator = trimmed.len() != after_separator.len();
            if let Some(rest) = trimmed.strip_prefix(host) {
                if bare_host_value_boundary(rest, had_space_after_separator) {
                    return true;
                }
            }
            if let Some(quoted_rest) = trimmed
                .strip_prefix('"')
                .or_else(|| trimmed.strip_prefix('\''))
            {
                let quote = trimmed.chars().next().unwrap_or_default();
                if let Some(rest) = quoted_rest.strip_prefix(host) {
                    if let Some(after_quote) = rest.strip_prefix(quote) {
                        return quoted_bare_host_value_boundary(after_quote);
                    }
                }
            }
            search_offset = idx + 1;
        }
    }
    false
}

fn contextual_label_start_ok(haystack: &str, idx: usize) -> bool {
    idx == 0
        || haystack[..idx]
            .chars()
            .next_back()
            .is_none_or(|ch| !contextual_identifier_char(ch))
}

fn contextual_label_end_ok(haystack: &str, idx: usize) -> bool {
    idx >= haystack.len()
        || haystack[idx..]
            .chars()
            .next()
            .is_none_or(|ch| !contextual_identifier_char(ch))
}

fn contextual_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn bare_host_value_boundary(rest: &str, had_space_after_separator: bool) -> bool {
    if rest.is_empty() {
        return true;
    }
    let first = rest.chars().next().unwrap_or_default();
    if matches!(
        first,
        ',' | ';' | '.' | ')' | ']' | '}' | '"' | '\'' | '/' | '\\'
    ) {
        return true;
    }
    if first.is_whitespace() {
        if !had_space_after_separator {
            return true;
        }
        let trimmed = rest.trim_start();
        return trimmed.is_empty()
            || trimmed.starts_with(|ch: char| {
                matches!(
                    ch,
                    ',' | ';' | '.' | ')' | ']' | '}' | '"' | '\'' | '/' | '\\'
                )
            });
    }
    false
}

fn quoted_bare_host_value_boundary(rest: &str) -> bool {
    rest.is_empty()
        || rest.starts_with(|ch: char| {
            matches!(
                ch,
                ',' | ';' | '.' | ')' | ']' | '}' | '"' | '\'' | '/' | '\\' | ':'
            ) || ch.is_whitespace()
        })
}

fn contains_blocked_bracketed_host(value: &str) -> bool {
    let mut remaining = value;
    while let Some(open) = remaining.find('[') {
        let after_open = &remaining[open + 1..];
        let Some(close) = after_open.find(']') else {
            return false;
        };
        let host = &after_open[..close];
        if host_token_is_blocked(host) {
            return true;
        }
        remaining = &after_open[close + 1..];
    }
    false
}

fn host_candidate_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | ':' | '[' | ']' | 'x' | 'X')
}

fn host_token_is_blocked(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }
    let has_numeric_port = !token.contains("::")
        && token.rsplit_once(':').is_some_and(|(_, port)| {
            !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit())
        });
    let bracketed_ipv6 = token
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(host, _)| host));
    let mut host = if let Some(host) = bracketed_ipv6 {
        host
    } else if has_numeric_port {
        token
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(token)
    } else {
        token
    };
    host = host
        .trim_matches(|ch: char| matches!(ch, '[' | ']' | '.' | ':'))
        .trim();
    if host.is_empty() {
        return false;
    }
    let lower_host = host.to_ascii_lowercase();
    if has_numeric_port && matches!(lower_host.as_str(), "internal" | "metadata" | "localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return blocked_private_ip(ip);
    }
    if host.as_bytes()[0].is_ascii_digit()
        && (host.contains('.') || single_token_ipv4_literal_like(host))
    {
        if host.contains('.') && private_ipv4_shorthand_prefix(host) {
            return true;
        }
        return parse_ipv4_style_host(host)
            .map(|ip| blocked_private_ip(IpAddr::V4(ip)))
            .unwrap_or(false);
    }
    private_domain_literal(host)
}

fn parse_ipv4_style_host(host: &str) -> Option<Ipv4Addr> {
    if host.is_empty()
        || !host.as_bytes()[0].is_ascii_digit()
        || !host
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == 'x' || ch == 'X' || ch == '.')
    {
        return None;
    }
    if !host.contains('.') {
        let value = parse_ipv4_number(host)?;
        return Some(Ipv4Addr::from(value));
    }
    let parts = host
        .split('.')
        .map(parse_ipv4_number)
        .collect::<Option<Vec<_>>>()?;
    if parts.len() != 4 || parts.iter().any(|part| *part > 255) {
        return None;
    }
    Some(Ipv4Addr::new(
        parts[0] as u8,
        parts[1] as u8,
        parts[2] as u8,
        parts[3] as u8,
    ))
}

fn single_token_ipv4_literal_like(token: &str) -> bool {
    token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
        .is_some_and(|rest| rest.len() >= 8 && rest.chars().all(|ch| ch.is_ascii_hexdigit()))
        || (token.len() >= 8 && token.chars().all(|ch| ch.is_ascii_digit()))
        || (token.len() >= 9
            && token.starts_with('0')
            && token.chars().all(|ch| matches!(ch, '0'..='7')))
}

fn private_ipv4_shorthand_prefix(host: &str) -> bool {
    let parts = host.split('.').collect::<Vec<_>>();
    let first = parts.first().and_then(|part| parse_ipv4_number(part));
    let second = parts.get(1).and_then(|part| parse_ipv4_number(part));
    match (first, second) {
        (Some(10), _) | (Some(127), _) | (Some(0), _) | (Some(169), Some(254)) => true,
        (Some(192), Some(168)) => true,
        (Some(172), Some(value)) if (16..=31).contains(&value) => true,
        (Some(100), Some(value)) if (64..=127).contains(&value) => true,
        (Some(198), Some(value)) if (18..=19).contains(&value) => true,
        _ => false,
    }
}

fn private_domain_literal(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host == "metadata.google.internal"
        || host == "instance-data.ec2.internal"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".local")
        || host.ends_with(".localdomain")
        || host.ends_with(".home.arpa")
}

fn parse_ipv4_number(value: &str) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    let (digits, radix) = if let Some(rest) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (rest, 16)
    } else if value.len() > 1 && value.starts_with('0') {
        (&value[1..], 8)
    } else {
        (value, 10)
    };
    u32::from_str_radix(digits, radix).ok()
}

fn blocked_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => blocked_private_ipv4(ip),
        IpAddr::V6(ip) => blocked_private_ipv6(ip),
    }
}

fn blocked_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip == Ipv4Addr::new(255, 255, 255, 255)
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
}

fn blocked_private_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return blocked_private_ipv4(mapped);
    }
    let segments = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
}

fn reject_unsafe_enrichment_text(
    report: &mut HistoricalEventCardMergeReport,
    card_index: usize,
    context: &str,
) {
    report.rejected_fields.push(context.to_string());
    report.debts.push(enrichment_debt(
        card_index,
        &format!("unsafe-text-{}", safe_debt_suffix(context)),
        &format!(
            "event-card enrichment field '{context}' contained URL-like, private-host, prompt, provider, or diagnostic text and was rejected"
        ),
        Vec::new(),
        vec![
            "Return only phase prose grounded in existing Claim Log IDs; keep raw URLs, prompt text, provider payloads, and diagnostics out of event-card enrichment."
                .to_string(),
        ],
    ));
}

fn safe_debt_suffix(value: &str) -> String {
    let mut suffix = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(48)
        .collect::<String>();
    if suffix.is_empty() {
        suffix = "field".to_string();
    }
    suffix
}

fn filter_known_ids(ids: Vec<String>, known: &HashSet<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| known.contains(id))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn merge_id_list(target: &mut Vec<String>, incoming: Vec<String>) {
    for item in incoming {
        if !target.iter().any(|existing| existing == &item) {
            target.push(item);
        }
    }
}

fn should_replace_text(current: &str, incoming: &str, prefer_richer: bool) -> bool {
    let current_len = current.chars().count();
    let incoming_len = incoming.chars().count();
    if current.trim().is_empty() {
        return true;
    }
    if prefer_richer {
        return incoming_len > current_len
            && (current_len < DEVELOPMENT_THIN_CHAR_THRESHOLD || incoming_len >= current_len + 20);
    }
    current_len < PHASE_FIELD_THIN_CHAR_THRESHOLD && incoming_len >= current_len
}

fn thin_optional_text(value: Option<&str>, threshold: usize) -> bool {
    value.is_none_or(|value| !useful_text(value, threshold))
}

fn useful_text(value: &str, threshold: usize) -> bool {
    let cleaned = sanitize_text(value, MAX_TEXT_CHARS);
    cleaned.chars().count() >= threshold && !placeholder_text(&cleaned)
}

fn placeholder_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let placeholders = [
        "todo",
        "tbd",
        "unknown",
        "n/a",
        "not specified",
        "missing",
        "placeholder",
        "추가 필요",
        "미정",
        "불명",
        "없음",
    ];
    placeholders
        .iter()
        .any(|placeholder| lower.contains(placeholder) || value.contains(placeholder))
}

fn sanitize_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

fn compact_claim_rows(claims: &[ResearchClaimLogEntry]) -> String {
    claims
        .iter()
        .take(MAX_PROMPT_CLAIMS)
        .map(|claim| {
            format!(
                "- {} | {} | support_source_card_ids={} | support_urls={}",
                sanitize_text(&claim.id, 32),
                sanitize_text(&claim.claim, 260),
                claim.support_source_card_ids.join(","),
                claim
                    .support_urls
                    .iter()
                    .take(3)
                    .map(|url| sanitize_text(url, 160))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_source_rows(sources: &[ResearchSourceCard]) -> String {
    sources
        .iter()
        .take(MAX_PROMPT_SOURCES)
        .map(|source| {
            format!(
                "- {} | {} | {} | {}",
                sanitize_text(&source.id, 32),
                sanitize_text(&source.title, 160),
                sanitize_text(&source.source_class, 80),
                sanitize_text(&source.url, 220)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn enrichment_debt(
    card_index: usize,
    suffix: &str,
    missing_evidence: &str,
    candidate_queries: Vec<String>,
    next_check_actions: Vec<String>,
) -> ResearchDebtItem {
    ResearchDebtItem {
        id: format!("event-card-enrichment-card-{card_index}-{suffix}"),
        severity: "medium".to_string(),
        failed_gate: Some("event_card_enrichment".to_string()),
        missing_evidence: sanitize_text(missing_evidence, MAX_TEXT_CHARS),
        required_source_class: None,
        candidate_queries: candidate_queries
            .into_iter()
            .map(|query| sanitize_text(&query, MAX_SHORT_TEXT_CHARS))
            .filter(|query| !query.is_empty())
            .take(MAX_ARRAY_ITEMS)
            .collect(),
        next_check_actions: next_check_actions
            .into_iter()
            .map(|action| sanitize_text(&action, MAX_SHORT_TEXT_CHARS))
            .filter(|action| !action.is_empty())
            .take(MAX_ARRAY_ITEMS)
            .collect(),
        status: "open".to_string(),
    }
}

fn anchor_tokens(value: &str) -> HashSet<String> {
    sanitize_text(value, MAX_TEXT_CHARS)
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| token.chars().count() >= 3)
        .collect()
}

impl EvidenceIndex {
    fn new(artifacts: &ResearchControllerArtifacts) -> Self {
        Self::from_parts(&artifacts.claim_log, &artifacts.source_cards)
    }

    fn from_parts(claims: &[ResearchClaimLogEntry], sources: &[ResearchSourceCard]) -> Self {
        let source_ids = sources
            .iter()
            .filter(|source| normalize_absolute_public_evidence_url(&source.url).is_some())
            .map(|source| source.id.clone())
            .collect::<HashSet<_>>();
        let claim_ids = claims
            .iter()
            .map(|claim| claim.id.clone())
            .collect::<HashSet<_>>();
        let mut supported_claim_ids = HashSet::new();
        let mut claim_support_sources = HashMap::new();
        let mut claim_texts = HashMap::new();
        for claim in claims {
            let known_support_sources = claim
                .support_source_card_ids
                .iter()
                .filter(|id| source_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            let has_public_url = claim
                .support_urls
                .iter()
                .any(|url| normalize_absolute_public_evidence_url(url).is_some());
            if !known_support_sources.is_empty() || has_public_url {
                supported_claim_ids.insert(claim.id.clone());
            }
            claim_texts.insert(claim.id.clone(), claim.claim.clone());
            claim_support_sources.insert(claim.id.clone(), known_support_sources);
        }
        Self {
            claim_ids,
            supported_claim_ids,
            source_ids,
            claim_support_sources,
            claim_texts,
        }
    }
}

fn prompt_source_rows(
    sources: &[ResearchSourceCard],
    evidence: &EvidenceIndex,
) -> Vec<ResearchSourceCard> {
    sources
        .iter()
        .filter(|source| {
            evidence.source_ids.contains(&source.id)
                && normalize_absolute_public_evidence_url(&source.url).is_some()
        })
        .cloned()
        .collect()
}

fn prompt_claim_rows(
    claims: &[ResearchClaimLogEntry],
    evidence: &EvidenceIndex,
) -> Vec<ResearchClaimLogEntry> {
    claims
        .iter()
        .filter(|claim| evidence.supported_claim_ids.contains(&claim.id))
        .map(|claim| {
            let mut trusted = claim.clone();
            trusted
                .support_source_card_ids
                .retain(|id| evidence.source_ids.contains(id));
            trusted.support_urls.retain(|url| {
                normalize_absolute_public_evidence_url(url)
                    .is_some_and(|normalized| !normalized.is_empty())
            });
            trusted
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquid_protocol::{
        NarrativeState, ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchSourceCard,
    };

    fn source(id: &str) -> ResearchSourceCard {
        ResearchSourceCard {
            id: id.to_string(),
            url: format!("https://example.org/{id}"),
            title: format!("Source {id}"),
            source_class: "authoritative_secondary".to_string(),
            ..ResearchSourceCard::default()
        }
    }

    fn claim(id: &str, source_id: &str) -> ResearchClaimLogEntry {
        ResearchClaimLogEntry {
            id: id.to_string(),
            claim: format!(
                "1904년 {source_id} 전선에서 일본과 러시아의 선택이 다음 국면을 압박했다."
            ),
            support_source_card_ids: vec![source_id.to_string()],
            confidence: Some("medium".to_string()),
            ..ResearchClaimLogEntry::default()
        }
    }

    fn weak_card() -> NarrativeEventCard {
        NarrativeEventCard {
            label: "초기 국면".to_string(),
            development: Some("짧다".to_string()),
            ..NarrativeEventCard::default()
        }
    }

    fn rich_card() -> NarrativeEventCard {
        NarrativeEventCard {
            label: "1904년 뤼순과 인천의 초기 전환".to_string(),
            timeframe: Some("1904년 2월".to_string()),
            actors: vec!["일본".to_string(), "러시아".to_string()],
            region_or_front: Some("뤼순·인천·한국 병참로".to_string()),
            trigger: Some("러시아 함대 압박과 한국 병참로 확보가 동시에 요구되었다.".to_string()),
            development: Some("일본은 뤼순의 러시아 함대를 압박하면서 인천과 한국 병참로를 확보해 만주 전선으로 넘어갈 조건을 만들었다. 이 국면은 해상 위협과 육상 진입로가 분리되지 않았다는 점에서 다음 만주 작전을 밀어냈다.".to_string()),
            outcome: Some("전장이 만주로 이동할 작전 조건이 만들어졌다.".to_string()),
            claim_log_ids: vec!["C1".to_string()],
            source_ids: vec!["S1".to_string()],
            causal_spine: vec![NarrativeCausalSpineStep {
                step_type: "forcing_factor".to_string(),
                description: "러시아 함대와 한국 병참로 문제가 초기 작전 방향을 강제했다.".to_string(),
                epistemic_status: Some("interpretation".to_string()),
                reasoning: Some("Claim Log가 함대 압박과 병참로 확보를 같은 국면의 조건으로 묶기 때문이다.".to_string()),
                limits: Vec::new(),
                claim_log_ids: vec!["C1".to_string()],
                source_ids: vec!["S1".to_string()],
            }],
            interpretive_layers: vec![NarrativeInterpretiveLayer {
                layer_type: "operations".to_string(),
                interpretation: "초기 작전은 단순 해전이 아니라 만주 전선 진입 조건을 만드는 복합 국면이었다.".to_string(),
                epistemic_status: Some("interpretation".to_string()),
                reasoning: Some("함대 압박과 병참로 확보가 함께 제시되어 작전·병참 층위가 결합된다.".to_string()),
                limits: Vec::new(),
                claim_log_ids: vec!["C1".to_string()],
                source_ids: vec!["S1".to_string()],
            }],
            confidence: Some("medium".to_string()),
            open_questions: Vec::new(),
        }
    }

    fn artifacts_with_card(card: NarrativeEventCard) -> ResearchControllerArtifacts {
        ResearchControllerArtifacts {
            source_cards: vec![source("S1")],
            claim_log: vec![claim("C1", "S1")],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![card],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        }
    }

    #[test]
    fn weak_historical_event_card_with_empty_spine_and_layers_is_selected() {
        let artifacts = artifacts_with_card(weak_card());
        let selected = select_weak_historical_event_cards(&artifacts, 2);
        assert_eq!(selected.len(), 1);
        assert!(selected[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("causal_spine")));
        assert!(selected[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("interpretive_layers")));
    }

    #[test]
    fn rich_grounded_event_card_is_not_selected() {
        let artifacts = artifacts_with_card(rich_card());
        let selected = select_weak_historical_event_cards(&artifacts, 2);
        assert!(selected.is_empty(), "unexpected reasons: {selected:?}");
    }

    #[test]
    fn evidence_ready_selection_skips_placeholders_and_keeps_bounded_weak_cards() {
        let mut weak_ready = rich_card();
        weak_ready.development = Some("짧다".to_string());
        weak_ready.causal_spine.clear();
        weak_ready.interpretive_layers.clear();

        let mut placeholder = weak_ready.clone();
        placeholder.label = "근거 연결 국면".to_string();

        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source("S1")],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1904년 뤼순과 인천의 초기 전환은 일본과 러시아가 한국 병참로와 만주 전선 진입 조건을 둘러싸고 충돌한 국면이었다.".to_string(),
                support_source_card_ids: vec!["S1".to_string()],
                confidence: Some("medium".to_string()),
                ..ResearchClaimLogEntry::default()
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![placeholder, weak_ready, rich_card()],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let weak_ready_reasons = historical_event_card_weakness_reasons_for_card(
            &artifacts,
            &artifacts.narrative_state.as_ref().unwrap().event_cards[1],
        );
        let selected = select_evidence_ready_weak_historical_event_cards(&artifacts, 1);
        assert_eq!(selected.len(), 1, "reasons={weak_ready_reasons:?}");
        assert_eq!(selected[0].index, 1);
        assert!(selected[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("causal_spine")));
    }

    #[test]
    fn readiness_debt_uses_narrative_enrichment_gate() {
        let debt = historical_enrichment_readiness_debt(
            "no_claim_log",
            "supported Claim Log rows are required before event-card enrichment",
        );

        assert_eq!(
            debt.failed_gate.as_deref(),
            Some("narrative_enrichment_readiness")
        );
        assert!(debt.missing_evidence.contains("Claim Log"));
    }

    #[test]
    fn metadata_support_urls_do_not_count_as_supported_claim_grounding() {
        let mut card = rich_card();
        card.claim_log_ids = vec!["C1".to_string()];
        card.source_ids = vec!["S1".to_string()];

        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source("S1")],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1904년 뤼순과 인천의 연결이 다음 국면을 압박했다.".to_string(),
                support_source_card_ids: Vec::new(),
                support_urls: vec![
                    "https://metadata.google.internal/computeMetadata/v1".to_string()
                ],
                confidence: Some("medium".to_string()),
                ..ResearchClaimLogEntry::default()
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![card],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let selected = select_weak_historical_event_cards(&artifacts, 2);
        assert_eq!(selected.len(), 1);
        assert!(selected[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("supported Claim Log rows")));
    }

    #[test]
    fn private_source_card_urls_do_not_count_as_supported_claim_grounding_or_prompt_rows() {
        let mut private_source = source("S1");
        private_source.url = "http://localhost:11434/internal".to_string();
        private_source.title = "Private Source".to_string();
        let mut card = rich_card();
        card.claim_log_ids = vec!["C1".to_string()];
        card.source_ids = vec!["S1".to_string()];

        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![private_source],
            claim_log: vec![claim("C1", "S1")],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![card],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let selected = select_weak_historical_event_cards(&artifacts, 2);
        assert_eq!(selected.len(), 1);
        assert!(selected[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("supported Claim Log rows")));

        let prompt = build_historical_event_card_enrichment_prompt(
            &artifacts,
            &selected[0],
            "러일전쟁의 전개",
        );
        assert!(!prompt.contains("localhost"));
        assert!(!prompt.contains("Private Source"));
        assert!(!prompt.contains("- C1 |"));
        assert!(!prompt.contains("- S1 |"));
    }

    #[test]
    fn duplicate_source_card_ids_do_not_reintroduce_private_prompt_rows() {
        let public_source = source("S1");
        let mut private_duplicate = source("S1");
        private_duplicate.url = "http://localhost:11434/internal".to_string();
        private_duplicate.title = "Private Duplicate Source".to_string();
        let mut card = weak_card();
        card.claim_log_ids = vec!["C1".to_string()];
        card.source_ids = vec!["S1".to_string()];

        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![public_source, private_duplicate],
            claim_log: vec![claim("C1", "S1")],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![card],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let selected = select_weak_historical_event_cards(&artifacts, 1);
        assert_eq!(selected.len(), 1);
        let prompt = build_historical_event_card_enrichment_prompt(
            &artifacts,
            &selected[0],
            "러일전쟁의 전개",
        );
        assert!(prompt.contains("https://example.org/S1"));
        assert!(!prompt.contains("localhost"));
        assert!(!prompt.contains("Private Duplicate Source"));
    }

    #[test]
    fn private_source_card_ids_are_rejected_as_enrichment_support() {
        let mut artifacts = artifacts_with_card(weak_card());
        artifacts.source_cards[0].url = "http://localhost:11434/internal".to_string();
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{"development":"러시아 함대 압박과 한국 병참로 확보가 결합되어 만주 진입 조건을 만들었다.","claim_log_ids":["C1"],"source_ids":["S1"]}"#,
        );
        assert!(!report.accepted());
        assert!(report
            .debts
            .iter()
            .any(|debt| debt.missing_evidence.contains("C1")));
        assert!(report
            .debts
            .iter()
            .any(|debt| debt.missing_evidence.contains("S1")));
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert!(!card.claim_log_ids.contains(&"C1".to_string()));
        assert!(!card.source_ids.contains(&"S1".to_string()));
    }

    #[test]
    fn unsafe_enrichment_text_is_rejected_even_with_valid_claim_support() {
        let mut artifacts = artifacts_with_card(weak_card());
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{
              "development":"러시아 함대 압박과 한국 병참로 확보가 결합되었다. http://169.254.169.254/latest/meta-data 이 문장은 충분히 길어 기존 설명을 대체하려 한다.",
              "claim_log_ids":["C1"],
              "source_ids":["S1"],
              "causal_spine":[{"step_type":"forcing_factor","description":"resolved prompt 내용을 반복한다.","epistemic_status":"interpretation","reasoning":"Claim Log가 이 국면을 뒷받침한다.","limits":[],"claim_log_ids":["C1"],"source_ids":["S1"]}],
              "open_questions":["provider payload를 확인하라"]
            }"#,
        );
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert_eq!(card.development.as_deref(), Some("짧다"));
        assert!(card.causal_spine.is_empty());
        assert!(card.open_questions.is_empty());
        assert!(report
            .debts
            .iter()
            .any(|debt| debt.id.contains("unsafe-text")));
    }

    #[test]
    fn ordinary_internal_prose_is_accepted() {
        let mut artifacts = artifacts_with_card(weak_card());
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{
              "development":"러시아 함대 압박과 한국 병참로 확보가 결합되었고, internal supply coordination 문제와 내부 정치 논쟁이 겹치며 다음 국면의 결정을 밀어붙였다.",
              "claim_log_ids":["C1"],
              "source_ids":["S1"]
            }"#,
        );
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];

        assert_ne!(card.development.as_deref(), Some("짧다"));
        assert!(
            report
                .debts
                .iter()
                .all(|debt| !debt.id.contains("unsafe-text")),
            "ordinary prose should not be rejected: {report:?}"
        );
    }

    #[test]
    fn source_or_resource_internal_prose_is_accepted() {
        for accepted_text in [
            "source: internal court memoranda shaped the debate and changed how ministers framed the next phase.",
            "resource: internal mobilization records explain why the next offensive slowed despite public victories.",
        ] {
            let mut artifacts = artifacts_with_card(weak_card());
            let raw = serde_json::json!({
                "development": format!("러시아 함대 압박과 한국 병참로 확보가 결합되었다. {accepted_text} 이 문장은 충분히 길어 기존 설명을 대체하려 한다."),
                "claim_log_ids": ["C1"],
                "source_ids": ["S1"]
            })
            .to_string();

            let report = merge_historical_event_card_enrichment_json(&mut artifacts, 0, &raw);
            let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
            assert_ne!(card.development.as_deref(), Some("짧다"));
            assert!(
                report
                    .debts
                    .iter()
                    .all(|debt| !debt.id.contains("unsafe-text")),
                "contextual prose should not be rejected: {accepted_text}; report={report:?}"
            );
        }
    }

    #[test]
    fn quoted_contextual_private_host_fragments_are_rejected() {
        for unsafe_text in [
            r#"json={"source":"metadata"}"#,
            r#""host": "internal""#,
            r#"endpoint='metadata'"#,
            r#"url = "internal""#,
        ] {
            let mut artifacts = artifacts_with_card(weak_card());
            let raw = serde_json::json!({
                "development": format!("러시아 함대 압박과 한국 병참로 확보가 결합되었다. {unsafe_text} 이 문장은 충분히 길어 기존 설명을 대체하려 한다."),
                "claim_log_ids": ["C1"],
                "source_ids": ["S1"]
            })
            .to_string();

            let report = merge_historical_event_card_enrichment_json(&mut artifacts, 0, &raw);
            let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
            assert_eq!(card.development.as_deref(), Some("짧다"));
            assert!(
                report
                    .debts
                    .iter()
                    .any(|debt| debt.id.contains("unsafe-text")),
                "quoted host-like fragment should be rejected: {unsafe_text}; report={report:?}"
            );
        }
    }

    #[test]
    fn host_like_internal_references_are_rejected() {
        for unsafe_text in [
            "10.0.0.1/latest 내부 주소를 확인해야 한다.",
            "source:192.168.1.1 라우터 진단을 확인해야 한다.",
            "fd00::1/latest 내부 IPv6 주소를 확인해야 한다.",
            "::ffff:127.0.0.1 로컬 매핑 주소를 확인해야 한다.",
            "2130706433/latest 단일 숫자 IPv4 주소를 확인해야 한다.",
            "0x7f000001/latest 16진 IPv4 주소를 확인해야 한다.",
            "017700000001/latest 8진 IPv4 주소를 확인해야 한다.",
            "db.internal:8080 내부 도메인을 확인해야 한다.",
            "service.local 내부 로컬 도메인을 확인해야 한다.",
            "source:internal 메모를 그대로 남긴다.",
            "source: internal.",
            "host=metadata 진단 조각을 그대로 옮긴다.",
            "internal:8080 포트가 열린 내부 호스트를 확인해야 한다.",
            "metadata:443 메타데이터 포트 흔적을 남긴다.",
            "[fd00::1]:443 내부 IPv6 포트 주소를 확인해야 한다.",
            "source:[fd00::1]:443 접두사가 붙은 내부 IPv6 주소를 확인해야 한다.",
            "json={\"source\":\"[fd00::1]:443\"} 구조화된 내부 IPv6 주소를 확인해야 한다.",
            "source:[::ffff:127.0.0.1]:8080 IPv4 매핑 내부 IPv6 주소를 확인해야 한다.",
            "127.1 짧은 IPv4 로컬 표기를 확인해야 한다.",
            "10.1 짧은 IPv4 사설 표기를 확인해야 한다.",
            "192.168.1 짧은 IPv4 사설 표기를 확인해야 한다.",
        ] {
            let mut artifacts = artifacts_with_card(weak_card());
            let raw = serde_json::json!({
                "development": format!("러시아 함대 압박과 한국 병참로 확보가 결합되었다. {unsafe_text} 이 문장은 충분히 길어 기존 설명을 대체하려 한다."),
                "claim_log_ids": ["C1"],
                "source_ids": ["S1"]
            })
            .to_string();

            let report = merge_historical_event_card_enrichment_json(&mut artifacts, 0, &raw);
            let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
            assert_eq!(card.development.as_deref(), Some("짧다"));
            assert!(
                report
                    .debts
                    .iter()
                    .any(|debt| debt.id.contains("unsafe-text")),
                "unsafe host text should be rejected: {unsafe_text}; report={report:?}"
            );
        }
    }

    #[test]
    fn invented_claim_log_ids_are_rejected_as_debt() {
        let mut artifacts = artifacts_with_card(weak_card());
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{"development":"러시아 함대 압박과 한국 병참로 확보가 결합된 국면이었다.","claim_log_ids":["C999"],"source_ids":["S1"]}"#,
        );
        assert!(report
            .debts
            .iter()
            .any(|debt| debt.missing_evidence.contains("C999")));
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert!(!card.claim_log_ids.contains(&"C999".to_string()));
        assert_eq!(card.development.as_deref(), Some("짧다"));
    }

    #[test]
    fn invented_source_card_ids_are_rejected_as_debt() {
        let mut artifacts = artifacts_with_card(weak_card());
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{"development":"러시아 함대 압박과 한국 병참로 확보가 결합되어 만주 진입 조건을 만들었다.","claim_log_ids":["C1"],"source_ids":["S999"]}"#,
        );
        assert!(report
            .debts
            .iter()
            .any(|debt| debt.missing_evidence.contains("S999")));
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert!(!card.source_ids.contains(&"S999".to_string()));
        assert!(card.source_ids.contains(&"S1".to_string()));
    }

    #[test]
    fn valid_enrichment_merges_into_matching_card() {
        let mut artifacts = artifacts_with_card(weak_card());
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{
              "timeframe":"1904년 2월",
              "actors":["일본","러시아"],
              "region_or_front":"뤼순·인천·한국 병참로",
              "trigger":"러시아 함대 압박과 한국 병참로 확보가 동시에 요구되었다.",
              "development":"일본은 뤼순의 러시아 함대를 압박하면서 인천과 한국 병참로를 확보해 만주 전선으로 넘어갈 조건을 만들었다. 이 국면은 해상 위협과 육상 진입로가 분리되지 않았다는 점에서 다음 만주 작전을 밀어냈다.",
              "outcome":"전장이 만주로 이동할 작전 조건이 만들어졌다.",
              "claim_log_ids":["C1"],
              "source_ids":["S1"],
              "causal_spine":[{"step_type":"forcing_factor","description":"러시아 함대와 한국 병참로 문제가 초기 작전 방향을 강제했다.","epistemic_status":"interpretation","reasoning":"Claim Log가 함대 압박과 병참로 확보를 같은 국면의 조건으로 묶기 때문이다.","limits":[],"claim_log_ids":["C1"],"source_ids":["S1"]}],
              "interpretive_layers":[{"layer_type":"operations","interpretation":"초기 작전은 단순 해전이 아니라 만주 전선 진입 조건을 만드는 복합 국면이었다.","epistemic_status":"interpretation","reasoning":"함대 압박과 병참로 확보가 함께 제시되어 작전·병참 층위가 결합된다.","limits":[],"claim_log_ids":["C1"],"source_ids":["S1"]}]
            }"#,
        );
        assert!(report.accepted());
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert_eq!(card.timeframe.as_deref(), Some("1904년 2월"));
        assert_eq!(card.causal_spine.len(), 1);
        assert_eq!(card.interpretive_layers.len(), 1);
    }

    #[test]
    fn invalid_enrichment_does_not_erase_existing_good_card_fields() {
        let mut artifacts = artifacts_with_card(rich_card());
        let original = artifacts.narrative_state.as_ref().unwrap().event_cards[0].clone();
        let report = merge_historical_event_card_enrichment_json(
            &mut artifacts,
            0,
            r#"{"development":"짧음","claim_log_ids":["C999"],"source_ids":["S999"],"causal_spine":[{"description":"bad","claim_log_ids":[]}]}"#,
        );
        assert!(!report.debts.is_empty());
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];
        assert_eq!(card, &original);
    }

    #[test]
    fn applicability_is_historical_strategy_only_but_orchestrator_ready() {
        let artifacts = artifacts_with_card(weak_card());
        assert!(historical_event_card_enrichment_applies(
            "[AI-Research]",
            Some("high"),
            Some("strict"),
            Some("러일전쟁의 전개"),
            None,
            None,
            &artifacts,
        ));
        assert!(!historical_event_card_enrichment_applies(
            "[AI-Research]",
            Some("high"),
            Some("strict"),
            Some("vLLM 배포 전략"),
            None,
            None,
            &artifacts,
        ));
    }
}
