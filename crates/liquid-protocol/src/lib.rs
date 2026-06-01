use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VALID_FILE_STATUSES: &[&str] = &["draft", "published", "archived"];
pub const VALID_SCRAPE_MODES: &[&str] = &["general", "geeknews"];
pub const VALID_RESEARCH_INTENSITIES: &[&str] = &["low", "medium", "high"];
pub const VALID_RESEARCH_QUALITY_DEPTHS: &[&str] = &["light", "standard", "strict"];

pub fn is_valid_file_status(status: &str) -> bool {
    VALID_FILE_STATUSES.contains(&status)
}

pub fn is_valid_scrape_mode(mode: &str) -> bool {
    VALID_SCRAPE_MODES.contains(&mode)
}

pub fn is_valid_research_intensity(intensity: &str) -> bool {
    VALID_RESEARCH_INTENSITIES.contains(&intensity)
}

pub fn is_valid_research_quality_depth(depth: &str) -> bool {
    VALID_RESEARCH_QUALITY_DEPTHS.contains(&depth)
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone, PartialEq, Eq)]
pub struct DrawerInfo {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub file_count: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone, PartialEq, Eq)]
pub struct EnginePreset {
    pub id: i64,
    pub name: String,
    pub engine_kind: String,
    pub provider: String,
    pub model: Option<String>,
    pub command: Option<String>,
    pub args_json: Option<String>,
    pub base_url: Option<String>,
    pub runtime_profile: Option<String>,
    pub default_intensity: String,
    pub web_search_enabled: String,
    pub fallback_execution: String,
    pub enabled: String,
    pub is_default: String,
    pub install_hint: Option<String>,
    pub limits_json: Option<String>,
    pub allowed_tools_json: Option<String>,
    pub allowed_skills_json: Option<String>,
    pub last_test_status: String,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, sqlx::FromRow, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub id: i64,
    pub file_id: Option<i64>,
    pub filename: Option<String>,
    pub original_name: String,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub user_prompt: Option<String>,
    pub source_file_ids: Option<String>,
    pub source_filenames: Option<String>,
    pub file_prefix: Option<String>,
    pub file_type: Option<String>,
    pub cleanup_files: Option<String>,
    pub research_type: Option<String>,
    pub research_mode: Option<String>,
    pub research_format: Option<String>,
    pub research_topic: Option<String>,
    pub research_instructions: Option<String>,
    pub prompt_version: Option<String>,
    pub resolved_system_prompt: Option<String>,
    pub resolved_user_prompt: Option<String>,
    pub web_search_requested: Option<String>,
    pub web_search_provider: Option<String>,
    pub engine_preset_id: Option<i64>,
    pub engine_preset_name: Option<String>,
    pub engine_kind: Option<String>,
    pub resolved_model: Option<String>,
    pub research_intensity: Option<String>,
    pub fallback_used: Option<String>,
    pub fallback_reason: Option<String>,
    pub quality_current_iteration: Option<i64>,
    pub quality_max_iterations: Option<i64>,
    pub quality_status: Option<String>,
    pub quality_depth: Option<String>,
    pub quality_last_failure: Option<String>,
    pub research_controller_stage: Option<String>,
    pub research_controller_iteration: Option<i64>,
    pub research_controller_max_iterations: Option<i64>,
    #[serde(skip_serializing)]
    pub research_controller_artifacts_json: Option<String>,
    #[serde(skip_serializing)]
    pub research_source_diagnostics_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    pub id: i64,
    pub file_id: Option<i64>,
    pub filename: Option<String>,
    pub original_name: String,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub source_filenames: Option<String>,
    pub file_prefix: Option<String>,
    pub file_type: Option<String>,
    pub cleanup_files: Option<String>,
    pub research_type: Option<String>,
    pub research_mode: Option<String>,
    pub research_format: Option<String>,
    pub research_topic: Option<String>,
    pub prompt_version: Option<String>,
    pub web_search_requested: Option<String>,
    pub web_search_provider: Option<String>,
    pub engine_preset_id: Option<i64>,
    pub engine_preset_name: Option<String>,
    pub engine_kind: Option<String>,
    pub resolved_model: Option<String>,
    pub research_intensity: Option<String>,
    pub fallback_used: Option<String>,
    pub fallback_reason: Option<String>,
    pub quality_current_iteration: Option<i64>,
    pub quality_max_iterations: Option<i64>,
    pub quality_status: Option<String>,
    pub quality_depth: Option<String>,
    pub quality_last_failure: Option<String>,
    pub research_controller_stage: Option<String>,
    pub research_controller_iteration: Option<i64>,
    pub research_controller_max_iterations: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct StatusUpdate {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DrawerPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DrawerAssignment {
    pub drawer_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EnginePresetCreatePayload {
    pub name: String,
    pub template_id: i64,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EnginePresetUpdatePayload {
    pub name: String,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TitleUpdate {
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FileMetadataUpdate {
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TagUpdate {
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ScrapRequest {
    pub url: String,
    pub translate: Option<bool>,
    pub model: Option<String>,
    pub references: Option<Vec<String>>,
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ActionRequest {
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MultiResearchRequest {
    pub filenames: Vec<String>,
    pub model: Option<String>,
    pub instructions: Option<String>,
    pub mode: String,
    pub format: Option<String>,
    pub research_type: Option<String>,
    pub engine_preset_id: Option<i64>,
    pub research_intensity: Option<String>,
    pub research_quality_max_iterations: Option<i64>,
    pub research_quality_depth: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TopicResearchRequest {
    pub topic: String,
    pub model: Option<String>,
    pub instructions: Option<String>,
    pub mode: String,
    pub format: Option<String>,
    pub research_type: Option<String>,
    pub engine_preset_id: Option<i64>,
    pub research_intensity: Option<String>,
    pub research_quality_max_iterations: Option<i64>,
    pub research_quality_depth: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PushRequest {
    pub title: String,
    pub content: String,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ScrapeTaskInput {
    pub url: String,
    pub references: Vec<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct TaskMetadata {
    pub source_file_ids: Option<String>,
    pub research_type: Option<String>,
    pub research_mode: Option<String>,
    pub research_format: Option<String>,
    pub research_topic: Option<String>,
    pub research_instructions: Option<String>,
    pub prompt_version: Option<String>,
    pub web_search_requested: Option<String>,
    pub web_search_provider: Option<String>,
    pub engine_preset_id: Option<i64>,
    pub engine_preset_name: Option<String>,
    pub engine_kind: Option<String>,
    pub resolved_model: Option<String>,
    pub research_intensity: Option<String>,
    pub fallback_used: Option<String>,
    pub fallback_reason: Option<String>,
    pub quality_max_iterations: Option<i64>,
    pub quality_depth: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct RetryTaskPayload {
    pub derive_task: Option<bool>,
    pub engine_preset_id: Option<i64>,
    pub research_intensity: Option<String>,
    pub research_quality_max_iterations: Option<i64>,
    pub research_quality_depth: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub ai_workers: usize,
    pub local_ai_workers: usize,
    pub ai_task_timeout_secs: u64,
    pub cli_launch_mode: String,
    pub cli_launcher_available: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TaskUpdateEvent {
    pub id: i64,
    pub status: String,
    pub original_name: String,
    pub quality_current_iteration: Option<i64>,
    pub quality_max_iterations: Option<i64>,
    pub quality_status: Option<String>,
    pub research_controller_stage: Option<String>,
    pub research_controller_iteration: Option<i64>,
    pub research_controller_max_iterations: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchControllerEvent {
    pub stage: String,
    pub iteration: i64,
    pub max_iterations: i64,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceCard {
    pub id: String,
    pub url: String,
    pub title: String,
    pub source_class: String,
    pub accessed_at: Option<String>,
    #[serde(default)]
    pub extracted_facts: Vec<String>,
    pub limitation: Option<String>,
    pub diagnostics_ref: Option<String>,
    pub confidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchClaimLogEntry {
    pub id: String,
    pub claim: String,
    pub claim_type: Option<String>,
    #[serde(default)]
    pub support_source_card_ids: Vec<String>,
    #[serde(default)]
    pub support_urls: Vec<String>,
    pub confidence: Option<String>,
    pub uncertainty_note: Option<String>,
    pub needs_verification: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchConflictMapEntry {
    pub id: String,
    pub topic: String,
    #[serde(default)]
    pub conflicting_claim_ids: Vec<String>,
    #[serde(default)]
    pub source_card_ids: Vec<String>,
    pub resolution_status: Option<String>,
    pub resolution_note: Option<String>,
    pub promoted_to_debt: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchDebtItem {
    pub id: String,
    pub severity: String,
    pub failed_gate: Option<String>,
    pub missing_evidence: String,
    pub required_source_class: Option<String>,
    #[serde(default)]
    pub candidate_queries: Vec<String>,
    #[serde(default)]
    pub next_check_actions: Vec<String>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchQualityGateArtifact {
    pub status: String,
    #[serde(default)]
    pub failure_messages: Vec<String>,
    pub unsupported_claim_count: usize,
    pub unresolved_conflict_count: usize,
    pub open_debt_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderArgumentNode {
    pub id: String,
    pub label: String,
    pub node_type: Option<String>,
    pub rationale: Option<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderArgumentEdge {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub relation: String,
    pub rationale: Option<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderArgumentGraph {
    #[serde(default)]
    pub nodes: Vec<ReaderArgumentNode>,
    #[serde(default)]
    pub edges: Vec<ReaderArgumentEdge>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderNarrativePlan {
    pub lead_section_id: Option<String>,
    #[serde(default)]
    pub section_ids: Vec<String>,
    #[serde(default)]
    pub transition_ids: Vec<String>,
    pub narrative_arc: Option<String>,
    pub ending_note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderSectionBrief {
    pub section_id: Option<String>,
    pub key_point: String,
    pub reader_goal: Option<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderCritiqueMetric {
    pub key: String,
    pub label: String,
    #[serde(
        default = "default_reader_critique_metric_status",
        deserialize_with = "deserialize_reader_critique_metric_status"
    )]
    pub status: String,
    pub rationale: Option<String>,
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
pub struct ReaderCritique {
    pub summary: Option<String>,
    #[serde(default)]
    pub strengths: Vec<String>,
    #[serde(default)]
    pub weaknesses: Vec<String>,
    #[serde(default)]
    pub improvement_priorities: Vec<String>,
    #[serde(default)]
    pub metrics: Vec<ReaderCritiqueMetric>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ReaderQualityArtifacts {
    #[serde(default)]
    pub argument_graph: Option<ReaderArgumentGraph>,
    #[serde(default)]
    pub narrative_plan: Option<ReaderNarrativePlan>,
    #[serde(default)]
    pub section_briefs: Vec<ReaderSectionBrief>,
    #[serde(default)]
    pub reader_critique: Option<ReaderCritique>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeTimelineEvent {
    pub id: String,
    pub label: String,
    pub date_anchor: Option<String>,
    pub significance: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeActor {
    pub id: String,
    pub label: String,
    pub role: Option<String>,
    pub relevance: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeCausalLink {
    pub id: String,
    pub cause: String,
    pub effect: String,
    pub rationale: Option<String>,
    #[serde(default)]
    pub derived_from: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeEvidenceLayer {
    pub id: String,
    pub label: String,
    pub purpose: Option<String>,
    #[serde(default)]
    pub derived_from: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeInterpretiveTension {
    pub id: String,
    pub question: String,
    pub competing_readings: Option<String>,
    pub current_status: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeImpact {
    pub id: String,
    pub label: String,
    pub scope: Option<String>,
    pub implication: Option<String>,
    #[serde(default)]
    pub derived_from: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeReaderQuestion {
    pub id: String,
    pub question: String,
    pub answer_status: Option<String>,
    pub answer_plan: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeSectionOutlineItem {
    pub id: String,
    pub heading: String,
    pub purpose: Option<String>,
    #[serde(default)]
    pub derived_from: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeTransition {
    pub id: String,
    pub from_section_id: Option<String>,
    pub to_section_id: Option<String>,
    pub bridge: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeOpenGap {
    pub id: String,
    pub gap_type: String,
    pub description: String,
    pub status: Option<String>,
    #[serde(default)]
    pub expected_claim_log_ids: Vec<String>,
    #[serde(default)]
    pub expected_source_card_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeCausalSpineStep {
    pub step_type: String,
    pub description: String,
    pub epistemic_status: Option<String>,
    pub reasoning: Option<String>,
    #[serde(default)]
    pub limits: Vec<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeInterpretiveLayer {
    pub layer_type: String,
    pub interpretation: String,
    pub epistemic_status: Option<String>,
    pub reasoning: Option<String>,
    #[serde(default)]
    pub limits: Vec<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeEventCard {
    pub label: String,
    pub timeframe: Option<String>,
    #[serde(default)]
    pub actors: Vec<String>,
    pub region_or_front: Option<String>,
    pub trigger: Option<String>,
    pub development: Option<String>,
    pub outcome: Option<String>,
    #[serde(default)]
    pub claim_log_ids: Vec<String>,
    #[serde(default)]
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub causal_spine: Vec<NarrativeCausalSpineStep>,
    #[serde(default)]
    pub interpretive_layers: Vec<NarrativeInterpretiveLayer>,
    pub confidence: Option<String>,
    #[serde(default)]
    pub open_questions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NarrativeState {
    pub version: u8,
    pub topic_frame: Option<String>,
    pub working_thesis: Option<String>,
    pub reader_promise: Option<String>,
    #[serde(default)]
    pub event_cards: Vec<NarrativeEventCard>,
    #[serde(default)]
    pub timeline: Vec<NarrativeTimelineEvent>,
    #[serde(default)]
    pub actors: Vec<NarrativeActor>,
    #[serde(default)]
    pub causal_chain: Vec<NarrativeCausalLink>,
    #[serde(default)]
    pub evidence_layers: Vec<NarrativeEvidenceLayer>,
    #[serde(default)]
    pub interpretive_tensions: Vec<NarrativeInterpretiveTension>,
    #[serde(default)]
    pub impacts: Vec<NarrativeImpact>,
    #[serde(default)]
    pub reader_questions: Vec<NarrativeReaderQuestion>,
    #[serde(default)]
    pub section_outline: Vec<NarrativeSectionOutlineItem>,
    #[serde(default)]
    pub transition_plan: Vec<NarrativeTransition>,
    #[serde(default, alias = "unresolved_structure_gaps")]
    pub open_gaps: Vec<NarrativeOpenGap>,
    pub last_iteration_summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchControllerArtifacts {
    pub version: u8,
    #[serde(default)]
    pub events: Vec<ResearchControllerEvent>,
    #[serde(default)]
    pub source_cards: Vec<ResearchSourceCard>,
    #[serde(default)]
    pub claim_log: Vec<ResearchClaimLogEntry>,
    #[serde(default)]
    pub conflict_map: Vec<ResearchConflictMapEntry>,
    #[serde(default)]
    pub research_debt: Vec<ResearchDebtItem>,
    pub narrative_state: Option<NarrativeState>,
    #[serde(default)]
    pub reader_quality: Option<ReaderQualityArtifacts>,
    pub quality_gate: Option<ResearchQualityGateArtifact>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceCandidateReport {
    pub title: String,
    pub url: String,
    pub source_class: Option<String>,
    pub source_quality: Option<String>,
    pub query: Option<String>,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceQueryReport {
    pub query: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub result_count: usize,
    pub adopted_count: usize,
    pub skipped_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourcePackReport {
    pub subject: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub queries: Vec<ResearchSourceQueryReport>,
    pub seeded_source_count: usize,
    pub discovered_source_count: usize,
    pub adopted_source_count: usize,
    #[serde(default)]
    pub adopted_candidates: Vec<ResearchSourceCandidateReport>,
    #[serde(default)]
    pub skipped_candidates: Vec<ResearchSourceCandidateReport>,
    #[serde(default)]
    pub coverage_misses: Vec<ResearchSourceCoverageMiss>,
    #[serde(skip_serializing, default)]
    pub source_pack: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceCoverageMiss {
    pub expected_host: Option<String>,
    pub expected_source_class: Option<String>,
    pub query: String,
    pub provider: Option<String>,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ScrapeRawCaptureDiagnostics {
    pub mode: String,
    pub path: Option<String>,
    pub hash: Option<String>,
    pub omitted_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ScrapeDiagnostics {
    pub original_url: String,
    pub normalized_url: String,
    pub final_url: Option<String>,
    pub status_class: String,
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub http_status_code: Option<u16>,
    pub extraction_strategy: Option<String>,
    pub title: Option<String>,
    pub content_type: Option<String>,
    pub raw_body_bytes: Option<usize>,
    pub raw_body_chars: Option<usize>,
    pub extracted_html_chars: usize,
    pub markdown_chars: usize,
    pub sufficiency_result: String,
    pub insufficiency_reason: Option<String>,
    #[serde(default)]
    pub reference_links: Vec<String>,
    pub accessed_at: String,
    pub raw_capture: ScrapeRawCaptureDiagnostics,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchContextPackingDiagnostics {
    pub strategy: String,
    pub included_source_card_count: usize,
    pub omitted_source_card_count: usize,
    pub included_excerpt_chars: usize,
    pub omitted_raw_chars: usize,
    pub total_raw_chars: usize,
    pub active_debt_count: usize,
    pub unresolved_conflict_count: usize,
    #[serde(default)]
    pub narrative_state_present: bool,
    #[serde(default)]
    pub narrative_timeline_event_count: usize,
    #[serde(default)]
    pub narrative_section_count: usize,
    #[serde(default)]
    pub narrative_evidence_layer_count: usize,
    #[serde(default)]
    pub narrative_interpretive_tension_count: usize,
    #[serde(default)]
    pub narrative_impact_count: usize,
    #[serde(default)]
    pub narrative_reader_question_count: usize,
    #[serde(default)]
    pub narrative_open_gap_count: usize,
    #[serde(default)]
    pub reader_quality_present: bool,
    #[serde(default)]
    pub reader_argument_node_count: usize,
    #[serde(default)]
    pub reader_argument_edge_count: usize,
    #[serde(default)]
    pub reader_narrative_plan_present: bool,
    #[serde(default)]
    pub reader_section_brief_count: usize,
    #[serde(default)]
    pub reader_critique_present: bool,
    #[serde(default)]
    pub reader_critique_metric_count: usize,
    #[serde(default)]
    pub reader_critique_failed_metric_count: usize,
    #[serde(default)]
    pub narrative_omitted_chars: usize,
    #[serde(default)]
    pub reader_quality_omitted_chars: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceDiagnosticsEnvelope {
    pub version: u8,
    pub subject: Option<String>,
    pub source_pack: Option<ResearchSourcePackReport>,
    #[serde(default)]
    pub scrapes: Vec<ScrapeDiagnostics>,
    pub context_packing: Option<ResearchContextPackingDiagnostics>,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchControllerArtifactsSummary {
    pub version: u8,
    pub event_count: usize,
    pub source_card_count: usize,
    pub claim_count: usize,
    pub conflict_count: usize,
    pub open_debt_count: usize,
    pub warning_count: usize,
    pub quality_gate_status: Option<String>,
    pub quality_gate_failure_count: usize,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchScrapeDiagnosticsSummary {
    pub status_class: String,
    pub failure_reason: Option<String>,
    pub user_message: Option<String>,
    pub http_status_code: Option<u16>,
    pub sufficiency_result: String,
    pub insufficiency_reason: Option<String>,
    pub original_url_host: Option<String>,
    pub final_url_host: Option<String>,
    pub reference_link_count: usize,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq, Eq)]
pub struct ResearchSourceDiagnosticsSummary {
    pub version: u8,
    pub subject: Option<String>,
    pub source_pack_status: Option<String>,
    pub source_pack_query_count: usize,
    pub source_pack_adopted_source_count: usize,
    pub source_pack_skipped_candidate_count: usize,
    pub scrape_count: usize,
    pub scrape_failure_count: usize,
    #[serde(default)]
    pub scrapes: Vec<ResearchScrapeDiagnosticsSummary>,
    pub context_packing: Option<ResearchContextPackingDiagnostics>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchBenchmarkMode {
    Fixture,
    Live,
    Replay,
}

#[derive(Debug, Clone)]
pub struct ResearchBenchmarkCaseInput {
    pub case_id: String,
    pub title: String,
    pub category: String,
    pub prompt: String,
    pub data_dir: PathBuf,
    pub mode: ResearchBenchmarkMode,
    pub model_input: Option<String>,
    pub research_intensity: String,
    pub quality_depth: String,
    pub quality_max_iterations: i64,
    pub cli_launch_mode: Option<String>,
    pub ai_task_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ResearchBenchmarkCaseResult {
    pub case_id: String,
    pub title: String,
    pub category: String,
    pub mode: ResearchBenchmarkMode,
    pub task_id: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub quality_status: Option<String>,
    pub quality_last_failure: Option<String>,
    pub research_controller_stage: Option<String>,
    pub research_controller_iteration: Option<i64>,
    pub research_controller_max_iterations: Option<i64>,
    pub output_filename: Option<String>,
    pub final_output: Option<String>,
    pub model_input: String,
}

#[derive(Debug, Clone)]
pub struct ResearchReplayCaseInput {
    pub case_id: String,
    pub title: String,
    pub category: String,
    pub prompt: String,
    pub draft_output: String,
    pub controller_artifacts_json: String,
    pub research_source_diagnostics_json: String,
    pub model_input: String,
    pub research_intensity: String,
    pub quality_depth: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_field_value_contracts_are_frozen() {
        assert_eq!(VALID_FILE_STATUSES, &["draft", "published", "archived"]);
        assert_eq!(VALID_SCRAPE_MODES, &["general", "geeknews"]);
        assert_eq!(VALID_RESEARCH_INTENSITIES, &["low", "medium", "high"]);
        assert_eq!(
            VALID_RESEARCH_QUALITY_DEPTHS,
            &["light", "standard", "strict"]
        );

        assert!(is_valid_file_status("draft"));
        assert!(!is_valid_file_status("deleted"));
        assert!(is_valid_scrape_mode("geeknews"));
        assert!(!is_valid_scrape_mode("rss"));
        assert!(is_valid_research_intensity("high"));
        assert!(!is_valid_research_intensity("strict"));
        assert!(is_valid_research_quality_depth("strict"));
        assert!(!is_valid_research_quality_depth("high"));
    }

    #[test]
    fn research_artifacts_keep_legacy_shape_defaults_in_contract_crate() {
        let artifacts: ResearchControllerArtifacts = serde_json::from_value(serde_json::json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "quality_gate": null,
            "warnings": []
        }))
        .expect("legacy artifacts should deserialize through liquid-protocol");

        assert!(artifacts.narrative_state.is_none());
        assert!(artifacts.reader_quality.is_none());
        assert!(artifacts.source_cards.is_empty());
        assert!(artifacts.claim_log.is_empty());
    }

    #[test]
    fn narrative_state_contract_aliases_and_nested_defaults_are_reusable() {
        let state: NarrativeState = serde_json::from_value(serde_json::json!({
            "version": 1,
            "topic_frame": "Policy explanation",
            "unresolved_structure_gaps": [
                {
                    "id": "G1",
                    "gap_type": "actor",
                    "description": "Need clearer institution coverage"
                }
            ]
        }))
        .expect("shared narrative state aliases should deserialize");

        assert!(state.event_cards.is_empty());
        assert!(state.timeline.is_empty());
        assert!(state.actors.is_empty());
        assert_eq!(state.open_gaps.len(), 1);
        assert_eq!(state.open_gaps[0].gap_type, "actor");
    }

    #[test]
    fn reader_quality_contract_round_trips_optional_sub_artifacts() {
        let artifacts: ResearchControllerArtifacts = serde_json::from_value(serde_json::json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "reader_quality": {
                "argument_graph": {
                    "nodes": [{
                        "id": "AQN1",
                        "label": "Core claim cluster",
                        "node_type": "support",
                        "rationale": "This cluster carries the main answer.",
                        "claim_log_ids": ["C1"],
                        "source_card_ids": ["S1"]
                    }],
                    "edges": []
                },
                "narrative_plan": {
                    "lead_section_id": "SEC1",
                    "section_ids": ["SEC1"],
                    "transition_ids": [],
                    "narrative_arc": "Background to consequence",
                    "ending_note": "Close with implication"
                },
                "section_briefs": [{
                    "section_id": "SEC1",
                    "key_point": "Open with the contested background.",
                    "reader_goal": "Orient the reader quickly",
                    "claim_log_ids": ["C1"],
                    "source_card_ids": ["S1"]
                }]
            },
            "quality_gate": null,
            "warnings": []
        }))
        .expect("reader quality should deserialize through liquid-protocol");

        let reader_quality = artifacts
            .reader_quality
            .expect("reader quality should be present");
        assert_eq!(
            reader_quality.argument_graph.as_ref().unwrap().nodes.len(),
            1
        );
        assert_eq!(reader_quality.section_briefs.len(), 1);

        let serialized = serde_json::to_value(&reader_quality).expect("serialize reader quality");
        assert!(serialized.get("argument_graph").is_some());
        assert!(serialized.get("section_briefs").is_some());
    }
}
