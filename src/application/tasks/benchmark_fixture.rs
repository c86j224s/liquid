use super::{BenchmarkFixture, QueueLane, ResearchBenchmarkCaseInput};
use crate::application::research_prompts::normalize_research_mode;
use crate::contracts::{
    NarrativeActor, NarrativeCausalLink, NarrativeEvidenceLayer, NarrativeImpact,
    NarrativeInterpretiveTension, NarrativeOpenGap, NarrativeReaderQuestion,
    NarrativeSectionOutlineItem, NarrativeState, NarrativeTimelineEvent, NarrativeTransition,
    ResearchClaimLogEntry, ResearchConflictMapEntry, ResearchControllerArtifacts, ResearchDebtItem,
    ResearchQualityGateArtifact, ResearchSourceCandidateReport, ResearchSourceCard,
    ResearchSourcePackReport, ResearchSourceQueryReport,
};

pub(super) fn benchmark_queue_lane(model_input: &str) -> QueueLane {
    if model_input.starts_with("cli:") {
        QueueLane::Cloud
    } else {
        QueueLane::Local
    }
}

pub(super) fn benchmark_engine_kind(model_input: &str) -> &'static str {
    if model_input.starts_with("cli:") {
        "cli"
    } else if model_input.starts_with("pi:") {
        "pi_ollama"
    } else {
        "ollama_legacy"
    }
}

pub(super) fn benchmark_model_name(model_input: &str) -> &str {
    model_input
        .strip_prefix("cli:")
        .or_else(|| model_input.strip_prefix("pi:"))
        .unwrap_or(model_input)
}

pub(super) fn benchmark_research_mode(category: &str) -> &'static str {
    match category {
        "local-recommendation" => normalize_research_mode("local"),
        "technology-concept" => normalize_research_mode("technology_concept"),
        "product-decision"
        | "numeric-comparison"
        | "niche-troubleshooting"
        | "technology-decision"
        | "technology-implementation" => normalize_research_mode("technology_implementation"),
        "historical-explanation" | "historical-research" => normalize_research_mode("historical"),
        _ => normalize_research_mode("general"),
    }
}

pub(super) fn build_benchmark_fixture(input: &ResearchBenchmarkCaseInput) -> BenchmarkFixture {
    let source_report = fixture_source_pack_report(input);
    BenchmarkFixture {
        attempt_outputs: vec![
            render_benchmark_fixture_output(input, false),
            render_benchmark_fixture_output(input, true),
        ],
        source_pack_report: Some(source_report),
    }
}

fn render_benchmark_fixture_output(
    input: &ResearchBenchmarkCaseInput,
    repair_complete: bool,
) -> String {
    let urls = benchmark_fixture_urls();
    let visible_urls = if repair_complete {
        &urls[..]
    } else {
        &urls[..3]
    };
    let source_cards = visible_urls
        .iter()
        .enumerate()
        .map(|(idx, (title, url))| ResearchSourceCard {
            id: format!("S{}", idx + 1),
            url: (*url).to_string(),
            title: (*title).to_string(),
            source_class: "official_or_primary".to_string(),
            accessed_at: Some("2026-05-13".to_string()),
            extracted_facts: vec![format!(
                "{} evidence pack for {} references {}",
                input.category, input.title, input.prompt
            )],
            limitation: Some(if repair_complete {
                "fixture mode uses deterministic evidence instead of live retrieval".to_string()
            } else {
                "first pass intentionally leaves evidence thin to exercise repair".to_string()
            }),
            diagnostics_ref: Some(format!("fixture-source-{}", idx + 1)),
            confidence: Some(if repair_complete { "high" } else { "medium" }.to_string()),
        })
        .collect::<Vec<_>>();
    let claim_log = visible_urls
        .iter()
        .enumerate()
        .map(|(idx, (_, url))| ResearchClaimLogEntry {
            id: format!("C{}", idx + 1),
            claim: format!(
                "{} case claim {} keeps the prompt terms visible for controller validation: {}",
                input.category,
                idx + 1,
                input.prompt
            ),
            claim_type: Some("benchmark_fixture".to_string()),
            support_source_card_ids: vec![format!("S{}", idx + 1)],
            support_urls: vec![(*url).to_string()],
            confidence: Some(if repair_complete { "high" } else { "medium" }.to_string()),
            uncertainty_note: Some(if repair_complete {
                "live search was not performed in deterministic fixture mode".to_string()
            } else {
                "repair iteration should add broader evidence coverage".to_string()
            }),
            needs_verification: Some(!repair_complete),
        })
        .collect::<Vec<_>>();
    let research_debt = if repair_complete {
        Vec::new()
    } else {
        vec![ResearchDebtItem {
            id: "D1".to_string(),
            severity: "high".to_string(),
            failed_gate: Some("quality_gate".to_string()),
            missing_evidence:
                "source audit and claim log are intentionally under-populated in fixture iteration 1"
                    .to_string(),
            required_source_class: Some("official_or_primary".to_string()),
            candidate_queries: vec![format!("{} stronger evidence", input.category)],
            next_check_actions: vec![
                "controller should trigger a second deterministic repair pass".to_string(),
            ],
            status: "open".to_string(),
        }]
    };
    let quality_gate = ResearchQualityGateArtifact {
        status: if repair_complete {
            "passed".to_string()
        } else {
            "failed".to_string()
        },
        failure_messages: if repair_complete {
            Vec::new()
        } else {
            vec!["fixture first pass should fail strict evidence thresholds".to_string()]
        },
        unsupported_claim_count: 0,
        unresolved_conflict_count: 0,
        open_debt_count: research_debt.len(),
    };
    let artifacts = ResearchControllerArtifacts {
        version: 1,
        events: Vec::new(),
        source_cards,
        claim_log,
        conflict_map: vec![ResearchConflictMapEntry {
            id: "X1".to_string(),
            topic: format!("{} evidence coverage", input.category),
            conflicting_claim_ids: vec!["C1".to_string()],
            source_card_ids: vec!["S1".to_string()],
            resolution_status: Some(if repair_complete {
                "resolved".to_string()
            } else {
                "needs_more_evidence".to_string()
            }),
            resolution_note: Some(if repair_complete {
                "repair iteration expanded the evidence pack".to_string()
            } else {
                "repair iteration should widen evidence breadth".to_string()
            }),
            promoted_to_debt: Some(!repair_complete),
        }],
        research_debt,
        narrative_state: benchmark_fixture_narrative_state(
            input,
            repair_complete,
            visible_urls.len(),
        ),
        reader_quality: None,
        quality_gate: Some(quality_gate),
        warnings: if repair_complete {
            Vec::new()
        } else {
            vec!["fixture-first-pass".to_string()]
        },
    };
    let artifact_json =
        serde_json::to_string_pretty(&artifacts).unwrap_or_else(|_| "{}".to_string());
    let source_audit_rows = visible_urls
        .iter()
        .enumerate()
        .map(|(idx, (title, url))| {
            format!("| {url} | {title} | 주장 {}에 대한 검증 근거 |\n", idx + 1)
        })
        .collect::<String>();
    let claim_rows = visible_urls
        .iter()
        .enumerate()
        .map(|(idx, (_, url))| {
            format!(
                "| 주장 {} | {url} | {} |\n",
                idx + 1,
                if repair_complete { "높음" } else { "중간" }
            )
        })
        .collect::<String>();
    let iteration_note = if repair_complete {
        "수정 완료된 두 번째 반복으로 충분한 근거 묶음과 검증 표 행을 채운 상태"
    } else {
        "의도적으로 근거 행 수를 줄인 첫 번째 반복으로, 엄격 검증이 추가 보수를 요구하도록 만든 상태"
    };
    format!(
        "## 최종 답변 (Final Answer)\n\
이 벤치마크 케이스의 핵심은 원문 요청 `{prompt}` 를 실제 연구 컨트롤러 경로에서 처리하면서, 단계와 순서, 행위자와 기관, 원인과 배경, 한계와 불확실성, 결과와 시사점을 분리해 설명하는 데 있다. \
카테고리 `{category}` 와 제목 `{title}` 는 단순 라벨이 아니라 판단의 초점을 고정하는 조건이며, 최종 보고서는 이를 반복해서 드러내야 topic relevance 검증이 흔들리지 않는다. \
이번 출력은 {iteration_note} 를 가정한다. \
첫째, chronology 관점에서는 요구사항 정리, evidence pack 구성, 근거별 점검, final synthesis 저장의 순서를 분리해서 보여 주어야 하며, 각 단계가 왜 다음 단계의 전제인지 설명해야 한다. \
둘째, actor 관점에서는 사용자 요청, 연구 컨트롤러, 출처 팩, 검증 단계, 후속 repair pass가 서로 다른 책임을 가지므로 어느 행위자가 사실 수집을 담당하고 어느 행위자가 판단 보수를 담당하는지 분명히 써야 한다. \
셋째, cause 와 background 측면에서는 고강도 조사 모드와 strict 품질 심사가 충분한 출처 폭과 더 강한 evidence breadth 를 요구하기 때문에, 근거가 얇을 때는 recommendation을 서두르지 않고 limit, uncertain 상태, 추가 확인 필요성을 먼저 노출해야 한다. \
넷째, 결과와 implication 측면에서는 이 구조가 benchmark harness가 웹 UI 없이도 DB, task, controller, artifact, diagnostics 흐름을 끝까지 실행하는지 검증하며, 사용자에게는 어떤 결론이 즉시 행동 가능한지와 무엇이 아직 research debt 로 남는지를 함께 알려 준다. \
마지막으로 이 fixture 결과는 live web retrieval 을 대체하는 결정론적 경로이므로, 공식 자료와 해설 자료, 보조 맥락 자료의 역할 구분을 남기면서도 실제 controller loop 와 quality repair loop 자체는 그대로 통과해야 한다.\n\n\
# 검증 부록\n\
## 출처 감사 (Source Audit)\n\
| URL | Source | 확인된 주장 |\n\
| --- | --- | --- |\n\
{source_audit_rows}\n\
## 주장 로그 (Claim Log)\n\
| Claim | Source URL | Confidence |\n\
| --- | --- | --- |\n\
{claim_rows}\n\
## 품질 게이트 (Quality Gate)\n\
- 상태: {quality_status}\n\
- 메모: {quality_note}\n\n\
## Research Artifact JSON\n\
[RESEARCH_ARTIFACT_JSON]\n\
```json\n\
{artifact_json}\n\
```",
        prompt = input.prompt,
        category = input.category,
        title = input.title,
        iteration_note = iteration_note,
        quality_status = if repair_complete {
            "passed"
        } else {
            "repair_required"
        },
        quality_note = if repair_complete {
            "deterministic fixture repair iteration completed with full evidence coverage"
        } else {
            "first fixture iteration intentionally leaves strict evidence checks unsatisfied"
        },
    )
}

fn benchmark_fixture_narrative_state(
    input: &ResearchBenchmarkCaseInput,
    repair_complete: bool,
    visible_item_count: usize,
) -> Option<NarrativeState> {
    let category = input.category.to_ascii_lowercase();
    let supports_narrative = category.contains("historical")
        || category.contains("policy")
        || category.contains("comparative")
        || category.contains("product");
    if !supports_narrative || visible_item_count == 0 {
        return None;
    }

    let claim_ids = (1..=visible_item_count)
        .map(|idx| format!("C{idx}"))
        .collect::<Vec<_>>();
    let source_ids = (1..=visible_item_count)
        .map(|idx| format!("S{idx}"))
        .collect::<Vec<_>>();
    let primary_claim_ids = claim_ids.iter().take(2).cloned().collect::<Vec<_>>();
    let primary_source_ids = source_ids.iter().take(2).cloned().collect::<Vec<_>>();
    let all_claim_ids = claim_ids.clone();
    let all_source_ids = source_ids.clone();

    let profile = if category.contains("historical") {
        (
            "history",
            "Chronology-first explanation with actor and consequence coverage.",
            "Keep sequence, institutions, and contested interpretations visible before concluding.",
            "Why events unfolded in that order and what they changed.",
            "Imperial court and field actors stay distinct from later interpretation.",
            "How much of the outcome is directly supported versus inferred from later synthesis?",
            "Long-run consequence for institutions or territorial control.",
            "What remains genuinely uncertain even after corroborated chronology is laid out?",
            "chronology",
            "first pass still needs a clearer bridge between chronology and consequence coverage",
        )
    } else if category.contains("policy") {
        (
            "policy",
            "Separate binding obligations from advisory framework guidance.",
            "Walk from scope and actors to obligations, then to uncertainty, gaps, and operational consequences.",
            "What is mandatory, who the duties attach to, and where interpretation remains open.",
            "Regulators, framework stewards, and deployers must stay visibly separated.",
            "Which obligations are textually binding versus implementation guidance or interpretation?",
            "Operational consequence for deployer controls, documentation, or governance.",
            "Which interpretive points still require legal or implementation follow-up?",
            "impact",
            "first pass still needs fuller operational consequence coverage for open compliance questions",
        )
    } else {
        (
            "comparative",
            "Comparison should progress from verified specs to tradeoffs and recommendation limits.",
            "Cover verified capabilities first, then sustained-use tradeoffs, then recommendation and uncertainty.",
            "Which option fits the workflow once memory, thermals, battery, and repairability are weighed together.",
            "Vendors, upgrade paths, and workflow constraints need explicit side-by-side treatment.",
            "Which tradeoffs come from official specs versus reviewer interpretation or context-dependent usage?",
            "Decision implication for local Rust and AI workflow fit.",
            "Which constraint still needs live confirmation before treating the recommendation as settled?",
            "reader_question",
            "first pass still needs a clearer uncertainty bridge between specs and recommendation limits",
        )
    };

    Some(NarrativeState {
        version: 1,
        topic_frame: Some(format!("{} fixture narrative for {}", profile.0, input.title)),
        working_thesis: Some(profile.1.to_string()),
        reader_promise: Some(profile.2.to_string()),
        event_cards: Vec::new(),
        timeline: vec![
            NarrativeTimelineEvent {
                id: "NE1".to_string(),
                label: "요구사항 정리".to_string(),
                date_anchor: Some("iteration-setup".to_string()),
                significance: Some(
                    "Establishes the reader path before evidence tables.".to_string(),
                ),
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
            NarrativeTimelineEvent {
                id: "NE2".to_string(),
                label: "evidence pack 구성".to_string(),
                date_anchor: Some("iteration-evidence".to_string()),
                significance: Some("Anchors the explanation in supported claims.".to_string()),
                expected_claim_log_ids: all_claim_ids.clone(),
                expected_source_card_ids: all_source_ids.clone(),
            },
            NarrativeTimelineEvent {
                id: "NE3".to_string(),
                label: "final synthesis 저장".to_string(),
                date_anchor: Some("iteration-finalization".to_string()),
                significance: Some(
                    "Turns support into reader-usable explanation without hiding limits."
                        .to_string(),
                ),
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
        ],
        actors: vec![
            NarrativeActor {
                id: "NA1".to_string(),
                label: "사용자 요청".to_string(),
                role: Some("scope".to_string()),
                relevance: Some(profile.3.to_string()),
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
            NarrativeActor {
                id: "NA2".to_string(),
                label: "연구 컨트롤러".to_string(),
                role: Some("support".to_string()),
                relevance: Some(profile.4.to_string()),
                expected_claim_log_ids: all_claim_ids.clone(),
                expected_source_card_ids: all_source_ids.clone(),
            },
        ],
        causal_chain: vec![
            NarrativeCausalLink {
                id: "NC1".to_string(),
                cause: "Strict benchmark mode requires visible traceability".to_string(),
                effect: "The answer must show structure, support, and limits in order."
                    .to_string(),
                rationale: Some(
                    "Narrative continuity improves readability only when it stays tied to supported claims."
                        .to_string(),
                ),
                derived_from: None,
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
            NarrativeCausalLink {
                id: "NC2".to_string(),
                cause: "Thin first-pass evidence or unresolved interpretation remains open"
                    .to_string(),
                effect: "The final answer must expose debt or uncertainty rather than flattening it."
                    .to_string(),
                rationale: Some(
                    "Preserves the evidence boundary when structure repair cannot close support gaps."
                        .to_string(),
                ),
                derived_from: None,
                expected_claim_log_ids: all_claim_ids.clone(),
                expected_source_card_ids: all_source_ids.clone(),
            },
        ],
        evidence_layers: vec![
            NarrativeEvidenceLayer {
                id: "NL1".to_string(),
                label: "Verified facts first".to_string(),
                purpose: Some(
                    "Lead with source-backed facts before interpretation or recommendation."
                        .to_string(),
                ),
                derived_from: None,
                expected_claim_log_ids: all_claim_ids.clone(),
                expected_source_card_ids: all_source_ids.clone(),
            },
            NarrativeEvidenceLayer {
                id: "NL2".to_string(),
                label: "Interpretation and limits second".to_string(),
                purpose: Some(
                    "Move from supported comparison or chronology into uncertainty and remaining gaps."
                        .to_string(),
                ),
                derived_from: None,
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
        ],
        interpretive_tensions: vec![NarrativeInterpretiveTension {
            id: "NT1".to_string(),
            question: profile.5.to_string(),
            competing_readings: Some("Reader-friendly synthesis should stay visible, but unsupported synthesis must remain uncertainty.".to_string()),
            current_status: Some(if repair_complete {
                "framed_with_supported_limits".to_string()
            } else {
                "open".to_string()
            }),
            expected_claim_log_ids: primary_claim_ids.clone(),
            expected_source_card_ids: primary_source_ids.clone(),
        }],
        impacts: vec![NarrativeImpact {
            id: "NI1".to_string(),
            label: profile.6.to_string(),
            scope: Some("reader-facing conclusion".to_string()),
            implication: Some(
                "The final recommendation or explanation should state this consequence explicitly."
                    .to_string(),
            ),
            derived_from: None,
            expected_claim_log_ids: primary_claim_ids.clone(),
            expected_source_card_ids: primary_source_ids.clone(),
        }],
        reader_questions: vec![NarrativeReaderQuestion {
            id: "NR1".to_string(),
            question: profile.7.to_string(),
            answer_status: Some(if repair_complete {
                "answered_or_limited".to_string()
            } else {
                "open".to_string()
            }),
            answer_plan: Some(
                "Answer with supported claims or leave the gap visible in limits/debt."
                    .to_string(),
            ),
            expected_claim_log_ids: primary_claim_ids.clone(),
            expected_source_card_ids: primary_source_ids.clone(),
        }],
        section_outline: vec![
            NarrativeSectionOutlineItem {
                id: "NS1".to_string(),
                heading: "Scope and framing".to_string(),
                purpose: Some("Define what the answer is trying to resolve.".to_string()),
                derived_from: None,
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
            NarrativeSectionOutlineItem {
                id: "NS2".to_string(),
                heading: "Verified facts and evidence".to_string(),
                purpose: Some("Lay out supported facts before interpretation.".to_string()),
                derived_from: None,
                expected_claim_log_ids: all_claim_ids.clone(),
                expected_source_card_ids: all_source_ids.clone(),
            },
            NarrativeSectionOutlineItem {
                id: "NS3".to_string(),
                heading: "Interpretation, impacts, and limits".to_string(),
                purpose: Some("Close with implications and any remaining uncertainty.".to_string()),
                derived_from: None,
                expected_claim_log_ids: primary_claim_ids.clone(),
                expected_source_card_ids: primary_source_ids.clone(),
            },
        ],
        transition_plan: vec![
            NarrativeTransition {
                id: "NX1".to_string(),
                from_section_id: Some("NS1".to_string()),
                to_section_id: Some("NS2".to_string()),
                bridge: "After scope is fixed, move directly into supported facts.".to_string(),
            },
            NarrativeTransition {
                id: "NX2".to_string(),
                from_section_id: Some("NS2".to_string()),
                to_section_id: Some("NS3".to_string()),
                bridge: "Once the supported facts are visible, explain implications and any unresolved limits.".to_string(),
            },
        ],
        open_gaps: if repair_complete {
            Vec::new()
        } else {
            vec![NarrativeOpenGap {
                id: "NG1".to_string(),
                gap_type: profile.8.to_string(),
                description: profile.9.to_string(),
                status: Some("open".to_string()),
                expected_claim_log_ids: primary_claim_ids,
                expected_source_card_ids: primary_source_ids,
            }]
        },
        last_iteration_summary: Some(if repair_complete {
            "Repair iteration preserved the narrative path while closing the first-pass structural gap."
                .to_string()
        } else {
            "First pass seeded narrative continuity, but at least one visible structural gap remains."
                .to_string()
        }),
    })
}

fn fixture_source_pack_report(input: &ResearchBenchmarkCaseInput) -> ResearchSourcePackReport {
    let adopted_candidates = benchmark_fixture_urls()
        .iter()
        .map(|(title, url)| ResearchSourceCandidateReport {
            title: (*title).to_string(),
            url: (*url).to_string(),
            source_class: Some("official_or_primary".to_string()),
            source_quality: Some("high".to_string()),
            query: Some(format!("{} {}", input.category, input.title)),
            rejection_reason: None,
        })
        .collect::<Vec<_>>();
    let source_pack = adopted_candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            format!(
                "- Source {} | {} | {}\n  URL: {}",
                idx + 1,
                candidate
                    .source_class
                    .as_deref()
                    .unwrap_or("official_or_primary"),
                candidate.title,
                candidate.url
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ResearchSourcePackReport {
        subject: Some(input.prompt.clone()),
        status: "success".to_string(),
        reason: Some("deterministic fixture source pack".to_string()),
        queries: vec![ResearchSourceQueryReport {
            query: format!("fixture {}", input.category),
            status: "success".to_string(),
            provider: None,
            result_count: adopted_candidates.len(),
            adopted_count: adopted_candidates.len(),
            skipped_count: 0,
            error: None,
        }],
        seeded_source_count: adopted_candidates.len(),
        discovered_source_count: 0,
        adopted_source_count: adopted_candidates.len(),
        adopted_candidates,
        skipped_candidates: Vec::new(),
        coverage_misses: Vec::new(),
        source_pack: Some(source_pack),
    }
}

fn benchmark_fixture_urls() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Britannica: Justinian I",
            "https://www.britannica.com/biography/Justinian-I",
        ),
        (
            "Wikipedia: Gothic War",
            "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)",
        ),
        (
            "World History: Justinian I",
            "https://www.worldhistory.org/Justinian_I/",
        ),
        (
            "Britannica: Narses",
            "https://www.britannica.com/biography/Narses-Byzantine-general",
        ),
        (
            "World History: Totila",
            "https://www.worldhistory.org/Totila/",
        ),
        (
            "Britannica: Ostrogoth",
            "https://www.britannica.com/topic/Ostrogoth",
        ),
        (
            "World History: Belisarius",
            "https://www.worldhistory.org/Belisarius/",
        ),
    ]
}
