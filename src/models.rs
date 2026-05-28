use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING: &str =
    "pi_local_source_pack_provenance_source_cards_scaffolded";
pub(crate) const PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING: &str =
    "pi_local_source_pack_claim_log_repaired_from_visible_output";
pub(crate) const PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF: &str =
    "internal:pi_local_source_pack_scaffold";
pub(crate) const PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT: &str =
    "pre-collected source-pack provenance only";
pub(crate) const PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION: &str =
    "provenance only; no model-emitted claim linkage was available";

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub(crate) struct FileMetadata {
    pub(crate) id: i64,
    pub(crate) filename: String,
    pub(crate) original_name: String,
    pub(crate) file_type: String,
    pub(crate) status: String,
    pub(crate) drawer_id: Option<i64>,
    pub(crate) uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileListItem {
    #[serde(flatten)]
    pub(crate) metadata: FileMetadata,
    pub(crate) content_preview: Option<String>,
    pub(crate) has_research_request: bool,
    pub(crate) tags: Vec<TagInfo>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct DocumentLinkInfo {
    pub(crate) id: i64,
    pub(crate) from_file_id: i64,
    pub(crate) to_file_id: i64,
    pub(crate) relation_type: String,
    pub(crate) created_by_task_id: Option<i64>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) document: ResearchSourceDocument,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct DocumentRelationships {
    pub(crate) sources: Vec<DocumentLinkInfo>,
    pub(crate) derivatives: Vec<DocumentLinkInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DocumentGraphNode {
    pub(crate) id: i64,
    pub(crate) filename: String,
    pub(crate) title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DocumentGraphEdge {
    pub(crate) id: i64,
    pub(crate) from_file_id: i64,
    pub(crate) to_file_id: i64,
    pub(crate) relation_type: String,
    pub(crate) created_by_task_id: Option<i64>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DocumentRelationshipGraph {
    pub(crate) root: DocumentGraphNode,
    pub(crate) nodes: Vec<DocumentGraphNode>,
    pub(crate) edges: Vec<DocumentGraphEdge>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct TagInfo {
    pub(crate) id: i64,
    pub(crate) label: String,
    pub(crate) slug: String,
    pub(crate) kind: String,
    pub(crate) source: String,
}

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

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
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

#[derive(Debug, Serialize)]
pub(crate) struct ResearchRequestInfo {
    pub(crate) task_id: i64,
    pub(crate) original_name: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) request_prompt: Option<String>,
    pub(crate) research_topic: Option<String>,
    pub(crate) research_instructions: Option<String>,
    pub(crate) source_filenames: Vec<String>,
    pub(crate) source_documents: Vec<ResearchSourceDocument>,
    pub(crate) relationships: DocumentRelationships,
    pub(crate) research_type: Option<String>,
    pub(crate) research_mode: Option<String>,
    pub(crate) research_format: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) engine_preset_name: Option<String>,
    pub(crate) engine_kind: Option<String>,
    pub(crate) resolved_model: Option<String>,
    pub(crate) research_intensity: Option<String>,
    pub(crate) quality_max_iterations: Option<i64>,
    pub(crate) quality_depth: Option<String>,
    pub(crate) quality_status: Option<String>,
    pub(crate) quality_last_failure: Option<String>,
    pub(crate) web_search_requested: Option<String>,
    pub(crate) web_search_provider: Option<String>,
    pub(crate) research_controller_artifacts_summary: Option<ResearchControllerArtifactsSummary>,
    pub(crate) research_source_diagnostics_summary: Option<ResearchSourceDiagnosticsSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchControllerEvent {
    pub(crate) stage: String,
    pub(crate) iteration: i64,
    pub(crate) max_iterations: i64,
    pub(crate) status: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceCard {
    pub(crate) id: String,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) source_class: String,
    pub(crate) accessed_at: Option<String>,
    #[serde(default)]
    pub(crate) extracted_facts: Vec<String>,
    pub(crate) limitation: Option<String>,
    pub(crate) diagnostics_ref: Option<String>,
    pub(crate) confidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchClaimLogEntry {
    pub(crate) id: String,
    pub(crate) claim: String,
    pub(crate) claim_type: Option<String>,
    #[serde(default)]
    pub(crate) support_source_card_ids: Vec<String>,
    #[serde(default)]
    pub(crate) support_urls: Vec<String>,
    pub(crate) confidence: Option<String>,
    pub(crate) uncertainty_note: Option<String>,
    pub(crate) needs_verification: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchConflictMapEntry {
    pub(crate) id: String,
    pub(crate) topic: String,
    #[serde(default)]
    pub(crate) conflicting_claim_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_card_ids: Vec<String>,
    pub(crate) resolution_status: Option<String>,
    pub(crate) resolution_note: Option<String>,
    pub(crate) promoted_to_debt: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchDebtItem {
    pub(crate) id: String,
    pub(crate) severity: String,
    pub(crate) failed_gate: Option<String>,
    pub(crate) missing_evidence: String,
    pub(crate) required_source_class: Option<String>,
    #[serde(default)]
    pub(crate) candidate_queries: Vec<String>,
    #[serde(default)]
    pub(crate) next_check_actions: Vec<String>,
    pub(crate) status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchQualityGateArtifact {
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) failure_messages: Vec<String>,
    pub(crate) unsupported_claim_count: usize,
    pub(crate) unresolved_conflict_count: usize,
    pub(crate) open_debt_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderArgumentNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) node_type: Option<String>,
    pub(crate) rationale: Option<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderArgumentEdge {
    pub(crate) id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_node_id: String,
    pub(crate) relation: String,
    pub(crate) rationale: Option<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderArgumentGraph {
    #[serde(default)]
    pub(crate) nodes: Vec<ReaderArgumentNode>,
    #[serde(default)]
    pub(crate) edges: Vec<ReaderArgumentEdge>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderNarrativePlan {
    pub(crate) lead_section_id: Option<String>,
    #[serde(default)]
    pub(crate) section_ids: Vec<String>,
    #[serde(default)]
    pub(crate) transition_ids: Vec<String>,
    pub(crate) narrative_arc: Option<String>,
    pub(crate) ending_note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderSectionBrief {
    pub(crate) section_id: Option<String>,
    pub(crate) key_point: String,
    pub(crate) reader_goal: Option<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderCritiqueMetric {
    pub(crate) key: String,
    pub(crate) label: String,
    #[serde(
        default = "default_reader_critique_metric_status",
        deserialize_with = "deserialize_reader_critique_metric_status"
    )]
    pub(crate) status: String,
    pub(crate) rationale: Option<String>,
}

fn default_reader_critique_metric_status() -> String {
    "unknown".to_string()
}

fn deserialize_reader_critique_metric_status<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .map(|status| status.trim().to_string())
        .filter(|status| !status.is_empty())
        .unwrap_or_else(default_reader_critique_metric_status))
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderCritique {
    pub(crate) summary: Option<String>,
    #[serde(default)]
    pub(crate) strengths: Vec<String>,
    #[serde(default)]
    pub(crate) weaknesses: Vec<String>,
    #[serde(default)]
    pub(crate) improvement_priorities: Vec<String>,
    #[serde(default)]
    pub(crate) metrics: Vec<ReaderCritiqueMetric>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReaderQualityArtifacts {
    #[serde(default)]
    pub(crate) argument_graph: Option<ReaderArgumentGraph>,
    #[serde(default)]
    pub(crate) narrative_plan: Option<ReaderNarrativePlan>,
    #[serde(default)]
    pub(crate) section_briefs: Vec<ReaderSectionBrief>,
    #[serde(default)]
    pub(crate) reader_critique: Option<ReaderCritique>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeTimelineEvent {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) date_anchor: Option<String>,
    pub(crate) significance: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeActor {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) role: Option<String>,
    pub(crate) relevance: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeCausalLink {
    pub(crate) id: String,
    pub(crate) cause: String,
    pub(crate) effect: String,
    pub(crate) rationale: Option<String>,
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeEvidenceLayer {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) purpose: Option<String>,
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeInterpretiveTension {
    pub(crate) id: String,
    pub(crate) question: String,
    pub(crate) competing_readings: Option<String>,
    pub(crate) current_status: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeImpact {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) scope: Option<String>,
    pub(crate) implication: Option<String>,
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeReaderQuestion {
    pub(crate) id: String,
    pub(crate) question: String,
    pub(crate) answer_status: Option<String>,
    pub(crate) answer_plan: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeSectionOutlineItem {
    pub(crate) id: String,
    pub(crate) heading: String,
    pub(crate) purpose: Option<String>,
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeTransition {
    pub(crate) id: String,
    pub(crate) from_section_id: Option<String>,
    pub(crate) to_section_id: Option<String>,
    pub(crate) bridge: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeOpenGap {
    pub(crate) id: String,
    pub(crate) gap_type: String,
    pub(crate) description: String,
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeCausalSpineStep {
    pub(crate) step_type: String,
    pub(crate) description: String,
    pub(crate) epistemic_status: Option<String>,
    pub(crate) reasoning: Option<String>,
    #[serde(default)]
    pub(crate) limits: Vec<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeInterpretiveLayer {
    pub(crate) layer_type: String,
    pub(crate) interpretation: String,
    pub(crate) epistemic_status: Option<String>,
    pub(crate) reasoning: Option<String>,
    #[serde(default)]
    pub(crate) limits: Vec<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeEventCard {
    pub(crate) label: String,
    pub(crate) timeframe: Option<String>,
    #[serde(default)]
    pub(crate) actors: Vec<String>,
    pub(crate) region_or_front: Option<String>,
    pub(crate) trigger: Option<String>,
    pub(crate) development: Option<String>,
    pub(crate) outcome: Option<String>,
    #[serde(default)]
    pub(crate) claim_log_ids: Vec<String>,
    #[serde(default)]
    pub(crate) source_ids: Vec<String>,
    #[serde(default)]
    pub(crate) causal_spine: Vec<NarrativeCausalSpineStep>,
    #[serde(default)]
    pub(crate) interpretive_layers: Vec<NarrativeInterpretiveLayer>,
    pub(crate) confidence: Option<String>,
    #[serde(default)]
    pub(crate) open_questions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct NarrativeState {
    pub(crate) version: u8,
    pub(crate) topic_frame: Option<String>,
    pub(crate) working_thesis: Option<String>,
    pub(crate) reader_promise: Option<String>,
    #[serde(default)]
    pub(crate) event_cards: Vec<NarrativeEventCard>,
    #[serde(default)]
    pub(crate) timeline: Vec<NarrativeTimelineEvent>,
    #[serde(default)]
    pub(crate) actors: Vec<NarrativeActor>,
    #[serde(default)]
    pub(crate) causal_chain: Vec<NarrativeCausalLink>,
    #[serde(default)]
    pub(crate) evidence_layers: Vec<NarrativeEvidenceLayer>,
    #[serde(default)]
    pub(crate) interpretive_tensions: Vec<NarrativeInterpretiveTension>,
    #[serde(default)]
    pub(crate) impacts: Vec<NarrativeImpact>,
    #[serde(default)]
    pub(crate) reader_questions: Vec<NarrativeReaderQuestion>,
    #[serde(default)]
    pub(crate) section_outline: Vec<NarrativeSectionOutlineItem>,
    #[serde(default)]
    pub(crate) transition_plan: Vec<NarrativeTransition>,
    #[serde(default, alias = "unresolved_structure_gaps")]
    pub(crate) open_gaps: Vec<NarrativeOpenGap>,
    pub(crate) last_iteration_summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchControllerArtifacts {
    pub(crate) version: u8,
    #[serde(default)]
    pub(crate) events: Vec<ResearchControllerEvent>,
    #[serde(default)]
    pub(crate) source_cards: Vec<ResearchSourceCard>,
    #[serde(default)]
    pub(crate) claim_log: Vec<ResearchClaimLogEntry>,
    #[serde(default)]
    pub(crate) conflict_map: Vec<ResearchConflictMapEntry>,
    #[serde(default)]
    pub(crate) research_debt: Vec<ResearchDebtItem>,
    pub(crate) narrative_state: Option<NarrativeState>,
    #[serde(default)]
    pub(crate) reader_quality: Option<ReaderQualityArtifacts>,
    pub(crate) quality_gate: Option<ResearchQualityGateArtifact>,
    #[serde(default)]
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceCandidateReport {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) source_class: Option<String>,
    pub(crate) source_quality: Option<String>,
    pub(crate) query: Option<String>,
    pub(crate) rejection_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceQueryReport {
    pub(crate) query: String,
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provider: Option<String>,
    pub(crate) result_count: usize,
    pub(crate) adopted_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourcePackReport {
    pub(crate) subject: Option<String>,
    pub(crate) status: String,
    pub(crate) reason: Option<String>,
    #[serde(default)]
    pub(crate) queries: Vec<ResearchSourceQueryReport>,
    pub(crate) seeded_source_count: usize,
    pub(crate) discovered_source_count: usize,
    pub(crate) adopted_source_count: usize,
    #[serde(default)]
    pub(crate) adopted_candidates: Vec<ResearchSourceCandidateReport>,
    #[serde(default)]
    pub(crate) skipped_candidates: Vec<ResearchSourceCandidateReport>,
    #[serde(default)]
    pub(crate) coverage_misses: Vec<ResearchSourceCoverageMiss>,
    #[serde(skip_serializing, default)]
    pub(crate) source_pack: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceCoverageMiss {
    pub(crate) expected_host: Option<String>,
    pub(crate) expected_source_class: Option<String>,
    pub(crate) query: String,
    pub(crate) provider: Option<String>,
    pub(crate) status: String,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScrapeRawCaptureDiagnostics {
    pub(crate) mode: String,
    pub(crate) path: Option<String>,
    pub(crate) hash: Option<String>,
    pub(crate) omitted_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScrapeDiagnostics {
    pub(crate) original_url: String,
    pub(crate) normalized_url: String,
    pub(crate) final_url: Option<String>,
    pub(crate) status_class: String,
    pub(crate) failure_reason: Option<String>,
    #[serde(default)]
    pub(crate) http_status_code: Option<u16>,
    pub(crate) extraction_strategy: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) content_type: Option<String>,
    pub(crate) raw_body_bytes: Option<usize>,
    pub(crate) raw_body_chars: Option<usize>,
    pub(crate) extracted_html_chars: usize,
    pub(crate) markdown_chars: usize,
    pub(crate) sufficiency_result: String,
    pub(crate) insufficiency_reason: Option<String>,
    #[serde(default)]
    pub(crate) reference_links: Vec<String>,
    pub(crate) accessed_at: String,
    pub(crate) raw_capture: ScrapeRawCaptureDiagnostics,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchContextPackingDiagnostics {
    pub(crate) strategy: String,
    pub(crate) included_source_card_count: usize,
    pub(crate) omitted_source_card_count: usize,
    pub(crate) included_excerpt_chars: usize,
    pub(crate) omitted_raw_chars: usize,
    pub(crate) total_raw_chars: usize,
    pub(crate) active_debt_count: usize,
    pub(crate) unresolved_conflict_count: usize,
    #[serde(default)]
    pub(crate) narrative_state_present: bool,
    #[serde(default)]
    pub(crate) narrative_timeline_event_count: usize,
    #[serde(default)]
    pub(crate) narrative_section_count: usize,
    #[serde(default)]
    pub(crate) narrative_evidence_layer_count: usize,
    #[serde(default)]
    pub(crate) narrative_interpretive_tension_count: usize,
    #[serde(default)]
    pub(crate) narrative_impact_count: usize,
    #[serde(default)]
    pub(crate) narrative_reader_question_count: usize,
    #[serde(default)]
    pub(crate) narrative_open_gap_count: usize,
    #[serde(default)]
    pub(crate) reader_quality_present: bool,
    #[serde(default)]
    pub(crate) reader_argument_node_count: usize,
    #[serde(default)]
    pub(crate) reader_argument_edge_count: usize,
    #[serde(default)]
    pub(crate) reader_narrative_plan_present: bool,
    #[serde(default)]
    pub(crate) reader_section_brief_count: usize,
    #[serde(default)]
    pub(crate) reader_critique_present: bool,
    #[serde(default)]
    pub(crate) reader_critique_metric_count: usize,
    #[serde(default)]
    pub(crate) reader_critique_failed_metric_count: usize,
    #[serde(default)]
    pub(crate) narrative_omitted_chars: usize,
    #[serde(default)]
    pub(crate) reader_quality_omitted_chars: usize,
    #[serde(default)]
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceDiagnosticsEnvelope {
    pub(crate) version: u8,
    pub(crate) subject: Option<String>,
    pub(crate) source_pack: Option<ResearchSourcePackReport>,
    #[serde(default)]
    pub(crate) scrapes: Vec<ScrapeDiagnostics>,
    pub(crate) context_packing: Option<ResearchContextPackingDiagnostics>,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchControllerArtifactsSummary {
    pub(crate) version: u8,
    pub(crate) event_count: usize,
    pub(crate) source_card_count: usize,
    pub(crate) claim_count: usize,
    pub(crate) conflict_count: usize,
    pub(crate) open_debt_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) quality_gate_status: Option<String>,
    pub(crate) quality_gate_failure_count: usize,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchScrapeDiagnosticsSummary {
    pub(crate) status_class: String,
    pub(crate) failure_reason: Option<String>,
    pub(crate) user_message: Option<String>,
    pub(crate) http_status_code: Option<u16>,
    pub(crate) sufficiency_result: String,
    pub(crate) insufficiency_reason: Option<String>,
    pub(crate) original_url_host: Option<String>,
    pub(crate) final_url_host: Option<String>,
    pub(crate) reference_link_count: usize,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResearchSourceDiagnosticsSummary {
    pub(crate) version: u8,
    pub(crate) subject: Option<String>,
    pub(crate) source_pack_status: Option<String>,
    pub(crate) source_pack_query_count: usize,
    pub(crate) source_pack_adopted_source_count: usize,
    pub(crate) source_pack_skipped_candidate_count: usize,
    pub(crate) scrape_count: usize,
    pub(crate) scrape_failure_count: usize,
    #[serde(default)]
    pub(crate) scrapes: Vec<ResearchScrapeDiagnosticsSummary>,
    pub(crate) context_packing: Option<ResearchContextPackingDiagnostics>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub(crate) struct ResearchSourceDocument {
    pub(crate) id: i64,
    pub(crate) filename: String,
    pub(crate) title: String,
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

pub(crate) fn friendly_scrape_failure_message(
    status_class: &str,
    failure_reason: Option<&str>,
    http_status_code: Option<u16>,
    insufficiency_reason: Option<&str>,
) -> Option<String> {
    let detail = failure_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let guidance = match status_class {
        "blocked" => {
            "스크랩이 차단되었습니다. 대상 사이트가 자동 수집을 막았거나 공개 인터넷에서 접근할 수 없는 주소여서 서버가 수집을 중단했습니다. 브라우저에서 직접 열리는 공개 문서인지 확인하고, 로그인이나 사내망이 필요한 페이지라면 접근 가능한 공개 링크로 다시 시도해 주세요."
        }
        "fetch_failed" => {
            "페이지를 가져오지 못했습니다. DNS 오류, 일시적인 네트워크 문제, 원격 서버 응답 실패가 원인일 수 있습니다. 잠시 후 다시 시도하거나 브라우저에서 같은 URL이 실제로 열리는지 확인해 주세요."
        }
        "redirect" => {
            "리다이렉트 처리에 실패했습니다. 중간 이동이 너무 많거나 최종 목적지 URL이 잘못되었을 수 있습니다. 단축 링크 대신 최종 문서 URL로 다시 시도해 주세요."
        }
        "insufficient_extraction" => {
            if insufficiency_reason == Some("markdown_below_minimum_threshold") {
                "페이지는 열렸지만 본문을 충분히 추출하지 못했습니다. 자바스크립트 의존 페이지, 접근 제한 페이지, 짧은 안내문일 수 있습니다. 본문이 직접 보이는 문서 링크나 PDF/원문 링크로 다시 시도해 주세요."
            } else {
                "페이지는 열렸지만 본문을 충분히 추출하지 못했습니다. 본문이 직접 보이는 공개 문서 링크로 다시 시도해 주세요."
            }
        }
        "invalid" => {
            "스크랩할 URL 형식이 올바르지 않습니다. http:// 또는 https:// 로 시작하는 공개 URL인지 확인해 주세요."
        }
        _ => {
            "스크랩 처리 중 예기치 않은 오류가 발생했습니다. 입력 URL을 다시 확인하고, 같은 문제가 반복되면 기술 세부를 함께 확인해 주세요."
        }
    };

    let mut lines = vec![guidance.to_string(), String::new()];
    if let Some(status_code) = http_status_code {
        lines.push(format!("HTTP 상태: {status_code}"));
    }
    lines.push(format!("진단 분류: {status_class}"));
    lines.push(format!("기술 세부: {detail}"));
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{friendly_scrape_failure_message, NarrativeState, ResearchControllerArtifacts};
    use serde_json::json;

    #[test]
    fn friendly_scrape_failure_message_guides_blocked_fetch_and_extraction_cases() {
        let blocked = friendly_scrape_failure_message(
            "blocked",
            Some("Blocked private or local target"),
            None,
            None,
        )
        .expect("blocked guidance");
        assert!(blocked.contains("스크랩이 차단되었습니다."));
        assert!(blocked.contains("진단 분류: blocked"));
        assert!(blocked.contains("기술 세부: Blocked private or local target"));
        assert!(!blocked.contains("HTTP 상태:"));

        let fetch_failed = friendly_scrape_failure_message(
            "fetch_failed",
            Some("Failed to fetch URL"),
            Some(403),
            None,
        )
        .expect("fetch guidance");
        assert!(fetch_failed.contains("페이지를 가져오지 못했습니다."));
        assert!(fetch_failed.contains("HTTP 상태: 403"));
        assert!(fetch_failed.contains("진단 분류: fetch_failed"));
        assert!(fetch_failed.contains("기술 세부: Failed to fetch URL"));

        let extraction = friendly_scrape_failure_message(
            "insufficient_extraction",
            Some("Extracted content too short"),
            None,
            Some("markdown_below_minimum_threshold"),
        )
        .expect("extraction guidance");
        assert!(extraction.contains("본문을 충분히 추출하지 못했습니다."));
        assert!(extraction.contains("진단 분류: insufficient_extraction"));
        assert!(extraction.contains("기술 세부: Extracted content too short"));
    }

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
