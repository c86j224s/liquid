use chrono::{DateTime, Utc};
#[allow(unused_imports)]
pub(crate) use liquid_files::{
    DocumentGraphEdge, DocumentGraphNode, DocumentLinkInfo, DocumentRelationshipGraph,
    DocumentRelationships, FileListItem, FileMetadata, ResearchSourceDocument, TagInfo,
};
#[allow(unused_imports)]
pub use liquid_protocol::{
    NarrativeActor, NarrativeCausalLink, NarrativeCausalSpineStep, NarrativeEventCard,
    NarrativeEvidenceLayer, NarrativeImpact, NarrativeInterpretiveLayer,
    NarrativeInterpretiveTension, NarrativeOpenGap, NarrativeReaderQuestion,
    NarrativeSectionOutlineItem, NarrativeState, NarrativeTimelineEvent, NarrativeTransition,
    ReaderArgumentEdge, ReaderArgumentGraph, ReaderArgumentNode, ReaderCritique,
    ReaderCritiqueMetric, ReaderNarrativePlan, ReaderQualityArtifacts, ReaderSectionBrief,
    ResearchBenchmarkCaseInput, ResearchBenchmarkCaseResult, ResearchBenchmarkMode,
    ResearchClaimLogEntry, ResearchConflictMapEntry, ResearchContextPackingDiagnostics,
    ResearchControllerArtifacts, ResearchControllerArtifactsSummary, ResearchControllerEvent,
    ResearchDebtItem, ResearchQualityGateArtifact, ResearchReplayCaseInput,
    ResearchScrapeDiagnosticsSummary, ResearchSourceCandidateReport, ResearchSourceCard,
    ResearchSourceCoverageMiss, ResearchSourceDiagnosticsEnvelope,
    ResearchSourceDiagnosticsSummary, ResearchSourcePackReport, ResearchSourceQueryReport,
    ScrapeDiagnostics, ScrapeRawCaptureDiagnostics,
};
#[allow(unused_imports)]
pub use liquid_research_artifacts::{
    PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING, PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT,
    PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION,
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF,
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING,
};
#[allow(unused_imports)]
pub(crate) use liquid_workspace::ResearchRequestInfo;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct FileTagInfo {
    pub(crate) file_id: i64,
    pub(crate) tag_id: i64,
    pub(crate) source: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub(crate) struct DrawerInfo {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) file_count: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct EnginePreset {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) engine_kind: String,
    pub(crate) provider: String,
    pub(crate) model: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) args_json: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) runtime_profile: Option<String>,
    pub(crate) default_intensity: String,
    pub(crate) web_search_enabled: String,
    pub(crate) fallback_execution: String,
    pub(crate) enabled: String,
    pub(crate) is_default: String,
    pub(crate) install_hint: Option<String>,
    pub(crate) limits_json: Option<String>,
    pub(crate) allowed_tools_json: Option<String>,
    pub(crate) allowed_skills_json: Option<String>,
    pub(crate) last_test_status: String,
    pub(crate) last_test_at: Option<DateTime<Utc>>,
    pub(crate) last_test_message: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct TaskInfo {
    pub(crate) id: i64,
    pub(crate) file_id: Option<i64>,
    pub(crate) filename: Option<String>,
    pub(crate) original_name: String,
    pub(crate) status: String,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) deleted_at: Option<DateTime<Utc>>,
    // Retry metadata
    pub(crate) model: Option<String>,
    pub(crate) system_prompt: Option<String>,
    pub(crate) user_prompt: Option<String>,
    pub(crate) source_file_ids: Option<String>, // JSON array
    pub(crate) source_filenames: Option<String>, // JSON array
    pub(crate) file_prefix: Option<String>,
    pub(crate) file_type: Option<String>,
    pub(crate) cleanup_files: Option<String>, // JSON array
    pub(crate) research_type: Option<String>,
    pub(crate) research_mode: Option<String>,
    pub(crate) research_format: Option<String>,
    pub(crate) research_topic: Option<String>,
    pub(crate) research_instructions: Option<String>,
    pub(crate) prompt_version: Option<String>,
    pub(crate) resolved_system_prompt: Option<String>,
    pub(crate) resolved_user_prompt: Option<String>,
    pub(crate) web_search_requested: Option<String>,
    pub(crate) web_search_provider: Option<String>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) engine_preset_name: Option<String>,
    pub(crate) engine_kind: Option<String>,
    pub(crate) resolved_model: Option<String>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) fallback_used: Option<String>,
    pub(crate) fallback_reason: Option<String>,
    pub(crate) quality_current_iteration: Option<i64>,
    pub(crate) quality_max_iterations: Option<i64>,
    pub(crate) quality_status: Option<String>,
    pub(crate) quality_depth: Option<String>,
    pub(crate) quality_last_failure: Option<String>,
    pub(crate) research_controller_stage: Option<String>,
    pub(crate) research_controller_iteration: Option<i64>,
    pub(crate) research_controller_max_iterations: Option<i64>,
    #[serde(skip_serializing)]
    pub(crate) research_controller_artifacts_json: Option<String>,
    #[serde(skip_serializing)]
    pub(crate) research_source_diagnostics_json: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct TaskSummary {
    pub(crate) id: i64,
    pub(crate) file_id: Option<i64>,
    pub(crate) filename: Option<String>,
    pub(crate) original_name: String,
    pub(crate) status: String,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) deleted_at: Option<DateTime<Utc>>,
    pub(crate) model: Option<String>,
    pub(crate) source_filenames: Option<String>,
    pub(crate) file_prefix: Option<String>,
    pub(crate) file_type: Option<String>,
    pub(crate) cleanup_files: Option<String>,
    pub(crate) research_type: Option<String>,
    pub(crate) research_mode: Option<String>,
    pub(crate) research_format: Option<String>,
    pub(crate) research_topic: Option<String>,
    pub(crate) prompt_version: Option<String>,
    pub(crate) web_search_requested: Option<String>,
    pub(crate) web_search_provider: Option<String>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) engine_preset_name: Option<String>,
    pub(crate) engine_kind: Option<String>,
    pub(crate) resolved_model: Option<String>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) fallback_used: Option<String>,
    pub(crate) fallback_reason: Option<String>,
    pub(crate) quality_current_iteration: Option<i64>,
    pub(crate) quality_max_iterations: Option<i64>,
    pub(crate) quality_status: Option<String>,
    pub(crate) quality_depth: Option<String>,
    pub(crate) quality_last_failure: Option<String>,
    pub(crate) research_controller_stage: Option<String>,
    pub(crate) research_controller_iteration: Option<i64>,
    pub(crate) research_controller_max_iterations: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StatusUpdate {
    pub(crate) status: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DrawerPayload {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DrawerAssignment {
    pub(crate) drawer_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EnginePresetCreatePayload {
    pub(crate) name: String,
    pub(crate) template_id: i64,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EnginePresetUpdatePayload {
    pub(crate) name: String,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TitleUpdate {
    pub(crate) title: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FileMetadataUpdate {
    pub(crate) title: String,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TagUpdate {
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScrapRequest {
    pub(crate) url: String,
    pub(crate) translate: Option<bool>,
    pub(crate) model: Option<String>,
    pub(crate) references: Option<Vec<String>>,
    pub(crate) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActionRequest {
    pub(crate) model: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MultiResearchRequest {
    pub(crate) filenames: Vec<String>,
    pub(crate) model: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) mode: String,
    pub(crate) format: Option<String>,
    pub(crate) research_type: Option<String>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) research_quality_max_iterations: Option<i64>,
    pub(crate) research_quality_depth: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TopicResearchRequest {
    pub(crate) topic: String,
    pub(crate) model: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) mode: String,
    pub(crate) format: Option<String>,
    pub(crate) research_type: Option<String>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) research_quality_max_iterations: Option<i64>,
    pub(crate) research_quality_depth: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PushRequest {
    pub(crate) title: String,
    pub(crate) content: String,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ScrapeTaskInput {
    pub(crate) url: String,
    pub(crate) references: Vec<String>,
    pub(crate) mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OllamaGenerateRequest {
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) stream: bool,
    pub(crate) system: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaGenerateResponse {
    pub(crate) response: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ModelOption {
    pub(crate) name: String,
    pub(crate) source: String,
}

#[derive(Default, Clone)]
pub(crate) struct TaskMetadata {
    pub(crate) source_file_ids: Option<String>,
    pub(crate) research_type: Option<String>,
    pub(crate) research_mode: Option<String>,
    pub(crate) research_format: Option<String>,
    pub(crate) research_topic: Option<String>,
    pub(crate) research_instructions: Option<String>,
    pub(crate) prompt_version: Option<String>,
    pub(crate) web_search_requested: Option<String>,
    pub(crate) web_search_provider: Option<String>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) engine_preset_name: Option<String>,
    pub(crate) engine_kind: Option<String>,
    pub(crate) resolved_model: Option<String>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) fallback_used: Option<String>,
    pub(crate) fallback_reason: Option<String>,
    pub(crate) quality_max_iterations: Option<i64>,
    pub(crate) quality_depth: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct RetryTaskPayload {
    pub(crate) derive_task: Option<bool>,
    pub(crate) engine_preset_id: Option<i64>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) research_quality_max_iterations: Option<i64>,
    pub(crate) research_quality_depth: Option<String>,
}

pub(crate) struct EngineResolution {
    pub(crate) model_input: String,
    pub(crate) metadata: TaskMetadata,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubUser {
    pub(crate) login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubLabel {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubIssue {
    pub(crate) title: String,
    pub(crate) state: String,
    pub(crate) user: Option<GitHubUser>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) body: Option<String>,
    pub(crate) labels: Vec<GitHubLabel>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubComment {
    pub(crate) user: Option<GitHubUser>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) body: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AppConfig {
    pub(crate) ai_workers: usize,
    pub(crate) local_ai_workers: usize,
    pub(crate) ai_task_timeout_secs: u64,
    pub(crate) cli_launch_mode: String,
    pub(crate) cli_launcher_available: bool,
}

#[cfg(test)]
mod tests {
    use super::{NarrativeState, ResearchControllerArtifacts};
    use serde_json::json;
    use std::fs;

    #[test]
    fn research_controller_artifacts_deserialize_without_narrative_state() {
        let artifacts: ResearchControllerArtifacts = serde_json::from_value(json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "quality_gate": {
                "status": "passed",
                "failure_messages": [],
                "unsupported_claim_count": 0,
                "unresolved_conflict_count": 0,
                "open_debt_count": 0
            },
            "warnings": []
        }))
        .expect("legacy artifacts should deserialize");
        assert!(artifacts.narrative_state.is_none());
    }

    #[test]
    fn narrative_state_round_trips_with_first_class_fields() {
        let artifacts: ResearchControllerArtifacts = serde_json::from_value(json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "narrative_state": {
                "version": 1,
                "topic_frame": "Late imperial administrative crisis",
                "working_thesis": "Institutional drift shaped the policy response.",
                "reader_promise": "Show the sequence, actors, causality, and consequences.",
                "event_cards": [
                    {
                        "label": "Initial decree phase",
                        "timeframe": "540s",
                        "actors": ["Imperial court", "Provincial officials"],
                        "region_or_front": "Eastern provinces",
                        "trigger": "Fiscal pressure triggered harsher enforcement",
                        "development": "Officials expanded enforcement through new administrative directives.",
                        "outcome": "Resistance hardened and policy legitimacy weakened.",
                        "source_ids": ["S1", "S2"],
                        "confidence": "medium",
                        "open_questions": ["Need clearer dating for the second directive"]
                    }
                ],
                "timeline": [
                    {
                        "id": "T1",
                        "label": "Initial decree",
                        "date_anchor": "540s",
                        "significance": "Sets the baseline",
                        "expected_claim_log_ids": ["C1"],
                        "expected_source_card_ids": ["S1"]
                    }
                ],
                "actors": [
                    {
                        "id": "A1",
                        "label": "Imperial court",
                        "role": "decision maker",
                        "relevance": "Drives the policy",
                        "expected_claim_log_ids": ["C1"],
                        "expected_source_card_ids": ["S1"]
                    }
                ],
                "causal_chain": [
                    {
                        "id": "L1",
                        "cause": "Fiscal pressure",
                        "effect": "Harsher enforcement",
                        "rationale": "Short-term revenue need",
                        "expected_claim_log_ids": ["C2"],
                        "expected_source_card_ids": ["S2"]
                    }
                ],
                "evidence_layers": [
                    {
                        "id": "E1",
                        "label": "Primary legal text first",
                        "purpose": "Anchor the chronology before interpretation",
                        "expected_claim_log_ids": ["C1", "C2"],
                        "expected_source_card_ids": ["S1", "S2"]
                    }
                ],
                "interpretive_tensions": [
                    {
                        "id": "X1",
                        "question": "Whether the reform was defensive or expansionary",
                        "competing_readings": "Modern historians disagree",
                        "current_status": "open",
                        "expected_claim_log_ids": ["C3"],
                        "expected_source_card_ids": ["S3"]
                    }
                ],
                "impacts": [
                    {
                        "id": "I1",
                        "label": "Regional administrative burden",
                        "scope": "Provincial officials",
                        "implication": "Explains downstream instability",
                        "expected_claim_log_ids": ["C4"],
                        "expected_source_card_ids": ["S4"]
                    }
                ],
                "reader_questions": [
                    {
                        "id": "Q1",
                        "question": "Why did the policy persist despite resistance?",
                        "answer_status": "partially_answered",
                        "answer_plan": "Tie actors to incentives",
                        "expected_claim_log_ids": ["C5"],
                        "expected_source_card_ids": ["S5"]
                    }
                ],
                "section_outline": [
                    {
                        "id": "SEC1",
                        "heading": "Background",
                        "purpose": "Orient the reader",
                        "expected_claim_log_ids": ["C1"],
                        "expected_source_card_ids": ["S1"]
                    }
                ],
                "transition_plan": [
                    {
                        "id": "TR1",
                        "from_section_id": "SEC1",
                        "to_section_id": "SEC2",
                        "bridge": "Move from setup into causal sequence"
                    }
                ],
                "open_gaps": [
                    {
                        "id": "G1",
                        "gap_type": "chronology",
                        "description": "Need a firmer date for the second edict",
                        "status": "open",
                        "expected_claim_log_ids": ["C6"],
                        "expected_source_card_ids": ["S6"]
                    }
                ],
                "last_iteration_summary": "Preserve the timeline and causal spine."
            },
            "quality_gate": null,
            "warnings": []
        }))
        .expect("narrative state should deserialize");

        let narrative = artifacts
            .narrative_state
            .as_ref()
            .expect("narrative state should be present");
        assert_eq!(narrative.event_cards.len(), 1);
        assert_eq!(narrative.evidence_layers.len(), 1);
        assert_eq!(narrative.interpretive_tensions.len(), 1);
        assert_eq!(narrative.impacts.len(), 1);
        assert_eq!(narrative.reader_questions.len(), 1);
        assert_eq!(narrative.open_gaps.len(), 1);

        let serialized = serde_json::to_value(&artifacts).expect("serialize artifacts");
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("event_cards"))
            .is_some());
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("evidence_layers"))
            .is_some());
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("interpretive_tensions"))
            .is_some());
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("impacts"))
            .is_some());
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("reader_questions"))
            .is_some());
        assert!(serialized
            .get("narrative_state")
            .and_then(|value| value.get("open_gaps"))
            .is_some());
    }

    #[test]
    fn narrative_state_alias_unresolved_structure_gaps_maps_to_open_gaps() {
        let state: NarrativeState = serde_json::from_value(json!({
            "version": 1,
            "unresolved_structure_gaps": [
                {
                    "id": "G1",
                    "gap_type": "actor",
                    "description": "Need clearer institution coverage"
                }
            ]
        }))
        .expect("alias should deserialize");

        assert_eq!(state.open_gaps.len(), 1);
        assert_eq!(state.open_gaps[0].gap_type, "actor");
    }

    #[test]
    fn reader_quality_round_trips_with_optional_sub_artifacts() {
        let artifacts: ResearchControllerArtifacts = serde_json::from_value(json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "reader_quality": {
                "argument_graph": {
                    "nodes": [
                        {
                            "id": "AQN1",
                            "label": "Core claim cluster",
                            "node_type": "support",
                            "rationale": "This cluster carries the main answer.",
                            "claim_log_ids": ["C1"],
                            "source_card_ids": ["S1"]
                        }
                    ],
                    "edges": [
                        {
                            "id": "AQE1",
                            "from_node_id": "AQN1",
                            "to_node_id": "AQN2",
                            "relation": "supports",
                            "rationale": "Bridge the key causal step.",
                            "claim_log_ids": ["C2"],
                            "source_card_ids": ["S2"]
                        }
                    ]
                },
                "narrative_plan": {
                    "lead_section_id": "SEC1",
                    "section_ids": ["SEC1", "SEC2"],
                    "transition_ids": ["TR1"],
                    "narrative_arc": "Background to consequence",
                    "ending_note": "Close with operational implication"
                },
                "section_briefs": [
                    {
                        "section_id": "SEC1",
                        "key_point": "Open with the contested background before resolving it.",
                        "reader_goal": "Orient the reader quickly",
                        "claim_log_ids": ["C1"],
                        "source_card_ids": ["S1"]
                    }
                ],
                "reader_critique": {
                    "summary": "The answer is clear but the middle transition is still weak.",
                    "strengths": ["Strong opening context"],
                    "weaknesses": ["Middle section jumps too quickly"],
                    "improvement_priorities": ["Tighten the transition into the evidence section"],
                    "metrics": [
                        {
                            "key": "clarity",
                            "label": "Reader clarity",
                            "status": "passed",
                            "rationale": "The opening frame is easy to follow."
                        },
                        {
                            "key": "transition",
                            "label": "Section transition",
                            "status": "needs_work",
                            "rationale": "The causal bridge is still thin."
                        }
                    ]
                }
            },
            "quality_gate": null,
            "warnings": []
        }))
        .expect("reader quality should deserialize");

        let reader_quality = artifacts
            .reader_quality
            .as_ref()
            .expect("reader quality should be present");
        assert_eq!(
            reader_quality
                .argument_graph
                .as_ref()
                .map(|graph| graph.nodes.len()),
            Some(1)
        );
        assert_eq!(reader_quality.section_briefs.len(), 1);
        assert_eq!(
            reader_quality
                .reader_critique
                .as_ref()
                .map(|critique| critique.metrics.len()),
            Some(2)
        );

        let serialized = serde_json::to_value(&artifacts).expect("serialize artifacts");
        assert!(serialized
            .get("reader_quality")
            .and_then(|value| value.get("argument_graph"))
            .is_some());
        assert!(serialized
            .get("reader_quality")
            .and_then(|value| value.get("section_briefs"))
            .is_some());
        assert!(serialized
            .get("reader_quality")
            .and_then(|value| value.get("reader_critique"))
            .is_some());
    }

    #[test]
    fn narrative_state_missing_nested_arrays_default_empty() {
        let state: NarrativeState = serde_json::from_value(json!({
            "version": 1,
            "topic_frame": "Policy explanation"
        }))
        .expect("minimal state should deserialize");

        assert!(state.event_cards.is_empty());
        assert!(state.timeline.is_empty());
        assert!(state.actors.is_empty());
        assert!(state.causal_chain.is_empty());
        assert!(state.evidence_layers.is_empty());
        assert!(state.interpretive_tensions.is_empty());
        assert!(state.impacts.is_empty());
        assert!(state.reader_questions.is_empty());
        assert!(state.section_outline.is_empty());
        assert!(state.transition_plan.is_empty());
        assert!(state.open_gaps.is_empty());
    }

    #[test]
    fn artifact_schema_docs_list_current_contract_keys() {
        let docs = fs::read_to_string("docs/experiments/research-richness/artifact-schema.md")
            .expect("artifact schema docs should be readable");

        for required in [
            "`source_cards`",
            "`claim_log`",
            "`conflict_map`",
            "`research_debt`",
            "`narrative_state`",
            "`reader_quality`",
            "`quality_gate`",
            "`narrative_state.event_cards`",
            "`reader_quality` is optional",
            "`research_source_diagnostics_json`",
            "`narrative_state_present`",
            "`reader_quality_present`",
        ] {
            assert!(
                docs.contains(required),
                "artifact schema docs should mention {required}"
            );
        }
    }

    #[test]
    fn contract_types_compile_via_legacy_and_workspace_paths() {
        let legacy: ResearchControllerArtifacts = serde_json::from_value(json!({
            "version": 1,
            "status": "running",
            "stage": "draft",
            "iteration": 1,
            "max_iterations": 2,
            "events": [],
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "quality_gate": null,
            "warnings": []
        }))
        .expect("legacy path should deserialize");

        let workspace: liquid_protocol::ResearchControllerArtifacts =
            serde_json::from_value(json!({
                "version": 1,
                "status": "running",
                "stage": "draft",
                "iteration": 1,
                "max_iterations": 2,
                "events": [],
                "source_cards": [],
                "claim_log": [],
                "conflict_map": [],
                "research_debt": [],
                "quality_gate": null,
                "warnings": []
            }))
            .expect("workspace path should deserialize");

        let _: liquid_protocol::ResearchControllerArtifacts = legacy.clone();
        let _: ResearchControllerArtifacts = workspace;
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) q: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TaskUpdateEvent {
    pub(crate) id: i64,
    pub(crate) status: String,
    pub(crate) original_name: String,
    pub(crate) quality_current_iteration: Option<i64>,
    pub(crate) quality_max_iterations: Option<i64>,
    pub(crate) quality_status: Option<String>,
    pub(crate) research_controller_stage: Option<String>,
    pub(crate) research_controller_iteration: Option<i64>,
    pub(crate) research_controller_max_iterations: Option<i64>,
}
