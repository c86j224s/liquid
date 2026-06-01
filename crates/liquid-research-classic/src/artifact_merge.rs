use std::collections::HashSet;

use liquid_protocol::{
    NarrativeCausalLink, NarrativeEventCard, NarrativeEvidenceLayer, NarrativeImpact,
    NarrativeOpenGap, NarrativeSectionOutlineItem, NarrativeState, ReaderQualityArtifacts,
    ResearchConflictMapEntry, ResearchControllerArtifacts, ResearchDebtItem,
    ResearchQualityGateArtifact, ResearchSourceDiagnosticsEnvelope,
};
use liquid_research_core::normalize_absolute_public_evidence_url;
use liquid_research_core::{conflict_has_matching_actionable_open_debt, debt_matches_conflict};

use crate::{
    candidate_queries_from_failure, debt_id_from_failure, required_source_class_from_failure,
};

pub fn research_quality_gate_from_failures(
    artifacts: &ResearchControllerArtifacts,
    failure_messages: &[String],
) -> ResearchQualityGateArtifact {
    ResearchQualityGateArtifact {
        status: if failure_messages.is_empty() {
            "passed".to_string()
        } else {
            "failed".to_string()
        },
        failure_messages: failure_messages.to_vec(),
        unsupported_claim_count: artifacts
            .claim_log
            .iter()
            .filter(|claim| {
                claim.support_source_card_ids.is_empty() && claim.support_urls.is_empty()
            })
            .count(),
        unresolved_conflict_count: artifacts
            .conflict_map
            .iter()
            .filter(|conflict| {
                !conflict
                    .resolution_status
                    .as_deref()
                    .is_some_and(conflict_status_is_terminally_resolved)
                    && conflict.promoted_to_debt != Some(true)
            })
            .count(),
        open_debt_count: artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
            .count(),
    }
}

pub fn conflict_status_is_terminally_resolved(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    matches!(
        normalized.as_str(),
        "resolved" | "resolved_by_synthesis" | "resolved_by_evidence"
    )
}

pub fn merge_research_controller_artifacts(
    current: &mut ResearchControllerArtifacts,
    incoming: ResearchControllerArtifacts,
) {
    current.source_cards = incoming.source_cards;
    current.claim_log = incoming.claim_log;
    current.conflict_map = incoming.conflict_map;
    current.research_debt = reconcile_research_debt_snapshot(
        std::mem::take(&mut current.research_debt),
        incoming.research_debt,
    );
    if let Some(incoming_narrative_state) = incoming.narrative_state {
        current.narrative_state = Some(merge_narrative_state(
            current.narrative_state.take(),
            incoming_narrative_state,
            &mut current.research_debt,
        ));
    }
    resync_derived_narrative_debt(current);
    if let Some(incoming_reader_quality) = incoming.reader_quality {
        current.reader_quality = Some(merge_reader_quality(
            current.reader_quality.take(),
            incoming_reader_quality,
        ));
    }
    if incoming.quality_gate.is_some() {
        current.quality_gate = incoming.quality_gate;
    }
    for warning in incoming.warnings {
        push_unique_warning(&mut current.warnings, warning);
    }
}

pub fn merge_research_controller_artifacts_with_trusted_source_urls(
    current: &mut ResearchControllerArtifacts,
    incoming: ResearchControllerArtifacts,
    trusted_source_urls: &HashSet<String>,
) {
    merge_research_controller_artifacts(current, incoming);
    strip_untrusted_planning_evidence_refs(current, trusted_source_urls);
    resync_derived_narrative_debt(current);
}

pub fn trusted_artifact_merge_source_urls(
    current: &ResearchControllerArtifacts,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> HashSet<String> {
    let mut trusted = current
        .source_cards
        .iter()
        .filter_map(|card| normalize_absolute_public_evidence_url(&card.url))
        .collect::<HashSet<_>>();
    trusted.extend(
        current
            .claim_log
            .iter()
            .flat_map(|claim| claim.support_urls.iter())
            .filter_map(|url| normalize_absolute_public_evidence_url(url)),
    );
    trusted.extend(
        collect_repair_search_known_urls(diagnostics)
            .into_iter()
            .filter_map(|url| normalize_absolute_public_evidence_url(&url)),
    );
    trusted
}

pub fn strip_untrusted_planning_evidence_refs(
    artifacts: &mut ResearchControllerArtifacts,
    trusted_source_urls: &HashSet<String>,
) {
    let trusted_source_ids = artifacts
        .source_cards
        .iter()
        .filter_map(|card| {
            let id = card.id.trim();
            let url = normalize_absolute_public_evidence_url(&card.url)?;
            (!id.is_empty() && trusted_source_urls.contains(&url)).then_some(id.to_string())
        })
        .collect::<HashSet<_>>();
    let trusted_claim_ids = artifacts
        .claim_log
        .iter()
        .filter_map(|claim| {
            let id = claim.id.trim();
            if id.is_empty() {
                return None;
            }
            let has_trusted_source_id = claim
                .support_source_card_ids
                .iter()
                .any(|source_id| trusted_source_ids.contains(source_id.trim()));
            let has_trusted_url = claim
                .support_urls
                .iter()
                .filter_map(|url| normalize_absolute_public_evidence_url(url))
                .any(|url| trusted_source_urls.contains(&url));
            (has_trusted_source_id || has_trusted_url).then_some(id.to_string())
        })
        .collect::<HashSet<_>>();

    let retain_claim_ids = |claim_ids: &mut Vec<String>| {
        claim_ids.retain(|claim_id| trusted_claim_ids.contains(claim_id.trim()));
    };
    let retain_source_ids = |source_ids: &mut Vec<String>| {
        source_ids.retain(|source_id| trusted_source_ids.contains(source_id.trim()));
    };

    if let Some(state) = artifacts.narrative_state.as_mut() {
        for card in &mut state.event_cards {
            retain_claim_ids(&mut card.claim_log_ids);
            retain_source_ids(&mut card.source_ids);
            for step in &mut card.causal_spine {
                retain_claim_ids(&mut step.claim_log_ids);
                retain_source_ids(&mut step.source_ids);
            }
            card.causal_spine
                .retain(|step| !step.claim_log_ids.is_empty());
            for layer in &mut card.interpretive_layers {
                retain_claim_ids(&mut layer.claim_log_ids);
                retain_source_ids(&mut layer.source_ids);
            }
            card.interpretive_layers
                .retain(|layer| !layer.claim_log_ids.is_empty());
        }
        for item in &mut state.timeline {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.actors {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.causal_chain {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.evidence_layers {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.interpretive_tensions {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.impacts {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.reader_questions {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.section_outline {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
        for item in &mut state.open_gaps {
            retain_claim_ids(&mut item.expected_claim_log_ids);
            retain_source_ids(&mut item.expected_source_card_ids);
        }
    }

    if let Some(reader_quality) = artifacts.reader_quality.as_mut() {
        if let Some(graph) = reader_quality.argument_graph.as_mut() {
            for node in &mut graph.nodes {
                retain_claim_ids(&mut node.claim_log_ids);
                retain_source_ids(&mut node.source_card_ids);
            }
            for edge in &mut graph.edges {
                retain_claim_ids(&mut edge.claim_log_ids);
                retain_source_ids(&mut edge.source_card_ids);
            }
        }
        for brief in &mut reader_quality.section_briefs {
            retain_claim_ids(&mut brief.claim_log_ids);
            retain_source_ids(&mut brief.source_card_ids);
        }
    }
}

pub fn reconcile_research_debt_snapshot(
    current: Vec<ResearchDebtItem>,
    incoming: Vec<ResearchDebtItem>,
) -> Vec<ResearchDebtItem> {
    let active_ids = incoming
        .iter()
        .map(|debt| debt.id.clone())
        .collect::<HashSet<_>>();
    let mut reconciled = incoming;
    for mut debt in current {
        if active_ids.contains(&debt.id) {
            continue;
        }
        if debt.status != "closed" {
            debt.status = "closed".to_string();
        }
        reconciled.push(debt);
    }
    reconciled
}

pub fn merge_reader_quality(
    current: Option<ReaderQualityArtifacts>,
    mut incoming: ReaderQualityArtifacts,
) -> ReaderQualityArtifacts {
    let Some(current) = current else {
        return incoming;
    };

    if incoming.argument_graph.is_none() {
        incoming.argument_graph = current.argument_graph;
    }
    if incoming.narrative_plan.is_none() {
        incoming.narrative_plan = current.narrative_plan;
    }
    if incoming.section_briefs.is_empty() {
        incoming.section_briefs = current.section_briefs;
    }
    if incoming.reader_critique.is_none() {
        incoming.reader_critique = current.reader_critique;
    }

    incoming
}

pub fn merge_narrative_state(
    current: Option<NarrativeState>,
    mut incoming: NarrativeState,
    research_debt: &mut Vec<ResearchDebtItem>,
) -> NarrativeState {
    let Some(current) = current else {
        enrich_narrative_state_from_event_cards(&mut incoming, research_debt);
        return incoming;
    };

    if incoming.topic_frame.is_none() {
        incoming.topic_frame = current.topic_frame;
    }
    if incoming.working_thesis.is_none() {
        incoming.working_thesis = current.working_thesis;
    }
    if incoming.reader_promise.is_none() {
        incoming.reader_promise = current.reader_promise;
    }
    if incoming.last_iteration_summary.is_none() {
        incoming.last_iteration_summary = current.last_iteration_summary;
    }
    let current_event_cards = current.event_cards.clone();
    if should_preserve_existing_event_cards(&current.event_cards, &incoming.event_cards) {
        incoming.event_cards = current.event_cards;
    } else if !current.event_cards.is_empty() && !incoming.event_cards.is_empty() {
        incoming.event_cards =
            merge_event_cards_preserving_existing_scope(current.event_cards, incoming.event_cards);
    }
    let event_cards_changed =
        !incoming.event_cards.is_empty() && current_event_cards != incoming.event_cards;
    if incoming.timeline.is_empty() {
        incoming.timeline = current.timeline;
    }
    if incoming.actors.is_empty() {
        incoming.actors = current.actors;
    }
    if incoming.causal_chain.is_empty() && !event_cards_changed {
        incoming.causal_chain = current.causal_chain;
    }
    if incoming.evidence_layers.is_empty() && !event_cards_changed {
        incoming.evidence_layers = current.evidence_layers;
    }
    if incoming.interpretive_tensions.is_empty() {
        incoming.interpretive_tensions = current.interpretive_tensions;
    }
    if incoming.impacts.is_empty() && !event_cards_changed {
        incoming.impacts = current.impacts;
    }
    if incoming.reader_questions.is_empty() {
        incoming.reader_questions = current.reader_questions;
    }
    if incoming.section_outline.is_empty() && !event_cards_changed {
        incoming.section_outline = current.section_outline;
    }
    if incoming.transition_plan.is_empty() {
        incoming.transition_plan = current.transition_plan;
    }
    incoming.open_gaps =
        merge_narrative_open_gaps(current.open_gaps, incoming.open_gaps, research_debt);
    enrich_narrative_state_from_event_cards(&mut incoming, research_debt);
    incoming
}

pub fn enrich_narrative_state_from_event_cards(
    state: &mut NarrativeState,
    research_debt: &mut Vec<ResearchDebtItem>,
) {
    let claim_linked_cards = state
        .event_cards
        .iter()
        .filter(|card| !card.claim_log_ids.is_empty())
        .collect::<Vec<_>>();
    if claim_linked_cards.is_empty() {
        return;
    }

    if state.causal_chain.is_empty()
        && claim_linked_cards.len() >= 2
        && claim_linked_cards.iter().all(|card| {
            card.timeframe
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        })
    {
        state.causal_chain = claim_linked_cards
            .windows(2)
            .enumerate()
            .map(|(idx, window)| {
                let from = window[0];
                let to = window[1];
                let cause = narrative_card_outcome_or_label(from);
                let effect = narrative_card_trigger_or_label(to);
                let mut expected_claim_log_ids = Vec::new();
                expected_claim_log_ids.extend(from.claim_log_ids.iter().cloned());
                expected_claim_log_ids.extend(to.claim_log_ids.iter().cloned());
                expected_claim_log_ids.sort();
                expected_claim_log_ids.dedup();
                let mut expected_source_card_ids = Vec::new();
                expected_source_card_ids.extend(from.source_ids.iter().cloned());
                expected_source_card_ids.extend(to.source_ids.iter().cloned());
                expected_source_card_ids.sort();
                expected_source_card_ids.dedup();
                NarrativeCausalLink {
                    id: format!("derived-link-{}", idx + 1),
                    cause: cause.clone(),
                    effect: effect.clone(),
                    rationale: None,
                    derived_from: Some("event_cards".to_string()),
                    expected_claim_log_ids,
                    expected_source_card_ids,
                }
            })
            .collect();
        upsert_derived_narrative_debt(
            research_debt,
            "derived-narrative-causal-chain",
            "narrative_state.causal_chain was derived from event_cards and still needs model-authored causal rationale before it can satisfy the interpretive-spine gate",
            "Write explicit causal rationale linking adjacent phases to supported Claim Log rows.",
        );
    } else if state.causal_chain.is_empty() && claim_linked_cards.len() >= 2 {
        upsert_derived_narrative_debt(
            research_debt,
            "derived-narrative-causal-chain-order",
            "event_cards lack explicit timeframe anchors required for safe causal_chain derivation",
            "Add timeframe anchors or model-authored causal_chain links with supported Claim Log refs.",
        );
    }

    if state.section_outline.is_empty() {
        state.section_outline = claim_linked_cards
            .iter()
            .take(6)
            .enumerate()
            .map(|(idx, card)| NarrativeSectionOutlineItem {
                id: format!("derived-section-{}", idx + 1),
                heading: narrative_card_label(card),
                purpose: None,
                derived_from: Some("event_cards".to_string()),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            })
            .collect();
        upsert_derived_narrative_debt(
            research_debt,
            "derived-narrative-section-outline",
            "narrative_state.section_outline was derived from event_cards and still needs model-authored section purpose",
            "Write section purposes that explain how each phase serves the central historical interpretation.",
        );
    }

    if state.evidence_layers.is_empty() {
        state.evidence_layers = claim_linked_cards
            .iter()
            .take(4)
            .enumerate()
            .map(|(idx, card)| NarrativeEvidenceLayer {
                id: format!("derived-layer-{}", idx + 1),
                label: format!("{}의 근거 층위", narrative_card_label(card)),
                purpose: None,
                derived_from: Some("event_cards".to_string()),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            })
            .collect();
        upsert_derived_narrative_debt(
            research_debt,
            "derived-narrative-evidence-layers",
            "narrative_state.evidence_layers was derived from event_cards and still needs model-authored evidence-layer purpose",
            "Explain which source/claim layer each phase uses and what interpretive limit it imposes.",
        );
    }

    if state.impacts.is_empty() {
        state.impacts = claim_linked_cards
            .iter()
            .rev()
            .take(2)
            .enumerate()
            .map(|(idx, card)| NarrativeImpact {
                id: format!("derived-impact-{}", idx + 1),
                label: narrative_card_outcome_or_label(card),
                scope: card.region_or_front.clone(),
                implication: None,
                derived_from: Some("event_cards".to_string()),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            })
            .collect();
        upsert_derived_narrative_debt(
            research_debt,
            "derived-narrative-impacts",
            "narrative_state.impacts was derived from event_cards and still needs model-authored impact interpretation",
            "State the short- and long-term significance of the phase outcomes with supported Claim Log refs.",
        );
    }
}

pub fn upsert_derived_narrative_debt(
    research_debt: &mut Vec<ResearchDebtItem>,
    id: &str,
    missing_evidence: &str,
    next_action: &str,
) {
    upsert_research_debt(
        research_debt,
        ResearchDebtItem {
            id: id.to_string(),
            severity: "medium".to_string(),
            failed_gate: Some("narrative_planning".to_string()),
            missing_evidence: missing_evidence.to_string(),
            required_source_class: None,
            candidate_queries: Vec::new(),
            next_check_actions: vec![next_action.to_string()],
            status: "open".to_string(),
        },
    );
}

pub fn resync_derived_narrative_debt(artifacts: &mut ResearchControllerArtifacts) {
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return;
    };
    if state
        .causal_chain
        .iter()
        .any(|item| item.derived_from.is_some())
    {
        upsert_derived_narrative_debt(
            &mut artifacts.research_debt,
            "derived-narrative-causal-chain",
            "narrative_state.causal_chain was derived from event_cards and still needs model-authored causal rationale before it can satisfy the interpretive-spine gate",
            "Write explicit causal rationale linking adjacent phases to supported Claim Log rows.",
        );
    }
    if state
        .section_outline
        .iter()
        .any(|item| item.derived_from.is_some())
    {
        upsert_derived_narrative_debt(
            &mut artifacts.research_debt,
            "derived-narrative-section-outline",
            "narrative_state.section_outline was derived from event_cards and still needs model-authored section purpose",
            "Write section purposes that explain how each phase serves the central historical interpretation.",
        );
    }
    if state
        .evidence_layers
        .iter()
        .any(|item| item.derived_from.is_some())
    {
        upsert_derived_narrative_debt(
            &mut artifacts.research_debt,
            "derived-narrative-evidence-layers",
            "narrative_state.evidence_layers was derived from event_cards and still needs model-authored evidence-layer purpose",
            "Explain which source/claim layer each phase uses and what interpretive limit it imposes.",
        );
    }
    if state.impacts.iter().any(|item| item.derived_from.is_some()) {
        upsert_derived_narrative_debt(
            &mut artifacts.research_debt,
            "derived-narrative-impacts",
            "narrative_state.impacts was derived from event_cards and still needs model-authored impact interpretation",
            "State the short- and long-term significance of the phase outcomes with supported Claim Log refs.",
        );
    }
}

pub fn narrative_card_label(card: &NarrativeEventCard) -> String {
    compact_narrative_text(&card.label)
}

pub fn narrative_card_trigger_or_label(card: &NarrativeEventCard) -> String {
    card.trigger
        .as_deref()
        .map(compact_narrative_text)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| narrative_card_label(card))
}

pub fn narrative_card_outcome_or_label(card: &NarrativeEventCard) -> String {
    card.outcome
        .as_deref()
        .map(compact_narrative_text)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| narrative_card_label(card))
}

pub fn compact_narrative_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn should_preserve_existing_event_cards(
    current: &[NarrativeEventCard],
    incoming: &[NarrativeEventCard],
) -> bool {
    if incoming.is_empty() {
        return !current.is_empty();
    }
    !current.is_empty()
        && narrative_event_card_richness_score(current)
            > narrative_event_card_richness_score(incoming)
        && narrative_event_card_claim_reference_count(current)
            >= narrative_event_card_claim_reference_count(incoming)
        && narrative_event_card_reference_score(current)
            >= narrative_event_card_reference_score(incoming)
}

pub fn narrative_event_card_claim_reference_count(cards: &[NarrativeEventCard]) -> usize {
    cards
        .iter()
        .map(|card| {
            card.claim_log_ids
                .iter()
                .filter(|id| !id.trim().is_empty())
                .count()
        })
        .sum()
}

pub fn narrative_event_card_reference_score(cards: &[NarrativeEventCard]) -> usize {
    cards
        .iter()
        .map(|card| {
            card.claim_log_ids
                .iter()
                .filter(|id| !id.trim().is_empty())
                .count()
                * 2
                + card
                    .source_ids
                    .iter()
                    .filter(|id| !id.trim().is_empty())
                    .count()
        })
        .sum()
}

pub fn merge_event_cards_preserving_existing_scope(
    current: Vec<NarrativeEventCard>,
    incoming: Vec<NarrativeEventCard>,
) -> Vec<NarrativeEventCard> {
    let mut used_incoming = vec![false; incoming.len()];
    let mut merged = Vec::with_capacity(current.len().max(incoming.len()));

    for current_card in current {
        if let Some((idx, incoming_card)) =
            incoming.iter().enumerate().find(|(idx, incoming_card)| {
                !used_incoming[*idx]
                    && event_cards_represent_same_scope(&current_card, incoming_card)
            })
        {
            used_incoming[idx] = true;
            merged.push(incoming_card.clone());
        } else {
            merged.push(current_card);
        }
    }

    for (idx, incoming_card) in incoming.into_iter().enumerate() {
        if !used_incoming[idx] {
            merged.push(incoming_card);
        }
    }

    merged
}

pub fn event_cards_represent_same_scope(
    left: &NarrativeEventCard,
    right: &NarrativeEventCard,
) -> bool {
    let left_label = normalize_event_card_key(&left.label);
    let right_label = normalize_event_card_key(&right.label);
    if !left_label.is_empty()
        && !right_label.is_empty()
        && (left_label == right_label
            || event_card_key_contains(&left_label, &right_label)
            || event_card_key_contains(&right_label, &left_label))
    {
        return true;
    }

    let left_terms = event_card_phase_terms(left);
    let right_terms = event_card_phase_terms(right);
    if !left_terms.is_empty() && !right_terms.is_empty() && !left_terms.is_disjoint(&right_terms) {
        return true;
    }

    let left_timeframe = left
        .timeframe
        .as_deref()
        .map(normalize_event_card_key)
        .unwrap_or_default();
    let right_timeframe = right
        .timeframe
        .as_deref()
        .map(normalize_event_card_key)
        .unwrap_or_default();
    !left_timeframe.is_empty() && left_timeframe == right_timeframe
}

pub fn event_card_phase_terms(card: &NarrativeEventCard) -> HashSet<&'static str> {
    let text = [
        card.label.as_str(),
        card.timeframe.as_deref().unwrap_or_default(),
        card.trigger.as_deref().unwrap_or_default(),
        card.development.as_deref().unwrap_or_default(),
        card.outcome.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let mut terms = HashSet::new();

    for (term, aliases) in [
        ("ancien_regime", &["구체제", "ancien", "old regime"][..]),
        (
            "national_assembly",
            &["국민의회", "national assembly", "bastille", "테니스코트"][..],
        ),
        (
            "constitutional_monarchy",
            &["입헌군주", "constitutional monarch", "1791 constitution"][..],
        ),
        (
            "republic_transition",
            &[
                "공화정",
                "왕정 폐지",
                "왕정폐지",
                "왕정 붕괴",
                "republic",
                "monarchy abolished",
            ][..],
        ),
        (
            "terror",
            &["공포정치", "공안위원회", "terror", "emergency government"][..],
        ),
        ("thermidor", &["테르미도르", "thermidor"][..]),
        (
            "directory",
            &["총재정부", "directory", "brumaire", "브뤼메르"][..],
        ),
        (
            "napoleonic",
            &["나폴레옹", "napoleon", "consulate", "통령정부"][..],
        ),
        (
            "european_order",
            &[
                "유럽 질서",
                "빈 회의",
                "세력균형",
                "세력 균형",
                "european order",
                "vienna",
                "settlement",
                "balance of power",
            ][..],
        ),
    ] {
        if aliases.iter().any(|alias| text.contains(alias)) {
            terms.insert(term);
        }
    }

    terms
}

pub fn event_card_key_contains(haystack: &str, needle: &str) -> bool {
    needle.chars().count() >= 4 && haystack.contains(needle)
}

pub fn normalize_event_card_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub fn narrative_event_card_richness_score(cards: &[NarrativeEventCard]) -> usize {
    cards
        .iter()
        .map(|card| {
            1usize
                + usize::from(
                    card.timeframe
                        .as_deref()
                        .is_some_and(|text| text.trim().chars().count() >= 2),
                )
                + usize::from(
                    card.actors
                        .iter()
                        .any(|actor| actor.trim().chars().count() >= 2),
                )
                + usize::from(
                    card.region_or_front
                        .as_deref()
                        .is_some_and(|text| text.trim().chars().count() >= 2),
                )
                + usize::from(
                    card.trigger
                        .as_deref()
                        .is_some_and(|text| text.trim().chars().count() >= 8),
                )
                + usize::from(
                    card.development
                        .as_deref()
                        .is_some_and(|text| text.trim().chars().count() >= 24),
                )
                + usize::from(
                    card.outcome
                        .as_deref()
                        .is_some_and(|text| text.trim().chars().count() >= 12),
                )
        })
        .sum()
}

pub fn merge_narrative_open_gaps(
    current: Vec<NarrativeOpenGap>,
    mut incoming: Vec<NarrativeOpenGap>,
    research_debt: &[ResearchDebtItem],
) -> Vec<NarrativeOpenGap> {
    let explicitly_closed = incoming
        .iter()
        .filter(|gap| narrative_gap_is_closed(gap))
        .map(|gap| gap.id.clone())
        .collect::<HashSet<_>>();
    let existing_ids = incoming
        .iter()
        .map(|gap| gap.id.clone())
        .collect::<HashSet<_>>();

    for gap in current {
        if narrative_gap_is_closed(&gap)
            || explicitly_closed.contains(&gap.id)
            || existing_ids.contains(&gap.id)
            || narrative_gap_converted_to_debt(&gap, research_debt)
        {
            continue;
        }
        incoming.push(gap);
    }

    incoming
}

pub fn narrative_gap_is_closed(gap: &NarrativeOpenGap) -> bool {
    gap.status
        .as_deref()
        .map(str::trim)
        .is_some_and(|status| matches!(status, "closed" | "resolved" | "converted_to_debt"))
}

pub fn narrative_gap_converted_to_debt(
    gap: &NarrativeOpenGap,
    research_debt: &[ResearchDebtItem],
) -> bool {
    research_debt.iter().any(|debt| {
        let debt_text = format!(
            "{} {} {} {}",
            debt.id,
            debt.missing_evidence,
            debt.candidate_queries.join(" "),
            debt.next_check_actions.join(" ")
        )
        .to_ascii_lowercase();
        debt.status != "closed"
            && (debt_text.contains(&gap.id.to_ascii_lowercase())
                || debt_text.contains(&gap.description.to_ascii_lowercase()))
    })
}

pub fn upsert_research_debt(current: &mut Vec<ResearchDebtItem>, incoming: ResearchDebtItem) {
    if let Some(existing) = current
        .iter_mut()
        .find(|existing| existing.id == incoming.id)
    {
        *existing = incoming;
    } else {
        current.push(incoming);
    }
}

pub fn push_unique_warning(warnings: &mut Vec<String>, warning: String) {
    if !warnings.iter().any(|existing| existing == &warning) {
        warnings.push(warning);
    }
}

pub fn close_research_debts_for_gate(
    debts: &mut [ResearchDebtItem],
    failed_gate: &str,
    active_debt_ids: Option<&HashSet<String>>,
) {
    for debt in debts.iter_mut() {
        if debt.failed_gate.as_deref() != Some(failed_gate) {
            continue;
        }
        if active_debt_ids.is_some_and(|ids| ids.contains(&debt.id)) {
            continue;
        }
        debt.status = "closed".to_string();
    }
}

pub fn sync_research_debts_for_gate(
    debts: &mut Vec<ResearchDebtItem>,
    failed_gate: &str,
    failure_messages: &[String],
    topic: Option<&str>,
) {
    let active_debt_ids = failure_messages
        .iter()
        .map(|failure| debt_id_from_failure(failure))
        .collect::<HashSet<_>>();
    close_research_debts_for_gate(debts, failed_gate, Some(&active_debt_ids));
    for failure in failure_messages {
        let debt_id = debt_id_from_failure(failure);
        upsert_research_debt(
            debts,
            ResearchDebtItem {
                id: debt_id,
                severity: "high".to_string(),
                failed_gate: Some(failed_gate.to_string()),
                missing_evidence: failure.clone(),
                required_source_class: required_source_class_from_failure(failure),
                candidate_queries: candidate_queries_from_failure(failure, topic),
                next_check_actions: if failed_gate == "artifact_quality" {
                    vec![
                        "Acquire or cite stronger evidence tied to Source Card IDs or full URLs."
                            .to_string(),
                        "Resolve the specific failed artifact gate before broad rewriting."
                            .to_string(),
                    ]
                } else {
                    vec![
                        "Repair the failed quality gate item with stronger evidence or a narrower claim."
                            .to_string(),
                    ]
                },
                status: "open".to_string(),
            },
        );
    }
}

pub fn normalize_deferred_conflicts_to_actionable_debt(
    artifacts: &mut ResearchControllerArtifacts,
) {
    for conflict_index in 0..artifacts.conflict_map.len() {
        let conflict = artifacts.conflict_map[conflict_index].clone();
        if !conflict_needs_deterministic_debt_promotion(&conflict) {
            continue;
        }

        if conflict_has_matching_actionable_open_debt(&conflict, &artifacts.research_debt) {
            artifacts.conflict_map[conflict_index].promoted_to_debt = Some(true);
            push_unique_warning(
                &mut artifacts.warnings,
                format!("conflict_auto_promoted_to_debt:{}", conflict.id),
            );
            continue;
        }

        let normalized_debt = build_actionable_conflict_debt(&conflict);
        if let Some(existing) = artifacts
            .research_debt
            .iter_mut()
            .find(|debt| debt_matches_conflict(debt, &conflict))
        {
            merge_actionable_conflict_debt(existing, &normalized_debt);
        } else {
            artifacts.research_debt.push(normalized_debt);
        }
        artifacts.conflict_map[conflict_index].promoted_to_debt = Some(true);
        push_unique_warning(
            &mut artifacts.warnings,
            format!("conflict_auto_promoted_to_debt:{}", conflict.id),
        );
    }
}

pub fn conflict_needs_deterministic_debt_promotion(conflict: &ResearchConflictMapEntry) -> bool {
    let Some(status) = conflict.resolution_status.as_deref() else {
        return false;
    };
    let normalized = status.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    normalized != "resolved"
        && normalized != "resolved_by_synthesis"
        && normalized != "resolved_by_evidence"
}

pub fn build_actionable_conflict_debt(conflict: &ResearchConflictMapEntry) -> ResearchDebtItem {
    let topic = conflict.topic.trim();
    let note = conflict
        .resolution_note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    let claim_refs = join_conflict_refs(&conflict.conflicting_claim_ids);
    let source_refs = join_conflict_refs(&conflict.source_card_ids);
    let mut missing_evidence = format!(
        "Deferred conflict {} remains unresolved for topic '{}'",
        conflict.id,
        if topic.is_empty() {
            "unspecified conflict"
        } else {
            topic
        }
    );
    if let Some(note) = note {
        missing_evidence.push_str(&format!(" ({note})"));
    }
    if !claim_refs.is_empty() {
        missing_evidence.push_str(&format!(". Claim refs: {claim_refs}"));
    }
    if !source_refs.is_empty() {
        missing_evidence.push_str(&format!(". Source card refs: {source_refs}"));
    }
    missing_evidence.push('.');

    ResearchDebtItem {
        id: conflict_debt_id(&conflict.id),
        severity: "high".to_string(),
        failed_gate: None,
        missing_evidence,
        required_source_class: None,
        candidate_queries: conflict_candidate_queries(conflict),
        next_check_actions: conflict_next_check_actions(conflict),
        status: "open".to_string(),
    }
}

pub fn merge_actionable_conflict_debt(
    existing: &mut ResearchDebtItem,
    normalized: &ResearchDebtItem,
) {
    if !existing.severity.eq_ignore_ascii_case("critical") {
        existing.severity = normalized.severity.clone();
    }
    existing.failed_gate = None;
    if existing.missing_evidence.trim().is_empty()
        || !existing
            .missing_evidence
            .to_ascii_lowercase()
            .contains(&normalized.id.to_ascii_lowercase())
    {
        existing.missing_evidence = normalized.missing_evidence.clone();
    }
    if existing.required_source_class.is_none() {
        existing.required_source_class = normalized.required_source_class.clone();
    }
    existing.candidate_queries =
        merge_conflict_debt_list(&existing.candidate_queries, &normalized.candidate_queries);
    existing.next_check_actions =
        merge_conflict_debt_list(&existing.next_check_actions, &normalized.next_check_actions);
    existing.status = "open".to_string();
}

pub fn merge_conflict_debt_list(existing: &[String], added: &[String]) -> Vec<String> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();
    for value in existing.iter().chain(added.iter()) {
        let normalized = value.trim();
        if normalized.is_empty() {
            continue;
        }
        let owned = normalized.to_string();
        if seen.insert(owned.clone()) {
            merged.push(owned);
        }
        if merged.len() >= 4 {
            break;
        }
    }
    merged
}

pub fn conflict_debt_id(conflict_id: &str) -> String {
    let compact = conflict_id
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "conflict-debt-{}",
        if compact.is_empty() {
            "deferred"
        } else {
            compact.as_str()
        }
    )
}

pub fn conflict_candidate_queries(conflict: &ResearchConflictMapEntry) -> Vec<String> {
    let mut queries = Vec::new();
    let topic = conflict.topic.trim();
    if !topic.is_empty() {
        queries.push(topic.to_string());
        queries.push(format!("{topic} official source"));
    }
    if let Some(note) = conflict
        .resolution_note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
    {
        if topic.is_empty() {
            queries.push(note.to_string());
        } else {
            queries.push(format!("{topic} {note}"));
        }
    }
    merge_conflict_debt_list(&[], &queries)
}

pub fn conflict_next_check_actions(conflict: &ResearchConflictMapEntry) -> Vec<String> {
    let topic = if conflict.topic.trim().is_empty() {
        "the deferred conflict"
    } else {
        conflict.topic.trim()
    };
    let claim_refs = if conflict.conflicting_claim_ids.is_empty() {
        "the affected claims".to_string()
    } else {
        format!(
            "claim IDs {}",
            join_conflict_refs(&conflict.conflicting_claim_ids)
        )
    };
    let source_refs = if conflict.source_card_ids.is_empty() {
        "new or stronger evidence".to_string()
    } else {
        format!(
            "source card IDs {}",
            join_conflict_refs(&conflict.source_card_ids)
        )
    };
    vec![
        format!(
            "Keep conflict {} visible as deferred debt until stronger evidence resolves '{}'.",
            conflict.id, topic
        ),
        format!(
            "Verify {claim_refs} against {source_refs}, then update conflict {} only after the evidence closes the gap.",
            conflict.id
        ),
    ]
}

pub fn join_conflict_refs(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn collect_repair_search_known_urls(
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> HashSet<String> {
    let mut known = HashSet::new();
    if let Some(source_pack) = diagnostics.and_then(|diagnostics| diagnostics.source_pack.as_ref())
    {
        for candidate in &source_pack.adopted_candidates {
            let url = candidate.url.trim();
            if !url.is_empty() {
                known.insert(url.to_string());
            }
        }
    }
    if let Some(diagnostics) = diagnostics {
        for scrape in &diagnostics.scrapes {
            for url in [
                Some(scrape.original_url.as_str()),
                Some(scrape.normalized_url.as_str()),
                scrape.final_url.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                let trimmed = url.trim();
                if !trimmed.is_empty() {
                    known.insert(trimmed.to_string());
                }
            }
        }
    }
    known
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquid_protocol::{
        ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchSourceCard,
        ResearchSourceDiagnosticsEnvelope, ResearchSourcePackReport,
    };

    #[test]
    fn trusted_artifact_merge_source_urls_rejects_relative_urls() {
        let current = ResearchControllerArtifacts {
            version: 1,
            source_cards: vec![ResearchSourceCard {
                id: "SC1".to_string(),
                url: "/relative/source-card".to_string(),
                title: "Relative".to_string(),
                source_class: "secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec![],
                limitation: None,
                diagnostics_ref: None,
                confidence: None,
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "CL1".to_string(),
                claim: "relative support".to_string(),
                claim_type: Some("fact".to_string()),
                support_source_card_ids: vec![],
                support_urls: vec!["/relative/support".to_string()],
                confidence: Some("medium".to_string()),
                uncertainty_note: None,
                needs_verification: Some(true),
            }],
            conflict_map: vec![],
            research_debt: vec![],
            narrative_state: None,
            reader_quality: None,
            quality_gate: None,
            warnings: vec![],
            events: vec![],
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("Example".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("Example".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: vec![],
                seeded_source_count: 0,
                discovered_source_count: 0,
                adopted_source_count: 1,
                adopted_candidates: vec![],
                skipped_candidates: vec![],
                coverage_misses: vec![],
                source_pack: Some("/relative/source-pack".to_string()),
            }),
            scrapes: vec![],
            context_packing: None,
        };

        let trusted = trusted_artifact_merge_source_urls(&current, Some(&diagnostics));

        assert!(trusted.is_empty());
    }
}
