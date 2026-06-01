use liquid_protocol::{
    NarrativeEventCard, NarrativeOpenGap, NarrativeSectionOutlineItem, NarrativeState,
    NarrativeTimelineEvent, ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchDebtItem,
};
use liquid_research_core::{
    normalize_absolute_public_evidence_url,
    research_artifact_text_contains_unsafe_location_reference,
};
use std::collections::{HashMap, HashSet};

const MAX_ENGINE_PHASES: usize = 10;
const MAX_PHASE_TEXT_CHARS: usize = 220;
const PHASE_DEBT_GATE: &str = "historical_phase_state";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalPhasePlan {
    pub phases: Vec<HistoricalResearchPhase>,
    pub warnings: Vec<String>,
    pub debts: Vec<ResearchDebtItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalResearchPhase {
    pub id: String,
    pub source_kind: &'static str,
    pub label: String,
    pub timeframe: Option<String>,
    pub actors: Vec<String>,
    pub region_or_front: Option<String>,
    pub trigger: Option<String>,
    pub expected_development_focus: Option<String>,
    pub expected_outcome: Option<String>,
    pub required_claim_topics: Vec<String>,
    pub candidate_source_hints: Vec<String>,
    pub referenced_claim_log_ids: Vec<String>,
    pub referenced_source_ids: Vec<String>,
    pub claim_log_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub readiness: HistoricalPhaseReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalPhaseReadiness {
    pub ready: bool,
    pub reason: HistoricalPhaseReadinessReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistoricalPhaseReadinessReason {
    Ready,
    MissingSourceCards,
    MissingClaimLog,
    MissingPhaseSpecificClaim,
    UnsupportedClaim,
    BroadGenericClaimOnly,
    MissingSource,
    InsufficientPhaseAnchors,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoricalPhaseStateReport {
    pub strategy: &'static str,
    pub applicable: bool,
    pub phase_count: usize,
    pub ready_phase_count: usize,
    pub repaired_event_cards: usize,
    pub removed_placeholder_cards: usize,
    pub debt_items: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ClaimAnchorMaterial {
    tokens: HashSet<String>,
}

#[derive(Debug)]
struct PhaseEvidenceIndex<'a> {
    source_ids: HashSet<String>,
    claims: Vec<&'a ResearchClaimLogEntry>,
    supported_claim_ids: HashSet<String>,
    claim_has_known_source: HashSet<String>,
    claim_sources: HashMap<String, Vec<String>>,
    claim_material: HashMap<String, ClaimAnchorMaterial>,
}

#[derive(Debug, Clone, Default)]
struct PhaseAnchorProfile {
    label_tokens: HashSet<String>,
    timeframe_tokens: HashSet<String>,
    actor_tokens: HashSet<String>,
    region_tokens: HashSet<String>,
    trigger_tokens: HashSet<String>,
}

pub fn historical_phase_state_should_run(
    file_prefix: &str,
    research_intensity: Option<&str>,
    quality_depth: Option<&str>,
    research_topic: Option<&str>,
    research_instructions: Option<&str>,
    evidence_subject: Option<&str>,
) -> bool {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return false;
    }
    if research_intensity != Some("high") || quality_depth != Some("strict") {
        return false;
    }
    historical_subject_like(
        &[
            research_topic.unwrap_or_default(),
            research_instructions.unwrap_or_default(),
            evidence_subject.unwrap_or_default(),
        ]
        .join(" "),
    )
}

pub fn stabilize_historical_phase_state(
    artifacts: &mut ResearchControllerArtifacts,
    subject: &str,
) -> HistoricalPhaseStateReport {
    let mut report = HistoricalPhaseStateReport {
        strategy: "historical_phase_state",
        applicable: true,
        ..HistoricalPhaseStateReport::default()
    };
    let plan = build_historical_phase_plan(artifacts, subject);
    report.phase_count = plan.phases.len();
    report.ready_phase_count = plan
        .phases
        .iter()
        .filter(|phase| phase.readiness.ready)
        .count();
    report.debt_items = plan.debts.len();
    report.warnings = plan.warnings.clone();

    sync_phase_debts(&mut artifacts.research_debt, &plan.debts);

    let state = artifacts
        .narrative_state
        .get_or_insert_with(|| NarrativeState {
            version: 1,
            ..NarrativeState::default()
        });
    if state.version == 0 {
        state.version = 1;
    }

    let before = state.event_cards.len();
    state
        .event_cards
        .retain(|card| !historical_phase_label_is_placeholder(&card.label));
    report.removed_placeholder_cards = before.saturating_sub(state.event_cards.len());

    for phase in plan
        .phases
        .iter()
        .filter(|phase| phase.readiness.ready)
        .take(MAX_ENGINE_PHASES)
    {
        let draft = phase_to_event_card(phase);
        if let Some(existing) = state
            .event_cards
            .iter_mut()
            .find(|card| event_card_matches_phase(card, phase))
        {
            if merge_phase_identity(existing, &draft) {
                report.repaired_event_cards += 1;
            }
        } else {
            state.event_cards.push(draft);
            report.repaired_event_cards += 1;
        }
    }

    push_unique_string(
        &mut report.warnings,
        format!(
            "historical_phase_state:phases={} ready={} repaired={} placeholder_removed={} debt={}",
            report.phase_count,
            report.ready_phase_count,
            report.repaired_event_cards,
            report.removed_placeholder_cards,
            report.debt_items
        ),
    );
    for warning in &report.warnings {
        push_unique_string(&mut artifacts.warnings, warning.clone());
    }
    report
}

pub fn build_historical_phase_plan(
    artifacts: &ResearchControllerArtifacts,
    subject: &str,
) -> HistoricalPhasePlan {
    let evidence = PhaseEvidenceIndex::new(artifacts);
    let mut phases = candidate_phases_from_existing_cards(artifacts);
    if let Some(state) = artifacts.narrative_state.as_ref() {
        phases.extend(candidate_phases_from_section_outline(
            &state.section_outline,
        ));
        phases.extend(candidate_phases_from_timeline(&state.timeline));
        phases.extend(candidate_phases_from_open_gaps(&state.open_gaps));
    }
    phases.extend(candidate_phases_from_subject(subject));
    dedupe_phases(&mut phases);
    phases.truncate(MAX_ENGINE_PHASES);

    let mut debts = Vec::new();
    if phases.is_empty() {
        debts.push(topic_phase_debt(subject));
    }
    let mut reason_counts = HashMap::<HistoricalPhaseReadinessReason, usize>::new();
    for phase in &mut phases {
        evaluate_phase_readiness(phase, &evidence, &mut debts, subject);
        *reason_counts.entry(phase.readiness.reason).or_insert(0) += 1;
    }
    let warnings = vec![
        format!(
            "historical_phase_plan:phases={} ready={}",
            phases.len(),
            phases.iter().filter(|phase| phase.readiness.ready).count()
        ),
        phase_reason_count_warning(&reason_counts),
    ];
    HistoricalPhasePlan {
        phases,
        warnings,
        debts,
    }
}

pub fn historical_phase_label_is_placeholder(label: &str) -> bool {
    let compact = normalize_space(label).to_ascii_lowercase();
    if compact.is_empty() {
        return true;
    }
    if phase_text_is_unsafe(&compact) {
        return true;
    }
    matches!(
        compact.as_str(),
        "phase"
            | "event phase"
            | "section 1"
            | "section 2"
            | "국면"
            | "단계"
            | "사건"
            | "근거 연결 국면"
    ) || compact
        .trim_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '.' | ')' | '-' | '_'))
        .trim()
        .is_empty()
        || contains_any(
            &compact,
            &[
                "placeholder",
                "not specified",
                "unknown",
                "todo",
                "tbd",
                "추가 필요",
                "미정",
                "불명",
            ],
        )
}

pub fn historical_phase_claim_is_ready_for_label(
    artifacts: &ResearchControllerArtifacts,
    label: &str,
) -> bool {
    let evidence = PhaseEvidenceIndex::new(artifacts);
    let mut phase = phase_from_seed(
        "label_probe",
        label,
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
    );
    let mut debts = Vec::new();
    evaluate_phase_readiness(&mut phase, &evidence, &mut debts, label);
    phase.readiness.ready
}

fn evaluate_phase_readiness(
    phase: &mut HistoricalResearchPhase,
    evidence: &PhaseEvidenceIndex<'_>,
    debts: &mut Vec<ResearchDebtItem>,
    subject: &str,
) {
    if evidence.source_ids.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::MissingSourceCards);
        debts.push(phase_readiness_debt(
            phase,
            "source cards are required before engine-owned historical phase state can ground event cards",
            subject,
        ));
        return;
    }
    if evidence.claims.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::MissingClaimLog);
        debts.push(phase_readiness_debt(
            phase,
            "phase-specific supported Claim Log rows are required before engine-owned historical phase state can ground event cards",
            subject,
        ));
        return;
    }

    let profile = PhaseAnchorProfile::from_phase(phase);
    if profile.anchor_group_count() < 2 {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::InsufficientPhaseAnchors);
        debts.push(phase_readiness_debt(
            phase,
            "phase candidate lacks concrete label/time/actor/front/trigger anchors",
            subject,
        ));
        return;
    }

    let mut ready_claims = Vec::new();
    let mut ready_sources = Vec::new();
    let mut missing_source_matches = Vec::new();
    let mut broad_matches = Vec::new();
    let mut unsupported_matches = Vec::new();
    let mut insufficient_matches = Vec::new();

    for claim in &evidence.claims {
        let claim_id = claim.id.trim();
        if claim_id.is_empty() {
            continue;
        }
        let referenced = phase
            .referenced_claim_log_ids
            .iter()
            .any(|id| id.trim() == claim_id);
        let score = phase_claim_grounding_score(&profile, claim_id, evidence);
        let identity_hit = phase_claim_identity_hit(&profile, claim_id, evidence);
        if score == 0 && !referenced {
            continue;
        }
        if !evidence.supported_claim_ids.contains(claim_id) {
            unsupported_matches.push(claim_id.to_string());
            continue;
        }
        if !evidence.claim_has_known_source.contains(claim_id) {
            missing_source_matches.push(claim_id.to_string());
            continue;
        }
        if identity_hit && score >= 2 {
            ready_claims.push(claim_id.to_string());
            ready_sources.extend(
                evidence
                    .claim_sources
                    .get(claim_id)
                    .cloned()
                    .unwrap_or_default(),
            );
        } else if referenced || score > 0 {
            broad_matches.push(claim_id.to_string());
        } else {
            insufficient_matches.push(claim_id.to_string());
        }
    }

    if !ready_claims.is_empty() {
        phase.claim_log_ids = unique_nonempty(ready_claims);
        phase.source_ids = unique_nonempty(ready_sources);
        phase.readiness = HistoricalPhaseReadiness {
            ready: true,
            reason: HistoricalPhaseReadinessReason::Ready,
        };
        if let Some(missing_fields) = phase_missing_identity_fields(phase) {
            debts.push(phase_readiness_debt(
                phase,
                &format!(
                    "phase claim is ready but the engine-owned card still lacks {missing_fields}"
                ),
                subject,
            ));
        }
        return;
    }

    if !missing_source_matches.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::MissingSource);
        debts.push(phase_readiness_debt(
            phase,
            "matching claim rows exist but do not resolve to known Source Cards",
            subject,
        ));
    } else if !broad_matches.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::BroadGenericClaimOnly);
        debts.push(phase_readiness_debt(
            phase,
            "phase has only broad whole-topic claims; add a phase-specific claim naming time/place/actors/front/development/outcome",
            subject,
        ));
    } else if !unsupported_matches.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::UnsupportedClaim);
        debts.push(phase_readiness_debt(
            phase,
            "phase-like claim rows exist but lack supported public evidence references",
            subject,
        ));
    } else if !insufficient_matches.is_empty() {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::InsufficientPhaseAnchors);
        debts.push(phase_readiness_debt(
            phase,
            "matching claim rows exist but do not share enough concrete anchors with the phase candidate",
            subject,
        ));
    } else {
        phase.readiness = not_ready(HistoricalPhaseReadinessReason::MissingPhaseSpecificClaim);
        debts.push(phase_readiness_debt(
            phase,
            "no phase-specific Claim Log row exists for this phase candidate",
            subject,
        ));
    }
}

fn phase_to_event_card(phase: &HistoricalResearchPhase) -> NarrativeEventCard {
    NarrativeEventCard {
        label: phase.label.clone(),
        timeframe: phase.timeframe.clone(),
        actors: phase.actors.clone(),
        region_or_front: phase.region_or_front.clone(),
        trigger: phase.trigger.clone(),
        development: None,
        outcome: None,
        claim_log_ids: phase.claim_log_ids.clone(),
        source_ids: phase.source_ids.clone(),
        causal_spine: Vec::new(),
        interpretive_layers: Vec::new(),
        confidence: None,
        open_questions: Vec::new(),
    }
}

fn merge_phase_identity(target: &mut NarrativeEventCard, draft: &NarrativeEventCard) -> bool {
    let before = target.clone();
    if historical_phase_label_is_placeholder(&target.label)
        || target.label.chars().count() < draft.label.chars().count()
    {
        target.label = draft.label.clone();
    }
    if target
        .timeframe
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        target.timeframe = draft.timeframe.clone();
    }
    if target.actors.is_empty() {
        target.actors = draft.actors.clone();
    }
    if target
        .region_or_front
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        target.region_or_front = draft.region_or_front.clone();
    }
    if target
        .trigger
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        target.trigger = draft.trigger.clone();
    }
    merge_unique(&mut target.claim_log_ids, &draft.claim_log_ids);
    merge_unique(&mut target.source_ids, &draft.source_ids);
    before != *target
}

fn candidate_phases_from_existing_cards(
    artifacts: &ResearchControllerArtifacts,
) -> Vec<HistoricalResearchPhase> {
    artifacts
        .narrative_state
        .as_ref()
        .into_iter()
        .flat_map(|state| state.event_cards.iter().enumerate())
        .filter_map(|(index, card)| phase_from_event_card(index, card))
        .collect()
}

fn phase_from_event_card(
    index: usize,
    card: &NarrativeEventCard,
) -> Option<HistoricalResearchPhase> {
    let label = normalize_space(&card.label);
    if historical_phase_label_is_placeholder(&label) {
        return None;
    }
    let timeframe = card.timeframe.clone().map(|value| compact_text(&value, 64));
    let actors = card
        .actors
        .iter()
        .map(|actor| compact_text(actor, 48))
        .filter(|actor| !phase_text_is_unsafe(actor))
        .filter(|actor| !actor.is_empty())
        .collect::<Vec<_>>();
    let region_or_front = card
        .region_or_front
        .as_deref()
        .and_then(|value| safe_phase_text(value, 96));
    let trigger = card
        .trigger
        .as_deref()
        .and_then(|value| safe_phase_text(value, 160));
    let expected_development_focus = card
        .development
        .as_deref()
        .and_then(|value| safe_phase_text(value, MAX_PHASE_TEXT_CHARS));
    let expected_outcome = card
        .outcome
        .as_deref()
        .and_then(|value| safe_phase_text(value, 160));
    Some(phase_from_seed(
        &format!("event_card_{index}"),
        &label,
        timeframe,
        actors,
        region_or_front,
        trigger,
        expected_development_focus,
        expected_outcome,
        card.claim_log_ids.clone(),
        card.source_ids.clone(),
    ))
}

fn candidate_phases_from_section_outline(
    items: &[NarrativeSectionOutlineItem],
) -> Vec<HistoricalResearchPhase> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let label = normalize_space(&item.heading);
            if historical_phase_label_is_placeholder(&label) || label.is_empty() {
                return None;
            }
            let purpose = item
                .purpose
                .as_deref()
                .map(|value| compact_text(value, 160));
            Some(phase_from_seed(
                &format!("section_outline_{index}"),
                &label,
                extract_timeframe(item.heading.as_str(), item.purpose.as_deref()),
                Vec::new(),
                extract_region(item.heading.as_str(), item.purpose.as_deref()),
                purpose.clone(),
                purpose,
                None,
                item.expected_claim_log_ids.clone(),
                item.expected_source_card_ids.clone(),
            ))
        })
        .collect()
}

fn candidate_phases_from_timeline(
    items: &[NarrativeTimelineEvent],
) -> Vec<HistoricalResearchPhase> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let label = normalize_space(&item.label);
            if historical_phase_label_is_placeholder(&label) || label.is_empty() {
                return None;
            }
            let significance = item
                .significance
                .as_deref()
                .map(|value| compact_text(value, 160));
            Some(phase_from_seed(
                &format!("timeline_{index}"),
                &label,
                item.date_anchor
                    .as_deref()
                    .map(|value| compact_text(value, 64)),
                Vec::new(),
                extract_region(item.label.as_str(), item.significance.as_deref()),
                significance.clone(),
                significance,
                None,
                item.expected_claim_log_ids.clone(),
                item.expected_source_card_ids.clone(),
            ))
        })
        .collect()
}

fn candidate_phases_from_open_gaps(items: &[NarrativeOpenGap]) -> Vec<HistoricalResearchPhase> {
    items
        .iter()
        .enumerate()
        .filter(|(_, gap)| open_gap_can_seed_phase(gap))
        .filter_map(|(index, gap)| {
            let label = phase_label_from_open_gap(gap);
            if label.is_empty() {
                return None;
            }
            Some(phase_from_seed(
                &format!("open_gap_{index}"),
                &label,
                extract_timeframe(gap.description.as_str(), None),
                Vec::new(),
                extract_region(gap.description.as_str(), None),
                None,
                Some(compact_text(&gap.description, 160)),
                None,
                gap.expected_claim_log_ids.clone(),
                gap.expected_source_card_ids.clone(),
            ))
        })
        .collect()
}

fn candidate_phases_from_subject(subject: &str) -> Vec<HistoricalResearchPhase> {
    if !wwi_prewar_crisis_subject(subject) {
        return Vec::new();
    }
    vec![
        phase_from_seed(
            "subject_wwi_morocco_1",
            "제1차 모로코 위기",
            Some("1905-1906년".to_string()),
            vec!["독일".to_string(), "프랑스".to_string(), "영국".to_string()],
            Some("모로코·알헤시라스".to_string()),
            Some("독일의 모로코 개입이 프랑스 영향권과 영국·프랑스 협조를 시험했다.".to_string()),
            Some("모로코 문제에서 독일의 압박과 영국·프랑스 협력이 동시에 드러난 국면".to_string()),
            Some("협상국 협조가 강화되고 독일은 외교적 고립감을 키웠다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_bosnia",
            "보스니아 병합 위기",
            Some("1908-1909년".to_string()),
            vec![
                "오스트리아-헝가리".to_string(),
                "세르비아".to_string(),
                "러시아".to_string(),
            ],
            Some("보스니아·발칸".to_string()),
            Some("오스트리아-헝가리의 보스니아 병합 선언이 세르비아와 러시아의 발칸 이해를 압박했다.".to_string()),
            Some("병합 선언, 세르비아 반발, 러시아 후퇴가 다음 위기의 체면 회복 압력으로 남은 국면".to_string()),
            Some("발칸의 민족·제국 이해가 유럽 동맹 정치와 직접 결합했다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_agadir",
            "아가디르 위기",
            Some("1911년".to_string()),
            vec!["독일".to_string(), "프랑스".to_string(), "영국".to_string()],
            Some("모로코·아가디르".to_string()),
            Some("독일의 판터호 파견이 프랑스의 모로코 행동과 영국의 해상 안보 우려를 자극했다.".to_string()),
            Some("모로코 식민 문제와 해군·외교 신뢰 문제가 얽히며 독일 고립 인식이 강화된 국면".to_string()),
            Some("영국과 프랑스의 안보 협력이 더 가시화되었다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_balkan_wars",
            "발칸 전쟁",
            Some("1912-1913년".to_string()),
            vec![
                "발칸 동맹".to_string(),
                "오스만 제국".to_string(),
                "세르비아".to_string(),
                "오스트리아-헝가리".to_string(),
            ],
            Some("발칸".to_string()),
            Some("오스만 세력 후퇴와 세르비아 팽창이 발칸의 세력 균형을 흔들었다.".to_string()),
            Some("전쟁 결과가 세르비아의 자신감과 오스트리아-헝가리의 위협 인식을 함께 키운 국면".to_string()),
            Some("발칸 지역 위기가 대국 동맹의 직접 개입 위험으로 이동했다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_sarajevo",
            "사라예보 암살",
            Some("1914년 6월".to_string()),
            vec![
                "오스트리아-헝가리".to_string(),
                "세르비아".to_string(),
                "가브릴로 프린치프".to_string(),
            ],
            Some("사라예보·보스니아".to_string()),
            Some("프란츠 페르디난트 대공 암살이 오스트리아-헝가리의 세르비아 응징 논리를 열었다.".to_string()),
            Some("암살 사건이 축적된 발칸 긴장을 외교 위기에서 전쟁 결정 문제로 바꾼 국면".to_string()),
            Some("7월 위기에서 최후통첩과 동맹 계산이 전면화되었다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_july_ultimatum",
            "7월 최후통첩",
            Some("1914년 7월".to_string()),
            vec![
                "오스트리아-헝가리".to_string(),
                "세르비아".to_string(),
                "독일".to_string(),
                "러시아".to_string(),
            ],
            Some("빈·베오그라드".to_string()),
            Some("오스트리아-헝가리의 세르비아 최후통첩과 독일 지지가 외교적 후퇴 공간을 좁혔다.".to_string()),
            Some("부분 수용에도 전쟁 결정이 진행되며 위기가 국지전에서 동맹 동원 문제로 번진 국면".to_string()),
            Some("세르비아 문제는 러시아와 독일의 동원 판단으로 넘어갔다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_mobilization",
            "러시아·독일 동원 위기",
            Some("1914년 7-8월".to_string()),
            vec!["러시아".to_string(), "독일".to_string(), "프랑스".to_string()],
            Some("동유럽·서유럽".to_string()),
            Some("러시아 동원과 독일의 대응 계획이 외교 위기를 군사 일정의 문제로 전환했다.".to_string()),
            Some("동원 결정과 작전 시간표가 협상 여지를 압축한 국면".to_string()),
            Some("전쟁은 발칸 분쟁을 넘어 유럽 대륙전으로 확대되었다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
        phase_from_seed(
            "subject_wwi_belgium",
            "벨기에 침공과 영국 참전",
            Some("1914년 8월".to_string()),
            vec!["독일".to_string(), "벨기에".to_string(), "영국".to_string(), "프랑스".to_string()],
            Some("벨기에·서부전선".to_string()),
            Some("독일의 벨기에 침공이 중립 보장과 영국 안보 판단을 전쟁 참전 문제로 만들었다.".to_string()),
            Some("서부전선 작전과 벨기에 중립 문제가 영국 참전을 촉발한 국면".to_string()),
            Some("전쟁은 유럽 대국 전쟁이자 세계적 전쟁으로 확대될 조건을 갖췄다.".to_string()),
            Vec::new(),
            Vec::new(),
        ),
    ]
}

fn wwi_prewar_crisis_subject(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    let wwi = contains_any(
        &lower,
        &[
            "world war i",
            "first world war",
            "wwi",
            "great war",
            "prewar",
            "pre-war",
        ],
    ) || contains_any(subject, &["1차 세계대전", "제1차 세계대전", "세계대전"]);
    let prewar_or_crisis = contains_any(
        &lower,
        &[
            "prewar",
            "pre-war",
            "before",
            "crisis",
            "diplomacy",
            "july crisis",
        ],
    ) || contains_any(
        subject,
        &[
            "전의",
            "전쟁 전",
            "전야",
            "위기",
            "외교",
            "7월 위기",
            "발발 전",
        ],
    );
    wwi && prewar_or_crisis
}

fn phase_from_seed(
    id_seed: &str,
    label: &str,
    timeframe: Option<String>,
    actors: Vec<String>,
    region_or_front: Option<String>,
    trigger: Option<String>,
    expected_development_focus: Option<String>,
    expected_outcome: Option<String>,
    referenced_claim_log_ids: Vec<String>,
    referenced_source_ids: Vec<String>,
) -> HistoricalResearchPhase {
    let label = safe_phase_text(label, 96).unwrap_or_else(|| "근거 연결 국면".to_string());
    let timeframe = timeframe.and_then(|value| safe_phase_text(&value, 64));
    let actors = actors
        .into_iter()
        .filter_map(|actor| safe_phase_text(&actor, 48))
        .collect::<Vec<_>>();
    let region_or_front = region_or_front.and_then(|value| safe_phase_text(&value, 96));
    let trigger = trigger.and_then(|value| safe_phase_text(&value, 160));
    let expected_development_focus =
        expected_development_focus.and_then(|value| safe_phase_text(&value, MAX_PHASE_TEXT_CHARS));
    let expected_outcome = expected_outcome.and_then(|value| safe_phase_text(&value, 160));
    let token_seed = [
        label.as_str(),
        timeframe.as_deref().unwrap_or_default(),
        region_or_front.as_deref().unwrap_or_default(),
        trigger.as_deref().unwrap_or_default(),
    ]
    .join(" ");
    let mut claim_topics = anchor_tokens(&token_seed).into_iter().collect::<Vec<_>>();
    if claim_topics.is_empty() {
        claim_topics.push(id_seed.to_string());
    }
    let id = stable_phase_id(
        id_seed,
        &label,
        timeframe.as_deref(),
        region_or_front.as_deref(),
    );
    let candidate_source_hints = candidate_query_terms(
        &claim_topics,
        timeframe.as_deref(),
        region_or_front.as_deref(),
    );
    HistoricalResearchPhase {
        id,
        source_kind: phase_source_kind(id_seed),
        label,
        timeframe,
        actors,
        region_or_front,
        trigger,
        expected_development_focus,
        expected_outcome,
        required_claim_topics: claim_topics.clone(),
        candidate_source_hints,
        referenced_claim_log_ids: unique_nonempty(referenced_claim_log_ids),
        referenced_source_ids: unique_nonempty(referenced_source_ids),
        claim_log_ids: Vec::new(),
        source_ids: Vec::new(),
        readiness: not_ready(HistoricalPhaseReadinessReason::MissingPhaseSpecificClaim),
    }
}

fn phase_source_kind(id_seed: &str) -> &'static str {
    if id_seed.starts_with("event_card") {
        "event_card"
    } else if id_seed.starts_with("section_outline") {
        "section_outline"
    } else if id_seed.starts_with("timeline") {
        "timeline"
    } else if id_seed.starts_with("open_gap") {
        "open_gap"
    } else if id_seed.starts_with("subject") {
        "subject"
    } else {
        "phase"
    }
}

fn not_ready(reason: HistoricalPhaseReadinessReason) -> HistoricalPhaseReadiness {
    HistoricalPhaseReadiness {
        ready: false,
        reason,
    }
}

fn phase_missing_identity_fields(phase: &HistoricalResearchPhase) -> Option<String> {
    let mut missing = Vec::new();
    if phase
        .timeframe
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        missing.push("timeframe");
    }
    if phase.actors.is_empty() {
        missing.push("actors");
    }
    if phase
        .region_or_front
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        missing.push("region/front");
    }
    if phase
        .trigger
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        missing.push("trigger");
    }
    (!missing.is_empty()).then_some(missing.join(", "))
}

fn event_card_matches_phase(card: &NarrativeEventCard, phase: &HistoricalResearchPhase) -> bool {
    let card_key = normalized_phase_key(&phase_identity_text(
        &card.label,
        card.timeframe.as_deref(),
        Some(&card.actors),
        card.region_or_front.as_deref(),
    ));
    let phase_key = normalized_phase_key(&phase_identity_text(
        &phase.label,
        phase.timeframe.as_deref(),
        Some(&phase.actors),
        phase.region_or_front.as_deref(),
    ));
    !card_key.is_empty() && card_key == phase_key
}

fn open_gap_can_seed_phase(gap: &NarrativeOpenGap) -> bool {
    let gap_type = gap.gap_type.to_ascii_lowercase();
    let description = gap.description.to_ascii_lowercase();
    contains_any(
        &gap_type,
        &[
            "phase",
            "chronology",
            "transition",
            "event",
            "war",
            "campaign",
        ],
    ) || contains_any(
        &description,
        &[
            "phase",
            "chronology",
            "transition",
            "campaign",
            "front",
            "국면",
            "연대기",
            "전개",
            "전선",
            "전쟁",
            "혁명",
        ],
    )
}

fn phase_label_from_open_gap(gap: &NarrativeOpenGap) -> String {
    let description = normalize_space(&gap.description);
    description
        .split(['.', '。', ';', '；'])
        .next()
        .map(|part| compact_text(part, 96))
        .unwrap_or_default()
}

fn extract_timeframe(primary: &str, secondary: Option<&str>) -> Option<String> {
    let combined = [primary, secondary.unwrap_or_default()].join(" ");
    let years = combined
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .take(2)
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    match years.as_slice() {
        [] => None,
        [one] => Some(format!("{one}년")),
        [first, second] => Some(format!("{first}-{second}년")),
        _ => None,
    }
}

fn extract_region(primary: &str, secondary: Option<&str>) -> Option<String> {
    let combined = normalize_space(&[primary, secondary.unwrap_or_default()].join(" "));
    let candidates = combined
        .split([',', '/', ';'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    candidates
        .into_iter()
        .find(|segment| {
            let lower = segment.to_ascii_lowercase();
            contains_any(
                &lower,
                &[
                    "front",
                    "region",
                    "theater",
                    "paris",
                    "versailles",
                    "italy",
                    "spain",
                    "africa",
                    "europe",
                    "frontier",
                ],
            ) || contains_any(
                segment,
                &[
                    "전선",
                    "전역",
                    "지역",
                    "파리",
                    "베르사유",
                    "이탈리아",
                    "이베리아",
                    "북아프리카",
                    "유럽",
                ],
            )
        })
        .map(|segment| compact_text(segment, 96))
}

fn phase_claim_grounding_score(
    profile: &PhaseAnchorProfile,
    claim_id: &str,
    evidence: &PhaseEvidenceIndex<'_>,
) -> usize {
    let Some(material) = evidence.claim_material.get(claim_id) else {
        return 0;
    };
    let label_hit =
        !profile.label_tokens.is_empty() && !profile.label_tokens.is_disjoint(&material.tokens);
    let timeframe_hit = !profile.timeframe_tokens.is_empty()
        && !profile.timeframe_tokens.is_disjoint(&material.tokens);
    let actor_hit =
        !profile.actor_tokens.is_empty() && !profile.actor_tokens.is_disjoint(&material.tokens);
    let region_hit =
        !profile.region_tokens.is_empty() && !profile.region_tokens.is_disjoint(&material.tokens);
    let trigger_hit =
        !profile.trigger_tokens.is_empty() && !profile.trigger_tokens.is_disjoint(&material.tokens);
    [label_hit, timeframe_hit, actor_hit, region_hit, trigger_hit]
        .into_iter()
        .filter(|hit| *hit)
        .count()
}

fn phase_claim_identity_hit(
    profile: &PhaseAnchorProfile,
    claim_id: &str,
    evidence: &PhaseEvidenceIndex<'_>,
) -> bool {
    let Some(material) = evidence.claim_material.get(claim_id) else {
        return false;
    };
    (!profile.label_tokens.is_empty() && !profile.label_tokens.is_disjoint(&material.tokens))
        || (!profile.timeframe_tokens.is_empty()
            && !profile.timeframe_tokens.is_disjoint(&material.tokens))
}

impl PhaseAnchorProfile {
    fn from_phase(phase: &HistoricalResearchPhase) -> Self {
        let mut actor_tokens = HashSet::new();
        for actor in &phase.actors {
            actor_tokens.extend(anchor_tokens(actor));
        }
        Self {
            label_tokens: anchor_tokens(&phase.label),
            timeframe_tokens: phase
                .timeframe
                .as_deref()
                .map(anchor_tokens)
                .unwrap_or_default(),
            actor_tokens,
            region_tokens: phase
                .region_or_front
                .as_deref()
                .map(anchor_tokens)
                .unwrap_or_default(),
            trigger_tokens: phase
                .trigger
                .as_deref()
                .map(anchor_tokens)
                .unwrap_or_default(),
        }
    }

    fn anchor_group_count(&self) -> usize {
        [
            !self.label_tokens.is_empty(),
            !self.timeframe_tokens.is_empty(),
            !self.actor_tokens.is_empty(),
            !self.region_tokens.is_empty(),
            !self.trigger_tokens.is_empty(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}

impl<'a> PhaseEvidenceIndex<'a> {
    fn new(artifacts: &'a ResearchControllerArtifacts) -> Self {
        let sources = artifacts
            .source_cards
            .iter()
            .filter(|source| normalize_absolute_public_evidence_url(&source.url).is_some())
            .map(|source| (source.id.trim().to_string(), source))
            .filter(|(id, _)| !id.is_empty())
            .collect::<HashMap<_, _>>();
        let source_ids = sources.keys().cloned().collect::<HashSet<_>>();
        let mut supported_claim_ids = HashSet::new();
        let mut claim_has_known_source = HashSet::new();
        let mut claim_sources = HashMap::new();
        let mut claim_material = HashMap::new();
        for claim in &artifacts.claim_log {
            let claim_id = claim.id.trim();
            if claim_id.is_empty() {
                continue;
            }
            let known_source_ids = unique_nonempty(
                claim
                    .support_source_card_ids
                    .iter()
                    .filter(|id| source_ids.contains(id.trim()))
                    .map(|id| id.trim().to_string())
                    .collect(),
            );
            let has_public_support_url = claim
                .support_urls
                .iter()
                .any(|url| normalize_absolute_public_evidence_url(url).is_some());
            if !known_source_ids.is_empty() || has_public_support_url {
                supported_claim_ids.insert(claim_id.to_string());
            }
            if !known_source_ids.is_empty() {
                claim_has_known_source.insert(claim_id.to_string());
            }
            claim_sources.insert(claim_id.to_string(), known_source_ids.clone());

            claim_material.insert(
                claim_id.to_string(),
                ClaimAnchorMaterial {
                    tokens: anchor_tokens(&claim.claim),
                },
            );
        }
        Self {
            source_ids,
            claims: artifacts.claim_log.iter().collect(),
            supported_claim_ids,
            claim_has_known_source,
            claim_sources,
            claim_material,
        }
    }
}

fn candidate_query_terms(
    topics: &[String],
    timeframe: Option<&str>,
    region_or_front: Option<&str>,
) -> Vec<String> {
    let mut queries = Vec::new();
    let mut parts = topics.iter().take(4).cloned().collect::<Vec<_>>();
    if let Some(timeframe) = timeframe {
        parts.push(timeframe.to_string());
    }
    if let Some(region_or_front) = region_or_front {
        parts.push(region_or_front.to_string());
    }
    if !parts.is_empty() {
        queries.push(compact_text(&parts.join(" "), 96));
    }
    queries
}

fn stable_phase_id(
    id_seed: &str,
    label: &str,
    timeframe: Option<&str>,
    region_or_front: Option<&str>,
) -> String {
    let base = normalized_phase_key(&phase_identity_text(
        label,
        timeframe,
        None,
        region_or_front,
    ));
    if base.is_empty() {
        format!("phase-{}", compact_text(id_seed, 24))
    } else {
        format!("phase-{}", base.chars().take(48).collect::<String>())
    }
}

fn phase_identity_text(
    label: &str,
    timeframe: Option<&str>,
    actors: Option<&[String]>,
    region_or_front: Option<&str>,
) -> String {
    let mut fragments = vec![label.to_string()];
    if let Some(timeframe) = timeframe {
        fragments.push(timeframe.to_string());
    }
    if let Some(actors) = actors {
        fragments.extend(actors.iter().cloned());
    }
    if let Some(region_or_front) = region_or_front {
        fragments.push(region_or_front.to_string());
    }
    fragments.join(" ")
}

fn normalized_phase_key(value: &str) -> String {
    let mut tokens = anchor_tokens(value).into_iter().collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens.join("-")
}

fn anchor_tokens(text: &str) -> HashSet<String> {
    let mut tokens = HashSet::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ('\u{AC00}'..='\u{D7A3}').contains(&ch) {
            current.push(if ch.is_ascii() {
                ch.to_ascii_lowercase()
            } else {
                ch
            });
        } else if !current.is_empty() {
            insert_anchor_token(&mut tokens, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        insert_anchor_token(&mut tokens, &current);
    }
    tokens
}

fn insert_anchor_token(tokens: &mut HashSet<String>, token: &str) {
    let lower = token.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "phase"
            | "event"
            | "history"
            | "historical"
            | "war"
            | "campaign"
            | "crisis"
            | "process"
            | "국면"
            | "단계"
            | "사건"
            | "역사"
            | "전쟁"
            | "위기"
            | "과정"
    ) {
        return;
    }
    if lower.chars().any(|ch| ch.is_ascii_digit())
        || lower.chars().count() >= if lower.is_ascii() { 4 } else { 2 }
    {
        tokens.insert(lower);
    }
}

fn compact_text(value: &str, limit: usize) -> String {
    normalize_space(value).chars().take(limit).collect()
}

fn normalize_space(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

fn phase_text_is_unsafe(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if research_artifact_text_contains_unsafe_location_reference(&lower)
        || lower.contains("resolved prompt")
        || lower.contains("resolved_user_prompt")
        || lower.contains("resolved user prompt")
        || lower.contains("system prompt")
        || lower.contains("provider payload")
        || lower.contains("raw provider payload")
        || lower.contains("controller artifact")
        || lower.contains("controller_artifact")
        || lower.contains("source diagnostics")
        || lower.contains("raw diagnostics")
        || lower.contains("[research_artifact_json]")
        || lower.contains("<script")
    {
        return true;
    }
    false
}

fn safe_phase_text(value: &str, limit: usize) -> Option<String> {
    let compact = compact_text(value, limit);
    (!compact.is_empty() && !phase_text_is_unsafe(&compact)).then_some(compact)
}

fn historical_subject_like(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "history",
            "historical",
            "war",
            "battle",
            "campaign",
            "revolution",
            "rebellion",
            "uprising",
            "dynasty",
            "empire",
            "treaty",
            "siege",
            "diplomacy",
        ],
    ) || contains_any(
        subject,
        &[
            "역사", "전쟁", "전투", "원정", "혁명", "반란", "봉기", "제국", "왕조", "조약", "공방",
            "외교",
        ],
    )
}

fn phase_readiness_debt(
    phase: &HistoricalResearchPhase,
    missing: &str,
    subject: &str,
) -> ResearchDebtItem {
    let safe_label =
        safe_phase_text(&phase.label, 96).unwrap_or_else(|| "unnamed phase".to_string());
    ResearchDebtItem {
        id: format!(
            "historical-phase-state-{}-{}",
            phase.id,
            readiness_reason_code(phase.readiness.reason)
        ),
        severity: "high".to_string(),
        failed_gate: Some(PHASE_DEBT_GATE.to_string()),
        missing_evidence: format!("historical phase '{}': {}", safe_label, missing),
        required_source_class: None,
        candidate_queries: phase_candidate_queries(subject, phase),
        next_check_actions: vec![
            "Add or repair a phase-specific supported Claim Log row before relying on this event card."
                .to_string(),
        ],
        status: "open".to_string(),
    }
}

fn topic_phase_debt(subject: &str) -> ResearchDebtItem {
    let safe_subject = safe_phase_text(subject, 96)
        .unwrap_or_else(|| "the requested historical topic".to_string());
    ResearchDebtItem {
        id: "historical-phase-state-topic-request-missing_claim".to_string(),
        severity: "high".to_string(),
        failed_gate: Some(PHASE_DEBT_GATE.to_string()),
        missing_evidence: format!(
            "historical phase state could not derive usable phase candidates from the current narrative scaffold for '{}'",
            safe_subject
        ),
        required_source_class: None,
        candidate_queries: safe_phase_text(subject, 96).into_iter().collect(),
        next_check_actions: vec![
            "Emit timeline, section-outline, open-gap, or event-card phase anchors before enrichment."
                .to_string(),
        ],
        status: "open".to_string(),
    }
}

fn phase_candidate_queries(subject: &str, phase: &HistoricalResearchPhase) -> Vec<String> {
    let mut queries = Vec::new();
    let mut parts = Vec::new();
    if let Some(subject) = safe_phase_text(subject, 48) {
        parts.push(subject);
    }
    if let Some(label) = safe_phase_text(&phase.label, 48) {
        parts.push(label);
    }
    if let Some(timeframe) = phase.timeframe.as_deref() {
        if let Some(timeframe) = safe_phase_text(timeframe, 32) {
            parts.push(timeframe);
        }
    }
    if let Some(region) = phase.region_or_front.as_deref() {
        if let Some(region) = safe_phase_text(region, 32) {
            parts.push(region);
        }
    }
    let combined = normalize_space(&parts.join(" "));
    if !combined.is_empty() {
        queries.push(combined);
    }
    if let Some(trigger) = phase.trigger.as_deref() {
        if let Some(trigger_query) = safe_phase_text(&format!("{} {}", phase.label, trigger), 96) {
            queries.push(trigger_query);
        }
    }
    if queries.is_empty() {
        queries.extend(phase.candidate_source_hints.iter().cloned().take(2));
    }
    unique_nonempty(queries)
}

fn phase_reason_count_warning(counts: &HashMap<HistoricalPhaseReadinessReason, usize>) -> String {
    let ordered = [
        HistoricalPhaseReadinessReason::Ready,
        HistoricalPhaseReadinessReason::MissingPhaseSpecificClaim,
        HistoricalPhaseReadinessReason::UnsupportedClaim,
        HistoricalPhaseReadinessReason::BroadGenericClaimOnly,
        HistoricalPhaseReadinessReason::MissingSource,
        HistoricalPhaseReadinessReason::InsufficientPhaseAnchors,
        HistoricalPhaseReadinessReason::MissingSourceCards,
        HistoricalPhaseReadinessReason::MissingClaimLog,
    ];
    let parts = ordered
        .iter()
        .filter_map(|reason| {
            counts
                .get(reason)
                .copied()
                .filter(|count| *count > 0)
                .map(|count| format!("{}={count}", readiness_reason_code(*reason)))
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "historical_phase_state_reasons:none=0".to_string()
    } else {
        format!("historical_phase_state_reasons:{}", parts.join(","))
    }
}

fn readiness_reason_code(reason: HistoricalPhaseReadinessReason) -> &'static str {
    match reason {
        HistoricalPhaseReadinessReason::Ready => "ready",
        HistoricalPhaseReadinessReason::MissingSourceCards => "missing_source_cards",
        HistoricalPhaseReadinessReason::MissingClaimLog => "missing_claim_log",
        HistoricalPhaseReadinessReason::MissingPhaseSpecificClaim => "missing_claim",
        HistoricalPhaseReadinessReason::UnsupportedClaim => "unsupported_claim",
        HistoricalPhaseReadinessReason::BroadGenericClaimOnly => "broad_generic_claim",
        HistoricalPhaseReadinessReason::MissingSource => "missing_source",
        HistoricalPhaseReadinessReason::InsufficientPhaseAnchors => "insufficient_phase_anchors",
    }
}

fn sync_phase_debts(current: &mut Vec<ResearchDebtItem>, incoming: &[ResearchDebtItem]) {
    let active_ids = incoming
        .iter()
        .map(|debt| debt.id.clone())
        .collect::<HashSet<_>>();
    for debt in current.iter_mut() {
        if debt.failed_gate.as_deref() != Some(PHASE_DEBT_GATE) {
            continue;
        }
        if !active_ids.contains(&debt.id) {
            debt.status = "closed".to_string();
        }
    }
    for debt in incoming {
        upsert_phase_debt(current, debt.clone());
    }
}

fn upsert_phase_debt(debts: &mut Vec<ResearchDebtItem>, debt: ResearchDebtItem) {
    if let Some(existing) = debts.iter_mut().find(|existing| existing.id == debt.id) {
        *existing = debt;
    } else {
        debts.push(debt);
    }
}

fn merge_unique(target: &mut Vec<String>, incoming: &[String]) {
    let mut seen = target.iter().cloned().collect::<HashSet<_>>();
    for value in incoming {
        let value = value.trim();
        if !value.is_empty() && seen.insert(value.to_string()) {
            target.push(value.to_string());
        }
    }
}

fn unique_nonempty(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

fn dedupe_phases(phases: &mut Vec<HistoricalResearchPhase>) {
    let mut seen = HashSet::new();
    phases.retain(|phase| {
        let key = normalized_phase_key(&phase_identity_text(
            &phase.label,
            phase.timeframe.as_deref(),
            Some(&phase.actors),
            phase.region_or_front.as_deref(),
        ));
        !key.is_empty() && seen.insert(key)
    });
}

fn push_unique_string(items: &mut Vec<String>, item: String) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquid_protocol::ResearchSourceCard;

    fn source(id: &str, fact: &str) -> ResearchSourceCard {
        ResearchSourceCard {
            id: id.to_string(),
            url: format!("https://example.org/{id}"),
            title: format!("Source {id}"),
            source_class: "authoritative_secondary".to_string(),
            extracted_facts: vec![fact.to_string()],
            ..ResearchSourceCard::default()
        }
    }

    fn claim(id: &str, text: &str, source_id: &str) -> ResearchClaimLogEntry {
        ResearchClaimLogEntry {
            id: id.to_string(),
            claim: text.to_string(),
            support_source_card_ids: vec![source_id.to_string()],
            confidence: Some("medium".to_string()),
            ..ResearchClaimLogEntry::default()
        }
    }

    #[test]
    fn placeholder_phase_labels_are_rejected() {
        for label in [
            "phase",
            "event phase",
            "section 1",
            "국면",
            "1.",
            "placeholder",
        ] {
            assert!(historical_phase_label_is_placeholder(label), "{label}");
        }
        assert!(!historical_phase_label_is_placeholder("보스니아 병합 위기"));
    }

    #[test]
    fn phase_text_sanitizer_blocks_encoded_private_host_references() {
        for value in [
            "source:2130706433/latest",
            "host=0x7f000001",
            "source:0177.0.0.1",
            "source:[::ffff:127.0.0.1]/latest",
            "source:[::ffff:7f00:1]/latest",
        ] {
            assert!(phase_text_is_unsafe(value), "missed unsafe value: {value}");
        }
        assert!(!phase_text_is_unsafe("보스니아 병합 위기와 발칸 외교"));
    }

    #[test]
    fn phase_plan_prefers_existing_cards_then_fallback_scaffold_surfaces() {
        let artifacts = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![NarrativeEventCard {
                    label: "구체적 개시 국면".to_string(),
                    timeframe: Some("1788-1789".to_string()),
                    actors: vec!["Louis XVI".to_string()],
                    region_or_front: Some("Versailles".to_string()),
                    trigger: Some("Fiscal breakdown".to_string()),
                    ..NarrativeEventCard::default()
                }],
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Republican transition".to_string(),
                    purpose: Some(
                        "1792-1793 monarchy collapse and republic declaration in Paris"
                            .to_string(),
                    ),
                    ..NarrativeSectionOutlineItem::default()
                }],
                timeline: vec![NarrativeTimelineEvent {
                    id: "T1".to_string(),
                    label: "Thermidorian reaction".to_string(),
                    date_anchor: Some("1794-1795".to_string()),
                    significance: Some("Paris backlash against emergency rule".to_string()),
                    ..NarrativeTimelineEvent::default()
                }],
                open_gaps: vec![NarrativeOpenGap {
                    id: "G1".to_string(),
                    gap_type: "phase".to_string(),
                    description:
                        "European order impact phase after 1790s coalition war remains underdeveloped"
                            .to_string(),
                    ..NarrativeOpenGap::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let plan = build_historical_phase_plan(
            &artifacts,
            "French Revolution background, development, Thermidor, and European order impact",
        );
        let labels = plan
            .phases
            .iter()
            .map(|phase| phase.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels[0], "구체적 개시 국면");
        assert!(labels.contains(&"Republican transition"));
        assert!(labels.contains(&"Thermidorian reaction"));
        assert!(labels
            .iter()
            .any(|label| label.contains("European order impact")));
    }

    #[test]
    fn placeholder_event_card_label_is_not_reinserted_even_with_identity_fields() {
        let mut artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1908-1909년 보스니아 병합 위기 국면에서 오스트리아-헝가리, 세르비아, 러시아가 충돌했다.",
            )],
            claim_log: vec![claim(
                "C1",
                "1908-1909년 보스니아 병합 위기 국면에서 오스트리아-헝가리의 병합 선언은 세르비아와 러시아를 압박했다.",
                "S1",
            )],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![NarrativeEventCard {
                    label: "근거 연결 국면".to_string(),
                    timeframe: Some("1908-1909년".to_string()),
                    actors: vec!["오스트리아-헝가리".to_string(), "세르비아".to_string()],
                    region_or_front: Some("발칸".to_string()),
                    trigger: Some("병합 선언".to_string()),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: vec!["S1".to_string()],
                    ..NarrativeEventCard::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let report =
            stabilize_historical_phase_state(&mut artifacts, "1차 세계대전 전의 위기와 외교");
        let cards = &artifacts.narrative_state.as_ref().unwrap().event_cards;

        assert!(report.removed_placeholder_cards >= 1);
        assert!(
            cards.iter().all(|card| {
                card.label != "근거 연결 국면"
                    && !historical_phase_label_is_placeholder(&card.label)
            }),
            "placeholder card was reinserted: {cards:?}"
        );
        assert!(
            cards.iter().any(|card| card.label == "보스니아 병합 위기"),
            "expected grounded fallback phase card, got {cards:?}"
        );
    }

    #[test]
    fn wwi_prewar_subject_constructs_default_phase_plan() {
        let artifacts = ResearchControllerArtifacts::default();

        let plan = build_historical_phase_plan(&artifacts, "1차 세계대전 전의 위기와 외교");
        let labels = plan
            .phases
            .iter()
            .map(|phase| phase.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"제1차 모로코 위기"));
        assert!(labels.contains(&"보스니아 병합 위기"));
        assert!(labels.contains(&"아가디르 위기"));
        assert!(labels.contains(&"발칸 전쟁"));
        assert!(labels.contains(&"사라예보 암살"));
        assert!(labels.contains(&"7월 최후통첩"));
        assert!(labels.contains(&"러시아·독일 동원 위기"));
        assert!(labels.contains(&"벨기에 침공과 영국 참전"));
        assert!(plan
            .phases
            .iter()
            .all(|phase| !historical_phase_label_is_placeholder(&phase.label)));
    }

    #[test]
    fn broad_whole_topic_claim_does_not_make_concrete_phase_ready() {
        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "The French Revolution unfolded through several phases and reshaped France and Europe.",
            )],
            claim_log: vec![claim(
                "C1",
                "The French Revolution unfolded through several phases and reshaped France and Europe.",
                "S1",
            )],
            narrative_state: Some(NarrativeState {
                version: 1,
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Republican transition".to_string(),
                    purpose: Some(
                        "1792-1793 monarchy collapse and republic declaration in Paris"
                            .to_string(),
                    ),
                    expected_claim_log_ids: vec!["C1".to_string()],
                    ..NarrativeSectionOutlineItem::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let plan = build_historical_phase_plan(
            &artifacts,
            "French Revolution background, development, republic",
        );
        let phase = plan
            .phases
            .iter()
            .find(|phase| phase.label == "Republican transition")
            .unwrap();

        assert!(!phase.readiness.ready);
        assert_eq!(
            phase.readiness.reason,
            HistoricalPhaseReadinessReason::BroadGenericClaimOnly
        );
    }

    #[test]
    fn source_fact_cannot_make_broad_claim_phase_specific() {
        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1908-1909년 보스니아 병합 위기 국면에서 오스트리아-헝가리, 세르비아, 러시아가 발칸 이해관계로 충돌했다.",
            )],
            claim_log: vec![claim(
                "C1",
                "1차 세계대전 전의 연쇄 위기는 열강의 상호 불신과 동맹 경직성을 키웠다.",
                "S1",
            )],
            ..ResearchControllerArtifacts::default()
        };

        let plan = build_historical_phase_plan(&artifacts, "1차 세계대전 전의 위기와 외교");
        let phase = plan
            .phases
            .iter()
            .find(|phase| phase.label == "보스니아 병합 위기")
            .unwrap();

        assert!(
            !phase.readiness.ready,
            "broad claim should not become phase-specific via source facts: {phase:?}"
        );
        assert!(
            matches!(
                phase.readiness.reason,
                HistoricalPhaseReadinessReason::MissingPhaseSpecificClaim
                    | HistoricalPhaseReadinessReason::BroadGenericClaimOnly
            ),
            "unexpected readiness reason: {:?}",
            phase.readiness.reason
        );
        assert!(phase.claim_log_ids.is_empty());
    }

    #[test]
    fn url_only_claim_support_is_not_ready_without_known_source_cards() {
        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1792-1793 republican transition happened in Paris after monarchy collapse.",
            )],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1792-1793 republican transition in Paris abolished the monarchy and declared the republic.".to_string(),
                support_source_card_ids: Vec::new(),
                support_urls: vec!["https://example.org/public-url-only".to_string()],
                ..ResearchClaimLogEntry::default()
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Republican transition".to_string(),
                    purpose: Some(
                        "1792-1793 monarchy collapse and republic declaration in Paris"
                            .to_string(),
                    ),
                    expected_claim_log_ids: vec!["C1".to_string()],
                    ..NarrativeSectionOutlineItem::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let plan =
            build_historical_phase_plan(&artifacts, "French Revolution republican transition");
        let phase = plan
            .phases
            .iter()
            .find(|phase| phase.label == "Republican transition")
            .unwrap();

        assert!(!phase.readiness.ready);
        assert_eq!(
            phase.readiness.reason,
            HistoricalPhaseReadinessReason::MissingSource
        );
    }

    #[test]
    fn stabilization_creates_ready_skeleton_cards_without_inventing_phase_prose() {
        let mut artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1788-1789 Versailles fiscal breakdown pushed Louis XVI and the Estates-General into open conflict.",
            )],
            claim_log: vec![claim(
                "C1",
                "1788-1789 Old Regime crisis at Versailles pushed Louis XVI and the Estates-General into fiscal and representative conflict.",
                "S1",
            )],
            narrative_state: Some(NarrativeState {
                version: 1,
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Old Regime crisis".to_string(),
                    purpose: Some(
                        "1788-1789 fiscal breakdown and representative deadlock at Versailles"
                            .to_string(),
                    ),
                    expected_claim_log_ids: vec!["C1".to_string()],
                    expected_source_card_ids: vec!["S1".to_string()],
                    ..NarrativeSectionOutlineItem::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let report = stabilize_historical_phase_state(
            &mut artifacts,
            "French Revolution background and development",
        );
        let state = artifacts.narrative_state.as_ref().unwrap();
        let card = state
            .event_cards
            .iter()
            .find(|card| card.label == "Old Regime crisis")
            .unwrap();

        assert_eq!(report.ready_phase_count, 1);
        assert_eq!(card.claim_log_ids, vec!["C1"]);
        assert_eq!(card.source_ids, vec!["S1"]);
        assert!(card.development.is_none());
        assert!(card.outcome.is_none());
    }

    #[test]
    fn wwi_phase_specific_claims_create_meaningful_ready_event_cards() {
        let mut artifacts = ResearchControllerArtifacts {
            source_cards: vec![
                source(
                    "S1",
                    "1908-1909년 보스니아 병합 위기 국면에서 오스트리아-헝가리, 세르비아, 러시아가 발칸 이해관계로 충돌했다.",
                ),
                source(
                    "S2",
                    "1914년 7월 최후통첩 국면에서 오스트리아-헝가리, 세르비아, 독일, 러시아의 외교 선택지가 좁아졌다.",
                ),
            ],
            claim_log: vec![
                claim(
                    "C1",
                    "1908-1909년 보스니아 병합 위기 국면에서 오스트리아-헝가리의 병합 선언은 세르비아와 러시아의 발칸 이해관계를 압박했다.",
                    "S1",
                ),
                claim(
                    "C2",
                    "1914년 7월 최후통첩 국면에서 오스트리아-헝가리는 독일의 지지를 배경으로 세르비아에 강경 조건을 제시했고 러시아의 대응 압력을 키웠다.",
                    "S2",
                ),
            ],
            ..ResearchControllerArtifacts::default()
        };

        let report =
            stabilize_historical_phase_state(&mut artifacts, "1차 세계대전 전의 위기와 외교");
        let cards = &artifacts.narrative_state.as_ref().unwrap().event_cards;

        assert!(report.phase_count >= 8);
        assert!(report.ready_phase_count >= 2);
        assert!(
            cards.iter().any(|card| {
                card.label == "보스니아 병합 위기"
                    && card.timeframe.as_deref() == Some("1908-1909년")
                    && card.claim_log_ids == vec!["C1"]
                    && card.source_ids == vec!["S1"]
            }),
            "cards={cards:?}; report={report:?}"
        );
        assert!(cards.iter().any(|card| {
            card.label == "7월 최후통첩"
                && card.timeframe.as_deref() == Some("1914년 7월")
                && card.claim_log_ids == vec!["C2"]
                && card.source_ids == vec!["S2"]
        }));
    }

    #[test]
    fn stabilization_preserves_richer_existing_cards_while_merging_ready_phase_refs() {
        let mut artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1792-1793 republican transition in Paris abolished the monarchy and declared the republic.",
            )],
            claim_log: vec![claim(
                "C1",
                "1792-1793 republican transition in Paris abolished the monarchy and declared the republic.",
                "S1",
            )],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![NarrativeEventCard {
                    label: "Republican transition".to_string(),
                    timeframe: Some("1792-1793".to_string()),
                    actors: vec!["National Convention".to_string()],
                    region_or_front: Some("Paris".to_string()),
                    trigger: Some("War pressure and monarchy collapse".to_string()),
                    development: Some(
                        "Existing richer development should remain untouched.".to_string(),
                    ),
                    outcome: Some("Existing outcome should remain untouched.".to_string()),
                    ..NarrativeEventCard::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        let report = stabilize_historical_phase_state(
            &mut artifacts,
            "French Revolution republican transition",
        );
        let card = &artifacts.narrative_state.as_ref().unwrap().event_cards[0];

        assert_eq!(report.ready_phase_count, 1);
        assert_eq!(
            card.development.as_deref(),
            Some("Existing richer development should remain untouched.")
        );
        assert_eq!(
            card.outcome.as_deref(),
            Some("Existing outcome should remain untouched.")
        );
        assert_eq!(card.claim_log_ids, vec!["C1"]);
        assert_eq!(card.source_ids, vec!["S1"]);
    }
}
