use clap::{Parser, ValueEnum};
use liquid::tasks::{replay_research_benchmark_case, ResearchReplayCaseInput};
use liquid::{
    has_visible_final_answer_section, run_research_benchmark_case, strip_research_artifact_blocks,
    ResearchBenchmarkCaseInput, ResearchBenchmarkCaseResult, ResearchBenchmarkMode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const DEFAULT_CASES_DIR: &str = "docs/experiments/research-richness/cases";
const DEFAULT_FIXTURE_RUNS_DIR: &str = "docs/experiments/research-richness/runs";
const DEFAULT_LIVE_RUNS_DIR_NAME: &str = "liquid-research-bench-live";
const DEFAULT_REPLAY_FIXTURE_ROOT: &str =
    "/tmp/research-live-rerun3.TmMkQf/runs/targeted-live-rerun3-20260515T120100Z";
const MAX_REPLAY_FIXTURE_BUNDLES: usize = 64;
const MAX_REPLAY_FIXTURE_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REPLAY_FIXTURE_TOTAL_BYTES: u64 = 24 * 1024 * 1024;
const BENCHMARK_ASSESSOR: &str = "research_bench_structured_rubric";
const BENCHMARK_SCORE_SOURCE: &str = "machine_readable_benchmark_artifacts";
const PIPELINE_EVIDENCE_MEASUREMENT: &str = "pipeline_evidence";
const LIVE_QUALITY_MEASUREMENT: &str = "live_non_deterministic_quality_measurement";
const REPLAY_QUALITY_MEASUREMENT: &str = "artifact_backed_replay_validation";
const PIPELINE_EVIDENCE_CAVEAT: &str =
    "Fixture-only pipeline evidence; not live research quality proof.";
const LIVE_QUALITY_CAVEAT: &str = "Live non-deterministic benchmark measurement; compare only against runs with matching provider/runtime settings.";
const REPLAY_QUALITY_CAVEAT: &str = "Deterministic replay from frozen live bundles; validates rendering and strict gates without fresh model or network calls.";
const SCORE_VISIBILITY: &str = "per_case_and_run_aggregate";

const SOURCE_AUDIT_HEADINGS: &[&str] = &["## source audit", "## 출처 감사"];
const CLAIM_LOG_HEADINGS: &[&str] = &["## claim log", "## 주장 로그", "## claims and evidence"];
const QUALITY_GATE_HEADINGS: &[&str] = &["## quality gate", "## 품질 게이트", "## quality check"];
const VERIFICATION_APPENDIX_HEADINGS: &[&str] = &[
    "# verification appendix",
    "# 검증 부록",
    "## verification appendix",
    "## 검증 부록",
];
const ACTION_OR_DECISION_TERMS: &[&str] = &[
    "recommend",
    "recommended",
    "should",
    "next",
    "decision",
    "choose",
    "action",
    "권장",
    "추천",
    "다음",
    "결론",
];
const SOURCE_CLASS_PRIMARY_MARKERS: &[&str] = &[
    "official_or_primary",
    "official",
    "primary",
    "documentation",
    "vendor",
    "project",
];
const SOURCE_CLASS_RUMOR_MARKERS: &[&str] = &[
    "rumor",
    "opinion",
    "forum",
    "social",
    "speculation",
    "secondary",
];
const READER_FACING_ARTIFACT_LEAK_MARKERS: &[&str] = &[
    "source pack",
    "target-host",
    "target host",
    "source-class",
    "source class",
    "open debt",
    "claim log",
    "quality gate",
    "source audit",
    "주장 로그",
    "품질 게이트",
    "출처 감사",
    "chronology/actor/cause/consequence",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BenchModeArg {
    Fixture,
    Live,
    Replay,
}

#[derive(Debug, Parser)]
#[command(author, version, about = "Headless research benchmark runner")]
struct Args {
    #[arg(long, default_value = DEFAULT_CASES_DIR)]
    cases_dir: PathBuf,
    #[arg(long, env = "LIQUID_BENCH_RUNS_DIR")]
    runs_dir: Option<PathBuf>,
    #[arg(long)]
    label: String,
    #[arg(long, value_enum, default_value_t = BenchModeArg::Fixture)]
    mode: BenchModeArg,
    #[arg(long, default_value = DEFAULT_REPLAY_FIXTURE_ROOT)]
    replay_fixture_root: PathBuf,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    model_input: Option<String>,
    #[arg(long)]
    engine_name: Option<String>,
    #[arg(long)]
    model_name: Option<String>,
    #[arg(long, default_value = "high")]
    research_intensity: String,
    #[arg(long, default_value = "strict")]
    quality_depth: String,
    #[arg(long, default_value_t = 2)]
    max_iterations: i64,
    #[arg(long, env = "LIQUID_CLI_LAUNCH_MODE")]
    cli_launch_mode: Option<String>,
    #[arg(long, env = "LIQUID_AI_TASK_TIMEOUT_SECS", default_value_t = 3600)]
    ai_task_timeout_secs: u64,
    #[arg(
        long,
        env = "LIQUID_BENCH_INCLUDE_RAW_DEBUG_ARTIFACTS",
        default_value_t = false
    )]
    include_raw_debug_artifacts: bool,
}

#[derive(Debug, Clone)]
struct BenchmarkCase {
    case_id: String,
    filename: String,
    title: String,
    category: String,
    prompt: String,
    must_pass_checks: Vec<String>,
    expected_failure_modes: Vec<String>,
}

#[derive(Debug, Clone)]
struct CaseExecutionPlan {
    case: BenchmarkCase,
    artifact_stem: String,
}

#[derive(Debug, Clone, Copy)]
struct MeasurementMetadata {
    measurement_kind: &'static str,
    evidence_caveat: &'static str,
    report_note_label: &'static str,
}

#[derive(Debug, Clone)]
struct RunExecution {
    case: BenchmarkCase,
    result: ResearchBenchmarkCaseResult,
    scorecard: BenchmarkCaseScorecard,
    artifact_paths: CaseArtifactPaths,
    replay_before: Option<ReplayBeforeState>,
}

#[derive(Debug, Clone, Serialize)]
struct ReplayBeforeState {
    status: String,
    quality_status: String,
    quality_last_failure: String,
    critical_failure_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkCaseScorecard {
    case_id: String,
    title: String,
    category: String,
    mode: String,
    measurement_kind: String,
    evidence_caveat: String,
    assessor: String,
    score_source: String,
    advisory_model_self_score: Option<f32>,
    overall_score: f64,
    any_critical_failure: bool,
    historical_overlay_trigger_count: usize,
    dimensions: Vec<BenchmarkRubricDimensionRecord>,
    critical_flags: Vec<BenchmarkCriticalFlagRecord>,
    visibility: BenchmarkCaseVisibility,
    metrics: BenchmarkEvidenceMetrics,
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay_before: Option<ReplayBeforeState>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkRubricDimensionRecord {
    key: String,
    label: String,
    score: u8,
    max_score: u8,
    assessor: String,
    score_source: String,
    visibility: String,
    rationale: String,
    evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkCriticalFlagRecord {
    key: String,
    label: String,
    triggered: bool,
    assessor: String,
    score_source: String,
    visibility: String,
    rationale: String,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkCaseVisibility {
    task_status: String,
    quality_status: String,
    source_pack_status: String,
    visible_final_answer: bool,
    visible_source_audit: bool,
    visible_claim_log: bool,
    visible_quality_gate: bool,
    visible_verification_appendix: bool,
    fixture_only_pipeline_evidence: bool,
    visible_in_aggregate: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
struct BenchmarkEvidenceMetrics {
    final_output_chars: usize,
    prompt_term_count: usize,
    prompt_term_hits: usize,
    reader_facing_prompt_echo_count: usize,
    reader_facing_artifact_leak_count: usize,
    section_count: usize,
    source_card_count: usize,
    official_source_card_count: usize,
    rumor_or_opinion_source_card_count: usize,
    distinct_source_host_count: usize,
    claim_count: usize,
    supported_claim_count: usize,
    support_url_count: usize,
    conflict_count: usize,
    resolved_conflict_count: usize,
    promoted_conflict_count: usize,
    open_debt_count: usize,
    next_action_count: usize,
    warning_count: usize,
    unsupported_claim_count: usize,
    unresolved_conflict_count: usize,
    quality_gate_failure_count: usize,
    source_pack_query_count: usize,
    source_pack_discovered_source_count: usize,
    source_pack_adopted_source_count: usize,
    source_pack_skipped_candidate_count: usize,
    scrape_count: usize,
    scrape_failure_count: usize,
    context_pack_included_source_card_count: usize,
    context_pack_omitted_source_card_count: usize,
    narrative_state_present: bool,
    narrative_timeline_event_count: usize,
    narrative_section_count: usize,
    narrative_evidence_layer_count: usize,
    narrative_interpretive_tension_count: usize,
    narrative_impact_count: usize,
    narrative_reader_question_count: usize,
    narrative_open_gap_count: usize,
    reader_quality_present: bool,
    reader_argument_node_count: usize,
    reader_argument_edge_count: usize,
    reader_narrative_plan_present: bool,
    reader_section_brief_count: usize,
    reader_critique_present: bool,
    reader_critique_metric_count: usize,
    reader_critique_failed_metric_count: usize,
    context_pack_narrative_state_present: bool,
    context_pack_narrative_open_gap_count: usize,
    context_pack_reader_quality_present: bool,
    context_pack_reader_section_brief_count: usize,
    context_pack_reader_critique_metric_count: usize,
    invalid_source_card_url_count: usize,
    target_host_miss_count: usize,
    source_class_miss_count: usize,
    historical_chronology_signal_count: usize,
    historical_actor_signal_count: usize,
    historical_geography_signal_count: usize,
    historical_cause_signal_count: usize,
    historical_consequence_signal_count: usize,
    historical_evidence_limit_signal_count: usize,
    historical_contested_interpretation_signal_count: usize,
    historical_scope_limit_signal_count: usize,
    historical_lens_coverage_count: usize,
    historical_comparison_signal_count: usize,
    historical_chronology_interpretation_split_signal_count: usize,
    historical_source_layer_signal_count: usize,
    historical_issue_map_signal_count: usize,
    historical_legacy_signal_count: usize,
    historical_follow_up_signal_count: usize,
    second_punic_visible_chars: usize,
    second_punic_phase_subsection_count: usize,
    second_punic_date_anchor_count: usize,
    second_punic_subject_anchor_count: usize,
    technology_design_judgment_signal_count: usize,
    technology_tradeoff_signal_count: usize,
    technology_verifiability_signal_count: usize,
    genre_section_richness_signal_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkRunStructuredReport {
    label: String,
    generated_at: String,
    mode: String,
    measurement_kind: String,
    evidence_caveat: String,
    engine_name: String,
    model_name: String,
    configured_search_providers: Vec<String>,
    research_intensity: String,
    quality_depth: String,
    max_iterations: i64,
    cases_dir: String,
    run_artifact_dir: String,
    summary: BenchmarkRunSummary,
    dimension_aggregates: Vec<BenchmarkDimensionAggregate>,
    cases: Vec<BenchmarkCaseScorecard>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkRunSummary {
    case_count: usize,
    completed_case_count: usize,
    failed_case_count: usize,
    quality_passed_case_count: usize,
    critical_failure_case_count: usize,
    overall_average_score: f64,
    overall_min_score: f64,
    overall_max_score: f64,
    status_counts: BTreeMap<String, usize>,
    quality_status_counts: BTreeMap<String, usize>,
    source_pack_status_counts: BTreeMap<String, usize>,
    critical_failure_case_ids: Vec<String>,
    quality_failed_case_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkDimensionAggregate {
    key: String,
    label: String,
    average_score: f64,
    min_score: u8,
    max_score: u8,
    case_count: usize,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedControllerArtifacts {
    #[serde(default)]
    source_cards: Vec<ParsedSourceCard>,
    #[serde(default)]
    claim_log: Vec<ParsedClaimLogEntry>,
    #[serde(default)]
    conflict_map: Vec<ParsedConflictEntry>,
    #[serde(default)]
    research_debt: Vec<ParsedDebtItem>,
    narrative_state: Option<ParsedNarrativeState>,
    reader_quality: Option<ParsedReaderQuality>,
    quality_gate: Option<ParsedQualityGate>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedNarrativeState {
    #[serde(default)]
    timeline: Vec<serde_json::Value>,
    #[serde(default)]
    evidence_layers: Vec<serde_json::Value>,
    #[serde(default)]
    interpretive_tensions: Vec<serde_json::Value>,
    #[serde(default)]
    impacts: Vec<serde_json::Value>,
    #[serde(default)]
    reader_questions: Vec<serde_json::Value>,
    #[serde(default)]
    section_outline: Vec<serde_json::Value>,
    #[serde(default)]
    open_gaps: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedReaderQuality {
    argument_graph: Option<ParsedArgumentGraph>,
    narrative_plan: Option<serde_json::Value>,
    #[serde(default)]
    section_briefs: Vec<serde_json::Value>,
    reader_critique: Option<ParsedReaderCritique>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedArgumentGraph {
    #[serde(default)]
    nodes: Vec<serde_json::Value>,
    #[serde(default)]
    edges: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedReaderCritique {
    #[serde(default)]
    metrics: Vec<ParsedReaderCritiqueMetric>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedReaderCritiqueMetric {
    #[serde(
        default = "default_reader_critique_metric_status",
        deserialize_with = "deserialize_reader_critique_metric_status"
    )]
    status: String,
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

#[derive(Debug, Deserialize, Default)]
struct ParsedSourceCard {
    id: String,
    url: String,
    title: String,
    source_class: String,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedClaimLogEntry {
    id: String,
    claim: String,
    #[serde(default)]
    support_source_card_ids: Vec<String>,
    #[serde(default)]
    support_urls: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedConflictEntry {
    id: String,
    topic: String,
    #[serde(default)]
    conflicting_claim_ids: Vec<String>,
    pub promoted_to_debt: Option<bool>,
    pub resolution_status: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedDebtItem {
    id: String,
    missing_evidence: String,
    status: String,
    #[serde(default)]
    candidate_queries: Vec<String>,
    #[serde(default)]
    next_check_actions: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedQualityGate {
    status: String,
    #[serde(default)]
    failure_messages: Vec<String>,
    unsupported_claim_count: usize,
    unresolved_conflict_count: usize,
    open_debt_count: usize,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedSourceDiagnosticsEnvelope {
    subject: Option<String>,
    source_pack: Option<ParsedSourcePackReport>,
    #[serde(default)]
    scrapes: Vec<ParsedScrapeDiagnostics>,
    context_packing: Option<ParsedContextPackingDiagnostics>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedSourcePackReport {
    status: String,
    reason: Option<String>,
    #[serde(default)]
    queries: Vec<ParsedSourceQueryReport>,
    discovered_source_count: usize,
    adopted_source_count: usize,
    #[serde(default)]
    skipped_candidates: Vec<serde_json::Value>,
    #[serde(default)]
    coverage_misses: Vec<ParsedSourceCoverageMiss>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedSourceQueryReport {
    query: String,
    status: String,
    #[serde(default)]
    provider: Option<String>,
    result_count: usize,
    adopted_count: usize,
    skipped_count: usize,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedScrapeDiagnostics {
    status_class: String,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedContextPackingDiagnostics {
    included_source_card_count: usize,
    omitted_source_card_count: usize,
    #[serde(default)]
    narrative_state_present: bool,
    #[serde(default)]
    narrative_timeline_event_count: usize,
    #[serde(default)]
    narrative_section_count: usize,
    #[serde(default)]
    narrative_evidence_layer_count: usize,
    #[serde(default)]
    narrative_interpretive_tension_count: usize,
    #[serde(default)]
    narrative_impact_count: usize,
    #[serde(default)]
    narrative_reader_question_count: usize,
    #[serde(default)]
    narrative_open_gap_count: usize,
    #[serde(default)]
    reader_quality_present: bool,
    #[serde(default)]
    reader_argument_node_count: usize,
    #[serde(default)]
    reader_argument_edge_count: usize,
    #[serde(default)]
    reader_narrative_plan_present: bool,
    #[serde(default)]
    reader_section_brief_count: usize,
    #[serde(default)]
    reader_critique_present: bool,
    #[serde(default)]
    reader_critique_metric_count: usize,
    #[serde(default)]
    reader_critique_failed_metric_count: usize,
}

#[derive(Debug, Deserialize, Default)]
struct ParsedSourceCoverageMiss {
    status: String,
    expected_host: Option<String>,
    expected_source_class: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ReplayFixtureSummary {
    case_id: String,
    category: String,
    #[serde(default)]
    title: Option<String>,
    status: String,
    quality_status: Option<String>,
    quality_last_failure: Option<String>,
    model_input: String,
}

#[derive(Debug, Clone)]
struct ReplayFixtureBundle {
    plan: CaseExecutionPlan,
    summary: ReplayFixtureSummary,
    final_output: String,
    controller_artifacts_json: String,
    source_diagnostics_json: String,
}

impl ReplayFixtureBundle {
    fn before_result(&self) -> ResearchBenchmarkCaseResult {
        ResearchBenchmarkCaseResult {
            case_id: self.plan.case.case_id.clone(),
            title: self.plan.case.title.clone(),
            category: self.plan.case.category.clone(),
            mode: ResearchBenchmarkMode::Replay,
            data_dir: PathBuf::new(),
            task_id: 0,
            status: self.summary.status.clone(),
            error_message: None,
            quality_status: self.summary.quality_status.clone(),
            quality_last_failure: self.summary.quality_last_failure.clone(),
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
            output_filename: None,
            final_output: Some(self.final_output.clone()),
            research_controller_artifacts_json: Some(self.controller_artifacts_json.clone()),
            research_source_diagnostics_json: Some(self.source_diagnostics_json.clone()),
            resolved_system_prompt: None,
            resolved_user_prompt: None,
            model_input: self.summary.model_input.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RubricDimensionSpec {
    key: &'static str,
    label: &'static str,
}

const RUBRIC_DIMENSIONS: [RubricDimensionSpec; 10] = [
    RubricDimensionSpec {
        key: "user_intent_fit",
        label: "User intent fit",
    },
    RubricDimensionSpec {
        key: "direct_answer_usefulness",
        label: "Direct answer usefulness",
    },
    RubricDimensionSpec {
        key: "evidence_claim_traceability",
        label: "Evidence/claim traceability",
    },
    RubricDimensionSpec {
        key: "source_quality_and_diversity",
        label: "Source quality and diversity",
    },
    RubricDimensionSpec {
        key: "official_rumor_opinion_separation",
        label: "Official/rumor/opinion separation",
    },
    RubricDimensionSpec {
        key: "uncertainty_and_conflict_handling",
        label: "Handling of uncertainty and conflicts",
    },
    RubricDimensionSpec {
        key: "information_resolution",
        label: "Information resolution compared with available sources",
    },
    RubricDimensionSpec {
        key: "practical_next_steps",
        label: "Practical next steps or decision support",
    },
    RubricDimensionSpec {
        key: "structure_and_readability",
        label: "Structure and readability",
    },
    RubricDimensionSpec {
        key: "genre_section_richness",
        label: "Genre/section richness",
    },
];

#[derive(Debug, Clone, Copy)]
struct CriticalFlagSpec {
    key: &'static str,
    label: &'static str,
}

const SECOND_PUNIC_WAR_MIN_VISIBLE_CHARS: usize = 900;
const SECOND_PUNIC_WAR_MIN_PHASE_SUBSECTIONS: usize = 6;
const SECOND_PUNIC_WAR_MIN_DATE_ANCHORS: usize = 6;
const SECOND_PUNIC_WAR_MIN_SUBJECT_ANCHORS: usize = 8;

const CRITICAL_FLAG_SPECS: [CriticalFlagSpec; 12] = [
    CriticalFlagSpec {
        key: "unsupported_factual_claim",
        label: "Unsupported factual claim that affects the conclusion",
    },
    CriticalFlagSpec {
        key: "fabricated_source_or_url",
        label: "Fabricated source, URL, date, price, or named entity",
    },
    CriticalFlagSpec {
        key: "fails_to_answer_question",
        label: "Fails to answer the user's actual question",
    },
    CriticalFlagSpec {
        key: "rumor_as_official_fact",
        label: "Treats rumor/opinion as official fact",
    },
    CriticalFlagSpec {
        key: "ignores_major_prompt_constraint",
        label: "Ignores a major constraint from the prompt",
    },
    CriticalFlagSpec {
        key: "final_answer_without_meaningful_evidence",
        label: "Produces final answer without meaningful evidence or uncertainty handling",
    },
    CriticalFlagSpec {
        key: "reader_facing_prompt_or_artifact_leakage",
        label: "Reader-facing final answer leaks prompt/controller-validation artifacts",
    },
    CriticalFlagSpec {
        key: "historical_missing_chronology",
        label: "Historical answer omits clear sequence or period context",
    },
    CriticalFlagSpec {
        key: "historical_missing_actors_or_geography",
        label: "Historical answer omits key actors or geographic scope",
    },
    CriticalFlagSpec {
        key: "historical_missing_causes_or_consequences",
        label: "Historical answer omits causal explanation or consequences",
    },
    CriticalFlagSpec {
        key: "historical_missing_limits_or_contested_interpretation",
        label: "Historical answer omits source limits, scope limits, or contested interpretations",
    },
    CriticalFlagSpec {
        key: "historical_campaign_phase_density_floor",
        label: "Historical campaign answer drops below the Second Punic War phase-density floor",
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    run(args)
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    validate_label(&args.label)?;
    let mode = match args.mode {
        BenchModeArg::Fixture => ResearchBenchmarkMode::Fixture,
        BenchModeArg::Live => ResearchBenchmarkMode::Live,
        BenchModeArg::Replay => ResearchBenchmarkMode::Replay,
    };
    let measurement = measurement_metadata(mode);
    let runs_dir = resolve_runs_dir(mode, args.runs_dir.as_deref());
    fs::create_dir_all(&runs_dir)?;
    let cases = load_cases(&args.cases_dir)?;
    let case_plans = build_case_execution_plans(cases)?;
    if case_plans.is_empty() {
        return Err(format!(
            "no benchmark case files found under {}",
            args.cases_dir.display()
        )
        .into());
    }
    let run_case_dir = runs_dir.join(&args.label);
    let report_path = runs_dir.join(format!("{}.md", args.label));
    preflight_run_output_paths(&run_case_dir, &report_path, &runs_dir, &args.label)?;
    fs::create_dir(&run_case_dir)?;
    let timestamp = chrono::Utc::now().to_rfc3339();
    let engine_name = args.engine_name.clone().unwrap_or_else(|| match mode {
        ResearchBenchmarkMode::Fixture => "Fixture Controller".to_string(),
        ResearchBenchmarkMode::Live => "Headless Research".to_string(),
        ResearchBenchmarkMode::Replay => "Artifact Replay".to_string(),
    });
    let model_name = args
        .model_name
        .clone()
        .or_else(|| args.model_input.clone())
        .unwrap_or_else(|| match mode {
            ResearchBenchmarkMode::Fixture => "fixture-research-bench".to_string(),
            ResearchBenchmarkMode::Live => "unspecified".to_string(),
            ResearchBenchmarkMode::Replay => "frozen-replay-fixtures".to_string(),
        });

    let mut run_executions = Vec::new();
    match mode {
        ResearchBenchmarkMode::Replay => {
            let bundles = load_replay_fixture_bundles(&args.replay_fixture_root, &case_plans)?;
            if bundles.is_empty() {
                return Err(format!(
                    "no replay fixtures found under {}",
                    args.replay_fixture_root.display()
                )
                .into());
            }
            for bundle in bundles {
                let before_result = bundle.before_result();
                let before_scorecard = build_case_scorecard(&bundle.plan.case, &before_result);
                let result = match replay_research_benchmark_case(ResearchReplayCaseInput {
                    case_id: bundle.plan.case.case_id.clone(),
                    title: bundle.plan.case.title.clone(),
                    category: bundle.plan.case.category.clone(),
                    prompt: bundle.plan.case.prompt.clone(),
                    draft_output: bundle.final_output,
                    controller_artifacts_json: bundle.controller_artifacts_json,
                    research_source_diagnostics_json: bundle.source_diagnostics_json,
                    model_input: bundle.summary.model_input.clone(),
                    research_intensity: args.research_intensity.clone(),
                    quality_depth: args.quality_depth.clone(),
                }) {
                    Ok(result) => result,
                    Err(error) => synthetic_failed_result_for_case(
                        &bundle.plan.case,
                        ResearchBenchmarkMode::Replay,
                        bundle.summary.model_input.as_str(),
                        PathBuf::new(),
                        "replay_failed",
                        error,
                    ),
                };
                run_executions.push(materialize_case_execution(
                    &run_case_dir,
                    &bundle.plan,
                    result,
                    Some(ReplayBeforeState {
                        status: before_result.status,
                        quality_status: before_result
                            .quality_status
                            .unwrap_or_else(|| "unknown".to_string()),
                        quality_last_failure: before_result
                            .quality_last_failure
                            .unwrap_or_else(|| "none".to_string()),
                        critical_failure_count: before_scorecard
                            .critical_flags
                            .iter()
                            .filter(|flag| flag.triggered)
                            .count(),
                    }),
                    args.include_raw_debug_artifacts,
                ));
            }
        }
        ResearchBenchmarkMode::Fixture | ResearchBenchmarkMode::Live => {
            let runtime = tokio::runtime::Runtime::new()?;
            for plan in &case_plans {
                let case_data_dir = args
                    .data_dir
                    .clone()
                    .map(|base| base.join(&args.label).join(&plan.artifact_stem))
                    .unwrap_or_else(|| {
                        std::env::temp_dir().join(format!(
                            "liquid-bench-{}-{}",
                            plan.artifact_stem,
                            Uuid::new_v4()
                        ))
                    });
                let result = match runtime.block_on(run_research_benchmark_case(
                    ResearchBenchmarkCaseInput {
                        case_id: plan.case.case_id.clone(),
                        title: plan.case.title.clone(),
                        category: plan.case.category.clone(),
                        prompt: plan.case.prompt.clone(),
                        data_dir: case_data_dir.clone(),
                        mode,
                        model_input: args.model_input.clone(),
                        research_intensity: args.research_intensity.clone(),
                        quality_depth: args.quality_depth.clone(),
                        quality_max_iterations: args.max_iterations,
                        cli_launch_mode: args.cli_launch_mode.clone(),
                        ai_task_timeout_secs: args.ai_task_timeout_secs,
                    },
                )) {
                    Ok(result) => result,
                    Err(error) => synthetic_failed_result_for_case(
                        &plan.case,
                        mode,
                        args.model_input
                            .as_deref()
                            .or(args.model_name.as_deref())
                            .unwrap_or("unspecified"),
                        case_data_dir.clone(),
                        "case_execution_failed",
                        error.to_string(),
                    ),
                };
                run_executions.push(materialize_case_execution(
                    &run_case_dir,
                    plan,
                    result,
                    None,
                    args.include_raw_debug_artifacts,
                ));
                if args.data_dir.is_none() {
                    let _ = fs::remove_dir_all(case_data_dir);
                }
            }
        }
    }

    let structured_report = build_structured_run_report(
        &args,
        mode,
        &timestamp,
        &engine_name,
        &model_name,
        &run_case_dir,
        &run_executions,
    );
    write_structured_run_outputs(&runs_dir, &args.label, &structured_report)?;
    let case_sections = run_executions
        .iter()
        .map(|execution| {
            render_case_section(
                &execution.case,
                &execution.result,
                &execution.scorecard,
                &execution.artifact_paths,
                execution.replay_before.as_ref(),
            )
        })
        .collect::<Vec<_>>();

    let report = format!(
        "# Research Richness Run: {}\n\n- Timestamp: {}\n- Mode: {}\n- Engine: {}\n- Model: {}\n- Structured scoring: authoritative machine rubric from persisted benchmark artifacts\n- {}: {}\n- JSON aggregate: {}.json\n- CSV rows: {}.csv\n- NDJSON rows: {}.ndjson\n\n## Aggregate Structured Summary\n{}\n\n## Dimension Averages\n{}\n\n## Case Inventory\n{}\n{}",
        args.label,
        timestamp,
        match mode {
            ResearchBenchmarkMode::Fixture => "fixture (deterministic)",
            ResearchBenchmarkMode::Live => "live (non-deterministic)",
            ResearchBenchmarkMode::Replay => "replay (deterministic frozen bundles)",
        },
        engine_name,
        model_name,
        measurement.report_note_label,
        measurement.evidence_caveat,
        args.label,
        args.label,
        args.label,
        render_markdown_aggregate_summary(&structured_report.summary),
        render_markdown_dimension_summary(&structured_report.dimension_aggregates),
        case_plans
            .iter()
            .map(|plan| format!(
                "- {}: {} ({})",
                plan.case.category, plan.case.title, plan.case.filename
            ))
            .collect::<Vec<_>>()
            .join("\n"),
        case_sections.join("\n")
    );
    secret_scan_report(&report, &run_executions)?;
    write_new_text_file(&report_path, &report)?;
    if matches!(mode, ResearchBenchmarkMode::Replay)
        && run_executions.iter().any(|execution| {
            execution.scorecard.visibility.quality_status != "passed"
                || execution
                    .scorecard
                    .critical_flags
                    .iter()
                    .any(|flag| flag.triggered)
        })
    {
        println!("{}", report_path.display());
        return Err("replay verification failed: at least one fixture remained untrusted or retained critical flags".into());
    }
    println!("{}", report_path.display());
    Ok(())
}

fn materialize_case_execution(
    run_case_dir: &Path,
    plan: &CaseExecutionPlan,
    mut result: ResearchBenchmarkCaseResult,
    replay_before: Option<ReplayBeforeState>,
    include_raw_debug_artifacts: bool,
) -> RunExecution {
    let mut scorecard = build_case_scorecard(&plan.case, &result);
    let artifact_paths = match write_case_artifacts(
        run_case_dir,
        &plan.artifact_stem,
        &result,
        &scorecard,
        include_raw_debug_artifacts,
    ) {
        Ok(paths) => paths,
        Err(error) => {
            result = synthetic_failed_result_from_existing(
                result,
                "artifact_write_failed",
                format!("benchmark artifact write failed: {error}"),
            );
            scorecard = build_case_scorecard(&plan.case, &result);
            let fallback = write_case_failure_fallback_artifacts(
                run_case_dir,
                &plan.artifact_stem,
                &result,
                &scorecard,
            );
            if let Some(fallback_error) = fallback.error {
                append_result_error_message(
                    &mut result,
                    format!("benchmark failure artifact fallback also failed: {fallback_error}"),
                );
            }
            fallback.paths
        }
    };

    RunExecution {
        case: plan.case.clone(),
        result,
        scorecard,
        artifact_paths,
        replay_before,
    }
}

fn resolve_runs_dir(mode: ResearchBenchmarkMode, runs_dir: Option<&Path>) -> PathBuf {
    runs_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| match mode {
            ResearchBenchmarkMode::Fixture | ResearchBenchmarkMode::Replay => {
                PathBuf::from(DEFAULT_FIXTURE_RUNS_DIR)
            }
            ResearchBenchmarkMode::Live => std::env::temp_dir().join(DEFAULT_LIVE_RUNS_DIR_NAME),
        })
}

fn load_replay_fixture_bundles(
    root: &Path,
    case_plans: &[CaseExecutionPlan],
) -> Result<Vec<ReplayFixtureBundle>, Box<dyn std::error::Error>> {
    if !root.is_dir() {
        return Err(format!("replay fixture root is missing: {}", root.display()).into());
    }
    let case_map = case_plans
        .iter()
        .map(|plan| (plan.case.case_id.clone(), plan.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut bundles = Vec::new();
    let mut summary_paths = fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            !file_name.ends_with("-controller-artifacts.json")
                && !file_name.ends_with("-source-diagnostics.json")
        })
        .collect::<Vec<_>>();
    summary_paths.sort();
    if summary_paths.len() > MAX_REPLAY_FIXTURE_BUNDLES {
        return Err(format!(
            "replay fixture root has too many case summaries: {} > {}",
            summary_paths.len(),
            MAX_REPLAY_FIXTURE_BUNDLES
        )
        .into());
    }
    let mut total_bytes = 0_u64;
    for path in summary_paths {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("invalid replay fixture filename: {}", path.display()))?;
        let summary = serde_json::from_str::<ReplayFixtureSummary>(&read_bounded_text_file(
            &path,
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?)?;
        let resolved_user_prompt = read_bounded_text_file(
            &root.join(format!("{stem}-resolved-user-prompt.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let plan = resolve_replay_fixture_plan(&summary, stem, &case_map, &resolved_user_prompt);
        let final_output = read_bounded_text_file(
            &root.join(format!("{stem}-final-output.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let controller_artifacts_json = read_bounded_text_file(
            &root.join(format!("{stem}-controller-artifacts.json")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let source_diagnostics_json = read_bounded_text_file(
            &root.join(format!("{stem}-source-diagnostics.json")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let _ = read_bounded_text_file(
            &root.join(format!("{stem}-resolved-system-prompt.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        bundles.push(ReplayFixtureBundle {
            plan,
            summary,
            final_output,
            controller_artifacts_json,
            source_diagnostics_json,
        });
    }
    bundles.sort_by(|left, right| left.plan.artifact_stem.cmp(&right.plan.artifact_stem));
    Ok(bundles)
}

fn read_bounded_text_file(
    path: &Path,
    total_bytes: &mut u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let file_bytes = fs::metadata(path)?.len();
    if file_bytes > max_file_bytes {
        return Err(format!(
            "replay fixture file is too large: {} bytes > {} for {}",
            file_bytes,
            max_file_bytes,
            path.display()
        )
        .into());
    }
    let next_total = total_bytes.saturating_add(file_bytes);
    if next_total > max_total_bytes {
        return Err(format!(
            "replay fixture root exceeds total byte limit: {} bytes > {}",
            next_total, max_total_bytes
        )
        .into());
    }
    *total_bytes = next_total;
    Ok(fs::read_to_string(path)?)
}

fn resolve_replay_fixture_plan(
    summary: &ReplayFixtureSummary,
    stem: &str,
    case_map: &BTreeMap<String, CaseExecutionPlan>,
    resolved_user_prompt: &str,
) -> CaseExecutionPlan {
    if let Some(plan) = case_map.get(&summary.case_id) {
        return plan.clone();
    }

    CaseExecutionPlan {
        case: BenchmarkCase {
            case_id: summary.case_id.clone(),
            filename: format!("{stem}.json"),
            title: summary
                .title
                .clone()
                .unwrap_or_else(|| humanize_replay_case_id(&summary.case_id)),
            category: summary.category.clone(),
            prompt: extract_replay_fixture_prompt(resolved_user_prompt),
            must_pass_checks: replay_fixture_must_pass_checks(&summary.case_id, &summary.category),
            expected_failure_modes: replay_fixture_expected_failure_modes(
                &summary.case_id,
                &summary.category,
            ),
        },
        artifact_stem: stem.to_string(),
    }
}

fn extract_replay_fixture_prompt(resolved_user_prompt: &str) -> String {
    let mut capture = false;
    let mut lines = Vec::new();
    for line in resolved_user_prompt.lines() {
        if line.trim() == "### User Request:" {
            capture = true;
            continue;
        }
        if !capture {
            continue;
        }
        if line
            .trim()
            .starts_with("[RESEARCH QUALITY REPAIR ITERATION")
        {
            break;
        }
        lines.push(line);
    }
    let prompt = lines.join("\n").trim().to_string();
    if prompt.is_empty() {
        resolved_user_prompt
            .lines()
            .take(24)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        prompt
    }
}

fn replay_fixture_must_pass_checks(case_id: &str, category: &str) -> Vec<String> {
    match (case_id, category) {
        ("02-current-policy-regulatory", _) => vec![
            "distinguishes voluntary NIST framework guidance from binding EU AI Act obligations using official sources".to_string(),
            "makes conflicts, open interpretive points, and role-scope limits visible instead of flattening them".to_string(),
            "keeps visible Source Audit URLs and Claim Log support for each core compliance comparison".to_string(),
        ],
        ("03-comparative-product-technical-decision", _) => vec![
            "cites current official technical specifications for memory ceilings, battery, and repairability or upgradeability".to_string(),
            "separates official specs from reviewer or third-party thermals and sustained-performance claims".to_string(),
            "gives a final recommendation that makes CUDA, unified-memory, and repairability tradeoffs explicit".to_string(),
        ],
        _ if category.contains("policy") || category.contains("regulatory") => vec![
            "uses official sources for the core policy or regulatory obligations".to_string(),
            "states what is verified, what conflicts, and what remains open".to_string(),
            "keeps visible Source Audit URLs and Claim Log support".to_string(),
        ],
        _ if category.contains("product") || category.contains("technical-decision") => vec![
            "cites current official specifications for the main decision criteria".to_string(),
            "separates official facts from third-party performance interpretation".to_string(),
            "gives a recommendation with explicit tradeoffs and uncertainties".to_string(),
        ],
        _ => vec![
            "preserves a visible Final Answer with readable prose".to_string(),
            "keeps visible Source Audit URLs and resolvable Claim Log support".to_string(),
            "states remaining limits or conflicts instead of fabricating certainty".to_string(),
        ],
    }
}

fn replay_fixture_expected_failure_modes(case_id: &str, category: &str) -> Vec<String> {
    match (case_id, category) {
        ("02-current-policy-regulatory", _) => vec![
            "deployer, provider, and downstream-provider roles are blurred into a false single obligation set".to_string(),
            "interpretive or enforcement uncertainty is hidden instead of labeled".to_string(),
            "service-desk summaries are treated as binding law without anchoring the official legal text".to_string(),
        ],
        ("03-comparative-product-technical-decision", _) => vec![
            "official specs and reviewer thermal claims are blended without labeling".to_string(),
            "the recommendation ignores CUDA, unified-memory, or repairability constraints".to_string(),
            "current specifications or battery limits are fabricated or left unstated".to_string(),
        ],
        _ if category.contains("policy") || category.contains("regulatory") => vec![
            "official and interpretive sources are collapsed into one certainty level".to_string(),
            "open policy conflicts are presented as resolved fact".to_string(),
        ],
        _ if category.contains("product") || category.contains("technical-decision") => vec![
            "recommendation is made without explicit tradeoff handling".to_string(),
            "specifications are presented as current without concrete support".to_string(),
        ],
        _ => vec![
            "visible evidence support remains missing".to_string(),
            "the conclusion claims more certainty than the artifacts support".to_string(),
        ],
    }
}

fn humanize_replay_case_id(case_id: &str) -> String {
    case_id
        .trim_matches(|ch: char| ch.is_ascii_digit() || ch == '-')
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_case_execution_plans(
    cases: Vec<BenchmarkCase>,
) -> Result<Vec<CaseExecutionPlan>, Box<dyn std::error::Error>> {
    let index_width = cases.len().max(1).to_string().len();
    cases
        .into_iter()
        .enumerate()
        .map(|(index, case)| {
            let slug = slugify(&case.case_id);
            if slug.is_empty() {
                return Err(format!(
                    "benchmark case id produces an empty slug and cannot be used safely: {}",
                    case.filename
                )
                .into());
            }
            Ok(CaseExecutionPlan {
                case,
                artifact_stem: format!("{:0width$}-{}", index + 1, slug, width = index_width),
            })
        })
        .collect()
}

fn render_case_section(
    case: &BenchmarkCase,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
    artifact_paths: &CaseArtifactPaths,
    replay_before: Option<&ReplayBeforeState>,
) -> String {
    format!(
        "\n## {title}\n\n- Category: {category}\n- Case file: {filename}\n- Prompt: {prompt}\n- Task status: {status}\n- Quality status: {quality_status}\n- Quality last failure: {quality_last_failure}\n- Replay before: {replay_before}\n- Structured overall score: {overall_score:.2}/5.00\n- Critical failure flags: {critical_failure_count}\n- Structured visibility: task={task_visibility}, quality={quality_visibility}, source_pack={source_pack_visibility}\n- Measurement kind: {measurement_kind}\n- Measurement caveat: {measurement_caveat}\n- Structured summary JSON: {summary_json}\n- Final output: {final_output}\n- Source diagnostics JSON: {diagnostics_json}\n- Controller artifacts JSON: {controller_json}\n- Resolved system prompt: {system_prompt}\n- Resolved user prompt: {user_prompt}\n\n### Rubric Dimensions\n{dimensions}\n\n### Critical Failure Flags\n{critical_flags}\n\n### Must-Pass Evidence Checks\n{checks}\n\n### Expected Failure Modes\n{failures}\n\n### Artifact And Diagnostic Availability\n- source diagnostics envelope: {source_diag_state}\n- controller artifacts envelope: {controller_state}\n- structured context packing diagnostics: {context_pack_state}\n- fixture-only pipeline evidence label: {fixture_label}\n- narrative state in controller artifacts: {narrative_state_present}\n- narrative metrics: timeline={narrative_timeline_event_count} sections={narrative_section_count} evidence_layers={narrative_evidence_layer_count} tensions={narrative_interpretive_tension_count} impacts={narrative_impact_count} reader_questions={narrative_reader_question_count} open_gaps={narrative_open_gap_count}\n- reader-quality metrics: present={reader_quality_present} argument_nodes={reader_argument_node_count} argument_edges={reader_argument_edge_count} narrative_plan={reader_narrative_plan_present} section_briefs={reader_section_brief_count} critique_present={reader_critique_present} critique_metrics={reader_critique_metric_count} critique_failed_metrics={reader_critique_failed_metric_count}\n- context-pack narrative diagnostics: present={context_pack_narrative_state_present} open_gaps={context_pack_narrative_open_gap_count}\n- context-pack reader-quality diagnostics: present={context_pack_reader_quality_present} section_briefs={context_pack_reader_section_brief_count} critique_metrics={context_pack_reader_critique_metric_count}\n",
        title = case.title,
        category = case.category,
        filename = case.filename,
        prompt = case.prompt,
        status = result.status,
        quality_status = result.quality_status.as_deref().unwrap_or("none"),
        quality_last_failure = result.quality_last_failure.as_deref().unwrap_or("none"),
        replay_before = replay_before
            .map(|before| format!(
                "status={} quality={} critical_flags={} last_failure={}",
                before.status,
                before.quality_status,
                before.critical_failure_count,
                before.quality_last_failure
            ))
            .unwrap_or_else(|| "not applicable".to_string()),
        overall_score = scorecard.overall_score,
        critical_failure_count = scorecard
            .critical_flags
            .iter()
            .filter(|flag| flag.triggered)
            .count(),
        task_visibility = scorecard.visibility.task_status,
        quality_visibility = scorecard.visibility.quality_status,
        source_pack_visibility = scorecard.visibility.source_pack_status,
        measurement_kind = scorecard.measurement_kind,
        measurement_caveat = scorecard.evidence_caveat,
        summary_json = artifact_paths
            .summary_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        final_output = artifact_paths
            .final_output_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| result
                .error_message
                .clone()
                .unwrap_or_else(|| "unavailable".to_string())),
        diagnostics_json = artifact_paths
            .diagnostics_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        controller_json = artifact_paths
            .controller_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        system_prompt = artifact_paths
            .resolved_system_prompt_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        user_prompt = artifact_paths
            .resolved_user_prompt_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        checks = case
            .must_pass_checks
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        failures = case
            .expected_failure_modes
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dimensions = scorecard
            .dimensions
            .iter()
            .map(|dimension| {
                format!(
                    "- {}: {}/5 [{}; {}]",
                    dimension.label, dimension.score, dimension.score_source, dimension.rationale
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        critical_flags = scorecard
            .critical_flags
            .iter()
            .map(|flag| {
                format!(
                    "- {}: {} ({})",
                    flag.label,
                    if flag.triggered { "triggered" } else { "clear" },
                    flag.rationale
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        source_diag_state = if result.research_source_diagnostics_json.is_some() {
            "present"
        } else {
            "missing"
        },
        controller_state = if result.research_controller_artifacts_json.is_some() {
            "present"
        } else {
            "missing"
        },
        context_pack_state = result
            .research_source_diagnostics_json
            .as_deref()
            .is_some_and(|json| json.contains("\"context_packing\""))
            .then_some("present")
            .unwrap_or("missing"),
        fixture_label = if scorecard.visibility.fixture_only_pipeline_evidence {
            PIPELINE_EVIDENCE_CAVEAT
        } else {
            "not applicable"
        },
        narrative_state_present = if scorecard.metrics.narrative_state_present {
            "present"
        } else {
            "missing"
        },
        narrative_timeline_event_count = scorecard.metrics.narrative_timeline_event_count,
        narrative_section_count = scorecard.metrics.narrative_section_count,
        narrative_evidence_layer_count = scorecard.metrics.narrative_evidence_layer_count,
        narrative_interpretive_tension_count =
            scorecard.metrics.narrative_interpretive_tension_count,
        narrative_impact_count = scorecard.metrics.narrative_impact_count,
        narrative_reader_question_count = scorecard.metrics.narrative_reader_question_count,
        narrative_open_gap_count = scorecard.metrics.narrative_open_gap_count,
        reader_quality_present = scorecard.metrics.reader_quality_present,
        reader_argument_node_count = scorecard.metrics.reader_argument_node_count,
        reader_argument_edge_count = scorecard.metrics.reader_argument_edge_count,
        reader_narrative_plan_present = scorecard.metrics.reader_narrative_plan_present,
        reader_section_brief_count = scorecard.metrics.reader_section_brief_count,
        reader_critique_present = scorecard.metrics.reader_critique_present,
        reader_critique_metric_count = scorecard.metrics.reader_critique_metric_count,
        reader_critique_failed_metric_count = scorecard.metrics.reader_critique_failed_metric_count,
        context_pack_narrative_state_present =
            scorecard.metrics.context_pack_narrative_state_present,
        context_pack_narrative_open_gap_count =
            scorecard.metrics.context_pack_narrative_open_gap_count,
        context_pack_reader_quality_present = scorecard.metrics.context_pack_reader_quality_present,
        context_pack_reader_section_brief_count =
            scorecard.metrics.context_pack_reader_section_brief_count,
        context_pack_reader_critique_metric_count =
            scorecard.metrics.context_pack_reader_critique_metric_count,
    )
}

#[derive(Debug, Default, Clone)]
struct CaseArtifactPaths {
    final_output_path: Option<PathBuf>,
    diagnostics_json_path: Option<PathBuf>,
    controller_json_path: Option<PathBuf>,
    resolved_system_prompt_path: Option<PathBuf>,
    resolved_user_prompt_path: Option<PathBuf>,
    summary_json_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct FallbackArtifactWriteOutcome {
    paths: CaseArtifactPaths,
    error: Option<String>,
}

fn synthetic_failed_result_for_case(
    case: &BenchmarkCase,
    mode: ResearchBenchmarkMode,
    model_input: &str,
    data_dir: PathBuf,
    stage: &str,
    error_message: impl Into<String>,
) -> ResearchBenchmarkCaseResult {
    let error_message = error_message.into();
    ResearchBenchmarkCaseResult {
        case_id: case.case_id.clone(),
        title: case.title.clone(),
        category: case.category.clone(),
        mode,
        data_dir,
        task_id: 0,
        status: "failed".to_string(),
        error_message: Some(error_message.clone()),
        quality_status: Some("failed".to_string()),
        quality_last_failure: Some(error_message),
        research_controller_stage: Some(stage.to_string()),
        research_controller_iteration: None,
        research_controller_max_iterations: None,
        output_filename: None,
        final_output: None,
        research_controller_artifacts_json: None,
        research_source_diagnostics_json: None,
        resolved_system_prompt: None,
        resolved_user_prompt: None,
        model_input: model_input.to_string(),
    }
}

fn synthetic_failed_result_from_existing(
    mut result: ResearchBenchmarkCaseResult,
    stage: &str,
    error_message: String,
) -> ResearchBenchmarkCaseResult {
    result.status = "failed".to_string();
    result.quality_status = Some("failed".to_string());
    result.quality_last_failure = Some(error_message.clone());
    result.research_controller_stage = Some(stage.to_string());
    append_result_error_message(&mut result, error_message);
    result
}

fn append_result_error_message(result: &mut ResearchBenchmarkCaseResult, message: String) {
    match result.error_message.as_mut() {
        Some(existing) if !existing.contains(&message) => {
            existing.push_str("; ");
            existing.push_str(&message);
        }
        Some(_) => {}
        None => result.error_message = Some(message),
    }
}

fn render_failure_markdown_artifact(
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> String {
    let critical_flags = scorecard
        .critical_flags
        .iter()
        .filter(|flag| flag.triggered)
        .map(|flag| format!("- {}: {}", flag.label, flag.rationale))
        .collect::<Vec<_>>();
    format!(
        "# Benchmark Case Failure\n\n- Case ID: {}\n- Title: {}\n- Category: {}\n- Mode: {}\n- Task status: {}\n- Quality status: {}\n- Controller stage: {}\n- Error: {}\n- Structured overall score: {:.2}/5.00\n- Critical failure flags: {}\n\n## Failure Summary\n{}\n\n## Artifact Availability\n- final output captured in task result: {}\n- controller artifacts JSON: {}\n- source diagnostics JSON: {}\n- resolved system prompt: {}\n- resolved user prompt: {}\n",
        result.case_id,
        result.title,
        result.category,
        benchmark_mode_label(result.mode),
        result.status,
        result.quality_status.as_deref().unwrap_or("unknown"),
        result
            .research_controller_stage
            .as_deref()
            .unwrap_or("unknown"),
        result
            .error_message
            .as_deref()
            .or(result.quality_last_failure.as_deref())
            .unwrap_or("unknown failure"),
        scorecard.overall_score,
        critical_flags.len(),
        if critical_flags.is_empty() {
            "- none recorded".to_string()
        } else {
            critical_flags.join("\n")
        },
        if result.final_output.is_some() {
            "yes"
        } else {
            "no"
        },
        if result.research_controller_artifacts_json.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.research_source_diagnostics_json.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.resolved_system_prompt.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.resolved_user_prompt.is_some() {
            "present"
        } else {
            "missing"
        },
    )
}

fn write_case_summary_json(
    path: &Path,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> Result<(), Box<dyn std::error::Error>> {
    write_new_text_file(
        path,
        &serde_json::to_string_pretty(&json!({
            "case_id": result.case_id,
            "title": result.title,
            "category": result.category,
            "mode": match result.mode {
                ResearchBenchmarkMode::Fixture => "fixture",
                ResearchBenchmarkMode::Live => "live",
                ResearchBenchmarkMode::Replay => "replay",
            },
            "task_id": result.task_id,
            "status": result.status,
            "error_message": result.error_message,
            "quality_status": result.quality_status,
            "quality_last_failure": result.quality_last_failure,
            "research_controller_stage": result.research_controller_stage,
            "research_controller_iteration": result.research_controller_iteration,
            "research_controller_max_iterations": result.research_controller_max_iterations,
            "output_filename": result.output_filename,
            "model_input": result.model_input,
            "structured_scorecard": scorecard,
        }))?,
    )
}

fn write_case_failure_fallback_artifacts(
    run_case_dir: &Path,
    artifact_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> FallbackArtifactWriteOutcome {
    let unique_suffix = Uuid::new_v4().simple().to_string();
    let fallback_stem = format!("{artifact_stem}-failure-{unique_suffix}");
    write_case_failure_fallback_artifacts_with_stem(run_case_dir, &fallback_stem, result, scorecard)
}

fn write_case_failure_fallback_artifacts_with_stem(
    run_case_dir: &Path,
    fallback_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> FallbackArtifactWriteOutcome {
    let failure_path = run_case_dir.join(format!("{fallback_stem}.md"));
    let mut paths = CaseArtifactPaths::default();
    if let Err(error) = write_new_text_file(
        &failure_path,
        &render_failure_markdown_artifact(result, scorecard),
    ) {
        return FallbackArtifactWriteOutcome {
            paths,
            error: Some(format!("failure markdown write failed: {error}")),
        };
    }
    paths.final_output_path = Some(failure_path);
    let summary_path = run_case_dir.join(format!("{fallback_stem}.json"));
    match write_case_summary_json(&summary_path, result, scorecard) {
        Ok(()) => {
            paths.summary_json_path = Some(summary_path);
            FallbackArtifactWriteOutcome { paths, error: None }
        }
        Err(error) => FallbackArtifactWriteOutcome {
            paths,
            error: Some(format!("failure summary JSON write failed: {error}")),
        },
    }
}

fn write_case_artifacts(
    run_case_dir: &Path,
    artifact_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
    include_raw_debug_artifacts: bool,
) -> Result<CaseArtifactPaths, Box<dyn std::error::Error>> {
    let mut paths = CaseArtifactPaths::default();
    let path = run_case_dir.join(format!("{artifact_stem}-final-output.md"));
    let output = case_final_output_artifact_markdown(result)
        .unwrap_or_else(|| render_failure_markdown_artifact(result, scorecard));
    write_new_text_file(&path, &output)?;
    paths.final_output_path = Some(path);
    if include_raw_debug_artifacts {
        if let Some(json_body) = result.research_source_diagnostics_json.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-source-diagnostics.json"));
            write_new_text_file(&path, json_body)?;
            paths.diagnostics_json_path = Some(path);
        }
        if let Some(json_body) = result.research_controller_artifacts_json.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-controller-artifacts.json"));
            write_new_text_file(&path, json_body)?;
            paths.controller_json_path = Some(path);
        }
        if let Some(prompt) = result.resolved_system_prompt.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-resolved-system-prompt.md"));
            write_new_text_file(&path, prompt)?;
            paths.resolved_system_prompt_path = Some(path);
        }
        if let Some(prompt) = result.resolved_user_prompt.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-resolved-user-prompt.md"));
            write_new_text_file(&path, prompt)?;
            paths.resolved_user_prompt_path = Some(path);
        }
    }
    let summary_path = run_case_dir.join(format!("{artifact_stem}.json"));
    write_case_summary_json(&summary_path, result, scorecard)?;
    paths.summary_json_path = Some(summary_path);
    Ok(paths)
}

fn case_final_output_artifact_markdown(result: &ResearchBenchmarkCaseResult) -> Option<String> {
    result
        .final_output
        .as_deref()
        .map(strip_internal_research_artifact_block_from_final_output)
}

fn strip_internal_research_artifact_block_from_final_output(output: &str) -> String {
    let stripped = strip_research_artifact_blocks(output);
    strip_internal_repair_metadata_lines(&stripped)
}

fn strip_internal_repair_metadata_lines(output: &str) -> String {
    output
        .lines()
        .filter(|line| !is_internal_repair_metadata_line(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn is_internal_repair_metadata_line(line: &str) -> bool {
    let normalized = line.trim();
    let lower = normalized.to_ascii_lowercase();
    lower.starts_with("- debt-research-quality-gate-failed-")
        || lower.contains("research quality gate failed:")
        || lower.contains(
            "repair the failed quality gate item with stronger evidence or a narrower claim",
        )
}

fn build_structured_run_report(
    args: &Args,
    mode: ResearchBenchmarkMode,
    timestamp: &str,
    engine_name: &str,
    model_name: &str,
    run_case_dir: &Path,
    executions: &[RunExecution],
) -> BenchmarkRunStructuredReport {
    let measurement = measurement_metadata(mode);
    let status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.result.status.clone()),
    );
    let quality_status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.scorecard.visibility.quality_status.clone()),
    );
    let source_pack_status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.scorecard.visibility.source_pack_status.clone()),
    );
    let overall_scores = executions
        .iter()
        .map(|execution| execution.scorecard.overall_score)
        .collect::<Vec<_>>();
    let critical_failure_case_ids = executions
        .iter()
        .filter(|execution| execution.scorecard.any_critical_failure)
        .map(|execution| execution.case.case_id.clone())
        .collect::<Vec<_>>();
    let quality_failed_case_ids = executions
        .iter()
        .filter(|execution| execution.scorecard.visibility.quality_status != "passed")
        .map(|execution| execution.case.case_id.clone())
        .collect::<Vec<_>>();
    BenchmarkRunStructuredReport {
        label: args.label.clone(),
        generated_at: timestamp.to_string(),
        mode: benchmark_mode_label(mode).to_string(),
        measurement_kind: measurement.measurement_kind.to_string(),
        evidence_caveat: measurement.evidence_caveat.to_string(),
        engine_name: engine_name.to_string(),
        model_name: model_name.to_string(),
        configured_search_providers: configured_search_providers(),
        research_intensity: args.research_intensity.clone(),
        quality_depth: args.quality_depth.clone(),
        max_iterations: args.max_iterations,
        cases_dir: args.cases_dir.display().to_string(),
        run_artifact_dir: run_case_dir.display().to_string(),
        summary: BenchmarkRunSummary {
            case_count: executions.len(),
            completed_case_count: executions
                .iter()
                .filter(|execution| execution.result.status == "completed")
                .count(),
            failed_case_count: executions
                .iter()
                .filter(|execution| execution.result.status != "completed")
                .count(),
            quality_passed_case_count: executions
                .iter()
                .filter(|execution| execution.scorecard.visibility.quality_status == "passed")
                .count(),
            critical_failure_case_count: critical_failure_case_ids.len(),
            overall_average_score: average_f64(&overall_scores),
            overall_min_score: min_f64(&overall_scores),
            overall_max_score: max_f64(&overall_scores),
            status_counts,
            quality_status_counts,
            source_pack_status_counts,
            critical_failure_case_ids,
            quality_failed_case_ids,
        },
        dimension_aggregates: build_dimension_aggregates(executions),
        cases: executions
            .iter()
            .map(|execution| {
                let mut scorecard = execution.scorecard.clone();
                scorecard.replay_before = execution.replay_before.clone();
                scorecard
            })
            .collect(),
    }
}

fn build_dimension_aggregates(executions: &[RunExecution]) -> Vec<BenchmarkDimensionAggregate> {
    RUBRIC_DIMENSIONS
        .iter()
        .map(|spec| {
            let scores = executions
                .iter()
                .filter_map(|execution| {
                    execution
                        .scorecard
                        .dimensions
                        .iter()
                        .find(|dimension| dimension.key == spec.key)
                        .map(|dimension| dimension.score)
                })
                .collect::<Vec<_>>();
            BenchmarkDimensionAggregate {
                key: spec.key.to_string(),
                label: spec.label.to_string(),
                average_score: average_u8(&scores),
                min_score: scores.iter().copied().min().unwrap_or(0),
                max_score: scores.iter().copied().max().unwrap_or(0),
                case_count: scores.len(),
            }
        })
        .collect()
}

fn write_structured_run_outputs(
    runs_dir: &Path,
    label: &str,
    report: &BenchmarkRunStructuredReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_path = runs_dir.join(format!("{label}.json"));
    let csv_path = runs_dir.join(format!("{label}.csv"));
    let ndjson_path = runs_dir.join(format!("{label}.ndjson"));
    write_new_text_file(&json_path, &serde_json::to_string_pretty(report)?)?;
    write_new_text_file(&csv_path, &render_case_scores_csv(report))?;
    write_new_text_file(&ndjson_path, &render_dimension_scores_ndjson(report)?)?;
    Ok(())
}

fn build_case_scorecard(
    case: &BenchmarkCase,
    result: &ResearchBenchmarkCaseResult,
) -> BenchmarkCaseScorecard {
    let measurement = measurement_metadata(result.mode);
    let visible_output = result
        .final_output
        .as_deref()
        .map(strip_research_artifact_blocks)
        .unwrap_or_default();
    let lower_visible_output = visible_output.to_lowercase();
    let reader_facing_output = reader_facing_output_before_appendix(&visible_output);
    let controller = result
        .research_controller_artifacts_json
        .as_deref()
        .and_then(|body| serde_json::from_str::<ParsedControllerArtifacts>(body).ok());
    let diagnostics = result
        .research_source_diagnostics_json
        .as_deref()
        .and_then(|body| serde_json::from_str::<ParsedSourceDiagnosticsEnvelope>(body).ok());
    let prompt_terms = significant_prompt_terms(&case.prompt);
    let prompt_term_hits = prompt_terms
        .iter()
        .filter(|term| lower_visible_output.contains(term.as_str()))
        .count();
    let reader_facing_prompt_echo_count =
        count_prompt_echo_windows(&case.prompt, &reader_facing_output.to_lowercase());
    let reader_facing_artifact_leak_count =
        count_reader_facing_artifact_leaks(&reader_facing_output.to_lowercase());
    let final_answer_present = result
        .final_output
        .as_deref()
        .is_some_and(has_visible_final_answer_section);
    let source_audit_present = contains_any_heading(&lower_visible_output, SOURCE_AUDIT_HEADINGS);
    let claim_log_present = contains_any_heading(&lower_visible_output, CLAIM_LOG_HEADINGS);
    let quality_gate_present = contains_any_heading(&lower_visible_output, QUALITY_GATE_HEADINGS);
    let verification_appendix_present =
        contains_any_heading(&lower_visible_output, VERIFICATION_APPENDIX_HEADINGS);
    let section_count = [
        final_answer_present,
        source_audit_present,
        claim_log_present,
        quality_gate_present,
        verification_appendix_present,
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let visibility = BenchmarkCaseVisibility {
        task_status: result.status.clone(),
        quality_status: result
            .quality_status
            .clone()
            .or_else(|| {
                controller
                    .as_ref()
                    .and_then(|artifacts| artifacts.quality_gate.as_ref())
                    .map(|gate| gate.status.clone())
            })
            .unwrap_or_else(|| "unknown".to_string()),
        source_pack_status: diagnostics
            .as_ref()
            .and_then(|envelope| envelope.source_pack.as_ref())
            .map(|report| report.status.clone())
            .unwrap_or_else(|| "missing".to_string()),
        visible_final_answer: final_answer_present,
        visible_source_audit: source_audit_present,
        visible_claim_log: claim_log_present,
        visible_quality_gate: quality_gate_present,
        visible_verification_appendix: verification_appendix_present,
        fixture_only_pipeline_evidence: matches!(result.mode, ResearchBenchmarkMode::Fixture),
        visible_in_aggregate: true,
    };
    let metrics = collect_benchmark_metrics(
        &visible_output,
        &reader_facing_output,
        case,
        prompt_terms.len(),
        prompt_term_hits,
        reader_facing_prompt_echo_count,
        reader_facing_artifact_leak_count,
        section_count,
        controller.as_ref(),
        diagnostics.as_ref(),
    );
    let advisory_model_self_score = extract_advisory_model_self_score(&visible_output);
    let critical_flags = build_critical_flags(case, &visibility, &metrics, controller.as_ref());
    let historical_overlay_trigger_count = critical_flags
        .iter()
        .filter(|flag| flag.triggered && flag.key.starts_with("historical_"))
        .count();
    let any_critical_failure = critical_flags.iter().any(|flag| flag.triggered);
    let dimensions = build_dimension_scores(
        case,
        result,
        &visibility,
        &metrics,
        controller.as_ref(),
        diagnostics.as_ref(),
    );
    let overall_score = average_u8(
        &dimensions
            .iter()
            .map(|dimension| dimension.score)
            .collect::<Vec<_>>(),
    );
    let overall_score =
        apply_historical_overlay_score_cap(overall_score, historical_overlay_trigger_count);
    let mut warnings = Vec::new();
    if controller.is_none() && result.research_controller_artifacts_json.is_some() {
        warnings.push("controller_artifacts_json could not be parsed".to_string());
    }
    if diagnostics.is_none() && result.research_source_diagnostics_json.is_some() {
        warnings.push("research_source_diagnostics_json could not be parsed".to_string());
    }
    if let Some(warning) = source_pack_provider_limitation_warning(
        diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.source_pack.as_ref()),
        &configured_search_providers(),
        result.mode,
    ) {
        warnings.push(warning);
    }
    if metrics.target_host_miss_count > 0 || metrics.source_class_miss_count > 0 {
        warnings.push(format!(
            "targeted source coverage misses remain visible: target_host_miss_count={} source_class_miss_count={}",
            metrics.target_host_miss_count, metrics.source_class_miss_count
        ));
    }
    if metrics.narrative_state_present || metrics.context_pack_narrative_state_present {
        warnings.push(format!(
            "narrative diagnostics: controller_present={} timeline={} sections={} evidence_layers={} tensions={} impacts={} reader_questions={} open_gaps={} context_pack_present={} context_pack_open_gaps={}",
            metrics.narrative_state_present,
            metrics.narrative_timeline_event_count,
            metrics.narrative_section_count,
            metrics.narrative_evidence_layer_count,
            metrics.narrative_interpretive_tension_count,
            metrics.narrative_impact_count,
            metrics.narrative_reader_question_count,
            metrics.narrative_open_gap_count,
            metrics.context_pack_narrative_state_present,
            metrics.context_pack_narrative_open_gap_count
        ));
    }
    if metrics.reader_quality_present || metrics.context_pack_reader_quality_present {
        warnings.push(format!(
            "reader-quality diagnostics: controller_present={} argument_nodes={} argument_edges={} narrative_plan={} section_briefs={} critique_present={} critique_metrics={} critique_failed_metrics={} context_pack_present={} context_pack_section_briefs={} context_pack_critique_metrics={}",
            metrics.reader_quality_present,
            metrics.reader_argument_node_count,
            metrics.reader_argument_edge_count,
            metrics.reader_narrative_plan_present,
            metrics.reader_section_brief_count,
            metrics.reader_critique_present,
            metrics.reader_critique_metric_count,
            metrics.reader_critique_failed_metric_count,
            metrics.context_pack_reader_quality_present,
            metrics.context_pack_reader_section_brief_count,
            metrics.context_pack_reader_critique_metric_count
        ));
    }
    warnings.push(
        measurement_metadata(result.mode)
            .evidence_caveat
            .to_string(),
    );
    if historical_overlay_trigger_count > 0 {
        warnings.push(format!(
            "historical overlay downgraded this case because {} category-specific history gate(s) triggered",
            historical_overlay_trigger_count
        ));
    }
    if is_historical_benchmark_category(&case.category) {
        if metrics.genre_section_richness_signal_count < 3 {
            warnings.push(format!(
                "historical richness remains shallow: visible richness coverage={} comparison={} chronology_interpretation={} source_layers={} issue_map={} legacy={} follow_up={}",
                metrics.genre_section_richness_signal_count,
                metrics.historical_comparison_signal_count,
                metrics.historical_chronology_interpretation_split_signal_count,
                metrics.historical_source_layer_signal_count,
                metrics.historical_issue_map_signal_count,
                metrics.historical_legacy_signal_count,
                metrics.historical_follow_up_signal_count
            ));
        }
        if !historical_hidden_artifacts_present(&metrics, controller.as_ref()) {
            warnings.push(
                "historical hidden planning artifacts are missing: persist useful narrative_state or reader_quality for strict/high historical runs"
                    .to_string(),
            );
        }
        let generic_open_debt_count = controller
            .as_ref()
            .map(historical_generic_open_debt_count)
            .unwrap_or(0);
        if generic_open_debt_count > 0 {
            warnings.push(format!(
                "historical open debt still uses generic missing-evidence placeholders: rows={}",
                generic_open_debt_count
            ));
        }
        if is_second_punic_benchmark_case(case)
            && (metrics.second_punic_visible_chars < SECOND_PUNIC_WAR_MIN_VISIBLE_CHARS
                || metrics.second_punic_phase_subsection_count
                    < SECOND_PUNIC_WAR_MIN_PHASE_SUBSECTIONS
                || metrics.second_punic_date_anchor_count < SECOND_PUNIC_WAR_MIN_DATE_ANCHORS
                || metrics.second_punic_subject_anchor_count < SECOND_PUNIC_WAR_MIN_SUBJECT_ANCHORS)
        {
            warnings.push(format!(
                "second punic war phase-density floor missed: visible_chars={} phase_subsections={} date_anchors={} subject_anchors={}",
                metrics.second_punic_visible_chars,
                metrics.second_punic_phase_subsection_count,
                metrics.second_punic_date_anchor_count,
                metrics.second_punic_subject_anchor_count
            ));
        }
    }
    BenchmarkCaseScorecard {
        case_id: case.case_id.clone(),
        title: case.title.clone(),
        category: case.category.clone(),
        mode: benchmark_mode_label(result.mode).to_string(),
        measurement_kind: measurement.measurement_kind.to_string(),
        evidence_caveat: measurement.evidence_caveat.to_string(),
        assessor: BENCHMARK_ASSESSOR.to_string(),
        score_source: BENCHMARK_SCORE_SOURCE.to_string(),
        advisory_model_self_score,
        overall_score,
        any_critical_failure,
        historical_overlay_trigger_count,
        dimensions,
        critical_flags,
        visibility,
        metrics,
        warnings,
        replay_before: None,
    }
}

fn collect_benchmark_metrics(
    visible_output: &str,
    reader_facing_output: &str,
    case: &BenchmarkCase,
    prompt_term_count: usize,
    prompt_term_hits: usize,
    reader_facing_prompt_echo_count: usize,
    reader_facing_artifact_leak_count: usize,
    section_count: usize,
    controller: Option<&ParsedControllerArtifacts>,
    diagnostics: Option<&ParsedSourceDiagnosticsEnvelope>,
) -> BenchmarkEvidenceMetrics {
    let (
        source_card_count,
        official_source_card_count,
        rumor_or_opinion_source_card_count,
        distinct_source_host_count,
        claim_count,
        supported_claim_count,
        support_url_count,
        conflict_count,
        resolved_conflict_count,
        promoted_conflict_count,
        open_debt_count,
        next_action_count,
        warning_count,
        unsupported_claim_count,
        unresolved_conflict_count,
        quality_gate_failure_count,
        invalid_source_card_url_count,
        narrative_state_present,
        narrative_timeline_event_count,
        narrative_section_count,
        narrative_evidence_layer_count,
        narrative_interpretive_tension_count,
        narrative_impact_count,
        narrative_reader_question_count,
        narrative_open_gap_count,
        reader_quality_present,
        reader_argument_node_count,
        reader_argument_edge_count,
        reader_narrative_plan_present,
        reader_section_brief_count,
        reader_critique_present,
        reader_critique_metric_count,
        reader_critique_failed_metric_count,
    ) = if let Some(controller) = controller {
        let source_hosts = controller
            .source_cards
            .iter()
            .filter_map(|card| normalized_host_from_url(&card.url))
            .collect::<BTreeSet<_>>();
        let supported_claim_count = controller
            .claim_log
            .iter()
            .filter(|claim| {
                !claim.support_source_card_ids.is_empty() || !claim.support_urls.is_empty()
            })
            .count();
        let support_url_count = controller
            .claim_log
            .iter()
            .map(|claim| claim.support_urls.len())
            .sum::<usize>();
        let resolved_conflict_count = controller
            .conflict_map
            .iter()
            .filter(|entry| {
                entry
                    .resolution_status
                    .as_deref()
                    .is_some_and(|status| status.eq_ignore_ascii_case("resolved"))
            })
            .count();
        let promoted_conflict_count = controller
            .conflict_map
            .iter()
            .filter(|entry| entry.promoted_to_debt == Some(true))
            .count();
        let open_debt_count = controller
            .research_debt
            .iter()
            .filter(|item| item.status.eq_ignore_ascii_case("open"))
            .count();
        let next_action_count = controller
            .research_debt
            .iter()
            .map(|item| item.next_check_actions.len())
            .sum::<usize>();
        let official_source_card_count = controller
            .source_cards
            .iter()
            .filter(|card| {
                let source_class = card.source_class.to_lowercase();
                SOURCE_CLASS_PRIMARY_MARKERS
                    .iter()
                    .any(|marker| source_class.contains(marker))
            })
            .count();
        let rumor_or_opinion_source_card_count = controller
            .source_cards
            .iter()
            .filter(|card| {
                let source_class = card.source_class.to_lowercase();
                SOURCE_CLASS_RUMOR_MARKERS
                    .iter()
                    .any(|marker| source_class.contains(marker))
            })
            .count();
        let invalid_source_card_url_count = controller
            .source_cards
            .iter()
            .filter(|card| !is_valid_http_url(&card.url))
            .count();
        let narrative_state_present = controller.narrative_state.is_some();
        let narrative_timeline_event_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.timeline.len())
            .unwrap_or_default();
        let narrative_section_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.section_outline.len())
            .unwrap_or_default();
        let narrative_evidence_layer_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.evidence_layers.len())
            .unwrap_or_default();
        let narrative_interpretive_tension_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.interpretive_tensions.len())
            .unwrap_or_default();
        let narrative_impact_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.impacts.len())
            .unwrap_or_default();
        let narrative_reader_question_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.reader_questions.len())
            .unwrap_or_default();
        let narrative_open_gap_count = controller
            .narrative_state
            .as_ref()
            .map(|state| state.open_gaps.len())
            .unwrap_or_default();
        let reader_quality_present = controller.reader_quality.is_some();
        let reader_argument_node_count = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.argument_graph.as_ref())
            .map(|graph| graph.nodes.len())
            .unwrap_or_default();
        let reader_argument_edge_count = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.argument_graph.as_ref())
            .map(|graph| graph.edges.len())
            .unwrap_or_default();
        let reader_narrative_plan_present = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.narrative_plan.as_ref())
            .is_some();
        let reader_section_brief_count = controller
            .reader_quality
            .as_ref()
            .map(|reader_quality| reader_quality.section_briefs.len())
            .unwrap_or_default();
        let reader_critique_present = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.reader_critique.as_ref())
            .is_some();
        let reader_critique_metric_count = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.reader_critique.as_ref())
            .map(|critique| critique.metrics.len())
            .unwrap_or_default();
        let reader_critique_failed_metric_count = controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.reader_critique.as_ref())
            .map(|critique| {
                critique
                    .metrics
                    .iter()
                    .filter(|metric| !metric.status.eq_ignore_ascii_case("passed"))
                    .count()
            })
            .unwrap_or_default();
        (
            controller.source_cards.len(),
            official_source_card_count,
            rumor_or_opinion_source_card_count,
            source_hosts.len(),
            controller.claim_log.len(),
            supported_claim_count,
            support_url_count,
            controller.conflict_map.len(),
            resolved_conflict_count,
            promoted_conflict_count,
            controller
                .quality_gate
                .as_ref()
                .map(|gate| gate.open_debt_count)
                .unwrap_or(open_debt_count),
            next_action_count,
            controller.warnings.len(),
            controller
                .quality_gate
                .as_ref()
                .map(|gate| gate.unsupported_claim_count)
                .unwrap_or_default(),
            controller
                .quality_gate
                .as_ref()
                .map(|gate| gate.unresolved_conflict_count)
                .unwrap_or_else(|| {
                    controller
                        .conflict_map
                        .len()
                        .saturating_sub(resolved_conflict_count)
                }),
            controller
                .quality_gate
                .as_ref()
                .map(|gate| gate.failure_messages.len())
                .unwrap_or_default(),
            invalid_source_card_url_count,
            narrative_state_present,
            narrative_timeline_event_count,
            narrative_section_count,
            narrative_evidence_layer_count,
            narrative_interpretive_tension_count,
            narrative_impact_count,
            narrative_reader_question_count,
            narrative_open_gap_count,
            reader_quality_present,
            reader_argument_node_count,
            reader_argument_edge_count,
            reader_narrative_plan_present,
            reader_section_brief_count,
            reader_critique_present,
            reader_critique_metric_count,
            reader_critique_failed_metric_count,
        )
    } else {
        (
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, false, 0, 0, 0, 0, 0, 0, 0, false,
            0, 0, false, 0, false, 0, 0,
        )
    };
    let (
        source_pack_query_count,
        source_pack_discovered_source_count,
        source_pack_adopted_source_count,
        source_pack_skipped_candidate_count,
        scrape_count,
        scrape_failure_count,
        context_pack_included_source_card_count,
        context_pack_omitted_source_card_count,
        context_pack_narrative_state_present,
        context_pack_narrative_open_gap_count,
        context_pack_reader_quality_present,
        context_pack_reader_section_brief_count,
        context_pack_reader_critique_metric_count,
        target_host_miss_count,
        source_class_miss_count,
    ) = if let Some(diagnostics) = diagnostics {
        (
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| report.queries.len())
                .unwrap_or_default(),
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| report.discovered_source_count)
                .unwrap_or_default(),
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| report.adopted_source_count)
                .unwrap_or_default(),
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| report.skipped_candidates.len())
                .unwrap_or_default(),
            diagnostics.scrapes.len(),
            diagnostics
                .scrapes
                .iter()
                .filter(|scrape| scrape.status_class != "success")
                .count(),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.included_source_card_count)
                .unwrap_or_default(),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.omitted_source_card_count)
                .unwrap_or_default(),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.narrative_state_present)
                .unwrap_or(false),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.narrative_open_gap_count)
                .unwrap_or_default(),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.reader_quality_present)
                .unwrap_or(false),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.reader_section_brief_count)
                .unwrap_or_default(),
            diagnostics
                .context_packing
                .as_ref()
                .map(|context| context.reader_critique_metric_count)
                .unwrap_or_default(),
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| {
                    report
                        .coverage_misses
                        .iter()
                        .filter(|miss| miss.expected_host.is_some() && miss.status != "adopted")
                        .count()
                })
                .unwrap_or_default(),
            diagnostics
                .source_pack
                .as_ref()
                .map(|report| {
                    report
                        .coverage_misses
                        .iter()
                        .filter(|miss| {
                            miss.expected_source_class.is_some() && miss.status != "adopted"
                        })
                        .count()
                })
                .unwrap_or_default(),
        )
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0, false, 0, false, 0, 0, 0, 0)
    };
    let historical_metrics = if is_historical_benchmark_category(&case.category) {
        collect_historical_overlay_metrics(reader_facing_output)
    } else {
        HistoricalOverlayMetrics::default()
    };
    let second_punic_metrics = if is_second_punic_benchmark_case(case) {
        collect_second_punic_benchmark_metrics(reader_facing_output)
    } else {
        SecondPunicBenchmarkMetrics::default()
    };
    let technology_genre_metrics = if is_technology_like_benchmark_category(&case.category) {
        collect_technology_genre_metrics(reader_facing_output)
    } else {
        TechnologyGenreMetrics::default()
    };
    let genre_section_richness_signal_count = if is_historical_benchmark_category(&case.category) {
        historical_metrics.richness_coverage_count()
    } else if is_technology_like_benchmark_category(&case.category) {
        technology_genre_metrics.coverage_count()
    } else {
        0
    };
    BenchmarkEvidenceMetrics {
        final_output_chars: visible_output.chars().count(),
        prompt_term_count,
        prompt_term_hits,
        reader_facing_prompt_echo_count,
        reader_facing_artifact_leak_count,
        section_count,
        source_card_count,
        official_source_card_count,
        rumor_or_opinion_source_card_count,
        distinct_source_host_count,
        claim_count,
        supported_claim_count,
        support_url_count,
        conflict_count,
        resolved_conflict_count,
        promoted_conflict_count,
        open_debt_count,
        next_action_count,
        warning_count,
        unsupported_claim_count,
        unresolved_conflict_count,
        quality_gate_failure_count,
        source_pack_query_count,
        source_pack_discovered_source_count,
        source_pack_adopted_source_count,
        source_pack_skipped_candidate_count,
        scrape_count,
        scrape_failure_count,
        context_pack_included_source_card_count,
        context_pack_omitted_source_card_count,
        narrative_state_present,
        narrative_timeline_event_count,
        narrative_section_count,
        narrative_evidence_layer_count,
        narrative_interpretive_tension_count,
        narrative_impact_count,
        narrative_reader_question_count,
        narrative_open_gap_count,
        reader_quality_present,
        reader_argument_node_count,
        reader_argument_edge_count,
        reader_narrative_plan_present,
        reader_section_brief_count,
        reader_critique_present,
        reader_critique_metric_count,
        reader_critique_failed_metric_count,
        context_pack_narrative_state_present,
        context_pack_narrative_open_gap_count,
        context_pack_reader_quality_present,
        context_pack_reader_section_brief_count,
        context_pack_reader_critique_metric_count,
        invalid_source_card_url_count,
        target_host_miss_count,
        source_class_miss_count,
        historical_chronology_signal_count: historical_metrics.chronology_signal_count,
        historical_actor_signal_count: historical_metrics.actor_signal_count,
        historical_geography_signal_count: historical_metrics.geography_signal_count,
        historical_cause_signal_count: historical_metrics.cause_signal_count,
        historical_consequence_signal_count: historical_metrics.consequence_signal_count,
        historical_evidence_limit_signal_count: historical_metrics.evidence_limit_signal_count,
        historical_contested_interpretation_signal_count: historical_metrics
            .contested_interpretation_signal_count,
        historical_scope_limit_signal_count: historical_metrics.scope_limit_signal_count,
        historical_lens_coverage_count: historical_metrics.coverage_count(),
        historical_comparison_signal_count: historical_metrics.comparison_signal_count,
        historical_chronology_interpretation_split_signal_count: historical_metrics
            .chronology_interpretation_split_signal_count,
        historical_source_layer_signal_count: historical_metrics.source_layer_signal_count,
        historical_issue_map_signal_count: historical_metrics.issue_map_signal_count,
        historical_legacy_signal_count: historical_metrics.legacy_signal_count,
        historical_follow_up_signal_count: historical_metrics.follow_up_signal_count,
        second_punic_visible_chars: second_punic_metrics.visible_chars,
        second_punic_phase_subsection_count: second_punic_metrics.phase_subsection_count,
        second_punic_date_anchor_count: second_punic_metrics.date_anchor_count,
        second_punic_subject_anchor_count: second_punic_metrics.subject_anchor_count,
        technology_design_judgment_signal_count: technology_genre_metrics
            .design_judgment_signal_count,
        technology_tradeoff_signal_count: technology_genre_metrics.tradeoff_signal_count,
        technology_verifiability_signal_count: technology_genre_metrics.verifiability_signal_count,
        genre_section_richness_signal_count,
    }
}

fn build_dimension_scores(
    case: &BenchmarkCase,
    result: &ResearchBenchmarkCaseResult,
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
    controller: Option<&ParsedControllerArtifacts>,
    diagnostics: Option<&ParsedSourceDiagnosticsEnvelope>,
) -> Vec<BenchmarkRubricDimensionRecord> {
    RUBRIC_DIMENSIONS
        .iter()
        .map(|spec| {
            let (score, rationale, evidence_refs) = match spec.key {
                "user_intent_fit" => score_user_intent_fit(case, result, visibility, metrics),
                "direct_answer_usefulness" => {
                    score_direct_answer_usefulness(result, visibility, metrics)
                }
                "evidence_claim_traceability" => score_evidence_claim_traceability(metrics),
                "source_quality_and_diversity" => score_source_quality_and_diversity(metrics),
                "official_rumor_opinion_separation" => {
                    score_official_rumor_opinion_separation(metrics)
                }
                "uncertainty_and_conflict_handling" => {
                    score_uncertainty_and_conflict_handling(metrics)
                }
                "information_resolution" => score_information_resolution(metrics),
                "practical_next_steps" => score_practical_next_steps(
                    visibility,
                    metrics,
                    result.final_output.as_deref().unwrap_or_default(),
                ),
                "structure_and_readability" => score_structure_and_readability(visibility, metrics),
                "genre_section_richness" => score_genre_section_richness(case, visibility, metrics),
                _ => (0, "unknown rubric dimension".to_string(), Vec::new()),
            };
            let mut evidence_refs = evidence_refs;
            if controller.is_none() {
                evidence_refs.push("controller_artifacts.missing".to_string());
            }
            if diagnostics.is_none() {
                evidence_refs.push("source_diagnostics.missing".to_string());
            }
            BenchmarkRubricDimensionRecord {
                key: spec.key.to_string(),
                label: spec.label.to_string(),
                score,
                max_score: 5,
                assessor: BENCHMARK_ASSESSOR.to_string(),
                score_source: BENCHMARK_SCORE_SOURCE.to_string(),
                visibility: SCORE_VISIBILITY.to_string(),
                rationale,
                evidence_refs,
            }
        })
        .collect()
}

fn build_critical_flags(
    case: &BenchmarkCase,
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
    controller: Option<&ParsedControllerArtifacts>,
) -> Vec<BenchmarkCriticalFlagRecord> {
    let prompt_hits_threshold = metrics.prompt_term_count.min(2);
    let rumor_or_opinion_as_official = metrics.rumor_or_opinion_source_card_count > 0
        && metrics.official_source_card_count == 0
        && visibility.quality_status == "passed";
    let final_answer_without_evidence = visibility.visible_final_answer
        && (metrics.supported_claim_count == 0
            || metrics.source_card_count == 0
            || (!visibility.visible_claim_log && metrics.claim_count == 0));
    CRITICAL_FLAG_SPECS
        .iter()
        .map(|spec| {
            let (triggered, rationale) = match spec.key {
                "unsupported_factual_claim" => (
                    metrics.unsupported_claim_count > 0,
                    format!(
                        "quality gate unsupported_claim_count={}",
                        metrics.unsupported_claim_count
                    ),
                ),
                "fabricated_source_or_url" => (
                    metrics.invalid_source_card_url_count > 0,
                    format!(
                        "invalid source card URL count={}",
                        metrics.invalid_source_card_url_count
                    ),
                ),
                "fails_to_answer_question" => (
                    !visibility.visible_final_answer,
                    format!(
                        "visible final answer section present={}",
                        visibility.visible_final_answer
                    ),
                ),
                "rumor_as_official_fact" => (
                    rumor_or_opinion_as_official,
                    format!(
                        "rumor/opinion sources={} official sources={} quality_status={}",
                        metrics.rumor_or_opinion_source_card_count,
                        metrics.official_source_card_count,
                        visibility.quality_status
                    ),
                ),
                "ignores_major_prompt_constraint" => (
                    metrics.prompt_term_hits < prompt_hits_threshold,
                    format!(
                        "prompt term hits={} threshold={} for case {}",
                        metrics.prompt_term_hits, prompt_hits_threshold, case.case_id
                    ),
                ),
                "final_answer_without_meaningful_evidence" => (
                    final_answer_without_evidence,
                    format!(
                        "visible_final_answer={} source_cards={} supported_claims={} claim_log_visible={}",
                        visibility.visible_final_answer,
                        metrics.source_card_count,
                        metrics.supported_claim_count,
                        visibility.visible_claim_log
                    ),
                ),
                "reader_facing_prompt_or_artifact_leakage" => (
                    metrics.reader_facing_prompt_echo_count > 0
                        || metrics.reader_facing_artifact_leak_count > 0,
                    format!(
                        "reader_facing_prompt_echo_count={} reader_facing_artifact_leak_count={}",
                        metrics.reader_facing_prompt_echo_count,
                        metrics.reader_facing_artifact_leak_count
                    ),
                ),
                "historical_missing_chronology" => (
                    is_historical_benchmark_category(&case.category)
                        && visibility.visible_final_answer
                        && metrics.historical_chronology_signal_count == 0,
                    format!(
                        "historical chronology signals={}",
                        metrics.historical_chronology_signal_count
                    ),
                ),
                "historical_missing_actors_or_geography" => (
                    is_historical_benchmark_category(&case.category)
                        && visibility.visible_final_answer
                        && (metrics.historical_actor_signal_count == 0
                            || metrics.historical_geography_signal_count == 0),
                    format!(
                        "historical actor signals={} geography signals={}",
                        metrics.historical_actor_signal_count,
                        metrics.historical_geography_signal_count
                    ),
                ),
                "historical_missing_causes_or_consequences" => (
                    is_historical_benchmark_category(&case.category)
                        && visibility.visible_final_answer
                        && (metrics.historical_cause_signal_count == 0
                            || metrics.historical_consequence_signal_count == 0),
                    format!(
                        "historical cause signals={} consequence signals={}",
                        metrics.historical_cause_signal_count,
                        metrics.historical_consequence_signal_count
                    ),
                ),
                "historical_missing_limits_or_contested_interpretation" => (
                    is_historical_benchmark_category(&case.category)
                        && visibility.visible_final_answer
                        && (metrics.historical_evidence_limit_signal_count == 0
                            || (metrics.historical_contested_interpretation_signal_count == 0
                                && metrics.historical_scope_limit_signal_count == 0)),
                    format!(
                        "historical evidence_limit signals={} contested signals={} scope_limit signals={}",
                        metrics.historical_evidence_limit_signal_count,
                        metrics.historical_contested_interpretation_signal_count,
                        metrics.historical_scope_limit_signal_count
                    ),
                ),
                "historical_campaign_phase_density_floor" => (
                    is_second_punic_benchmark_case(case)
                        && visibility.visible_final_answer
                        && (metrics.second_punic_visible_chars
                            < SECOND_PUNIC_WAR_MIN_VISIBLE_CHARS
                            || metrics.second_punic_phase_subsection_count
                                < SECOND_PUNIC_WAR_MIN_PHASE_SUBSECTIONS
                            || metrics.second_punic_date_anchor_count
                                < SECOND_PUNIC_WAR_MIN_DATE_ANCHORS
                            || metrics.second_punic_subject_anchor_count
                                < SECOND_PUNIC_WAR_MIN_SUBJECT_ANCHORS),
                    format!(
                        "second_punic_visible_chars={} phase_subsections={} date_anchors={} subject_anchors={}",
                        metrics.second_punic_visible_chars,
                        metrics.second_punic_phase_subsection_count,
                        metrics.second_punic_date_anchor_count,
                        metrics.second_punic_subject_anchor_count
                    ),
                ),
                _ => (false, "unknown critical flag".to_string()),
            };
            let mut rationale = rationale;
            if controller.is_none() {
                rationale.push_str("; controller_artifacts missing");
            }
            BenchmarkCriticalFlagRecord {
                key: spec.key.to_string(),
                label: spec.label.to_string(),
                triggered,
                assessor: BENCHMARK_ASSESSOR.to_string(),
                score_source: BENCHMARK_SCORE_SOURCE.to_string(),
                visibility: SCORE_VISIBILITY.to_string(),
                rationale,
            }
        })
        .collect()
}

fn score_user_intent_fit(
    case: &BenchmarkCase,
    result: &ResearchBenchmarkCaseResult,
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let mut score = 0;
    if result.final_output.is_some() {
        score += 1;
    }
    if visibility.visible_final_answer {
        score += 1;
    }
    if result.status == "completed" {
        score += 1;
    }
    if metrics.prompt_term_hits >= metrics.prompt_term_count.min(2) {
        score += 1;
    }
    if visibility.quality_status == "passed" {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "prompt term hits {}/{} for case {} with quality status {}",
            metrics.prompt_term_hits,
            metrics.prompt_term_count,
            case.case_id,
            visibility.quality_status
        ),
        vec![
            "final_output.present".to_string(),
            "final_answer.section".to_string(),
            "quality_status".to_string(),
        ],
    )
}

fn score_direct_answer_usefulness(
    result: &ResearchBenchmarkCaseResult,
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let mut score = 0;
    if result.final_output.is_some() {
        score += 1;
    }
    if visibility.visible_final_answer {
        score += 2;
    }
    if metrics.final_output_chars >= 700 {
        score += 1;
    }
    if result.status == "completed" {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "final output chars={} visible final answer={} task status={}",
            metrics.final_output_chars, visibility.visible_final_answer, result.status
        ),
        vec![
            "final_output.chars".to_string(),
            "final_answer.section".to_string(),
        ],
    )
}

fn score_evidence_claim_traceability(
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let ratio = if metrics.claim_count == 0 {
        0.0
    } else {
        metrics.supported_claim_count as f64 / metrics.claim_count as f64
    };
    let score = if metrics.claim_count == 0 || metrics.source_card_count == 0 {
        0
    } else if ratio < 0.5 {
        2
    } else if ratio < 1.0 {
        3
    } else if metrics.unsupported_claim_count == 0 && metrics.source_card_count >= 3 {
        5
    } else {
        4
    };
    (
        score,
        format!(
            "supported claims={} total claims={} unsupported_claim_count={}",
            metrics.supported_claim_count, metrics.claim_count, metrics.unsupported_claim_count
        ),
        vec![
            "claim_log.count".to_string(),
            "claim_log.supported_count".to_string(),
            "quality_gate.unsupported_claim_count".to_string(),
        ],
    )
}

fn score_source_quality_and_diversity(
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let mut score = 0;
    if metrics.source_card_count > 0 {
        score += 1;
    }
    if metrics.source_pack_adopted_source_count >= 2 || metrics.source_card_count >= 2 {
        score += 1;
    }
    if metrics.distinct_source_host_count >= 2 {
        score += 1;
    }
    if metrics.official_source_card_count >= 2 {
        score += 1;
    }
    if metrics.distinct_source_host_count >= 4 && metrics.official_source_card_count >= 3 {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "source_cards={} distinct_hosts={} official_sources={} adopted_sources={}",
            metrics.source_card_count,
            metrics.distinct_source_host_count,
            metrics.official_source_card_count,
            metrics.source_pack_adopted_source_count
        ),
        vec![
            "source_cards.count".to_string(),
            "source_cards.official_count".to_string(),
            "source_pack.adopted_source_count".to_string(),
        ],
    )
}

fn score_official_rumor_opinion_separation(
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let mut score = 0;
    if metrics.source_card_count > 0 {
        score += 1;
    }
    if metrics.official_source_card_count > 0 {
        score += 1;
    }
    if metrics.unsupported_claim_count == 0 {
        score += 1;
    }
    if metrics.rumor_or_opinion_source_card_count == 0 {
        score += 1;
    }
    if metrics.supported_claim_count == metrics.claim_count && metrics.claim_count > 0 {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "official_sources={} rumor_or_opinion_sources={} unsupported_claim_count={}",
            metrics.official_source_card_count,
            metrics.rumor_or_opinion_source_card_count,
            metrics.unsupported_claim_count
        ),
        vec![
            "source_cards.official_count".to_string(),
            "source_cards.rumor_or_opinion_count".to_string(),
            "quality_gate.unsupported_claim_count".to_string(),
        ],
    )
}

fn score_uncertainty_and_conflict_handling(
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let score = if metrics.conflict_count == 0 && metrics.open_debt_count == 0 {
        4
    } else if metrics.unresolved_conflict_count > 0 && metrics.open_debt_count == 0 {
        1
    } else {
        let mut score = 1;
        if metrics.resolved_conflict_count + metrics.promoted_conflict_count
            >= metrics.conflict_count
        {
            score += 2;
        }
        if metrics.next_action_count > 0 || metrics.open_debt_count == 0 {
            score += 1;
        }
        if metrics.quality_gate_failure_count == 0 || metrics.warning_count > 0 {
            score += 1;
        }
        score.min(5)
    };
    (
        score,
        format!(
            "conflicts={} resolved={} promoted_to_debt={} open_debt={} next_actions={}",
            metrics.conflict_count,
            metrics.resolved_conflict_count,
            metrics.promoted_conflict_count,
            metrics.open_debt_count,
            metrics.next_action_count
        ),
        vec![
            "conflict_map.count".to_string(),
            "research_debt.open_count".to_string(),
            "research_debt.next_action_count".to_string(),
        ],
    )
}

fn score_information_resolution(metrics: &BenchmarkEvidenceMetrics) -> (u8, String, Vec<String>) {
    let mut score = 0;
    if metrics.source_pack_adopted_source_count > 0 || metrics.source_card_count > 0 {
        score += 1;
    }
    if metrics.source_card_count >= 3 {
        score += 1;
    }
    if metrics.claim_count >= 3 {
        score += 1;
    }
    if metrics.context_pack_included_source_card_count > 0 || metrics.source_pack_query_count > 0 {
        score += 1;
    }
    if metrics.unsupported_claim_count == 0 && metrics.unresolved_conflict_count == 0 {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "adopted_sources={} source_cards={} claims={} context_included_sources={}",
            metrics.source_pack_adopted_source_count,
            metrics.source_card_count,
            metrics.claim_count,
            metrics.context_pack_included_source_card_count
        ),
        vec![
            "source_pack.adopted_source_count".to_string(),
            "source_cards.count".to_string(),
            "claim_log.count".to_string(),
            "context_packing.included_source_card_count".to_string(),
        ],
    )
}

fn score_practical_next_steps(
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
    final_output: &str,
) -> (u8, String, Vec<String>) {
    let lower_output = final_output.to_lowercase();
    let has_decision_terms = ACTION_OR_DECISION_TERMS
        .iter()
        .any(|term| lower_output.contains(term));
    let mut score = 0;
    if visibility.visible_final_answer {
        score += 2;
    }
    if has_decision_terms {
        score += 1;
    }
    if metrics.open_debt_count == 0 || metrics.next_action_count > 0 {
        score += 1;
    }
    if metrics.prompt_term_hits >= metrics.prompt_term_count.min(2) && metrics.prompt_term_count > 0
    {
        score += 1;
    }
    (
        score.min(5),
        format!(
            "decision_terms={} open_debt={} next_actions={} prompt_hits={}/{}",
            has_decision_terms,
            metrics.open_debt_count,
            metrics.next_action_count,
            metrics.prompt_term_hits,
            metrics.prompt_term_count
        ),
        vec![
            "final_answer.section".to_string(),
            "research_debt.next_action_count".to_string(),
        ],
    )
}

fn score_structure_and_readability(
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    let mut score = metrics.section_count.min(5) as i32;
    if metrics.reader_facing_prompt_echo_count > 0 {
        score -= 1;
    }
    if metrics.reader_facing_artifact_leak_count > 0 {
        score -= 2;
    }
    let score = score.clamp(0, 5) as u8;
    (
        score,
        format!(
            "sections present: final_answer={} source_audit={} claim_log={} quality_gate={} appendix={} reader_facing_prompt_echo_count={} reader_facing_artifact_leak_count={}",
            visibility.visible_final_answer,
            visibility.visible_source_audit,
            visibility.visible_claim_log,
            visibility.visible_quality_gate,
            visibility.visible_verification_appendix,
            metrics.reader_facing_prompt_echo_count,
            metrics.reader_facing_artifact_leak_count
        ),
        vec![
            "visible_output.sections".to_string(),
            "verification_appendix.section".to_string(),
            "reader_facing_output.leak_scan".to_string(),
        ],
    )
}

fn score_genre_section_richness(
    case: &BenchmarkCase,
    visibility: &BenchmarkCaseVisibility,
    metrics: &BenchmarkEvidenceMetrics,
) -> (u8, String, Vec<String>) {
    if !visibility.visible_final_answer {
        return (
            0,
            "visible final answer section missing".to_string(),
            vec!["final_answer.section".to_string()],
        );
    }

    if is_historical_benchmark_category(&case.category) {
        let coverage = metrics.genre_section_richness_signal_count;
        let score = match coverage {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 | 5 => 4,
            _ => 5,
        };
        return (
            score,
            format!(
                "historical richness coverage={} comparison={} chronology_interpretation={} source_layers={} issue_map={} legacy={} follow_up={}",
                coverage,
                metrics.historical_comparison_signal_count,
                metrics.historical_chronology_interpretation_split_signal_count,
                metrics.historical_source_layer_signal_count,
                metrics.historical_issue_map_signal_count,
                metrics.historical_legacy_signal_count,
                metrics.historical_follow_up_signal_count
            ),
            vec![
                "reader_facing_output.historical_scaffold".to_string(),
                "historical.genre_section_richness".to_string(),
            ],
        );
    }

    if is_technology_like_benchmark_category(&case.category) {
        let coverage = metrics.genre_section_richness_signal_count;
        let score = match coverage {
            0 => 0,
            1 => 2,
            2 => 4,
            _ => 5,
        };
        return (
            score,
            format!(
                "technology richness coverage={} design_judgment={} tradeoff={} verifiability={}",
                coverage,
                metrics.technology_design_judgment_signal_count,
                metrics.technology_tradeoff_signal_count,
                metrics.technology_verifiability_signal_count
            ),
            vec![
                "reader_facing_output.technology_scaffold".to_string(),
                "technology.genre_section_richness".to_string(),
            ],
        );
    }

    let score = if metrics.section_count >= 4 {
        5
    } else if metrics.section_count >= 3 {
        4
    } else {
        3
    };
    (
        score,
        format!(
            "generic category={} visible sections={}",
            case.category, metrics.section_count
        ),
        vec!["visible_output.sections".to_string()],
    )
}

fn reader_facing_output_before_appendix(visible_output: &str) -> String {
    let lower = visible_output.to_lowercase();
    let boundary = VERIFICATION_APPENDIX_HEADINGS
        .iter()
        .filter_map(|heading| lower.find(heading))
        .min();
    boundary
        .map(|index| visible_output[..index].trim().to_string())
        .unwrap_or_else(|| visible_output.trim().to_string())
}

fn count_reader_facing_artifact_leaks(lower_reader_facing_output: &str) -> usize {
    READER_FACING_ARTIFACT_LEAK_MARKERS
        .iter()
        .filter(|marker| lower_reader_facing_output.contains(**marker))
        .count()
}

#[derive(Debug, Clone, Copy, Default)]
struct HistoricalOverlayMetrics {
    chronology_signal_count: usize,
    actor_signal_count: usize,
    geography_signal_count: usize,
    cause_signal_count: usize,
    consequence_signal_count: usize,
    evidence_limit_signal_count: usize,
    contested_interpretation_signal_count: usize,
    scope_limit_signal_count: usize,
    comparison_signal_count: usize,
    chronology_interpretation_split_signal_count: usize,
    source_layer_signal_count: usize,
    issue_map_signal_count: usize,
    legacy_signal_count: usize,
    follow_up_signal_count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct SecondPunicBenchmarkMetrics {
    visible_chars: usize,
    phase_subsection_count: usize,
    date_anchor_count: usize,
    subject_anchor_count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct TechnologyGenreMetrics {
    design_judgment_signal_count: usize,
    tradeoff_signal_count: usize,
    verifiability_signal_count: usize,
}

impl HistoricalOverlayMetrics {
    fn coverage_count(self) -> usize {
        [
            self.chronology_signal_count > 0,
            self.actor_signal_count > 0,
            self.geography_signal_count > 0,
            self.cause_signal_count > 0,
            self.consequence_signal_count > 0,
            self.evidence_limit_signal_count > 0,
            self.contested_interpretation_signal_count > 0,
            self.scope_limit_signal_count > 0,
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }

    fn richness_coverage_count(self) -> usize {
        [
            self.comparison_signal_count > 0,
            self.chronology_interpretation_split_signal_count > 0,
            self.source_layer_signal_count > 0,
            self.issue_map_signal_count > 0,
            self.legacy_signal_count > 0,
            self.follow_up_signal_count > 0,
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}

impl TechnologyGenreMetrics {
    fn coverage_count(self) -> usize {
        [
            self.design_judgment_signal_count > 0,
            self.tradeoff_signal_count > 0,
            self.verifiability_signal_count > 0,
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}

fn is_historical_benchmark_category(category: &str) -> bool {
    matches!(category, "historical-research" | "historical-explanation")
}

fn is_second_punic_benchmark_case(case: &BenchmarkCase) -> bool {
    let subject = format!("{} {}", case.title, case.filename).to_ascii_lowercase();
    let prompt = case.prompt.to_ascii_lowercase();
    let combined = format!("{subject} {prompt}");
    let subject_has_first_or_third = benchmark_has_non_second_punic_war_marker(&subject);
    let combined_has_first_or_third = benchmark_has_non_second_punic_war_marker(&combined);
    let subject_has_hannibal_marker = benchmark_has_hannibal_marker(&subject);
    let combined_has_hannibal_marker = benchmark_has_hannibal_marker(&combined);
    let subject_has_second_punic_marker = benchmark_has_second_punic_marker(&subject);
    let combined_has_second_punic_marker = benchmark_has_second_punic_marker(&combined);

    if benchmark_second_punic_has_comparative_scope(
        &combined,
        combined_has_first_or_third,
        combined_has_second_punic_marker,
    ) && !benchmark_second_punic_has_centered_focus(
        &subject,
        &prompt,
        subject_has_first_or_third,
        subject_has_second_punic_marker,
        subject_has_hannibal_marker,
    ) {
        return false;
    }

    if combined_has_first_or_third && !combined_has_second_punic_marker {
        return false;
    }
    if combined_has_second_punic_marker || combined_has_hannibal_marker {
        return true;
    }

    combined.contains("포에니 전쟁")
        && ["제2차", "2차", "한니발"]
            .iter()
            .any(|marker| combined.contains(marker))
}

fn benchmark_has_non_second_punic_war_marker(text: &str) -> bool {
    [
        "first punic war",
        "3rd punic war",
        "third punic war",
        "제1차 포에니 전쟁",
        "제3차 포에니 전쟁",
        "1차 포에니 전쟁",
        "3차 포에니 전쟁",
        "제1차 포에닉 전쟁",
        "제3차 포에닉 전쟁",
    ]
    .iter()
    .any(|marker| text.contains(&marker.to_ascii_lowercase()))
}

fn benchmark_has_hannibal_marker(text: &str) -> bool {
    ["hannibal", "한니발"]
        .iter()
        .any(|marker| text.contains(&marker.to_ascii_lowercase()))
}

fn benchmark_has_second_punic_marker(text: &str) -> bool {
    [
        "second punic war",
        "2nd punic war",
        "제2차 포에니 전쟁",
        "2차 포에니 전쟁",
        "제2차 포에닉 전쟁",
    ]
    .iter()
    .any(|marker| text.contains(&marker.to_ascii_lowercase()))
}

fn benchmark_second_punic_has_comparative_scope(
    text: &str,
    has_first_or_third: bool,
    has_second_punic: bool,
) -> bool {
    [
        "compare",
        "comparison",
        "comparative",
        "all punic wars",
        "all three punic wars",
        "three punic wars",
        "across the punic wars",
        "포에니 전쟁 전체",
        "전체 포에니 전쟁",
        "세 차례 포에니 전쟁",
        "포에니 전쟁 비교",
        "비교 개관",
        "비교사",
    ]
    .iter()
    .any(|marker| text.contains(&marker.to_ascii_lowercase()))
        || (has_first_or_third && has_second_punic)
}

fn benchmark_second_punic_has_centered_focus(
    subject: &str,
    prompt: &str,
    subject_has_first_or_third: bool,
    subject_has_second_punic: bool,
    subject_has_hannibal: bool,
) -> bool {
    if (subject_has_second_punic || subject_has_hannibal)
        && !benchmark_second_punic_has_comparative_scope(
            subject,
            subject_has_first_or_third,
            subject_has_second_punic,
        )
    {
        return true;
    }

    [
        "hannibal and the second punic war",
        "second punic war campaign",
        "hannibal's campaign",
        "campaign of hannibal",
        "focus on the second punic war",
        "focus on hannibal",
        "center on the second punic war",
        "center on hannibal",
        "centered on the second punic war",
        "centered on hannibal",
        "especially the second punic war",
        "especially hannibal",
        "with emphasis on the second punic war",
        "with emphasis on hannibal",
        "제2차 포에니 전쟁을 중심으로",
        "제2차 포에니 전쟁 중심",
        "제2차 포에니 전쟁에 초점",
        "한니발과 제2차 포에니 전쟁",
        "한니발 중심",
        "한니발을 중심으로",
        "한니발에 초점",
        "한니발 원정",
    ]
    .iter()
    .any(|marker| {
        let marker = marker.to_ascii_lowercase();
        subject.contains(&marker) || prompt.contains(&marker)
    })
}

fn collect_second_punic_benchmark_metrics(
    reader_facing_output: &str,
) -> SecondPunicBenchmarkMetrics {
    SecondPunicBenchmarkMetrics {
        visible_chars: reader_facing_output.trim().chars().count(),
        phase_subsection_count: reader_facing_output
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("### ") || trimmed.starts_with("#### ")
            })
            .count(),
        date_anchor_count: count_second_punic_date_anchor_sentences(reader_facing_output),
        subject_anchor_count: count_second_punic_subject_anchors(reader_facing_output),
    }
}

fn count_second_punic_date_anchor_sentences(text: &str) -> usize {
    text.split(|ch| matches!(ch, '.' | '!' | '?' | '\n'))
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .filter(|sentence| sentence_has_second_punic_date_anchor(sentence))
        .count()
}

fn sentence_has_second_punic_date_anchor(sentence: &str) -> bool {
    sentence_has_year_marker(sentence)
        || ["bce", "bc", "ce", "ad", "기원전", "기원후", "세기"]
            .iter()
            .any(|marker| sentence.to_ascii_lowercase().contains(marker))
}

fn sentence_has_year_marker(text: &str) -> bool {
    let chars = text.chars().collect::<Vec<_>>();
    for window in chars.windows(5) {
        if window[..4].iter().all(|ch| ch.is_ascii_digit())
            && matches!(window[4], '년' | '-' | '–' | '—' | '.')
        {
            return true;
        }
    }
    false
}

fn count_second_punic_subject_anchors(text: &str) -> usize {
    let lower = text.to_ascii_lowercase();
    let ascii_markers = [
        "hannibal", "carthage", "roman", "rome", "scipio", "cannae", "zama", "iberia", "italy",
        "alps", "africa", "sicily",
    ];
    let non_ascii_markers = [
        "한니발",
        "카르타고",
        "로마",
        "스키피오",
        "칸나에",
        "자마",
        "이베리아",
        "이탈리아",
        "알프스",
        "북아프리카",
        "시칠리아",
    ];
    let ascii_hits = ascii_markers
        .iter()
        .map(|marker| count_ascii_word_marker_occurrences(&lower, marker))
        .sum::<usize>();
    let non_ascii_hits = non_ascii_markers
        .iter()
        .map(|marker| lower.match_indices(&marker.to_ascii_lowercase()).count())
        .sum::<usize>();
    ascii_hits + non_ascii_hits
}

fn count_ascii_word_marker_occurrences(haystack: &str, marker: &str) -> usize {
    haystack
        .match_indices(marker)
        .filter(|(start, matched)| {
            let end = *start + matched.len();
            let before = haystack[..*start].chars().next_back();
            let after = haystack[end..].chars().next();
            !is_ascii_word_char(before) && !is_ascii_word_char(after)
        })
        .count()
}

fn historical_hidden_artifacts_present(
    metrics: &BenchmarkEvidenceMetrics,
    controller: Option<&ParsedControllerArtifacts>,
) -> bool {
    let _ = metrics;
    controller.is_some_and(parsed_historical_hidden_artifacts_have_useful_grounding)
}

fn parsed_historical_hidden_artifacts_have_useful_grounding(
    controller: &ParsedControllerArtifacts,
) -> bool {
    let valid_source_ids = controller
        .source_cards
        .iter()
        .filter(|card| is_valid_http_url(&card.url))
        .map(|card| card.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    let valid_claim_ids = controller
        .claim_log
        .iter()
        .filter(|claim| {
            claim
                .support_source_card_ids
                .iter()
                .map(|id| id.trim())
                .any(|id| valid_source_ids.contains(id))
                || claim.support_urls.iter().any(|url| is_valid_http_url(url))
        })
        .map(|claim| claim.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    if valid_claim_ids.is_empty() && valid_source_ids.is_empty() {
        return false;
    }

    let grounded_narrative_points = controller
        .narrative_state
        .as_ref()
        .map(|state| {
            [
                state.timeline.as_slice(),
                state.evidence_layers.as_slice(),
                state.interpretive_tensions.as_slice(),
                state.impacts.as_slice(),
                state.reader_questions.as_slice(),
                state.section_outline.as_slice(),
                state.open_gaps.as_slice(),
            ]
            .into_iter()
            .filter(|items| {
                items.iter().any(|item| {
                    parsed_json_item_has_grounded_refs(item, &valid_claim_ids, &valid_source_ids)
                })
            })
            .count()
        })
        .unwrap_or_default();
    if grounded_narrative_points >= 3 {
        return true;
    }

    let Some(reader_quality) = controller.reader_quality.as_ref() else {
        return false;
    };
    let grounded_argument_graph = reader_quality.argument_graph.as_ref().is_some_and(|graph| {
        graph.nodes.iter().any(|node| {
            parsed_json_item_has_grounded_refs(node, &valid_claim_ids, &valid_source_ids)
        }) && (graph.nodes.len() >= 2
            || graph.edges.iter().any(|edge| {
                parsed_json_item_has_grounded_refs(edge, &valid_claim_ids, &valid_source_ids)
            }))
    });
    let grounded_section_brief_count = reader_quality
        .section_briefs
        .iter()
        .filter(|brief| {
            parsed_json_item_has_grounded_refs(brief, &valid_claim_ids, &valid_source_ids)
        })
        .count();
    usize::from(grounded_argument_graph) + usize::from(grounded_section_brief_count >= 2) >= 2
}

fn parsed_json_item_has_grounded_refs(
    item: &serde_json::Value,
    valid_claim_ids: &BTreeSet<String>,
    valid_source_ids: &BTreeSet<String>,
) -> bool {
    parsed_json_string_array_intersects(item, "claim_log_ids", valid_claim_ids)
        || parsed_json_string_array_intersects(item, "expected_claim_log_ids", valid_claim_ids)
        || parsed_json_string_array_intersects(item, "source_card_ids", valid_source_ids)
        || parsed_json_string_array_intersects(item, "expected_source_card_ids", valid_source_ids)
}

fn parsed_json_string_array_intersects(
    item: &serde_json::Value,
    key: &str,
    valid_ids: &BTreeSet<String>,
) -> bool {
    item.get(key)
        .and_then(serde_json::Value::as_array)
        .is_some_and(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .any(|id| valid_ids.contains(id))
        })
}

fn historical_generic_open_debt_count(artifacts: &ParsedControllerArtifacts) -> usize {
    artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .filter(|debt| historical_missing_evidence_is_generic(&debt.missing_evidence))
        .count()
}

fn historical_missing_evidence_is_generic(missing_evidence: &str) -> bool {
    let normalized = missing_evidence
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "missing evidence not specified"
                | "missing evidence"
                | "evidence not specified"
                | "unspecified"
        )
        || normalized.contains("missing evidence not specified")
}

fn is_technology_like_benchmark_category(category: &str) -> bool {
    matches!(
        category,
        "product-decision"
            | "niche-troubleshooting"
            | "numeric-comparison"
            | "technology-concept"
            | "technology-decision"
            | "technology-implementation"
    )
}

fn count_marker_hits(haystack: &str, markers: &[&str]) -> usize {
    markers
        .iter()
        .filter(|marker| marker_hit(haystack, marker))
        .count()
}

fn extract_markdown_headings(lower_text: &str) -> Vec<&str> {
    lower_text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                return None;
            }
            Some(trimmed.trim_start_matches('#').trim())
        })
        .collect()
}

fn heading_has_phrase(headings: &[&str], phrases: &[&str]) -> bool {
    headings
        .iter()
        .any(|heading| phrases.iter().any(|phrase| heading.contains(phrase)))
}

fn heading_has_all_keywords(headings: &[&str], keywords: &[&str]) -> bool {
    headings
        .iter()
        .any(|heading| keywords.iter().all(|keyword| heading.contains(keyword)))
}

fn bool_signal(value: bool) -> usize {
    usize::from(value)
}

fn marker_hit(haystack: &str, marker: &str) -> bool {
    if is_ascii_word_marker(marker) {
        contains_ascii_word_marker(haystack, marker)
    } else {
        haystack.contains(marker)
    }
}

fn is_ascii_word_marker(marker: &str) -> bool {
    !marker.is_empty()
        && marker
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn contains_ascii_word_marker(haystack: &str, marker: &str) -> bool {
    haystack.match_indices(marker).any(|(start, matched)| {
        let end = start + matched.len();
        let before = haystack[..start].chars().next_back();
        let after = haystack[end..].chars().next();
        !is_ascii_word_char(before) && !is_ascii_word_char(after)
    })
}

fn is_ascii_word_char(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn collect_historical_overlay_metrics(reader_facing_output: &str) -> HistoricalOverlayMetrics {
    let lower = reader_facing_output.to_lowercase();
    let headings = extract_markdown_headings(&lower);
    let has_chronology_interpretation_split = count_marker_hits(
        &lower,
        &[
            "전개 순서와 해석",
            "연대기와 해석",
            "전개와 해석",
            "chronology and interpretation",
        ],
    ) > 0
        || ((heading_has_phrase(
            &headings,
            &[
                "배경과 전개 순서",
                "배경과 전개",
                "배경과 현재 맥락",
                "전개 순서",
                "연대기",
                "timeline",
            ],
        ) || heading_has_all_keywords(&headings, &["전개", "순서"]))
            && (heading_has_phrase(
                &headings,
                &[
                    "확인된 사실과 불확실성",
                    "결론: 확인된 사실과 불확실성",
                    "사실과 불확실성",
                    "해석의 한계",
                    "주요 쟁점과 한계",
                    "쟁점과 한계",
                    "facts and uncertainties",
                ],
            ) || heading_has_all_keywords(&headings, &["불확실"])
                || heading_has_all_keywords(&headings, &["해석", "한계"])));
    let has_source_layer_signal = count_marker_hits(
        &lower,
        &[
            "사료 층위",
            "사료와 연구",
            "자료 층위",
            "source layers",
            "evidence layers",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "사료 신뢰성과 해석의 한계",
                "사료의 한계와 해석",
                "자료 신뢰성과 해석의 한계",
                "source reliability and limits",
            ],
        )
        || heading_has_all_keywords(&headings, &["사료", "한계"])
        || heading_has_all_keywords(&headings, &["자료", "한계"]);
    let has_issue_map_signal = count_marker_hits(
        &lower,
        &[
            "쟁점 지도",
            "핵심 쟁점",
            "논점 지도",
            "쟁점과 해석",
            "debate map",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "주요 쟁점과 한계",
                "쟁점과 한계",
                "논쟁과 한계",
                "해석 쟁점",
            ],
        )
        || heading_has_all_keywords(&headings, &["쟁점", "한계"])
        || heading_has_all_keywords(&headings, &["논쟁", "한계"]);
    let has_legacy_signal = count_marker_hits(
        &lower,
        &[
            "후대 영향",
            "장기 영향",
            "후속 영향",
            "legacy and impact",
            "afterlives",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "결과와 영향",
                "영향과 결과",
                "의미와 영향",
                "후대 영향",
                "장기 영향",
                "impact and consequences",
            ],
        )
        || heading_has_all_keywords(&headings, &["결과", "영향"]);
    HistoricalOverlayMetrics {
        chronology_signal_count: count_marker_hits(
            &lower,
            &[
                "chronology",
                "timeline",
                "period",
                "sequence",
                "earlier",
                "later",
                "before",
                "after",
                "연대",
                "시기",
                "순서",
                "이후",
                "이전",
                "당시",
            ],
        ),
        actor_signal_count: count_marker_hits(
            &lower,
            &[
                "actors",
                "actor",
                "emperor",
                "emperors",
                "ruler",
                "rulers",
                "leader",
                "leaders",
                "institution",
                "institutions",
                "faction",
                "factions",
                "army",
                "armies",
                "military",
                "elite",
                "elites",
                "황제",
                "세력",
                "인물",
                "지배층",
                "집단",
                "권력",
                "기관",
            ],
        ),
        geography_signal_count: count_marker_hits(
            &lower,
            &[
                "geography",
                "region",
                "regional",
                "frontier",
                "border",
                "province",
                "territory",
                "도시",
                "지역",
                "영역",
                "국경",
                "지리",
                "변방",
                "속주",
            ],
        ),
        cause_signal_count: count_marker_hits(
            &lower,
            &[
                "cause",
                "causes",
                "because",
                "driven by",
                "pressures",
                "background",
                "원인",
                "배경",
                "계기",
                "압박",
            ],
        ),
        consequence_signal_count: count_marker_hits(
            &lower,
            &[
                "consequence",
                "consequences",
                "aftermath",
                "impact",
                "resulted in",
                "결과",
                "영향",
                "여파",
                "파급",
            ],
        ),
        evidence_limit_signal_count: count_marker_hits(
            &lower,
            &[
                "evidence is limited",
                "source limits",
                "uncertain",
                "uncertainty",
                "records are thin",
                "사료의 한계",
                "근거의 한계",
                "기록이 제한적",
                "불확실",
                "확인 범위",
            ],
        ),
        contested_interpretation_signal_count: count_marker_hits(
            &lower,
            &[
                "contested",
                "debated",
                "interpretation",
                "scholars disagree",
                "disputed",
                "논쟁",
                "이견",
                "해석이 갈린",
                "견해가 갈린",
            ],
        ),
        scope_limit_signal_count: count_marker_hits(
            &lower,
            &[
                "scope",
                "this answer focuses on",
                "this comparison stays within",
                "범위를 좁혀",
                "이 글에서는",
                "여기서는",
                "범위는",
            ],
        ),
        comparison_signal_count: count_marker_hits(
            &lower,
            &[
                "동시대 비교",
                "비교 관점",
                "같은 시기 다른 사례",
                "동시대 사례",
                "contemporary comparison",
                "parallel case",
            ],
        ),
        chronology_interpretation_split_signal_count: bool_signal(
            has_chronology_interpretation_split,
        ),
        source_layer_signal_count: bool_signal(has_source_layer_signal),
        issue_map_signal_count: bool_signal(has_issue_map_signal),
        legacy_signal_count: bool_signal(has_legacy_signal),
        follow_up_signal_count: count_marker_hits(
            &lower,
            &[
                "후속 탐색",
                "추가 탐색",
                "후속 질문",
                "다음 질문",
                "follow-up questions",
                "further reading",
            ],
        ),
    }
}

fn collect_technology_genre_metrics(reader_facing_output: &str) -> TechnologyGenreMetrics {
    let lower = reader_facing_output.to_lowercase();
    let headings = extract_markdown_headings(&lower);
    let has_design_judgment_signal = count_marker_hits(
        &lower,
        &[
            "설계 판단",
            "선택 이유",
            "권장 구조",
            "design choice",
            "why this design",
            "recommended architecture",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "핵심 스케줄링 모델",
                "핵심 모델",
                "설계 포인트",
                "구현 계획 권고",
                "권장 구현",
                "recommended implementation",
                "design overview",
            ],
        )
        || heading_has_all_keywords(&headings, &["핵심", "모델"])
        || heading_has_all_keywords(&headings, &["설계", "포인트"])
        || heading_has_all_keywords(&headings, &["구현", "권고"]);
    let has_tradeoff_signal = count_marker_hits(
        &lower,
        &[
            "트레이드오프",
            "trade-off",
            "tradeoff",
            "장단점",
            "복잡도",
            "latency vs",
            "throughput vs",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "메모리 ordering",
                "memory ordering",
                "blocking, parking, wakeup",
                "blocking/parking/wakeup",
                "numa와 cache 고려",
                "성능 최적화",
                "hazard",
            ],
        )
        || heading_has_all_keywords(&headings, &["메모리", "ordering"])
        || heading_has_all_keywords(&headings, &["blocking", "wakeup"])
        || heading_has_all_keywords(&headings, &["parking", "wakeup"])
        || heading_has_all_keywords(&headings, &["numa", "cache"])
        || heading_has_all_keywords(&headings, &["성능", "최적화"]);
    let has_verifiability_signal = count_marker_hits(
        &lower,
        &[
            "검증 방법",
            "검증 가능성",
            "측정 계획",
            "벤치마크",
            "benchmark",
            "profile",
            "profiling",
            "reproduce",
            "재현",
            "test plan",
        ],
    ) > 0
        || heading_has_phrase(
            &headings,
            &[
                "대표 구현과 신뢰도 구분",
                "신뢰도 구분",
                "확인된 사실과 불확실성",
                "벤치마크 계획",
                "성능 최적화/벤치마크",
                "implementation confidence",
            ],
        )
        || heading_has_all_keywords(&headings, &["신뢰도", "구분"])
        || heading_has_all_keywords(&headings, &["사실", "불확실성"])
        || heading_has_all_keywords(&headings, &["벤치마크", "계획"]);
    TechnologyGenreMetrics {
        design_judgment_signal_count: bool_signal(has_design_judgment_signal),
        tradeoff_signal_count: bool_signal(has_tradeoff_signal),
        verifiability_signal_count: bool_signal(has_verifiability_signal),
    }
}

fn apply_historical_overlay_score_cap(overall_score: f64, triggered_count: usize) -> f64 {
    match triggered_count {
        0 => overall_score,
        1 => overall_score.min(2.5),
        _ => overall_score.min(2.0),
    }
}

fn count_prompt_echo_windows(prompt: &str, lower_reader_facing_output: &str) -> usize {
    let tokens = prompt
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '-')
                .to_lowercase()
        })
        .filter(|token| token.chars().count() >= 3)
        .collect::<Vec<_>>();
    if tokens.len() < 4 {
        return 0;
    }
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for window in tokens.windows(4) {
        let phrase = window.join(" ");
        if seen.insert(phrase.clone()) && lower_reader_facing_output.contains(&phrase) {
            count += 1;
        }
    }
    count
}

fn render_markdown_aggregate_summary(summary: &BenchmarkRunSummary) -> String {
    [
        format!("- Cases: {}", summary.case_count),
        format!("- Completed: {}", summary.completed_case_count),
        format!("- Quality passed: {}", summary.quality_passed_case_count),
        format!(
            "- Critical failure cases: {} ({})",
            summary.critical_failure_case_count,
            if summary.critical_failure_case_ids.is_empty() {
                "none".to_string()
            } else {
                summary.critical_failure_case_ids.join(", ")
            }
        ),
        format!(
            "- Overall score range: {:.2} to {:.2} (avg {:.2})",
            summary.overall_min_score, summary.overall_max_score, summary.overall_average_score
        ),
        format!(
            "- Source pack statuses: {}",
            render_count_map(&summary.source_pack_status_counts)
        ),
        format!(
            "- Quality statuses: {}",
            render_count_map(&summary.quality_status_counts)
        ),
    ]
    .join("\n")
}

fn render_markdown_dimension_summary(aggregates: &[BenchmarkDimensionAggregate]) -> String {
    let mut lines = vec![
        "| Dimension | Avg | Min | Max | Cases |".to_string(),
        "| --- | --- | --- | --- | --- |".to_string(),
    ];
    lines.extend(aggregates.iter().map(|aggregate| {
        format!(
            "| {} | {:.2} | {} | {} | {} |",
            aggregate.label,
            aggregate.average_score,
            aggregate.min_score,
            aggregate.max_score,
            aggregate.case_count
        )
    }));
    lines.join("\n")
}

fn render_case_scores_csv(report: &BenchmarkRunStructuredReport) -> String {
    let mut header = vec![
        "label",
        "case_id",
        "title",
        "category",
        "mode",
        "overall_score",
        "any_critical_failure",
        "task_status",
        "quality_status",
        "source_pack_status",
        "advisory_model_self_score",
        "historical_overlay_trigger_count",
        "historical_lens_coverage_count",
        "replay_before_status",
        "replay_before_quality_status",
        "replay_before_critical_failure_count",
        "replay_before_quality_last_failure",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    header.extend(RUBRIC_DIMENSIONS.iter().map(|spec| spec.key.to_string()));
    header.extend(
        [
            "reader_quality_present",
            "reader_argument_node_count",
            "reader_argument_edge_count",
            "reader_narrative_plan_present",
            "reader_section_brief_count",
            "reader_critique_present",
            "reader_critique_metric_count",
            "reader_critique_failed_metric_count",
            "context_pack_reader_quality_present",
            "context_pack_reader_section_brief_count",
            "context_pack_reader_critique_metric_count",
        ]
        .into_iter()
        .map(String::from),
    );
    let mut lines = vec![header.join(",")];
    for case in &report.cases {
        let mut row = vec![
            csv_escape(&report.label),
            csv_escape(&case.case_id),
            csv_escape(&case.title),
            csv_escape(&case.category),
            csv_escape(&case.mode),
            format!("{:.2}", case.overall_score),
            case.any_critical_failure.to_string(),
            csv_escape(&case.visibility.task_status),
            csv_escape(&case.visibility.quality_status),
            csv_escape(&case.visibility.source_pack_status),
            case.advisory_model_self_score
                .map(|value| format!("{value:.2}"))
                .unwrap_or_default(),
            case.historical_overlay_trigger_count.to_string(),
            case.metrics.historical_lens_coverage_count.to_string(),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.status.as_str())
                    .unwrap_or(""),
            ),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.quality_status.as_str())
                    .unwrap_or(""),
            ),
            case.replay_before
                .as_ref()
                .map(|before| before.critical_failure_count.to_string())
                .unwrap_or_default(),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.quality_last_failure.as_str())
                    .unwrap_or(""),
            ),
        ];
        row.extend(RUBRIC_DIMENSIONS.iter().map(|spec| {
            case.dimensions
                .iter()
                .find(|dimension| dimension.key == spec.key)
                .map(|dimension| dimension.score.to_string())
                .unwrap_or_default()
        }));
        row.extend([
            case.metrics.reader_quality_present.to_string(),
            case.metrics.reader_argument_node_count.to_string(),
            case.metrics.reader_argument_edge_count.to_string(),
            case.metrics.reader_narrative_plan_present.to_string(),
            case.metrics.reader_section_brief_count.to_string(),
            case.metrics.reader_critique_present.to_string(),
            case.metrics.reader_critique_metric_count.to_string(),
            case.metrics.reader_critique_failed_metric_count.to_string(),
            case.metrics.context_pack_reader_quality_present.to_string(),
            case.metrics
                .context_pack_reader_section_brief_count
                .to_string(),
            case.metrics
                .context_pack_reader_critique_metric_count
                .to_string(),
        ]);
        lines.push(row.join(","));
    }
    lines.join("\n")
}

fn render_dimension_scores_ndjson(
    report: &BenchmarkRunStructuredReport,
) -> Result<String, serde_json::Error> {
    let mut lines = Vec::new();
    for case in &report.cases {
        for dimension in &case.dimensions {
            lines.push(serde_json::to_string(&json!({
                "label": report.label,
                "case_id": case.case_id,
                "title": case.title,
                "category": case.category,
                "mode": case.mode,
                "measurement_kind": case.measurement_kind,
                "dimension_key": dimension.key,
                "dimension_label": dimension.label,
                "score": dimension.score,
                "max_score": dimension.max_score,
                "assessor": dimension.assessor,
                "score_source": dimension.score_source,
                "visibility": dimension.visibility,
                "overall_score": case.overall_score,
                "any_critical_failure": case.any_critical_failure,
                "historical_overlay_trigger_count": case.historical_overlay_trigger_count,
                "historical_lens_coverage_count": case.metrics.historical_lens_coverage_count,
                "quality_status": case.visibility.quality_status,
                "source_pack_status": case.visibility.source_pack_status,
                "fixture_only_pipeline_evidence": case.visibility.fixture_only_pipeline_evidence,
                "replay_before": case.replay_before,
            }))?);
        }
    }
    Ok(lines.join("\n"))
}

fn benchmark_mode_label(mode: ResearchBenchmarkMode) -> &'static str {
    match mode {
        ResearchBenchmarkMode::Fixture => "fixture",
        ResearchBenchmarkMode::Live => "live",
        ResearchBenchmarkMode::Replay => "replay",
    }
}

fn measurement_metadata(mode: ResearchBenchmarkMode) -> MeasurementMetadata {
    match mode {
        ResearchBenchmarkMode::Fixture => MeasurementMetadata {
            measurement_kind: PIPELINE_EVIDENCE_MEASUREMENT,
            evidence_caveat: PIPELINE_EVIDENCE_CAVEAT,
            report_note_label: "Fixture note",
        },
        ResearchBenchmarkMode::Live => MeasurementMetadata {
            measurement_kind: LIVE_QUALITY_MEASUREMENT,
            evidence_caveat: LIVE_QUALITY_CAVEAT,
            report_note_label: "Live note",
        },
        ResearchBenchmarkMode::Replay => MeasurementMetadata {
            measurement_kind: REPLAY_QUALITY_MEASUREMENT,
            evidence_caveat: REPLAY_QUALITY_CAVEAT,
            report_note_label: "Replay note",
        },
    }
}

fn preflight_run_output_paths(
    run_case_dir: &Path,
    report_path: &Path,
    runs_dir: &Path,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let aggregate_paths = [
        report_path.to_path_buf(),
        runs_dir.join(format!("{label}.json")),
        runs_dir.join(format!("{label}.csv")),
        runs_dir.join(format!("{label}.ndjson")),
    ];
    if run_case_dir.exists() {
        return Err(format!(
            "benchmark output directory already exists: {}",
            run_case_dir.display()
        )
        .into());
    }
    if let Some(existing) = aggregate_paths.iter().find(|path| path.exists()) {
        return Err(format!("benchmark report already exists: {}", existing.display()).into());
    }
    Ok(())
}

fn secret_scan_report(
    report: &str,
    executions: &[RunExecution],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut findings = Vec::new();
    scan_sensitive_text("aggregate report", report, &mut findings);
    for execution in executions {
        if let Some(output) = execution.result.final_output.as_deref() {
            scan_sensitive_text(
                &format!("case {} final output", execution.case.case_id),
                output,
                &mut findings,
            );
        }
        if let Some(json_body) = execution.result.research_source_diagnostics_json.as_deref() {
            scan_sensitive_text(
                &format!("case {} source diagnostics", execution.case.case_id),
                json_body,
                &mut findings,
            );
        }
        if let Some(json_body) = execution
            .result
            .research_controller_artifacts_json
            .as_deref()
        {
            scan_sensitive_text(
                &format!("case {} controller artifacts", execution.case.case_id),
                json_body,
                &mut findings,
            );
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!("secret/raw-payload scan failed: {}", findings.join(" | ")).into())
    }
}

fn scan_sensitive_text(label: &str, text: &str, findings: &mut Vec<String>) {
    let lower = text.to_ascii_lowercase();
    let patterns = [
        ("api_key", "raw provider key field"),
        ("client_secret", "raw client secret field"),
        ("x-naver-client-secret", "raw provider header"),
        ("authorization:", "raw authorization header"),
        ("bearer ", "bearer token fragment"),
        ("raw provider payload", "raw provider payload marker"),
        ("response body:", "raw response body marker"),
    ];
    for (needle, description) in patterns {
        if lower.contains(needle) {
            findings.push(format!("{label} contains {description}"));
        }
    }
}

fn configured_search_providers() -> Vec<String> {
    if let Ok(value) = std::env::var("LIQUID_RESEARCH_SEARCH_PROVIDERS") {
        let providers = value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        if !providers.is_empty() {
            return providers;
        }
    }
    if let Ok(value) = std::env::var("LIQUID_RESEARCH_SEARCH_PROVIDER") {
        let value = value.trim();
        if !value.is_empty() {
            return vec![value.to_string()];
        }
    }
    vec!["duckduckgo".to_string()]
}

fn source_pack_provider_limitation_warning(
    source_pack: Option<&ParsedSourcePackReport>,
    configured_providers: &[String],
    mode: ResearchBenchmarkMode,
) -> Option<String> {
    if !matches!(
        mode,
        ResearchBenchmarkMode::Live | ResearchBenchmarkMode::Replay
    ) {
        return None;
    }
    let source_pack = source_pack?;
    let successful_providers = source_pack
        .queries
        .iter()
        .filter(|query| query.status == "success")
        .filter_map(|query| query.provider.as_deref())
        .collect::<Vec<_>>();
    if successful_providers.is_empty() {
        if configured_providers.len() == 1 {
            return Some(format!(
                "single-provider source-pack limitation: configured provider {} was the only live discovery backend available for this run",
                configured_providers[0]
            ));
        }
        return None;
    }
    let distinct_successful_providers = successful_providers
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if distinct_successful_providers.len() == 1 {
        let provider = distinct_successful_providers
            .iter()
            .next()
            .copied()
            .unwrap_or("unknown");
        let qualifier = if configured_providers.len() > 1 {
            "all successful source-pack queries converged on one provider despite multiple configured fallbacks"
        } else {
            "all successful source-pack queries relied on the only configured provider"
        };
        return Some(format!(
            "single-provider source-pack limitation: provider={provider}; {qualifier}"
        ));
    }
    None
}

fn significant_prompt_terms(prompt: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "with",
        "from",
        "that",
        "this",
        "into",
        "about",
        "compare",
        "including",
        "current",
        "their",
        "there",
        "where",
        "which",
        "would",
        "could",
        "should",
        "your",
        "have",
        "more",
        "than",
        "when",
        "what",
        "does",
        "into",
        "while",
        "using",
        "user",
        "prompt",
    ];
    let stop_words = STOP_WORDS.iter().copied().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut terms = Vec::new();
    for token in prompt
        .split(|ch: char| !ch.is_alphanumeric())
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.chars().count() >= 3)
    {
        if stop_words.contains(token.as_str()) || !seen.insert(token.clone()) {
            continue;
        }
        terms.push(token);
    }
    terms
}

fn contains_any_heading(text: &str, headings: &[&str]) -> bool {
    headings.iter().any(|heading| text.contains(heading))
}

fn extract_advisory_model_self_score(visible_output: &str) -> Option<f32> {
    let lines = visible_output.lines().collect::<Vec<_>>();
    let mut heading_index = None;
    for (index, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains("0-5 score") {
            heading_index = Some(index);
            break;
        }
    }
    let heading_index = heading_index?;
    let mut section_body_lines = Vec::new();
    for line in lines.iter().skip(heading_index + 1) {
        if line.trim_start().starts_with('#') {
            break;
        }
        section_body_lines.push(*line);
    }
    let section_body = section_body_lines.join("\n");
    for token in section_body.split(|ch: char| !(ch.is_ascii_digit() || ch == '.')) {
        if token.is_empty() {
            continue;
        }
        if let Ok(value) = token.parse::<f32>() {
            if (0.0..=5.0).contains(&value) {
                return Some(value);
            }
        }
    }
    None
}

fn normalized_host_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    parsed.host_str().map(|host| host.to_lowercase())
}

fn is_valid_http_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    match parsed.host() {
        Some(url::Host::Domain(host)) => public_benchmark_domain_host(host),
        Some(url::Host::Ipv4(ip)) => public_benchmark_ip(std::net::IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => public_benchmark_ip(std::net::IpAddr::V6(ip)),
        None => false,
    }
}

fn public_benchmark_domain_host(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    !lower.is_empty()
        && lower != "localhost"
        && !lower.ends_with(".localhost")
        && !lower.ends_with(".local")
        && !lower.ends_with(".internal")
}

fn public_benchmark_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_loopback()
                && !ip.is_private()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && ip != std::net::Ipv4Addr::new(255, 255, 255, 255)
                && octets[0] != 0
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_benchmark_ip(std::net::IpAddr::V4(mapped));
            }
            !ip.is_loopback()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
        }
    }
}

fn count_by_key<I>(values: I) -> BTreeMap<String, usize>
where
    I: Iterator<Item = String>,
{
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

fn render_count_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn average_u8(scores: &[u8]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().map(|score| *score as f64).sum::<f64>() / scores.len() as f64
    }
}

fn average_f64(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

fn min_f64(scores: &[f64]) -> f64 {
    scores.iter().copied().reduce(f64::min).unwrap_or(0.0)
}

fn max_f64(scores: &[f64]) -> f64 {
    scores.iter().copied().reduce(f64::max).unwrap_or(0.0)
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn write_new_text_file(path: &Path, body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(body.as_bytes())?;
    Ok(())
}

fn validate_label(label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let valid = label
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'));
    if !valid || label.contains("..") {
        return Err("invalid label".into());
    }
    Ok(())
}

fn load_cases(dir: &Path) -> Result<Vec<BenchmarkCase>, Box<dyn std::error::Error>> {
    let mut paths = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| parse_case_file(&path))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_case_file(path: &Path) -> Result<BenchmarkCase, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut title = String::new();
    let mut category = String::new();
    let mut prompt = String::new();
    let mut must_pass_checks = Vec::new();
    let mut expected_failure_modes = Vec::new();
    let mut section = "";
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("Title: ") {
            title = value.trim().to_string();
            section = "";
            continue;
        }
        if let Some(value) = line.strip_prefix("Category: ") {
            category = value.trim().to_string();
            section = "";
            continue;
        }
        if let Some(value) = line.strip_prefix("Prompt: ") {
            prompt = value.trim().to_string();
            section = "";
            continue;
        }
        if line.trim() == "Must-Pass Evidence Checks:" {
            section = "checks";
            continue;
        }
        if line.trim() == "Expected Failure Modes:" {
            section = "failures";
            continue;
        }
        if let Some(value) = line.trim().strip_prefix("- ") {
            match section {
                "checks" => must_pass_checks.push(value.to_string()),
                "failures" => expected_failure_modes.push(value.to_string()),
                _ => {}
            }
        }
    }
    if title.is_empty() || category.is_empty() || prompt.is_empty() {
        return Err(format!("invalid benchmark case file: {}", path.display()).into());
    }
    Ok(BenchmarkCase {
        case_id: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("case")
            .to_string(),
        filename: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        title,
        category,
        prompt,
        must_pass_checks,
        expected_failure_modes,
    })
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug.split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_case(case_id: &str, filename: &str) -> BenchmarkCase {
        BenchmarkCase {
            case_id: case_id.to_string(),
            filename: filename.to_string(),
            title: "Title".to_string(),
            category: "category".to_string(),
            prompt: "Compare two current developer laptops for a local Rust and AI workflow"
                .to_string(),
            must_pass_checks: vec!["Includes source-backed laptop comparison".to_string()],
            expected_failure_modes: vec!["Thin evidence should stay visible".to_string()],
        }
    }

    fn historical_case(case_id: &str, filename: &str) -> BenchmarkCase {
        BenchmarkCase {
            case_id: case_id.to_string(),
            filename: filename.to_string(),
            title: "Historical Title".to_string(),
            category: "historical-explanation".to_string(),
            prompt: "Explain a historical turning point with chronology, actors, geography, causes, consequences, and source limits."
                .to_string(),
            must_pass_checks: vec!["Provides a history-focused explanation".to_string()],
            expected_failure_modes: vec!["Shallow historical overview should fail overlay gates".to_string()],
        }
    }

    fn second_punic_case(case_id: &str, filename: &str) -> BenchmarkCase {
        BenchmarkCase {
            case_id: case_id.to_string(),
            filename: filename.to_string(),
            title: "Hannibal and the Second Punic War".to_string(),
            category: "historical-explanation".to_string(),
            prompt: "Explain Hannibal and the Second Punic War with phased chronology, campaign fronts, actors, consequences, and source limits."
                .to_string(),
            must_pass_checks: vec!["Preserves visible campaign phase density".to_string()],
            expected_failure_modes: vec!["Flat Hannibal overview without phase subsections should fail".to_string()],
        }
    }

    fn technology_case(case_id: &str, filename: &str) -> BenchmarkCase {
        BenchmarkCase {
            case_id: case_id.to_string(),
            filename: filename.to_string(),
            title: "Technology Title".to_string(),
            category: "niche-troubleshooting".to_string(),
            prompt: "Explain the design choice, trade-offs, and verification plan for a technical system change."
                .to_string(),
            must_pass_checks: vec!["Provides a technical recommendation".to_string()],
            expected_failure_modes: vec!["Hand-wavy answer without trade-offs".to_string()],
        }
    }

    fn sample_result() -> ResearchBenchmarkCaseResult {
        ResearchBenchmarkCaseResult {
            case_id: "case".to_string(),
            title: "Title".to_string(),
            category: "category".to_string(),
            mode: ResearchBenchmarkMode::Fixture,
            data_dir: PathBuf::from("/tmp/case"),
            task_id: 1,
            status: "completed".to_string(),
            error_message: None,
            quality_status: Some("passed".to_string()),
            quality_last_failure: None,
            research_controller_stage: Some("final".to_string()),
            research_controller_iteration: Some(2),
            research_controller_max_iterations: Some(2),
            output_filename: Some("output.md".to_string()),
            final_output: Some(
                "## 최종 답변 (Final Answer)\nRust and AI workflow comparison with a concrete recommendation.\n\n# 검증 부록\n## 출처 감사 (Source Audit)\n- Apple docs\n- Lenovo docs\n\n## 주장 로그 (Claim Log)\n- C1 supported by S1 and S2\n\n## 품질 게이트 (Quality Gate)\n- passed\n\n## 0-5 Score\n4.0\n\n[RESEARCH_ARTIFACT_JSON]\n```json\n{\"version\":1}\n```".to_string(),
            ),
            research_controller_artifacts_json: Some(
                serde_json::to_string(&json!({
                    "version": 1,
                    "source_cards": [
                        {
                            "id": "S1",
                            "url": "https://developer.apple.com/documentation/apple-silicon",
                            "title": "Apple Silicon",
                            "source_class": "official_or_primary"
                        },
                        {
                            "id": "S2",
                            "url": "https://support.lenovo.com/us/en/solutions/ht516908",
                            "title": "Lenovo Thermal Guidance",
                            "source_class": "official_or_primary"
                        }
                    ],
                    "claim_log": [
                        {
                            "id": "C1",
                            "claim": "MacBook and ThinkPad tradeoffs are grounded in official docs",
                            "support_source_card_ids": ["S1", "S2"],
                            "support_urls": ["https://developer.apple.com/documentation/apple-silicon"]
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
                    },
                    "warnings": []
                }))
                .unwrap(),
            ),
            research_source_diagnostics_json: Some(
                serde_json::to_string(&json!({
                    "version": 1,
                    "subject": "Compare two current developer laptops for a local Rust and AI workflow",
                    "source_pack": {
                        "subject": "Compare two current developer laptops for a local Rust and AI workflow",
                        "status": "success",
                        "reason": "fixture evidence",
                        "queries": [
                            {
                                "query": "developer laptops official source",
                                "status": "success",
                                "result_count": 2,
                                "adopted_count": 2,
                                "skipped_count": 0,
                                "error": null
                            }
                        ],
                        "seeded_source_count": 0,
                        "discovered_source_count": 2,
                        "adopted_source_count": 2,
                        "adopted_candidates": [],
                        "skipped_candidates": []
                    },
                    "scrapes": [],
                    "context_packing": {
                        "strategy": "artifact_led",
                        "included_source_card_count": 2,
                        "omitted_source_card_count": 0,
                        "included_excerpt_chars": 400,
                        "omitted_raw_chars": 0,
                        "total_raw_chars": 400,
                        "active_debt_count": 0,
                        "unresolved_conflict_count": 0,
                        "notes": []
                    }
                }))
                .unwrap(),
            ),
            resolved_system_prompt: Some("system".to_string()),
            resolved_user_prompt: Some("user".to_string()),
            model_input: "cli:fixture-research-bench".to_string(),
        }
    }

    #[test]
    fn technology_concept_category_uses_technology_benchmark_scoring() {
        assert!(is_technology_like_benchmark_category("technology-concept"));
    }

    #[test]
    fn case_execution_plans_prefix_colliding_slugs() {
        let plans = build_case_execution_plans(vec![
            sample_case("A!", "A!.md"),
            sample_case("A?", "A?.md"),
        ])
        .unwrap();

        assert_eq!(plans[0].artifact_stem, "1-a");
        assert_eq!(plans[1].artifact_stem, "2-a");
    }

    #[test]
    fn case_execution_plans_reject_empty_slugs() {
        let err = build_case_execution_plans(vec![sample_case("!!!", "!!!.md")]).unwrap_err();
        assert!(err.to_string().contains("produces an empty slug"));
    }

    #[test]
    fn extract_replay_fixture_prompt_stops_before_repair_instructions() {
        let prompt = extract_replay_fixture_prompt(
            "### User Request:\nCompare official laptop specifications.\n- include memory ceilings\n\n[RESEARCH QUALITY REPAIR ITERATION 2/2]\nDo not keep this.",
        );

        assert!(prompt.contains("Compare official laptop specifications."));
        assert!(prompt.contains("include memory ceilings"));
        assert!(!prompt.contains("Do not keep this."));
    }

    #[test]
    fn replay_loader_synthesizes_plan_for_targeted_live_fixture_without_case_file() {
        let root = std::env::temp_dir().join(format!("research-bench-replay-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let stem = "2-03-comparative-product-technical-decision";
        fs::write(
            root.join(format!("{stem}.json")),
            serde_json::to_string(&json!({
                "case_id": "03-comparative-product-technical-decision",
                "title": "Comparative Product Technical Decision Rerun",
                "category": "comparative-product-technical-decision",
                "status": "completed",
                "quality_status": "untrusted",
                "quality_last_failure": "missing visible source audit",
                "model_input": "cli:codex"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(format!("{stem}-final-output.md")),
            "## Final Answer\nDraft output",
        )
        .unwrap();
        fs::write(
            root.join(format!("{stem}-controller-artifacts.json")),
            "{\"version\":1,\"source_cards\":[],\"claim_log\":[],\"conflict_map\":[],\"research_debt\":[],\"quality_gate\":{\"status\":\"failed\",\"failure_messages\":[\"missing visible source audit\"],\"unsupported_claim_count\":0,\"unresolved_conflict_count\":0,\"open_debt_count\":0},\"warnings\":[]}",
        )
        .unwrap();
        fs::write(
            root.join(format!("{stem}-source-diagnostics.json")),
            "{\"version\":1,\"source_pack\":{\"status\":\"success\",\"queries\":[],\"discovered_source_count\":0,\"adopted_source_count\":0,\"skipped_candidates\":[]},\"scrapes\":[],\"context_packing\":null}",
        )
        .unwrap();
        fs::write(
            root.join(format!("{stem}-resolved-system-prompt.md")),
            "system prompt",
        )
        .unwrap();
        fs::write(
            root.join(format!("{stem}-resolved-user-prompt.md")),
            "### User Request:\nCompare Apple MacBook Pro and Framework Laptop options for a local Rust and AI workflow.\n\n[RESEARCH QUALITY REPAIR ITERATION 2/2]\nrepair notes",
        )
        .unwrap();

        let case_plans = vec![CaseExecutionPlan {
            case: sample_case("01-product-decision", "01-product-decision.md"),
            artifact_stem: "1-01-product-decision".to_string(),
        }];
        let bundles = load_replay_fixture_bundles(&root, &case_plans).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(
            bundles[0].plan.case.case_id,
            "03-comparative-product-technical-decision"
        );
        assert_eq!(
            bundles[0].plan.case.title,
            "Comparative Product Technical Decision Rerun"
        );
        assert_eq!(
            bundles[0].plan.case.prompt,
            "Compare Apple MacBook Pro and Framework Laptop options for a local Rust and AI workflow."
        );
        assert_eq!(bundles[0].plan.artifact_stem, stem);
        assert!(bundles[0]
            .plan
            .case
            .must_pass_checks
            .iter()
            .any(|check| check.contains("current official technical specifications")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn live_runs_default_outside_repo() {
        let runs_dir = resolve_runs_dir(ResearchBenchmarkMode::Live, None);
        assert_eq!(
            runs_dir,
            std::env::temp_dir().join(DEFAULT_LIVE_RUNS_DIR_NAME)
        );
    }

    #[test]
    fn write_case_artifacts_refuses_to_overwrite_existing_files() {
        let dir =
            std::env::temp_dir().join(format!("research-bench-artifact-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case", "case.md");
        let result = sample_result();
        let scorecard = build_case_scorecard(&case, &result);

        write_case_artifacts(&dir, "1-case", &result, &scorecard, false).unwrap();
        let err = write_case_artifacts(&dir, "1-case", &result, &scorecard, false).unwrap_err();

        assert!(err.to_string().contains("File exists"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_case_artifacts_writes_failure_markdown_when_final_output_is_missing() {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-failure-artifact-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case-failed", "case-failed.md");
        let mut result = sample_result();
        result.case_id = "case-failed".to_string();
        result.title = "Failed Case".to_string();
        result.status = "failed".to_string();
        result.quality_status = Some("failed".to_string());
        result.error_message = Some("model invocation failed".to_string());
        result.quality_last_failure = Some("model invocation failed".to_string());
        result.research_controller_stage = Some("case_execution_failed".to_string());
        result.final_output = None;
        let scorecard = build_case_scorecard(&case, &result);

        let paths =
            write_case_artifacts(&dir, "1-case-failed", &result, &scorecard, false).unwrap();
        let artifact = fs::read_to_string(paths.final_output_path.unwrap()).unwrap();

        assert!(artifact.contains("# Benchmark Case Failure"));
        assert!(artifact.contains("model invocation failed"));
        assert!(artifact.contains("final output captured in task result: no"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_case_artifacts_strips_internal_research_artifact_block_from_final_output() {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-sanitized-final-output-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case", "case.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변 (Final Answer)\n독자용 요약입니다.\n\n# 검증 부록\n## 출처 감사 (Source Audit)\n- Example\n## Research Debt\n- debt-research-quality-gate-failed-source-audit-url-count [open]: Research quality gate failed: source audit URL count 0 is below required minimum 7 | next=Repair the failed quality gate item with stronger evidence or a narrower claim.\n\n[RESEARCH_ARTIFACT_JSON]\n```json\n{\"narrative_state\":{\"version\":1},\"warnings\":[\"<repair_prompt>\",\"quality gate failed\",\"claim log underfilled\"]}\n```".to_string(),
        );
        result.research_controller_artifacts_json =
            Some("{\"version\":1,\"narrative_state\":{\"version\":1}}".to_string());
        let scorecard = build_case_scorecard(&case, &result);

        let paths = write_case_artifacts(&dir, "1-case", &result, &scorecard, false).unwrap();
        let final_output = fs::read_to_string(paths.final_output_path.unwrap()).unwrap();

        assert!(final_output.contains("## 최종 답변 (Final Answer)"));
        assert!(final_output.contains("# 검증 부록"));
        assert!(!final_output.contains("[RESEARCH_ARTIFACT_JSON]"));
        assert!(!final_output.contains("narrative_state"));
        assert!(!final_output.contains("<repair_prompt>"));
        assert!(!final_output.contains("quality gate failed"));
        assert!(!final_output.contains("claim log underfilled"));
        assert!(!final_output.contains("debt-research-quality-gate-failed-"));
        assert!(!final_output.contains("Repair the failed quality gate item"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_case_artifacts_skips_raw_debug_artifacts_by_default() {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-raw-debug-default-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case", "case.md");
        let mut result = sample_result();
        result.final_output = Some("## 최종 답변 (Final Answer)\n독자용 요약입니다.".to_string());
        result.research_source_diagnostics_json = Some("{\"subject\":\"secret\"}".to_string());
        result.research_controller_artifacts_json = Some(
            "{\"version\":1,\"narrative_state\":{\"version\":1,\"timeline\":[{\"id\":\"N1\",\"label\":\"배경\"}]}}".to_string(),
        );
        result.resolved_system_prompt = Some("system prompt".to_string());
        result.resolved_user_prompt = Some("user prompt".to_string());
        let scorecard = build_case_scorecard(&case, &result);

        let paths = write_case_artifacts(&dir, "1-case", &result, &scorecard, false).unwrap();

        assert!(paths.diagnostics_json_path.is_none());
        assert!(paths.controller_json_path.is_none());
        assert!(paths.resolved_system_prompt_path.is_none());
        assert!(paths.resolved_user_prompt_path.is_none());
        assert!(!dir.join("1-case-source-diagnostics.json").exists());
        assert!(!dir.join("1-case-controller-artifacts.json").exists());
        assert!(!dir.join("1-case-resolved-system-prompt.md").exists());
        assert!(!dir.join("1-case-resolved-user-prompt.md").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_case_artifacts_preserves_controller_artifacts_json_when_raw_debug_opted_in() {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-controller-artifacts-retention-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case", "case.md");
        let mut result = sample_result();
        result.final_output = Some("## 최종 답변 (Final Answer)\n독자용 요약입니다.".to_string());
        result.research_controller_artifacts_json = Some(
            "{\"version\":1,\"narrative_state\":{\"version\":1,\"timeline\":[{\"id\":\"N1\",\"label\":\"배경\"}]}}".to_string(),
        );
        let scorecard = build_case_scorecard(&case, &result);

        let paths = write_case_artifacts(&dir, "1-case", &result, &scorecard, true).unwrap();
        let controller_json = fs::read_to_string(paths.controller_json_path.unwrap()).unwrap();

        assert!(controller_json.contains("narrative_state"));
        assert!(controller_json.contains("\"timeline\""));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_case_failure_fallback_artifacts_preserves_markdown_path_when_summary_json_write_fails()
    {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-fallback-summary-failure-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let case = sample_case("case-failed", "case-failed.md");
        let mut result = sample_result();
        result.case_id = "case-failed".to_string();
        result.title = "Failed Case".to_string();
        result.status = "failed".to_string();
        result.quality_status = Some("failed".to_string());
        result.error_message = Some("model invocation failed".to_string());
        result.quality_last_failure = Some("model invocation failed".to_string());
        result.research_controller_stage = Some("case_execution_failed".to_string());
        result.final_output = None;
        let scorecard = build_case_scorecard(&case, &result);
        let fallback_stem = "1-case-failed-fallback";
        fs::write(
            dir.join(format!("{fallback_stem}.json")),
            "preexisting collision",
        )
        .unwrap();

        let outcome = write_case_failure_fallback_artifacts_with_stem(
            &dir,
            fallback_stem,
            &result,
            &scorecard,
        );

        assert!(outcome
            .error
            .as_deref()
            .is_some_and(|message| message.contains("failure summary JSON write failed")));
        assert!(outcome
            .paths
            .final_output_path
            .as_ref()
            .is_some_and(|path| path.ends_with(format!("{fallback_stem}.md"))));
        assert!(outcome.paths.summary_json_path.is_none());
        let artifact = fs::read_to_string(
            outcome
                .paths
                .final_output_path
                .as_ref()
                .expect("failure markdown path preserved"),
        )
        .unwrap();
        assert!(artifact.contains("# Benchmark Case Failure"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn materialize_case_execution_converts_artifact_write_failure_into_failed_execution() {
        let dir = std::env::temp_dir().join(format!(
            "research-bench-materialize-failure-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1-case-final-output.md"), "preexisting collision").unwrap();
        let plan = CaseExecutionPlan {
            case: sample_case("case", "case.md"),
            artifact_stem: "1-case".to_string(),
        };
        let execution = materialize_case_execution(&dir, &plan, sample_result(), None, false);

        assert_eq!(execution.result.status, "failed");
        assert_eq!(execution.result.quality_status.as_deref(), Some("failed"));
        assert!(execution
            .result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("benchmark artifact write failed")));
        assert!(execution
            .artifact_paths
            .final_output_path
            .as_ref()
            .is_some_and(|path| path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("failure-"))));
        let failure_artifact = fs::read_to_string(
            execution
                .artifact_paths
                .final_output_path
                .as_ref()
                .expect("fallback artifact path"),
        )
        .unwrap();
        assert!(failure_artifact.contains("# Benchmark Case Failure"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn structured_scorecard_populates_all_rubric_dimensions_and_fixture_caveat() {
        let case = sample_case("case", "case.md");
        let result = sample_result();
        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.measurement_kind, PIPELINE_EVIDENCE_MEASUREMENT);
        assert_eq!(scorecard.evidence_caveat, PIPELINE_EVIDENCE_CAVEAT);
        assert_eq!(scorecard.dimensions.len(), RUBRIC_DIMENSIONS.len());
        assert!(scorecard.visibility.fixture_only_pipeline_evidence);
        assert_eq!(scorecard.advisory_model_self_score, Some(4.0));
        assert!(scorecard
            .dimensions
            .iter()
            .all(|dimension| dimension.visibility == SCORE_VISIBILITY));
    }

    #[test]
    fn scorecard_captures_narrative_metrics_without_counting_them_as_evidence() {
        let case = historical_case("case-history-narrative", "case-history-narrative.md");
        let mut result = sample_result();
        result.category = case.category.clone();
        result.research_controller_artifacts_json = Some(
            serde_json::to_string(&json!({
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
                "narrative_state": {
                    "version": 1,
                    "timeline": [{"id":"NE1","label":"배경 형성"}],
                    "evidence_layers": [{"id":"NL1","label":"확인된 사실"}],
                    "interpretive_tensions": [{"id":"NT1","question":"핵심 해석 차이"}],
                    "impacts": [{"id":"NI1","label":"장기 영향"}],
                    "reader_questions": [{"id":"NQ1","question":"독자가 바로 확인할 쟁점"}],
                    "section_outline": [{"id":"NS1","heading":"배경"}],
                    "open_gaps": [{"id":"NG1","gap_type":"impact","description":"추가 확인 필요"}]
                },
                "reader_quality": {
                    "argument_graph": {
                        "nodes": [{"id":"AQN1","label":"핵심 주장","claim_log_ids":["C1"],"source_card_ids":["S1"]}],
                        "edges": [{"id":"AQE1","from_node_id":"AQN1","to_node_id":"AQN2","relation":"supports"}]
                    },
                    "narrative_plan": {
                        "lead_section_id": "NS1",
                        "section_ids": ["NS1"],
                        "transition_ids": ["TR1"]
                    },
                    "section_briefs": [{"section_id":"NS1","key_point":"핵심 배경부터 제시"}],
                    "reader_critique": {
                        "metrics": [
                            {"key":"clarity","label":"독자 명확성","status":"passed"},
                            {"key":"transition","label":"전환","status":"needs_work"}
                        ]
                    }
                },
                "quality_gate": {
                    "status": "passed",
                    "failure_messages": [],
                    "unsupported_claim_count": 0,
                    "unresolved_conflict_count": 0,
                    "open_debt_count": 0
                },
                "warnings": []
            }))
            .unwrap(),
        );
        result.research_source_diagnostics_json = Some(
            serde_json::to_string(&json!({
                "version": 1,
                "source_pack": {"status": "success", "queries": [], "discovered_source_count": 1, "adopted_source_count": 1, "skipped_candidates": []},
                "scrapes": [],
                "context_packing": {
                    "included_source_card_count": 1,
                    "omitted_source_card_count": 0,
                    "narrative_state_present": true,
                    "narrative_timeline_event_count": 1,
                    "narrative_section_count": 1,
                    "narrative_evidence_layer_count": 1,
                    "narrative_interpretive_tension_count": 1,
                    "narrative_impact_count": 1,
                    "narrative_reader_question_count": 1,
                    "narrative_open_gap_count": 1,
                    "reader_quality_present": true,
                    "reader_argument_node_count": 1,
                    "reader_argument_edge_count": 1,
                    "reader_narrative_plan_present": true,
                    "reader_section_brief_count": 1,
                    "reader_critique_present": true,
                    "reader_critique_metric_count": 2,
                    "reader_critique_failed_metric_count": 1
                }
            }))
            .unwrap(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.metrics.narrative_state_present);
        assert_eq!(scorecard.metrics.narrative_timeline_event_count, 1);
        assert_eq!(scorecard.metrics.narrative_section_count, 1);
        assert_eq!(scorecard.metrics.narrative_evidence_layer_count, 1);
        assert_eq!(scorecard.metrics.narrative_interpretive_tension_count, 1);
        assert_eq!(scorecard.metrics.narrative_impact_count, 1);
        assert_eq!(scorecard.metrics.narrative_reader_question_count, 1);
        assert_eq!(scorecard.metrics.narrative_open_gap_count, 1);
        assert!(scorecard.metrics.reader_quality_present);
        assert_eq!(scorecard.metrics.reader_argument_node_count, 1);
        assert_eq!(scorecard.metrics.reader_argument_edge_count, 1);
        assert!(scorecard.metrics.reader_narrative_plan_present);
        assert_eq!(scorecard.metrics.reader_section_brief_count, 1);
        assert!(scorecard.metrics.reader_critique_present);
        assert_eq!(scorecard.metrics.reader_critique_metric_count, 2);
        assert_eq!(scorecard.metrics.reader_critique_failed_metric_count, 1);
        assert!(scorecard.metrics.context_pack_reader_quality_present);
        assert_eq!(scorecard.metrics.context_pack_reader_section_brief_count, 1);
        assert_eq!(
            scorecard.metrics.context_pack_reader_critique_metric_count,
            2
        );
        assert!(scorecard.metrics.context_pack_narrative_state_present);
        assert!(scorecard
            .warnings
            .iter()
            .any(|warning| warning.contains("narrative diagnostics")));
        assert!(scorecard
            .warnings
            .iter()
            .any(|warning| warning.contains("reader-quality diagnostics")));
        assert_eq!(scorecard.metrics.supported_claim_count, 1);
    }

    #[test]
    fn parsed_controller_artifacts_default_missing_reader_critique_metric_status() {
        let controller = serde_json::from_value::<ParsedControllerArtifacts>(json!({
            "version": 1,
            "source_cards": [],
            "claim_log": [],
            "conflict_map": [],
            "research_debt": [],
            "reader_quality": {
                "reader_critique": {
                    "metrics": [
                        {
                            "key": "clarity",
                            "label": "Reader clarity"
                        },
                        {
                            "key": "momentum",
                            "label": "Reader momentum",
                            "status": null
                        }
                    ]
                }
            }
        }))
        .expect("controller artifacts should deserialize");

        let metrics = &controller
            .reader_quality
            .as_ref()
            .and_then(|reader_quality| reader_quality.reader_critique.as_ref())
            .expect("reader critique should be present")
            .metrics;
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].status, "unknown");
        assert_eq!(metrics[1].status, "unknown");
    }

    #[test]
    fn live_scorecard_and_run_report_use_live_measurement_metadata() {
        let case = sample_case("case-live", "case-live.md");
        let mut result = sample_result();
        result.mode = ResearchBenchmarkMode::Live;
        result.model_input = "cli:codex".to_string();
        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.measurement_kind, LIVE_QUALITY_MEASUREMENT);
        assert_eq!(scorecard.evidence_caveat, LIVE_QUALITY_CAVEAT);
        assert!(!scorecard.visibility.fixture_only_pipeline_evidence);

        let execution = RunExecution {
            case: case.clone(),
            result: result.clone(),
            scorecard: scorecard.clone(),
            artifact_paths: CaseArtifactPaths::default(),
            replay_before: None,
        };
        let args = Args {
            cases_dir: PathBuf::from(DEFAULT_CASES_DIR),
            runs_dir: None,
            label: "live-report".to_string(),
            mode: BenchModeArg::Live,
            replay_fixture_root: PathBuf::from(DEFAULT_REPLAY_FIXTURE_ROOT),
            data_dir: None,
            model_input: Some("cli:codex".to_string()),
            engine_name: Some("Codex CLI".to_string()),
            model_name: Some("codex".to_string()),
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            max_iterations: 2,
            cli_launch_mode: None,
            ai_task_timeout_secs: 3600,
            include_raw_debug_artifacts: false,
        };
        let report = build_structured_run_report(
            &args,
            ResearchBenchmarkMode::Live,
            "2026-05-15T00:00:00Z",
            "Codex CLI",
            "codex",
            Path::new("/tmp/live-report"),
            &[execution],
        );

        assert_eq!(report.measurement_kind, LIVE_QUALITY_MEASUREMENT);
        assert_eq!(report.evidence_caveat, LIVE_QUALITY_CAVEAT);
    }

    #[test]
    fn benchmark_visibility_accepts_quality_final_answer_aliases() {
        let case = sample_case("case-alias", "case-alias.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 결론\nRust and AI workflow comparison with a concrete recommendation.\n\n# 검증 부록\n## 출처 감사 (Source Audit)\n- Apple docs\n\n## 주장 로그 (Claim Log)\n- C1 supported by S1\n\n## 품질 게이트 (Quality Gate)\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.visibility.visible_final_answer);
        assert!(scorecard
            .critical_flags
            .iter()
            .find(|flag| flag.key == "fails_to_answer_question")
            .is_some_and(|flag| !flag.triggered));
    }

    #[test]
    fn clean_reader_facing_answer_with_appendix_does_not_trigger_leakage_flag() {
        let case = sample_case("case-clean", "case-clean.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n남산 아침 러닝은 북측 순환로 기준으로 오르막 강도가 비교적 분명해 초반 운동 목적에 맞습니다. 운동 직후에는 남산공원 접근성이 좋은 카페를 고르면 이동 부담을 줄일 수 있습니다. 다만 영업시간과 샤워 가능 여부는 당일 재확인이 필요합니다.\n\n# 검증 부록\n## 출처 감사\n- 남산공원 안내\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.metrics.reader_facing_prompt_echo_count, 0);
        assert_eq!(scorecard.metrics.reader_facing_artifact_leak_count, 0);
        assert!(scorecard
            .critical_flags
            .iter()
            .find(|flag| flag.key == "reader_facing_prompt_or_artifact_leakage")
            .is_some_and(|flag| !flag.triggered));
    }

    #[test]
    fn reader_facing_prompt_and_artifact_leakage_triggers_critical_flag_and_penalty() {
        let mut case = sample_case("case-leak", "case-leak.md");
        case.prompt = "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward."
            .to_string();
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\nWrite a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul. 이 답변은 source pack 정리와 target-host/source-class miss, open debt, chronology/actor/cause/consequence 메모를 그대로 남긴 상태입니다.\n\n# 검증 부록\n## 출처 감사\n- 남산공원 안내\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let readability = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "structure_and_readability")
            .expect("readability dimension");

        assert!(scorecard.metrics.reader_facing_prompt_echo_count > 0);
        assert!(scorecard.metrics.reader_facing_artifact_leak_count > 0);
        assert!(scorecard
            .critical_flags
            .iter()
            .find(|flag| flag.key == "reader_facing_prompt_or_artifact_leakage")
            .is_some_and(|flag| flag.triggered));
        assert!(readability.score < 5);
    }

    #[test]
    fn clean_english_prose_with_actors_causes_and_consequences_is_not_flagged_as_artifact_leak() {
        let case = sample_case("case-clean-english", "case-clean-english.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## Final Answer\nThe policy changed because multiple actors faced different incentives, and the downstream consequences were uneven across local operators. The answer explains the causes and consequences directly for the reader without exposing any controller or validation metadata.\n\n# Verification Appendix\n## Source Audit\n- Official policy page\n\n## Claim Log\n- C1 supported by S1\n\n## Quality Gate\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.metrics.reader_facing_artifact_leak_count, 0);
        assert!(scorecard
            .critical_flags
            .iter()
            .find(|flag| flag.key == "reader_facing_prompt_or_artifact_leakage")
            .is_some_and(|flag| !flag.triggered));
    }

    #[test]
    fn korean_internal_validation_labels_before_appendix_trigger_artifact_leakage_flag() {
        let case = sample_case("case-korean-leak", "case-korean-leak.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 문단은 독자용 결론처럼 보이지만 품질 게이트, 출처 감사, 주장 로그를 본문에서 그대로 설명하고 있어 내부 검증 어휘가 노출됩니다.\n\n# 검증 부록\n## 출처 감사\n- 남산공원 안내\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.metrics.reader_facing_artifact_leak_count > 0);
        assert!(scorecard
            .critical_flags
            .iter()
            .find(|flag| flag.key == "reader_facing_prompt_or_artifact_leakage")
            .is_some_and(|flag| flag.triggered));
    }

    #[test]
    fn live_scorecard_warns_when_source_pack_success_is_single_provider_dominant() {
        let case = sample_case("case-live-provider", "case-live-provider.md");
        let mut result = sample_result();
        result.mode = ResearchBenchmarkMode::Live;
        result.model_input = "cli:codex".to_string();
        result.research_source_diagnostics_json = Some(
            serde_json::to_string(&json!({
                "version": 1,
                "subject": "Compare Apple MacBook Pro and Framework Laptop 13 specifications",
                "source_pack": {
                    "subject": "Compare Apple MacBook Pro and Framework Laptop 13 specifications",
                    "status": "success",
                    "reason": null,
                    "queries": [
                        {
                            "query": "Apple MacBook Pro official technical specifications",
                            "status": "success",
                            "provider": "duckduckgo",
                            "result_count": 2,
                            "adopted_count": 2,
                            "skipped_count": 0,
                            "error": null
                        },
                        {
                            "query": "Framework Laptop 13 official technical specifications",
                            "status": "success",
                            "provider": "duckduckgo",
                            "result_count": 2,
                            "adopted_count": 2,
                            "skipped_count": 0,
                            "error": null
                        }
                    ],
                    "discovered_source_count": 4,
                    "adopted_source_count": 4,
                    "skipped_candidates": []
                },
                "scrapes": [],
                "context_packing": null
            }))
            .unwrap(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.warnings.iter().any(|warning| {
            warning.contains("single-provider source-pack limitation")
                && warning.contains("duckduckgo")
        }));
    }

    #[test]
    fn historical_overlay_flags_shallow_history_and_caps_overall_score() {
        let case = historical_case("case-history-shallow", "case-history-shallow.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 사건은 중요했고 결국 제도의 방향을 바꾸었습니다. 독자는 큰 흐름만 이해하면 됩니다.\n\n# 검증 부록\n## 출처 감사\n- 백과사전 개요\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.historical_overlay_trigger_count >= 2);
        assert!(scorecard.any_critical_failure);
        assert!(scorecard.overall_score <= 2.0);
        assert!(scorecard
            .critical_flags
            .iter()
            .any(|flag| flag.key == "historical_missing_chronology" && flag.triggered));
    }

    #[test]
    fn historical_overlay_accepts_richer_history_answer_with_limits() {
        let case = historical_case("case-history-rich", "case-history-rich.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 변화는 3세기 후반의 위기 이후에 나타났고, 먼저 중앙 권력의 재정 정비 시도와 이후 지방 세력의 반응을 순서대로 봐야 이해할 수 있습니다. 핵심 행위자는 황제, 군 지휘층, 지방 엘리트였으며, 사건의 무대도 수도와 국경 지역처럼 서로 다른 지리적 조건을 가진 공간으로 나뉘었습니다. 배경에는 군사 압박과 재정 불안이 함께 있었고, 그 결과 행정 운영 방식과 지역 통제 방식에도 장기적인 영향이 남았습니다. 다만 사료의 한계 때문에 모든 동기와 결과를 단정할 수는 없고, 일부 해석은 오늘날 연구자들 사이에서도 견해가 갈립니다. 여기서는 확실히 확인되는 전개와 의미에 범위를 좁혀 설명합니다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n- 백과사전 개요\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.historical_overlay_trigger_count, 0);
        assert!(scorecard
            .critical_flags
            .iter()
            .filter(|flag| flag.key.starts_with("historical_"))
            .all(|flag| !flag.triggered));
        assert!(scorecard.metrics.historical_lens_coverage_count >= 6);
    }

    #[test]
    fn historical_overlay_does_not_count_factor_as_actor_signal() {
        let case = historical_case("case-history-factor", "case-history-factor.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이후 전개는 특정 지역의 경제적 factors와 군사 압박을 원인으로 삼아 설명할 수 있습니다. 그 결과 행정 운영 방식에 영향이 남았지만, 사료의 한계와 논쟁 때문에 모든 동기를 단정하기는 어렵습니다. 여기서는 확인 가능한 범위를 좁혀 설명합니다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert_eq!(scorecard.metrics.historical_actor_signal_count, 0);
        assert!(scorecard.critical_flags.iter().any(|flag| {
            flag.key == "historical_missing_actors_or_geography" && flag.triggered
        }));
    }

    #[test]
    fn historical_overlay_counts_common_english_actor_terms_without_matching_factors() {
        let case = historical_case("case-history-actors", "case-history-actors.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 시기에는 emperors와 rival rulers가 서로 다른 선택을 했고, the army and military elites also shaped the outcome. 이후 전개는 economic factors와 국경 지역의 압박이 함께 작용한 결과로 볼 수 있지만, 여기서 actors는 군과 지배층처럼 실제 행위 주체를 가리킵니다. 다만 사료의 한계와 해석 차이 때문에 모든 동기를 단정할 수는 없습니다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.metrics.historical_actor_signal_count >= 4);
        assert!(!scorecard.critical_flags.iter().any(|flag| {
            flag.key == "historical_missing_actors_or_geography" && flag.triggered
        }));
    }

    #[test]
    fn historical_genre_richness_penalizes_flat_answers_without_visible_history_axes() {
        let case = historical_case("case-history-flat", "case-history-flat.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 사건은 특정 시기의 위기 속에서 일어났고 주요 행위자와 지역이 얽혀 있었습니다. 배경에는 재정 압박과 군사 문제가 있었고 그 결과 제도 변화가 뒤따랐습니다. 사료의 한계와 해석 차이도 있지만, 전체적으로는 위기 대응의 사례로 볼 수 있습니다.\n\n# 검증 부록\n## 출처 감사\n- 백과사전 개요\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert_eq!(scorecard.metrics.genre_section_richness_signal_count, 0);
        assert!(richness.score <= 1);
    }

    #[test]
    fn historical_genre_richness_rewards_visible_history_scaffold_axes() {
        let case = historical_case("case-history-scaffold", "case-history-scaffold.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n역사적 전환점의 핵심은 사건 자체보다 그것을 어떻게 읽느냐에 있습니다.\n\n### 동시대 비교\n같은 시기 다른 지역 사례와 나란히 놓아 보면 이 변화가 예외인지 구조적 흐름인지 더 분명해집니다.\n\n### 전개 순서와 해석\n먼저 사건의 전개 순서를 짚고, 그다음 후대 연구가 이 흐름을 어떻게 해석하는지 구분해 읽어야 합니다.\n\n### 사료 층위\n동시대 기록, 후대 서술, 물질 자료, 현대 연구는 서로 다른 강점과 한계를 보여 줍니다.\n\n### 쟁점 지도\n핵심 쟁점은 동기의 해석, 정책의 효과, 그리고 승자의 서사가 얼마나 개입했는가입니다.\n\n### 후대 영향\n직접적 결과뿐 아니라 이후 제도와 정치 언어에 남긴 장기 영향도 함께 봐야 합니다.\n\n### 후속 탐색 질문\n다음 질문은 어떤 자료 층위가 가장 큰 공백을 남기는지, 그리고 비교 사례가 해석을 어떻게 바꾸는지입니다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n- 현대 연구\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert!(scorecard.metrics.genre_section_richness_signal_count >= 6);
        assert_eq!(richness.score, 5);
    }

    #[test]
    fn historical_scorecard_warns_when_hidden_artifacts_and_richness_are_shallow() {
        let case = historical_case("case-history-warning", "case-history-warning.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 사건은 특정 시기의 위기 속에서 일어났고 주요 행위자와 지역이 얽혀 있었습니다. 배경에는 재정 압박과 군사 문제가 있었고 그 결과 제도 변화가 뒤따랐습니다. 사료의 한계와 해석 차이도 있지만, 전체적으로는 위기 대응의 사례로 볼 수 있습니다.\n\n# 검증 부록\n## 출처 감사\n- 백과사전 개요\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard
            .warnings
            .iter()
            .any(|warning| warning.contains("historical richness remains shallow")));
        assert!(scorecard.warnings.iter().any(|warning| {
            warning.contains("historical hidden planning artifacts are missing")
        }));
    }

    #[test]
    fn second_punic_scorecard_flags_shallow_phase_density_regression() {
        let case = second_punic_case("case-second-punic-flat", "second-punic-flat.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n한니발은 로마를 위협했지만 결국 전쟁은 로마의 승리로 끝났다. 218 BCE와 216 BCE가 중요했다는 점만 간단히 언급하고, 카르타고와 로마의 긴 전개는 한 문단으로 압축한다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.critical_flags.iter().any(|flag| {
            flag.key == "historical_campaign_phase_density_floor" && flag.triggered
        }));
        assert!(scorecard
            .warnings
            .iter()
            .any(|warning| { warning.contains("second punic war phase-density floor missed") }));
    }

    #[test]
    fn second_punic_scorecard_accepts_dense_phase_density() {
        let case = second_punic_case("case-second-punic-rich", "second-punic-rich.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 전쟁은 장기 원정과 다전선 전환을 단계별로 읽어야 한다.\n\n### Phase 1. Saguntum Crisis (219-218 BCE)\nHannibal과 Carthage 지휘부는 Iberia에서 Saguntum 위기를 전면전으로 바꾸었고 Rome의 외교 대응은 실패했다.\n\n### Phase 2. Alpine Invasion (218 BCE)\nHannibal은 Alps를 넘어 Italy로 진입했고 Roman 집정관들은 북부 전선을 급히 재편했다.\n\n### Phase 3. Trasimene And Cannae (217-216 BCE)\nHannibal은 Trasimene과 Cannae에서 Roman 야전군을 무너뜨리며 동맹과 지휘 체계에 충격을 주었다.\n\n### Phase 4. Roman Endurance (215-212 BCE)\nRome은 Italy와 Sicily에서 다전선 동원을 유지하며 Carthage의 단기 결전을 피했다.\n\n### Phase 5. Iberian Reversal (211-206 BCE)\nScipio는 Iberia에서 Carthage의 기반을 흔들었고 Hannibal의 전략적 깊이를 줄였다.\n\n### Phase 6. African Decision (204-202 BCE)\nScipio의 Africa 상륙은 Hannibal의 귀환과 Zama 이전의 최종 전환을 만들었다.\n\n### Phase 7. Settlement (201 BCE)\n201 BCE 강화는 Carthage를 제약하고 Rome의 지중해 우위를 굳히는 결과로 이어졌다.\n\n### 동시대 비교\n같은 시기 다른 전쟁과 비교하면 Rome의 회복 방식이 더 선명해진다.\n\n### 전개 순서와 해석\n연대기적 국면과 후대 해석을 분리해 읽어야 한다.\n\n### 사료 층위\nPolybius, Livy, modern scholarship의 층위를 나눠 봐야 한다.\n\n### 쟁점 지도\n전략적 genius와 구조적 자원 격차 사이의 해석 경쟁이 남는다.\n\n### 후대 영향\nRoman expansion과 Mediterranean order 재편이라는 장기 효과가 뒤따랐다.\n\n### 후속 탐색 질문\n동맹 유지 비용과 Carthaginian internal politics를 더 따져볼 필요가 있다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n- 현대 연구\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.critical_flags.iter().all(|flag| {
            flag.key != "historical_campaign_phase_density_floor" || !flag.triggered
        }));
        assert!(
            scorecard.metrics.second_punic_phase_subsection_count
                >= SECOND_PUNIC_WAR_MIN_PHASE_SUBSECTIONS
        );
    }

    #[test]
    fn second_punic_scorecard_exempts_all_punic_comparison_without_centered_focus() {
        let case = BenchmarkCase {
            case_id: "case-all-punic-comparison".to_string(),
            filename: "all-punic-comparison.md".to_string(),
            title: "Compare the First, Second, and Third Punic Wars".to_string(),
            category: "historical-explanation".to_string(),
            prompt: "Provide a comparative overview across all three Punic Wars, noting how the Second Punic War differs within the wider Roman-Carthaginian sequence.".to_string(),
            must_pass_checks: vec!["Keeps comparative framing".to_string()],
            expected_failure_modes: vec!["Should not be benchmarked as a centered Hannibal phase-density case".to_string()],
        };
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n세 차례 포에니 전쟁은 각각 해전 중심의 초기 충돌, 한니발이 끼어든 중간 전쟁, 그리고 카르타고 파괴로 이어진 최종 전쟁으로 비교할 수 있다.\n\n# 검증 부록\n## 출처 감사\n- 비교 개관\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(!is_second_punic_benchmark_case(&case));
        assert!(scorecard.critical_flags.iter().all(|flag| {
            flag.key != "historical_campaign_phase_density_floor" || !flag.triggered
        }));
    }

    #[test]
    fn second_punic_scorecard_keeps_centered_comparative_exception() {
        let case = BenchmarkCase {
            case_id: "case-second-punic-centered-comparison".to_string(),
            filename: "second-punic-centered-comparison.md".to_string(),
            title: "Compare all Punic Wars with emphasis on Hannibal and the Second Punic War".to_string(),
            category: "historical-explanation".to_string(),
            prompt: "Across the Punic Wars, keep the comparison but center the campaign narrative on Hannibal and the Second Punic War.".to_string(),
            must_pass_checks: vec!["Keeps centered Second Punic focus".to_string()],
            expected_failure_modes: vec!["Centered Hannibal comparison should still hit the phase-density benchmark floor".to_string()],
        };
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n한니발의 전역을 비교 틀 안에 두더라도 한 문단 요약으로 끝내면 안 된다. 218 BCE, 216 BCE, 202 BCE만 짧게 언급하고 넘어가면 중심 전역의 국면 밀도가 무너진다.\n\n# 검증 부록\n## 출처 감사\n- 비교 개관\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(is_second_punic_benchmark_case(&case));
        assert!(scorecard.critical_flags.iter().any(|flag| {
            flag.key == "historical_campaign_phase_density_floor" && flag.triggered
        }));
    }

    #[test]
    fn historical_scorecard_warns_when_hidden_planning_refs_use_private_source_urls() {
        let case = historical_case(
            "case-history-private-ref-warning",
            "case-history-private.md",
        );
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n역사적 전환점의 핵심은 사건 자체보다 그것을 어떻게 읽느냐에 있습니다.\n\n### 동시대 비교\n같은 시기 다른 지역 사례와 비교합니다.\n\n### 전개 순서와 해석\n전개 순서와 해석을 분리합니다.\n\n### 사료 층위\n사료 층위의 한계를 드러냅니다.\n\n# 검증 부록\n## 출처 감사\n- private metadata source\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );
        result.research_controller_artifacts_json = Some(
            r#"{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "http://169.254.169.254/latest/meta-data/",
      "title": "private metadata",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "Private URL must not ground planning.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "reader_quality": {
    "argument_graph": {
      "nodes": [
        {"id": "N1", "label": "배경", "claim_log_ids": ["C1"], "source_card_ids": ["S1"]},
        {"id": "N2", "label": "해석", "claim_log_ids": ["C1"], "source_card_ids": ["S1"]}
      ],
      "edges": []
    },
    "section_briefs": [
      {"section_id": "S1", "key_point": "전개", "claim_log_ids": ["C1"], "source_card_ids": ["S1"]},
      {"section_id": "S2", "key_point": "해석", "claim_log_ids": ["C1"], "source_card_ids": ["S1"]}
    ]
  },
  "research_debt": []
}
"#
            .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.warnings.iter().any(|warning| {
            warning.contains("historical hidden planning artifacts are missing")
        }));
    }

    #[test]
    fn benchmark_url_guard_rejects_non_public_ip_ranges() {
        for raw_url in [
            "http://127.0.0.1/private",
            "http://10.0.0.5/internal",
            "http://100.64.0.1/carrier-nat",
            "http://198.18.0.1/benchmark-net",
            "http://0.0.0.0/unspecified",
            "http://255.255.255.255/broadcast",
            "http://[::1]/private",
            "http://[fd00::1]/private",
            "http://[fe80::1]/link-local",
            "http://[::ffff:127.0.0.1]/mapped-loopback",
        ] {
            assert!(
                !is_valid_http_url(raw_url),
                "non-public benchmark URL should be rejected: {raw_url}"
            );
        }
        assert!(is_valid_http_url("https://docs.vllm.ai/en/latest/"));
    }

    #[test]
    fn historical_scorecard_warns_on_generic_open_debt_placeholder() {
        let case = historical_case("case-history-debt-warning", "case-history-debt-warning.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n역사적 전환점의 핵심은 사건 자체보다 그것을 어떤 층위의 증거와 해석으로 읽느냐에 있습니다.\n\n### 동시대 비교\n같은 시기 다른 지역 사례와 나란히 놓아 보면 이 변화가 예외인지 구조적 흐름인지 더 분명해집니다.\n\n### 전개 순서와 해석\n먼저 사건의 전개 순서를 짚고, 그다음 후대 연구가 이 흐름을 어떻게 해석하는지 구분해 읽어야 합니다.\n\n### 사료 층위\n동시대 기록, 후대 서술, 물질 자료, 현대 연구는 서로 다른 강점과 한계를 보여 줍니다.\n\n### 쟁점 지도\n핵심 쟁점은 동기의 해석, 정책의 효과, 그리고 승자의 서사가 얼마나 개입했는가입니다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- failed\n"
                .to_string(),
        );
        result.research_controller_artifacts_json = Some(
            r#"{
  "version": 1,
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": [
    {
      "id": "D1",
      "severity": "medium",
      "missing_evidence": "missing evidence not specified",
      "candidate_queries": ["phase-specific primary source"],
      "next_check_actions": ["name the missing phase explicitly"],
      "status": "open"
    }
  ]
}"#
            .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);

        assert!(scorecard.warnings.iter().any(|warning| {
            warning
                .contains("historical open debt still uses generic missing-evidence placeholders")
        }));
    }

    #[test]
    fn historical_genre_richness_recognizes_natural_korean_history_section_headings() {
        let case = historical_case(
            "case-history-natural-headings",
            "case-history-natural-headings.md",
        );
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 전쟁은 단순한 정복이 아니라 장기적인 재편의 출발점이었다.\n\n## 배경과 전개 순서\n초기 개입은 빠르게 성공했지만 이후 재집결과 다전선 압박이 이어졌다.\n\n## 결과와 영향\n직접적인 승리와 별개로 장기적인 재정 부담과 지역 질서 재편이 뒤따랐다.\n\n## 비교 관점\n동시대 다른 전쟁과 나란히 놓고 보면 이 사례의 구조적 특징이 선명해진다.\n\n## 주요 쟁점과 한계\n정책 책임, 전쟁 비용, 후대 서술의 편향을 함께 따져야 한다.\n\n## 사료 신뢰성과 해석의 한계\n동시대 기록과 후대 서술은 강점과 한계가 다르므로 층위를 나눠 읽어야 한다.\n\n## 결론: 확인된 사실과 불확실성\n확인된 흐름과 남는 불확실성을 구분해야 과장된 단정이 줄어든다.\n\n# 검증 부록\n## 출처 감사\n- 사료 번역본\n- 현대 연구\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert!(scorecard.metrics.historical_comparison_signal_count > 0);
        assert!(
            scorecard
                .metrics
                .historical_chronology_interpretation_split_signal_count
                > 0
        );
        assert!(scorecard.metrics.historical_source_layer_signal_count > 0);
        assert!(scorecard.metrics.historical_issue_map_signal_count > 0);
        assert!(scorecard.metrics.historical_legacy_signal_count > 0);
        assert!(scorecard.metrics.genre_section_richness_signal_count >= 5);
        assert!(richness.score >= 4);
    }

    #[test]
    fn technology_genre_richness_recognizes_design_tradeoff_and_verifiability() {
        let case = technology_case("case-tech-rich", "case-tech-rich.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 변경은 짧은 임계구간을 유지하는 설계 판단이 핵심입니다. 트레이드오프는 구현 복잡도 증가 대신 tail latency를 줄이는 데 있고, 장단점은 디버깅 난이도와 처리량 안정성 사이에서 갈립니다. 검증 방법은 재현 가능한 benchmark 입력과 profiling 단계, 실패 시 되돌릴 test plan을 함께 두는 것입니다.\n\n# 검증 부록\n## 출처 감사\n- 구현 문서\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert!(scorecard.metrics.technology_design_judgment_signal_count > 0);
        assert!(scorecard.metrics.technology_tradeoff_signal_count > 0);
        assert!(scorecard.metrics.technology_verifiability_signal_count > 0);
        assert!(richness.score >= 4);
    }

    #[test]
    fn technology_genre_richness_zero_coverage_scores_zero() {
        let case = technology_case("case-tech-flat", "case-tech-flat.md");
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 구현은 여러 작업을 병렬로 나눠 처리하며 기본 흐름과 구성 요소를 차례대로 설명합니다. 마지막에는 운영 시 주의할 점을 짧게 덧붙입니다.\n\n# 검증 부록\n## 출처 감사\n- 구현 문서\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert_eq!(scorecard.metrics.genre_section_richness_signal_count, 0);
        assert_eq!(scorecard.metrics.technology_design_judgment_signal_count, 0);
        assert_eq!(scorecard.metrics.technology_tradeoff_signal_count, 0);
        assert_eq!(scorecard.metrics.technology_verifiability_signal_count, 0);
        assert_eq!(richness.score, 0);
    }

    #[test]
    fn technology_genre_richness_recognizes_natural_implementation_guide_headings() {
        let case = technology_case(
            "case-tech-natural-headings",
            "case-tech-natural-headings.md",
        );
        let mut result = sample_result();
        result.final_output = Some(
            "## 최종 답변\n이 구현은 짧은 임계구간과 예측 가능한 깨우기 경로를 동시에 확보하는 것이 핵심이다.\n\n### 1. 핵심 스케줄링 모델\nworker-local 우선 처리와 steal fallback을 분리해 tail latency를 줄인다.\n\n### 2. Chase-Lev Deque 설계 포인트\npush/pop/steal 경계를 분명히 나눠 경쟁 구간을 줄인다.\n\n### 3. 메모리 Ordering\nacquire/release 위치를 잘못 두면 드문 손상이 반복 재현되기 어렵다.\n\n### 4. Blocking, Parking, Wakeup\n유휴 스레드 정지와 깨우기 순서를 잘못 설계하면 불필요한 지연이 커진다.\n\n### 5. 대표 구현과 신뢰도 구분\n실서비스 검증이 있는 구현과 개념 증명 수준 코드를 나눠 참고해야 한다.\n\n### 6. 구현 계획 권고\n먼저 단일 NUMA 도메인에서 계측 가능한 최소 구현을 만들고 이후 확장하는 편이 안전하다.\n\n### 7. 확인된 사실과 불확실성\ndeque 경계와 wakeup 정책은 비교적 확실하지만, 실제 워크로드별 최적점은 벤치마크로 다시 확인해야 한다.\n\n# 검증 부록\n## 출처 감사\n- 구현 문서\n\n## 주장 로그\n- C1 supported by S1\n\n## 품질 게이트\n- passed\n"
                .to_string(),
        );

        let scorecard = build_case_scorecard(&case, &result);
        let richness = scorecard
            .dimensions
            .iter()
            .find(|dimension| dimension.key == "genre_section_richness")
            .expect("genre richness dimension");

        assert!(scorecard.metrics.technology_design_judgment_signal_count > 0);
        assert!(scorecard.metrics.technology_tradeoff_signal_count > 0);
        assert!(scorecard.metrics.technology_verifiability_signal_count > 0);
        assert!(scorecard.metrics.genre_section_richness_signal_count >= 3);
        assert!(richness.score >= 4);
    }

    #[test]
    fn hidden_html_artifact_blocks_do_not_satisfy_visible_section_scoring() {
        let case = sample_case("case-html", "case-html.md");
        let mut result = sample_result();
        result.final_output = Some(
            "Visible intro only.\n<script type=\"application/json\" data-research-artifacts>{\"ghost\":\"## Final Answer\\n## Claim Log\\n## Quality Gate\\n## 0-5 Score\\n5\"}</script>".to_string(),
        );
        result.research_controller_artifacts_json = Some(
            serde_json::to_string(&json!({
                "version": 1,
                "source_cards": [],
                "claim_log": [],
                "conflict_map": [],
                "research_debt": [],
                "quality_gate": {
                    "status": "failed",
                    "failure_messages": ["missing visible sections"],
                    "unsupported_claim_count": 0,
                    "unresolved_conflict_count": 0,
                    "open_debt_count": 0
                },
                "warnings": []
            }))
            .unwrap(),
        );
        let scorecard = build_case_scorecard(&case, &result);

        assert!(!scorecard.visibility.visible_final_answer);
        assert!(!scorecard.visibility.visible_claim_log);
        assert!(!scorecard.visibility.visible_quality_gate);
        assert_eq!(scorecard.advisory_model_self_score, None);
    }

    #[test]
    fn structured_run_outputs_keep_per_case_failures_visible() {
        let case = sample_case("case", "case.md");
        let success_result = sample_result();
        let mut failed_result = sample_result();
        failed_result.case_id = "case-failed".to_string();
        failed_result.title = "Failed Case".to_string();
        failed_result.status = "failed".to_string();
        failed_result.quality_status = Some("failed".to_string());
        failed_result.final_output = None;
        failed_result.research_controller_artifacts_json = Some(
            serde_json::to_string(&json!({
                "version": 1,
                "source_cards": [],
                "claim_log": [],
                "conflict_map": [],
                "research_debt": [],
                "quality_gate": {
                    "status": "failed",
                    "failure_messages": ["missing evidence"],
                    "unsupported_claim_count": 1,
                    "unresolved_conflict_count": 0,
                    "open_debt_count": 1
                },
                "warnings": ["thin"]
            }))
            .unwrap(),
        );
        failed_result.research_source_diagnostics_json = Some(
            serde_json::to_string(&json!({
                "version": 1,
                "source_pack": {
                    "status": "empty",
                    "reason": "no live candidates",
                    "queries": [],
                    "discovered_source_count": 0,
                    "adopted_source_count": 0,
                    "skipped_candidates": []
                },
                "scrapes": [],
                "context_packing": null
            }))
            .unwrap(),
        );

        let success_execution = RunExecution {
            case: case.clone(),
            result: success_result.clone(),
            scorecard: build_case_scorecard(&case, &success_result),
            artifact_paths: CaseArtifactPaths::default(),
            replay_before: None,
        };
        let failed_execution = RunExecution {
            case: BenchmarkCase {
                case_id: "case-failed".to_string(),
                filename: "failed.md".to_string(),
                title: "Failed Case".to_string(),
                category: "category".to_string(),
                prompt: case.prompt.clone(),
                must_pass_checks: case.must_pass_checks.clone(),
                expected_failure_modes: case.expected_failure_modes.clone(),
            },
            result: failed_result.clone(),
            scorecard: build_case_scorecard(
                &BenchmarkCase {
                    case_id: "case-failed".to_string(),
                    filename: "failed.md".to_string(),
                    title: "Failed Case".to_string(),
                    category: "category".to_string(),
                    prompt: case.prompt.clone(),
                    must_pass_checks: case.must_pass_checks.clone(),
                    expected_failure_modes: case.expected_failure_modes.clone(),
                },
                &failed_result,
            ),
            artifact_paths: CaseArtifactPaths::default(),
            replay_before: None,
        };
        let args = Args {
            cases_dir: PathBuf::from(DEFAULT_CASES_DIR),
            runs_dir: None,
            label: "fixture-report".to_string(),
            mode: BenchModeArg::Fixture,
            replay_fixture_root: PathBuf::from(DEFAULT_REPLAY_FIXTURE_ROOT),
            data_dir: None,
            model_input: None,
            engine_name: None,
            model_name: None,
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            max_iterations: 2,
            cli_launch_mode: None,
            ai_task_timeout_secs: 3600,
            include_raw_debug_artifacts: false,
        };
        let report = build_structured_run_report(
            &args,
            ResearchBenchmarkMode::Fixture,
            "2026-05-15T00:00:00Z",
            "Fixture Controller",
            "fixture-research-bench",
            Path::new("docs/experiments/research-richness/runs/fixture-report"),
            &[success_execution, failed_execution],
        );

        assert_eq!(report.summary.case_count, 2);
        assert_eq!(report.summary.critical_failure_case_count, 1);
        assert_eq!(
            report.summary.critical_failure_case_ids,
            vec!["case-failed".to_string()]
        );
        assert_eq!(
            report.summary.source_pack_status_counts.get("empty"),
            Some(&1)
        );
        assert!(render_case_scores_csv(&report).contains("case-failed"));
        assert!(render_dimension_scores_ndjson(&report)
            .unwrap()
            .contains("\"case_id\":\"case-failed\""));
    }

    #[test]
    fn structured_run_outputs_include_replay_before_in_json_csv_and_ndjson() {
        let case = sample_case("case-replay", "case-replay.md");
        let result = sample_result();
        let execution = RunExecution {
            case: case.clone(),
            result: result.clone(),
            scorecard: build_case_scorecard(&case, &result),
            artifact_paths: CaseArtifactPaths::default(),
            replay_before: Some(ReplayBeforeState {
                status: "completed".to_string(),
                quality_status: "untrusted".to_string(),
                quality_last_failure: "missing visible source audit".to_string(),
                critical_failure_count: 1,
            }),
        };
        let args = Args {
            cases_dir: PathBuf::from(DEFAULT_CASES_DIR),
            runs_dir: None,
            label: "replay-report".to_string(),
            mode: BenchModeArg::Replay,
            replay_fixture_root: PathBuf::from(DEFAULT_REPLAY_FIXTURE_ROOT),
            data_dir: None,
            model_input: None,
            engine_name: None,
            model_name: None,
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            max_iterations: 2,
            cli_launch_mode: None,
            ai_task_timeout_secs: 3600,
            include_raw_debug_artifacts: false,
        };
        let report = build_structured_run_report(
            &args,
            ResearchBenchmarkMode::Replay,
            "2026-05-15T00:00:00Z",
            "Artifact Replay",
            "frozen-replay-fixtures",
            Path::new("docs/experiments/research-richness/runs/replay-report"),
            &[execution],
        );
        let json = serde_json::to_string(&report).unwrap();
        let csv = render_case_scores_csv(&report);
        let ndjson = render_dimension_scores_ndjson(&report).unwrap();

        assert!(json.contains("\"replay_before\""));
        assert!(json.contains("\"quality_last_failure\":\"missing visible source audit\""));
        assert!(csv.contains("replay_before_status"));
        assert!(csv.contains("untrusted"));
        assert!(ndjson.contains("\"replay_before\":{"));
        assert!(ndjson.contains("\"quality_last_failure\":\"missing visible source audit\""));
    }

    #[test]
    fn preflight_run_output_paths_rejects_existing_aggregate_files() {
        let dir = std::env::temp_dir().join(format!("research-bench-preflight-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let report_path = dir.join("collision.md");
        let json_path = dir.join("collision.json");
        fs::write(&json_path, "{}").unwrap();

        let err =
            preflight_run_output_paths(&dir.join("collision"), &report_path, &dir, "collision")
                .unwrap_err();

        assert!(err.to_string().contains("collision.json"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn read_bounded_text_file_rejects_oversized_replay_fixture_file() {
        let dir =
            std::env::temp_dir().join(format!("research-bench-bounded-read-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture.json");
        fs::write(&path, "0123456789").unwrap();
        let mut total_bytes = 0;

        let err = read_bounded_text_file(&path, &mut total_bytes, 4, 100).unwrap_err();

        assert!(err.to_string().contains("replay fixture file is too large"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_replay_fixture_bundles_rejects_excess_fixture_summary_count() {
        let dir =
            std::env::temp_dir().join(format!("research-bench-replay-count-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        for index in 0..=MAX_REPLAY_FIXTURE_BUNDLES {
            fs::write(
                dir.join(format!("fixture-{index}.json")),
                r#"{"case_id":"case","category":"category","status":"completed","quality_status":"passed","quality_last_failure":null,"model_input":"fixture"}"#,
            )
            .unwrap();
        }

        let err = load_replay_fixture_bundles(&dir, &[]).unwrap_err();

        assert!(err
            .to_string()
            .contains("replay fixture root has too many case summaries"));
        let _ = fs::remove_dir_all(dir);
    }
}
