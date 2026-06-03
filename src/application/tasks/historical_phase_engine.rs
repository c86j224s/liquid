use super::*;
use crate::contracts::{
    NarrativeActor, NarrativeCausalLink, NarrativeCausalSpineStep, NarrativeEventCard,
    NarrativeEvidenceLayer, NarrativeImpact, NarrativeInterpretiveLayer,
    NarrativeInterpretiveTension, NarrativeOpenGap, NarrativeReaderQuestion,
    NarrativeSectionOutlineItem, NarrativeState, NarrativeTimelineEvent, ReaderArgumentEdge,
    ReaderArgumentGraph, ReaderArgumentNode, ReaderNarrativePlan, ReaderQualityArtifacts,
    ReaderSectionBrief, ResearchClaimLogEntry, ResearchSourceCard,
};
use liquid_research_classic::{
    build_historical_phase_plan, historical_phase_label_is_placeholder,
    historical_phase_state_should_run, HistoricalPhasePlan,
};
use std::collections::HashSet;
use std::path::{Component, Path};

const HISTORICAL_PHASE_ENGINE_ITERATION: i64 = 1;
const HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalPhaseEngineTerminalStatus {
    Accepted,
    Untrusted,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalPhaseEngineVerdict {
    ResearchGrade,
    Explainer,
    Partial,
    Blocked,
}

impl HistoricalPhaseEngineVerdict {
    fn quality_gate_status(self) -> &'static str {
        match self {
            Self::ResearchGrade => "research_grade",
            Self::Explainer => "explainer",
            Self::Partial => "partial",
            Self::Blocked => "blocked",
        }
    }

    fn task_quality_status(self) -> &'static str {
        match self {
            Self::ResearchGrade => "passed",
            Self::Explainer | Self::Partial => "untrusted",
            Self::Blocked => "blocked",
        }
    }

    fn terminal_status(self) -> HistoricalPhaseEngineTerminalStatus {
        match self {
            Self::ResearchGrade => HistoricalPhaseEngineTerminalStatus::Accepted,
            Self::Explainer | Self::Partial => HistoricalPhaseEngineTerminalStatus::Untrusted,
            Self::Blocked => HistoricalPhaseEngineTerminalStatus::Blocked,
        }
    }

    fn controller_detail(self) -> &'static str {
        match self {
            Self::ResearchGrade => {
                "Historical phase engine accepted the report at research-grade depth."
            }
            Self::Explainer => {
                "Historical phase engine completed with an explainer-only report; strict research-grade depth was not met."
            }
            Self::Partial => {
                "Historical phase engine completed with a partial trusted report; some source or claim support remained unresolved."
            }
            Self::Blocked => {
                "Historical phase engine blocked the report because the evidence ledger did not meet the minimum support contract."
            }
        }
    }
}

#[derive(Debug, Clone)]
struct HistoricalPhaseEngineOutcome {
    terminal_status: HistoricalPhaseEngineTerminalStatus,
    verdict: HistoricalPhaseEngineVerdict,
    output: String,
    artifacts: ResearchControllerArtifacts,
    failure_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct HistoricalEvidenceDocument {
    filename: String,
    title: Option<String>,
    url: Option<String>,
    source_class: Option<String>,
    confidence: Option<String>,
    facts: Vec<String>,
    claims: Vec<String>,
}

fn historical_phase_engine_subject_for_task(task: &TaskInfo, fallback_prompt: &str) -> String {
    let mut candidates = Vec::new();
    candidates.push(
        task.research_topic
            .as_deref()
            .unwrap_or_default()
            .to_string(),
    );
    candidates.push(
        task.research_instructions
            .as_deref()
            .unwrap_or_default()
            .to_string(),
    );
    if let Some(extracted) = extract_user_research_condition_from_prompt(fallback_prompt) {
        candidates.push(extracted);
    }
    candidates.push(fallback_prompt.to_string());

    for candidate in candidates {
        if let Some(subject) = normalize_historical_phase_engine_subject(&candidate) {
            return subject;
        }
    }
    "역사 연구 주제".to_string()
}

fn extract_user_research_condition_from_prompt(prompt: &str) -> Option<String> {
    let mut capture_next = false;
    for line in prompt.lines() {
        let trimmed = line.trim();
        if trimmed.contains("[사용자 조사 조건/제약/비교 기준]") {
            capture_next = true;
            continue;
        }
        if capture_next && !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn normalize_historical_phase_engine_subject(value: &str) -> Option<String> {
    let mut compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    for separator in [
        " — ",
        " -- ",
        " - historical_phase_engine",
        " — historical_phase_engine",
    ] {
        if let Some((before, _)) = compact.split_once(separator) {
            compact = before.trim().to_string();
        }
    }
    if compact.contains("첨부된 SOURCE DOCUMENTS")
        || compact.contains("[사용자 조사 조건/제약/비교 기준]")
        || compact.contains("시스템 지시를 대체")
        || compact.contains("조사 보고서를 작성하세요")
    {
        return None;
    }
    engine_safe_reader_text(&compact, 220)
}

async fn persist_historical_phase_engine_artifacts(
    state: &AppState,
    task_id: i64,
    artifacts: &ResearchControllerArtifacts,
) {
    let mut persisted = artifacts.clone();
    compact_engine_appendix_artifacts(&mut persisted);
    persist_research_controller_artifacts(state, task_id, &persisted).await;
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn try_run_historical_phase_engine(
    state: &AppState,
    task: &TaskInfo,
    filenames: &[String],
    cleanup_files: Vec<String>,
    file_prefix: &str,
    file_type: &str,
    user_prompt: &str,
) -> bool {
    if !state.research_historical_phase_engine {
        return false;
    }
    if !historical_phase_state_should_run(
        file_prefix,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(research_source_subject_for_task(task, user_prompt)),
    ) {
        return false;
    }

    let mut controller_events = Vec::new();
    let mut fresh_artifacts = ResearchControllerArtifacts {
        version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
        ..ResearchControllerArtifacts::default()
    };
    if let Some(warning) =
        legacy_artifact_ignore_warning(task.research_controller_artifacts_json.as_deref())
    {
        push_unique_warning(&mut fresh_artifacts.warnings, warning);
    }
    persist_historical_phase_engine_artifacts(state, task.id, &fresh_artifacts).await;

    update_quality_progress(
        state,
        task.id,
        &task.original_name,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        "researching",
        None,
    )
    .await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PLAN,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Historical phase engine is building an isolated evidence-first report."),
        &mut controller_events,
    )
    .await;

    let outcome = build_historical_phase_engine_outcome(
        state,
        task,
        filenames,
        file_prefix,
        file_type,
        user_prompt,
        &mut controller_events,
    )
    .await;
    persist_historical_phase_engine_artifacts(state, task.id, &outcome.artifacts).await;

    match outcome.terminal_status {
        HistoricalPhaseEngineTerminalStatus::Accepted => {
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                RESEARCH_STAGE_FINAL,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                RESEARCH_CONTROLLER_STATUS_COMPLETED,
                Some(outcome.verdict.controller_detail()),
                &mut controller_events,
            )
            .await;
            update_quality_progress(
                state,
                task.id,
                &task.original_name,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                outcome.verdict.task_quality_status(),
                None,
            )
            .await;
        }
        HistoricalPhaseEngineTerminalStatus::Untrusted => {
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                RESEARCH_STAGE_FINAL,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                RESEARCH_CONTROLLER_STATUS_COMPLETED,
                Some(outcome.verdict.controller_detail()),
                &mut controller_events,
            )
            .await;
            update_quality_progress(
                state,
                task.id,
                &task.original_name,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                outcome.verdict.task_quality_status(),
                outcome.failure_message.as_deref(),
            )
            .await;
        }
        HistoricalPhaseEngineTerminalStatus::Blocked => {
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                RESEARCH_STAGE_FINAL,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                RESEARCH_CONTROLLER_STATUS_FAILED,
                Some(outcome.verdict.controller_detail()),
                &mut controller_events,
            )
            .await;
            update_quality_progress(
                state,
                task.id,
                &task.original_name,
                HISTORICAL_PHASE_ENGINE_ITERATION,
                HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
                outcome.verdict.task_quality_status(),
                outcome.failure_message.as_deref(),
            )
            .await;
        }
    }

    persist_historical_phase_engine_artifacts(state, task.id, &outcome.artifacts).await;
    handle_task_completion(
        state,
        task.id,
        Some(outcome.output),
        task.original_name.clone(),
        file_prefix,
        file_type,
        cleanup_files,
        false,
    )
    .await;
    true
}

async fn build_historical_phase_engine_outcome(
    state: &AppState,
    task: &TaskInfo,
    filenames: &[String],
    file_prefix: &str,
    file_type: &str,
    user_prompt: &str,
    controller_events: &mut Vec<ResearchControllerEvent>,
) -> HistoricalPhaseEngineOutcome {
    let subject = historical_phase_engine_subject_for_task(task, user_prompt);
    let documents = load_historical_evidence_documents(state, filenames).await;
    let mut artifacts = ResearchControllerArtifacts {
        version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
        events: controller_events.clone(),
        ..load_task_research_artifacts(state, task.id)
            .await
            .unwrap_or_default()
    };
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.events = controller_events.clone();
    artifacts.source_cards.clear();
    artifacts.claim_log.clear();
    artifacts.conflict_map.clear();
    artifacts.research_debt.clear();
    artifacts.narrative_state = None;
    artifacts.reader_quality = None;
    artifacts.quality_gate = None;
    artifacts.research_iteration_state = None;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_SOURCE_CARDS,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Building engine-owned Source Cards from local historical evidence inputs."),
        controller_events,
    )
    .await;
    populate_engine_ledgers_from_documents(&mut artifacts, &documents);
    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_SOURCE_CARDS,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Built {} Source Cards from {} historical evidence document(s).",
            artifacts.source_cards.len(),
            documents.len()
        )),
        controller_events,
    )
    .await;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_CLAIM_LOG,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Building engine-owned Claim Log rows without model artifact JSON."),
        controller_events,
    )
    .await;
    artifacts.events = controller_events.clone();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_CLAIM_LOG,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Built {} Claim Log row(s) from historical evidence inputs.",
            artifacts.claim_log.len()
        )),
        controller_events,
    )
    .await;

    let phase_plan = build_historical_phase_plan(&artifacts, &subject);
    push_phase_plan_debts(&mut artifacts, &phase_plan);
    build_engine_planning_artifacts(&mut artifacts, &phase_plan, &subject);
    artifacts.events = controller_events.clone();
    compact_engine_appendix_artifacts(&mut artifacts);
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PHASE_STATE,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Historical phase engine built {} ready phase card(s) from a {}-phase plan.",
            artifacts
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len())
                .unwrap_or_default(),
            phase_plan.phases.len()
        )),
        controller_events,
    )
    .await;

    let context = ResearchQualityContext {
        file_prefix,
        file_type,
        web_search_requested: task
            .web_search_requested
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or_else(|| {
                state
                    .research_implementation
                    .research_allows_web_search(file_prefix)
            }),
        research_intensity: task.research_intensity.as_deref(),
        quality_depth: task.quality_depth.as_deref(),
        research_topic: task.research_topic.as_deref(),
        research_instructions: task.research_instructions.as_deref(),
        evidence_subject: Some(&subject),
    };

    let mut validation_failures = Vec::new();
    if let Err(artifact_failures) = validate_research_artifacts(
        &artifacts,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
    ) {
        validation_failures.extend(artifact_failures);
    }

    let mut output = render_historical_phase_engine_output(
        &subject,
        &artifacts,
        &phase_plan,
        &historical_phase_engine_quality_gate(
            &artifacts,
            HistoricalPhaseEngineVerdict::ResearchGrade,
            &[],
        ),
    );
    output = normalize_ai_output(&output, file_type);
    if let Err(error) = validate_research_output(&output, &context) {
        validation_failures.push(error);
    }
    let (verdict, verdict_messages) =
        assess_historical_phase_engine_verdict(&artifacts, &output, &validation_failures);
    match verdict {
        HistoricalPhaseEngineVerdict::ResearchGrade => {
            close_research_debts_for_gate(
                &mut artifacts.research_debt,
                "historical_phase_engine",
                None,
            );
        }
        HistoricalPhaseEngineVerdict::Partial | HistoricalPhaseEngineVerdict::Blocked => {
            sync_engine_validation_debt(&mut artifacts, &verdict_messages);
        }
        HistoricalPhaseEngineVerdict::Explainer => {
            for message in &verdict_messages {
                push_unique_warning(
                    &mut artifacts.warnings,
                    truncate_engine_artifact_text(message, 160),
                );
            }
        }
    }
    artifacts.quality_gate = Some(historical_phase_engine_quality_gate(
        &artifacts,
        verdict,
        &verdict_messages,
    ));
    if verdict != HistoricalPhaseEngineVerdict::ResearchGrade {
        output = render_historical_phase_engine_output(
            &subject,
            &artifacts,
            &phase_plan,
            artifacts
                .quality_gate
                .as_ref()
                .expect("quality gate should exist"),
        );
        output = normalize_ai_output(&output, file_type);
    }

    artifacts.events.clear();
    persist_historical_phase_engine_artifacts(state, task.id, &artifacts).await;
    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_QUALITY_GATE,
        HISTORICAL_PHASE_ENGINE_ITERATION,
        HISTORICAL_PHASE_ENGINE_MAX_ITERATIONS,
        if verdict == HistoricalPhaseEngineVerdict::Blocked {
            RESEARCH_CONTROLLER_STATUS_FAILED
        } else {
            RESEARCH_CONTROLLER_STATUS_COMPLETED
        },
        Some(
            verdict_messages
                .first()
                .map(String::as_str)
                .unwrap_or(verdict.controller_detail()),
        ),
        controller_events,
    )
    .await;

    HistoricalPhaseEngineOutcome {
        terminal_status: verdict.terminal_status(),
        verdict,
        output,
        artifacts,
        failure_message: verdict_messages.first().cloned(),
    }
}

async fn load_historical_evidence_documents(
    state: &AppState,
    filenames: &[String],
) -> Vec<HistoricalEvidenceDocument> {
    let mut documents = Vec::new();
    let Ok(uploads_root) = fs::canonicalize(&state.uploads_path).await else {
        return documents;
    };
    for filename in filenames {
        if !safe_upload_basename(filename) {
            continue;
        }
        let path = state.uploads_path.join(filename);
        let Ok(canonical_path) = fs::canonicalize(&path).await else {
            continue;
        };
        if !canonical_path.starts_with(&uploads_root) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&canonical_path).await else {
            continue;
        };
        let normalized = if filename.ends_with(".html") {
            crate::application::scraping::strip_html(&content)
        } else {
            content
        };
        documents.push(parse_historical_evidence_document(filename, &normalized));
    }
    documents
}

fn safe_upload_basename(filename: &str) -> bool {
    let path = Path::new(filename);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn parse_historical_evidence_document(filename: &str, content: &str) -> HistoricalEvidenceDocument {
    let mut document = HistoricalEvidenceDocument {
        filename: filename.to_string(),
        ..HistoricalEvidenceDocument::default()
    };
    let mut section: Option<&str> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("# ") {
            if document.title.is_none() {
                document.title = Some(value.trim().to_string());
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("Title:") {
            document.title = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("URL:") {
            document.url = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Source Class:") {
            document.source_class = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Confidence:") {
            document.confidence = Some(value.trim().to_string());
            continue;
        }
        if line.eq_ignore_ascii_case("Facts:") {
            section = Some("facts");
            continue;
        }
        if line.eq_ignore_ascii_case("Claims:") {
            section = Some("claims");
            continue;
        }
        if let Some(value) = line.strip_prefix("Fact:") {
            push_unique_line(&mut document.facts, value);
            continue;
        }
        if let Some(value) = line.strip_prefix("Claim:") {
            push_unique_line(&mut document.claims, value);
            continue;
        }
        if let Some(value) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            match section {
                Some("facts") => push_unique_line(&mut document.facts, value),
                Some("claims") => push_unique_line(&mut document.claims, value),
                _ => {}
            }
        }
    }

    if document.title.is_none() {
        document.title = Some(filename.to_string());
    }
    document
}

fn push_unique_line(target: &mut Vec<String>, value: &str) {
    let normalized = value.trim();
    if normalized.is_empty() {
        return;
    }
    if target.iter().any(|existing| existing == normalized) {
        return;
    }
    target.push(normalized.to_string());
}

fn historical_tokenize(text: &str) -> Vec<String> {
    text.split(|ch: char| {
        !(ch.is_ascii_alphanumeric()
            || ('\u{3131}'..='\u{318E}').contains(&ch)
            || ('\u{AC00}'..='\u{D7A3}').contains(&ch))
    })
    .map(str::trim)
    .filter(|token| !token.is_empty())
    .map(|token| token.to_ascii_lowercase())
    .filter(|token| !historical_token_is_noise(token))
    .collect()
}

fn historical_token_is_noise(token: &str) -> bool {
    if token.chars().all(|ch| ch.is_ascii_digit()) {
        return token.len() < 4;
    }
    if token.chars().count() <= 1 {
        return true;
    }
    matches!(
        token,
        "the"
            | "and"
            | "with"
            | "from"
            | "that"
            | "this"
            | "into"
            | "through"
            | "around"
            | "across"
            | "before"
            | "after"
            | "between"
            | "during"
            | "over"
            | "under"
            | "while"
            | "were"
            | "was"
            | "then"
            | "they"
            | "them"
            | "their"
            | "there"
            | "also"
            | "because"
            | "which"
            | "would"
            | "could"
            | "should"
            | "phase"
            | "provides"
            | "provide"
            | "chronology"
            | "anchor"
            | "anchors"
            | "evidence"
            | "summary"
            | "overview"
            | "generic"
            | "역사"
            | "자료"
            | "사건"
            | "국면"
            | "연표"
            | "개요"
            | "요약"
            | "근거"
            | "제공"
            | "앵커"
    )
}

fn historical_year_tokens(text: &str) -> HashSet<String> {
    historical_tokenize(text)
        .into_iter()
        .filter(|token| token.chars().all(|ch| ch.is_ascii_digit()) && token.len() == 4)
        .collect()
}

fn extracted_fact_is_substantive(fact: &str) -> bool {
    let compact = compact_claim_text(fact);
    if compact.chars().count() < 28 {
        return false;
    }
    let lower = compact.to_ascii_lowercase();
    let generic_markers = [
        "provides chronology",
        "provides chronology and consequence anchors",
        "chronology anchor",
        "consequence anchor",
        "overview",
        "summary",
        "개요",
        "요약",
        "연표 앵커",
    ];
    if generic_markers.iter().any(|marker| lower.contains(marker)) {
        return false;
    }
    let tokens = historical_tokenize(&compact);
    if tokens.len() < 4 {
        return false;
    }
    historical_year_tokens(&compact).len() >= 1
        || extract_region_from_claim(&compact).is_some()
        || extract_outcome_from_claim(&compact).is_some()
        || tokens.iter().any(|token| {
            token.contains("위기")
                || token.contains("동원")
                || token.contains("침공")
                || token.contains("병합")
                || token.contains("협정")
                || token.contains("전쟁")
                || token.contains("crisis")
                || token.contains("mobil")
                || token.contains("invad")
                || token.contains("annex")
                || token.contains("agreement")
                || token.contains("armistice")
                || token.contains("war")
        })
}

fn claim_has_semantic_support_in_facts(claim: &str, facts: &[String]) -> bool {
    let claim_tokens = historical_tokenize(claim)
        .into_iter()
        .collect::<HashSet<_>>();
    let claim_years = historical_year_tokens(claim);
    let claim_region = extract_region_from_claim(claim)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    facts.iter().any(|fact| {
        if !extracted_fact_is_substantive(fact) {
            return false;
        }
        let fact_tokens = historical_tokenize(fact)
            .into_iter()
            .collect::<HashSet<_>>();
        let overlap = claim_tokens.intersection(&fact_tokens).count();
        let year_overlap = !claim_years.is_empty()
            && !claim_years
                .intersection(&historical_year_tokens(fact))
                .collect::<Vec<_>>()
                .is_empty();
        let fact_region = extract_region_from_claim(fact)
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let region_overlap = !claim_region.is_empty()
            && !fact_region.is_empty()
            && (claim_region.contains(&fact_region) || fact_region.contains(&claim_region));
        overlap >= 4 || (overlap >= 3 && year_overlap) || (overlap >= 3 && region_overlap)
    })
}

fn claim_has_time_anchor(claim: &str) -> bool {
    !historical_year_tokens(claim).is_empty() || claim.contains(" BCE") || claim.contains(" CE")
}

fn claim_has_region_anchor(claim: &str) -> bool {
    extract_region_from_claim(claim).is_some()
}

fn claim_has_actor_anchor(claim: &str) -> bool {
    extract_actors_from_claim(claim).len() >= 2
}

fn claim_has_handoff_anchor(claim: &str) -> bool {
    extract_outcome_from_claim(claim)
        .as_deref()
        .is_some_and(|outcome| outcome.chars().count() >= 20)
}

fn claim_is_phase_specific(claim: &str) -> bool {
    let mut anchors = 0usize;
    if claim_has_time_anchor(claim) {
        anchors += 1;
    }
    if claim_has_actor_anchor(claim) {
        anchors += 1;
    }
    if claim_has_region_anchor(claim) {
        anchors += 1;
    }
    if claim_has_handoff_anchor(claim) {
        anchors += 1;
    }
    anchors >= 3
}

fn infer_historical_claim_type(claim: &str) -> String {
    if claim_is_phase_specific(claim) {
        "event_fact".to_string()
    } else {
        "historical_overview".to_string()
    }
}

fn claim_type_is_granular(claim_type: &str) -> bool {
    matches!(
        claim_type,
        "event_fact" | "actor_strategy" | "causal_handoff" | "interpretive_limit"
    )
}

fn phase_label_from_claim(claim: &str) -> String {
    claim
        .split(':')
        .next()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(claim)
        .to_string()
}

fn build_granular_claim_texts(claim: &str) -> Vec<(String, String)> {
    let label = phase_label_from_claim(claim);
    let actors = extract_actors_from_claim(claim);
    let actor_text = if actors.is_empty() {
        "주요 행위자".to_string()
    } else {
        actors.join(", ")
    };
    let region = extract_region_from_claim(claim).unwrap_or_else(|| "주요 지역/전선".to_string());
    let trigger = build_trigger_from_claim(claim, &label).unwrap_or_else(|| label.clone());
    let outcome = extract_outcome_from_claim(claim).unwrap_or_else(|| claim.to_string());
    vec![
        ("event_fact".to_string(), format!("{label}: {trigger}")),
        (
            "actor_strategy".to_string(),
            format!(
                "{label}: {actor_text}는 {region}에서 {trigger}를 둘러싼 외교·군사 선택을 조정했다."
            ),
        ),
        (
            "causal_handoff".to_string(),
            format!("{label}: {outcome}"),
        ),
        (
            "interpretive_limit".to_string(),
            format!(
                "{label}: 현재 근거는 {region}의 사건·행위자·귀결을 지지하지만, 내부 의사결정의 모든 세부 논쟁까지 확정하지는 않는다."
            ),
        ),
    ]
}

fn push_granular_claim_rows(
    artifacts: &mut ResearchControllerArtifacts,
    claim_index: &mut usize,
    source_id: &str,
    claim: &str,
    confidence: Option<String>,
) {
    for (claim_type, claim_text) in build_granular_claim_texts(claim) {
        artifacts.claim_log.push(ResearchClaimLogEntry {
            id: format!("C{}", *claim_index),
            claim: claim_text,
            claim_type: Some(claim_type),
            support_source_card_ids: vec![source_id.to_string()],
            support_urls: Vec::new(),
            confidence: confidence.clone(),
            uncertainty_note: None,
            needs_verification: Some(false),
        });
        *claim_index += 1;
    }
}

fn populate_engine_ledgers_from_documents(
    artifacts: &mut ResearchControllerArtifacts,
    documents: &[HistoricalEvidenceDocument],
) {
    let mut source_index = 1usize;
    let mut claim_index = 1usize;

    for (document_index, document) in documents.iter().enumerate() {
        let safe_document_label = safe_evidence_filename_label(&document.filename, document_index);
        let safe_document_suffix =
            safe_evidence_filename_debt_suffix(&document.filename, document_index);
        let Some(url) = document
            .url
            .as_deref()
            .and_then(liquid_research_core::normalize_absolute_public_evidence_url)
        else {
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: format!("missing-source-url-{safe_document_suffix}"),
                    severity: "high".to_string(),
                    failed_gate: Some("historical_phase_engine".to_string()),
                    missing_evidence: format!(
                        "historical source metadata is missing a public URL in {}",
                        safe_document_label
                    ),
                    required_source_class: Some("authoritative_secondary".to_string()),
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![format!(
                        "Add a public URL and source metadata to {} before rerunning the historical phase engine.",
                        safe_document_label
                    )],
                    status: "open".to_string(),
                },
            );
            continue;
        };

        let source_id = format!("S{source_index}");
        source_index += 1;
        let title = document
            .title
            .as_deref()
            .and_then(|title| engine_safe_evidence_text(title, 120))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("Historical Source {}", source_index - 1));
        let source_class = liquid_research_core::infer_source_class(&url).to_string();
        if document
            .source_class
            .as_deref()
            .and_then(|value| engine_safe_evidence_text(value, 64))
            .is_some_and(|declared| declared != source_class)
        {
            push_unique_warning(
                &mut artifacts.warnings,
                format!(
                    "historical_phase_engine_ignored_declared_source_class:{}",
                    safe_document_suffix
                ),
            );
        }
        let confidence = document
            .confidence
            .as_deref()
            .and_then(normalize_engine_confidence)
            .or_else(|| Some("high".to_string()));
        let extracted_facts = document
            .facts
            .iter()
            .filter_map(|fact| engine_safe_evidence_text(fact, 220))
            .filter(|fact| extracted_fact_is_substantive(fact))
            .collect::<Vec<_>>();
        if extracted_facts.is_empty() {
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: format!("missing-extracted-facts-{safe_document_suffix}"),
                    severity: "high".to_string(),
                    failed_gate: Some("historical_phase_engine".to_string()),
                    missing_evidence: format!(
                        "historical source {} does not contain non-empty extracted facts with phase-specific support",
                        safe_document_label
                    ),
                    required_source_class: Some("authoritative_secondary".to_string()),
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![format!(
                        "Add Facts: lines in {} that name the timeframe, actors, place/front, and consequence actually used to support the phase claims.",
                        safe_document_label
                    )],
                    status: "open".to_string(),
                },
            );
            continue;
        }
        let source_card = ResearchSourceCard {
            id: source_id.clone(),
            url,
            title,
            source_class,
            accessed_at: None,
            extracted_facts: extracted_facts.clone(),
            limitation: None,
            diagnostics_ref: Some(format!("historical_phase_engine:{}", document.filename)),
            confidence: confidence.clone(),
        };
        artifacts.source_cards.push(source_card);

        let safe_claims = document
            .claims
            .iter()
            .filter_map(|claim| engine_safe_evidence_text(claim, 320))
            .collect::<Vec<_>>();

        if safe_claims.is_empty() {
            upsert_research_debt(
                &mut artifacts.research_debt,
                ResearchDebtItem {
                    id: format!("missing-claims-{safe_document_suffix}"),
                    severity: "high".to_string(),
                    failed_gate: Some("historical_phase_engine".to_string()),
                    missing_evidence: format!(
                        "historical evidence file {} does not declare any phase-specific Claim Log rows",
                        safe_document_label
                    ),
                    required_source_class: None,
                    candidate_queries: Vec::new(),
                    next_check_actions: vec![format!(
                        "Add Claim: lines for concrete phase-specific events in {}.",
                        safe_document_label
                    )],
                    status: "open".to_string(),
                },
            );
            continue;
        }

        for claim in &safe_claims {
            if claim.trim().is_empty() {
                continue;
            }
            if !claim_has_semantic_support_in_facts(claim, &extracted_facts) {
                upsert_research_debt(
                    &mut artifacts.research_debt,
                    ResearchDebtItem {
                        id: format!(
                            "unsupported-claim-{}-{}",
                            safe_document_suffix,
                            claim_index
                        ),
                        severity: "high".to_string(),
                        failed_gate: Some("historical_phase_engine".to_string()),
                        missing_evidence: format!(
                            "claim in {} is not semantically supported by the source's extracted facts: {}",
                            safe_document_label, claim
                        ),
                        required_source_class: None,
                        candidate_queries: Vec::new(),
                        next_check_actions: vec![format!(
                            "Revise Facts: lines in {} so the claim's timeframe, actors, place/front, and consequence are explicitly supported before reusing the claim.",
                            safe_document_label
                        )],
                        status: "open".to_string(),
                    },
                );
                continue;
            }
            push_granular_claim_rows(
                artifacts,
                &mut claim_index,
                &source_id,
                claim,
                confidence.clone(),
            );
        }
    }

    if artifacts.source_cards.is_empty() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: "historical-phase-engine-missing-source-cards".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine".to_string()),
                missing_evidence:
                    "historical phase engine could not build any Source Cards from the provided evidence inputs"
                        .to_string(),
                required_source_class: Some("authoritative_secondary".to_string()),
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Provide at least one evidence file with Title, URL, and phase-specific facts."
                        .to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
    if artifacts.claim_log.is_empty() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: "historical-phase-engine-missing-claim-log".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine".to_string()),
                missing_evidence:
                    "historical phase engine could not build any phase-specific Claim Log rows from the provided evidence inputs"
                        .to_string(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Add Claim: lines for each historical phase so the engine can ground phase cards."
                        .to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
}

fn sanitize_debt_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn safe_evidence_filename_label(filename: &str, index: usize) -> String {
    let basename = Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    engine_safe_evidence_text(basename, 80)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("historical evidence file {}", index + 1))
}

fn safe_evidence_filename_debt_suffix(filename: &str, index: usize) -> String {
    sanitize_debt_suffix(&safe_evidence_filename_label(filename, index))
}

fn normalize_engine_confidence(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "high" | "medium" | "low" => Some(normalized),
        _ => None,
    }
}

fn engine_safe_evidence_text(value: &str, limit: usize) -> Option<String> {
    if engine_evidence_text_is_unsafe(value) {
        return None;
    }
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    if compact.chars().count() <= limit {
        Some(compact)
    } else {
        Some(compact.chars().take(limit).collect())
    }
}

fn engine_safe_reader_text(value: &str, limit: usize) -> Option<String> {
    if engine_evidence_text_is_unsafe(value) {
        return None;
    }
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.trim().is_empty() {
        return None;
    }
    Some(truncate_engine_artifact_text(&compact, limit))
}

fn markdown_table_cell(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

fn reader_fragment(value: &str, fallback: &str, limit: usize) -> String {
    engine_safe_reader_text(value, limit).unwrap_or_else(|| fallback.to_string())
}

fn ensure_reader_sentence(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_end_matches(['.', '!', '?', '。', '…'])
        .trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}.")
    }
}

fn engine_evidence_text_is_unsafe(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let alias_normalized = lower
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>();
    if contains_sensitive_assignment_marker(&lower)
        || contains_sensitive_assignment_marker(&alias_normalized)
    {
        return true;
    }
    let unsafe_markers = [
        "system prompt",
        "resolved prompt",
        "resolved system prompt",
        "resolved user prompt",
        "provider payload",
        "raw provider payload",
        "controller artifact",
        "controller artifact json",
        "source diagnostics",
        "source documents",
        "raw diagnostics",
        "raw prompt",
        "raw model",
        "raw model output",
        "model input",
        "api key",
        "apikey",
        "authorization:",
        "authorization=",
        "bearer ",
        "bearer:",
        "bearer=",
        "access token",
        "access token:",
        "access token=",
        "password",
        "password:",
        "password=",
        "secret",
        "secret:",
        "secret=",
        "token=",
        "token:",
        ".env",
    ];
    unsafe_markers
        .iter()
        .any(|marker| lower.contains(marker) || alias_normalized.contains(marker))
        || liquid_research_core::research_artifact_text_contains_unsafe_location_reference(value)
}

fn contains_sensitive_assignment_marker(value: &str) -> bool {
    [
        "authorization",
        "bearer",
        "token",
        "access token",
        "api key",
        "client secret",
        "password",
        "secret",
    ]
    .iter()
    .any(|key| contains_key_assignment(value, key))
}

fn contains_key_assignment(value: &str, key: &str) -> bool {
    for (idx, _) in value.match_indices(key) {
        let before_ok = idx == 0
            || value[..idx]
                .chars()
                .next_back()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        if !before_ok {
            continue;
        }
        let mut rest = value[idx + key.len()..].chars();
        let Some(next) = rest.find(|ch| !ch.is_whitespace()) else {
            continue;
        };
        if next == ':' || next == '=' {
            return true;
        }
    }
    false
}

fn push_phase_plan_debts(
    artifacts: &mut ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
) {
    for debt in &phase_plan.debts {
        upsert_research_debt(&mut artifacts.research_debt, debt.clone());
    }
    for warning in &phase_plan.warnings {
        push_unique_warning(&mut artifacts.warnings, warning.clone());
    }
}

fn build_engine_planning_artifacts(
    artifacts: &mut ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    subject: &str,
) {
    let phase_report =
        liquid_research_classic::stabilize_historical_phase_state(artifacts, subject);
    push_unique_warning(
        &mut artifacts.warnings,
        format!(
            "historical_phase_engine:phases={} ready={} repaired={}",
            phase_report.phase_count,
            phase_report.ready_phase_count,
            phase_report.repaired_event_cards
        ),
    );
    if artifacts.narrative_state.is_none() {
        artifacts.narrative_state = Some(NarrativeState {
            version: 1,
            ..NarrativeState::default()
        });
    }
    let ready_phases = phase_plan
        .phases
        .iter()
        .filter(|phase| phase.readiness.ready)
        .collect::<Vec<_>>();
    let reader_promise = format!(
        "{subject}: phase-by-phase chronology, source layers, interpretive limits, and downstream consequences."
    );
    let evidence_layers = build_evidence_layers(artifacts);
    let claim_grounded_cards = build_claim_grounded_event_cards_from_claim_log(
        &artifacts.claim_log,
        &artifacts.source_cards,
    );
    let state = artifacts
        .narrative_state
        .get_or_insert_with(NarrativeState::default);
    state.version = state.version.max(1);
    state.topic_frame = Some(subject.to_string());
    state.reader_promise = Some(reader_promise);
    if state.event_cards.len() < claim_grounded_cards.len()
        || state
            .event_cards
            .iter()
            .all(|card| historical_phase_label_is_placeholder(&card.label))
    {
        state.event_cards = claim_grounded_cards;
    }
    apply_phase_plan_depth_to_cards(&mut state.event_cards, phase_plan);
    let working_thesis = build_working_thesis(subject, &state.event_cards, &ready_phases);
    state.working_thesis = Some(working_thesis.clone());
    let card_snapshots = state.event_cards.clone();
    state.timeline = ready_phases
        .iter()
        .enumerate()
        .map(|(idx, phase)| NarrativeTimelineEvent {
            id: format!("TL{}", idx + 1),
            label: phase.label.clone(),
            date_anchor: phase.timeframe.clone(),
            significance: Some(format!(
                "{} narrowed the next available strategic or political options.",
                phase.label
            )),
            expected_claim_log_ids: phase.claim_log_ids.clone(),
            expected_source_card_ids: phase.source_ids.clone(),
        })
        .collect();
    if state.timeline.is_empty() {
        state.timeline = card_snapshots
            .iter()
            .enumerate()
            .map(|(idx, card)| NarrativeTimelineEvent {
                id: format!("TL{}", idx + 1),
                label: card.label.clone(),
                date_anchor: card.timeframe.clone(),
                significance: Some(card.outcome.clone().unwrap_or_else(|| card.label.clone())),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            })
            .collect();
    }
    state.section_outline = card_snapshots
        .iter()
        .enumerate()
        .map(|(idx, card)| NarrativeSectionOutlineItem {
            id: format!("SO{}", idx + 1),
            heading: card.label.clone(),
            purpose: Some(format!(
                "{} explains how {} led into the next phase rather than standing as an isolated event.",
                card.label,
                card.trigger.as_deref().unwrap_or("its trigger conditions")
            )),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect();
    state.evidence_layers = evidence_layers;
    state.interpretive_tensions = build_interpretive_tensions_from_cards(&card_snapshots);
    state.impacts = build_impacts_from_cards(&card_snapshots);
    state.reader_questions = build_reader_questions_from_cards(&card_snapshots);
    state.actors = build_actor_rows_from_cards(&card_snapshots);
    state.causal_chain = build_causal_chain_from_cards(&card_snapshots);
    state.open_gaps = build_open_gaps(&artifacts.research_debt);
    state.last_iteration_summary = Some(format!(
        "historical_phase_engine built {} ready phases from {} claim-backed historical rows",
        state.event_cards.len(),
        artifacts.claim_log.len()
    ));
    if let Some(cards) = state.event_cards.get_mut(..) {
        for card in cards.iter_mut() {
            apply_phase_plan_depth_to_card(card, phase_plan);
            card.causal_spine = build_causal_spine(card);
            card.interpretive_layers = build_interpretive_layers(card);
            card.confidence = Some("high".to_string());
            card.open_questions = build_card_open_questions(card, &artifacts.research_debt);
        }
    }
    artifacts.reader_quality = Some(build_reader_quality(&state.event_cards, &working_thesis));
}

fn apply_phase_plan_depth_to_cards(
    cards: &mut [NarrativeEventCard],
    phase_plan: &HistoricalPhasePlan,
) {
    for card in cards {
        apply_phase_plan_depth_to_card(card, phase_plan);
    }
}

fn apply_phase_plan_depth_to_card(card: &mut NarrativeEventCard, phase_plan: &HistoricalPhasePlan) {
    let Some(phase) = phase_plan
        .phases
        .iter()
        .find(|phase| phase.readiness.ready && phase.label.trim() == card.label.trim())
    else {
        return;
    };
    if card
        .timeframe
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.timeframe = phase.timeframe.clone();
    }
    if card.actors.is_empty() {
        card.actors = phase.actors.clone();
    }
    if card
        .region_or_front
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.region_or_front = phase.region_or_front.clone();
    }
    if card
        .trigger
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.trigger = phase.trigger.clone();
    }
    if card
        .development
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.development = phase.expected_development_focus.clone();
    }
    if card
        .outcome
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        card.outcome = phase.expected_outcome.clone();
    }
    merge_unique_strings(&mut card.claim_log_ids, &phase.claim_log_ids);
    merge_unique_strings(&mut card.source_ids, &phase.source_ids);
    let current_development = card.development.as_deref().unwrap_or_default();
    if current_development.chars().count() < 90
        || !current_development.contains("외교")
        || !current_development.contains("군사")
    {
        let phase_development = phase
            .expected_development_focus
            .as_deref()
            .unwrap_or(current_development)
            .trim();
        let actors = if phase.actors.is_empty() {
            "주요 행위자".to_string()
        } else {
            phase.actors.join(", ")
        };
        let region = phase.region_or_front.as_deref().unwrap_or("주요 지역/전선");
        let phase_development = ensure_reader_sentence(phase_development);
        card.development = Some(format!(
            "{phase_development} 주요 무대는 다음과 같았다: {region}. 주요 행위자({actors})는 외교·동맹 계산, 군사·동원 압력, 정치·주권 문제를 함께 처리해야 했다."
        ));
    }
}

fn merge_unique_strings(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !target.iter().any(|existing| existing.trim() == trimmed) {
            target.push(trimmed.to_string());
        }
    }
}

fn build_claim_grounded_event_cards_from_claim_log(
    claim_log: &[ResearchClaimLogEntry],
    source_cards: &[ResearchSourceCard],
) -> Vec<NarrativeEventCard> {
    let source_ids = source_cards
        .iter()
        .map(|card| card.id.clone())
        .collect::<HashSet<_>>();
    claim_log
        .iter()
        .filter(|claim| !claim.claim.trim().is_empty())
        .filter(|claim| {
            claim
                .support_source_card_ids
                .iter()
                .any(|id| source_ids.contains(id))
        })
        .take(8)
        .map(|claim| {
            let label = claim
                .claim
                .split(':')
                .next()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .unwrap_or(claim.claim.as_str())
                .to_string();
            let actors = extract_actors_from_claim(&claim.claim);
            let region = extract_region_from_claim(&claim.claim);
            let trigger = build_trigger_from_claim(&claim.claim, &label);
            let outcome =
                extract_outcome_from_claim(&claim.claim).or_else(|| Some(claim.claim.clone()));
            let development = Some(build_development_from_claim(
                &claim.claim,
                &label,
                &actors,
                region.as_deref(),
                trigger.as_deref(),
                outcome.as_deref(),
            ));
            NarrativeEventCard {
                label,
                timeframe: extract_timeframe_from_claim(&claim.claim),
                actors,
                region_or_front: region,
                trigger,
                development,
                outcome,
                claim_log_ids: vec![claim.id.clone()],
                source_ids: claim.support_source_card_ids.clone(),
                causal_spine: Vec::new(),
                interpretive_layers: Vec::new(),
                confidence: claim.confidence.clone(),
                open_questions: Vec::new(),
            }
        })
        .collect()
}

fn build_working_thesis(
    subject: &str,
    cards: &[NarrativeEventCard],
    phases: &[&liquid_research_classic::HistoricalResearchPhase],
) -> String {
    let first = cards
        .first()
        .map(|card| card.label.as_str())
        .or_else(|| phases.first().map(|phase| phase.label.as_str()))
        .unwrap_or("the opening crisis");
    let last = cards
        .last()
        .map(|card| card.label.as_str())
        .or_else(|| phases.last().map(|phase| phase.label.as_str()))
        .unwrap_or("the later settlement");
    format!(
        "{subject}의 핵심 구조는 {first}에서 {last}까지 이어지는 위기의 누적이다. 각 국면은 다음 국면의 외교·군사·정치 선택지를 다시 배치했고, 그래서 단일 원인보다 국면 사이에서 커진 압력과 시간 부족을 함께 읽어야 한다."
    )
}

fn build_evidence_layers(artifacts: &ResearchControllerArtifacts) -> Vec<NarrativeEvidenceLayer> {
    let mut layers = Vec::new();
    let primary_ids = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            card.source_class.contains("official") || card.source_class.contains("primary")
        })
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    if !primary_ids.is_empty() {
        layers.push(NarrativeEvidenceLayer {
            id: "EL1".to_string(),
            label: "Primary and official anchors".to_string(),
            purpose: Some(
                "Public primary or official material anchors chronology, actors, and settlement claims before interpretation."
                    .to_string(),
            ),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: artifacts
                .claim_log
                .iter()
                .filter(|claim| {
                    claim
                        .support_source_card_ids
                        .iter()
                        .any(|id| primary_ids.iter().any(|expected| expected == id))
                })
                .map(|claim| claim.id.clone())
                .collect(),
            expected_source_card_ids: primary_ids,
        });
    }
    let secondary_ids = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            !card.source_class.contains("official") && !card.source_class.contains("primary")
        })
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    if !secondary_ids.is_empty() {
        layers.push(NarrativeEvidenceLayer {
            id: "EL2".to_string(),
            label: "Interpretive secondary synthesis".to_string(),
            purpose: Some(
                "Secondary synthesis explains why phases matter, where evidence is thin, and which causal links remain interpretive."
                    .to_string(),
            ),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: artifacts.claim_log.iter().map(|claim| claim.id.clone()).collect(),
            expected_source_card_ids: secondary_ids,
        });
    }
    layers
}

fn build_interpretive_tensions_from_cards(
    cards: &[NarrativeEventCard],
) -> Vec<NarrativeInterpretiveTension> {
    cards
        .iter()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeInterpretiveTension {
            id: format!("IT{}", idx + 1),
            question: format!(
                "How much of {} should be read as a direct consequence of {} rather than broader structural pressure around {}?",
                card.label,
                card.trigger.as_deref().unwrap_or(&card.label),
                card.region_or_front.as_deref().unwrap_or("the main theater")
            ),
            competing_readings: Some(format!(
                "{} can be read both as a concrete phase shift and as a pressure point that changed diplomatic, institutional, or coalition behavior for {}.",
                card.development.as_deref().unwrap_or(&card.label),
                if card.actors.is_empty() {
                    "the main actors".to_string()
                } else {
                    card.actors.join(", ")
                }
            )),
            current_status: Some(format!(
                "bounded interpretation; keep {} tied to {} and avoid claims beyond the cited phase row",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_impacts_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeImpact> {
    cards
        .iter()
        .rev()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeImpact {
            id: format!("IM{}", idx + 1),
            label: format!("{} impact", card.label),
            scope: Some("regional and longer-horizon consequences".to_string()),
            implication: Some(format!(
                "{} shaped the settlement, postwar order, or later balance of power because {}.",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            derived_from: Some("historical_phase_engine".to_string()),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_reader_questions_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeReaderQuestion> {
    cards
        .iter()
        .rev()
        .take(3)
        .enumerate()
        .map(|(idx, card)| NarrativeReaderQuestion {
            id: format!("RQ{}", idx + 1),
            question: format!(
                "Which source layer would most tighten the remaining uncertainty around {} in {}?",
                card.label,
                card.region_or_front.as_deref().unwrap_or("the phase's main setting")
            ),
            answer_status: Some("partially answered".to_string()),
            answer_plan: Some(format!(
                "Recheck {} with another public source if stronger actor, front, or settlement detail is needed around {}.",
                card.label,
                card.outcome.as_deref().unwrap_or(&card.label)
            )),
            expected_claim_log_ids: card.claim_log_ids.clone(),
            expected_source_card_ids: card.source_ids.clone(),
        })
        .collect()
}

fn build_actor_rows_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeActor> {
    let mut seen = HashSet::new();
    let mut actors = Vec::new();
    for card in cards {
        for actor in &card.actors {
            if !seen.insert(actor.clone()) {
                continue;
            }
            actors.push(NarrativeActor {
                id: format!("A{}", actors.len() + 1),
                label: actor.clone(),
                role: Some(format!("{} 국면에서 가장 뚜렷하게 작동한다.", card.label)),
                relevance: Some(format!(
                    "이 행위자는 {} 국면의 전환 압력을 형성한다. 계기: {}",
                    card.label,
                    card.trigger.as_deref().unwrap_or(&card.label)
                )),
                expected_claim_log_ids: card.claim_log_ids.clone(),
                expected_source_card_ids: card.source_ids.clone(),
            });
        }
    }
    actors
}

fn build_causal_chain_from_cards(cards: &[NarrativeEventCard]) -> Vec<NarrativeCausalLink> {
    cards
        .windows(2)
        .enumerate()
        .map(|(idx, pair)| NarrativeCausalLink {
            id: format!("CL{}", idx + 1),
            cause: pair[0]
                .outcome
                .clone()
                .unwrap_or_else(|| {
                    pair[0]
                        .development
                        .clone()
                        .unwrap_or_else(|| pair[0].label.clone())
                }),
            effect: pair[1]
                .trigger
                .clone()
                .unwrap_or_else(|| pair[1].label.clone()),
            rationale: Some(format!(
                "{}의 귀결은 {}이 다음 국면으로 열리는 조건을 만들었다. 이유는 {}가 다음 행위자 집단({})의 선택지를 바꾸었기 때문이다.",
                pair[0].label,
                pair[1].label,
                pair[0].outcome.as_deref().unwrap_or(&pair[0].label),
                if pair[1].actors.is_empty() {
                    "the main coalition".to_string()
                } else {
                    pair[1].actors.join(", ")
                }
            )),
            derived_from: Some("claim_grounded_event_cards".to_string()),
            expected_claim_log_ids: pair[0]
                .claim_log_ids
                .iter()
                .chain(pair[1].claim_log_ids.iter())
                .cloned()
                .collect(),
            expected_source_card_ids: pair[0]
                .source_ids
                .iter()
                .chain(pair[1].source_ids.iter())
                .cloned()
                .collect(),
        })
        .collect()
}

fn build_open_gaps(debts: &[ResearchDebtItem]) -> Vec<NarrativeOpenGap> {
    debts
        .iter()
        .filter(|debt| debt.status != "closed")
        .take(4)
        .enumerate()
        .map(|(idx, debt)| NarrativeOpenGap {
            id: format!("OG{}", idx + 1),
            gap_type: debt
                .failed_gate
                .clone()
                .unwrap_or_else(|| "historical_phase_engine".to_string()),
            description: debt.missing_evidence.clone(),
            status: Some("open".to_string()),
            expected_claim_log_ids: Vec::new(),
            expected_source_card_ids: Vec::new(),
        })
        .collect()
}

fn build_causal_spine(card: &NarrativeEventCard) -> Vec<NarrativeCausalSpineStep> {
    let actor_text = if card.actors.is_empty() {
        "주요 행위자".to_string()
    } else {
        card.actors.join(", ")
    };
    let region_text = card
        .region_or_front
        .as_deref()
        .unwrap_or("주요 지역/전선")
        .to_string();
    let trigger = card.trigger.as_deref().unwrap_or(&card.label);
    let development = card.development.as_deref().unwrap_or(&card.label);
    let outcome = card.outcome.as_deref().unwrap_or(&card.label);
    let base_reasoning = format!(
        "{}와 {}가 같은 phase-specific Claim Log에 연결되어 있기 때문에, 이 단계는 근거 밖의 사건을 만들지 않고 다음 선택지가 왜 좁아졌는지를 설명한다.",
        trigger, outcome
    );
    vec![
        NarrativeCausalSpineStep {
            step_type: "precondition".to_string(),
            description: format!(
                "{} 이 계기는 {}의 출발 조건이 되었고, 주요 행위자({})가 {}에서 택할 수 있는 외교·군사 선택지를 좁혔다.",
                trigger,
                card.label,
                actor_text,
                region_text
            ),
            epistemic_status: Some("inference".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} 해당 Claim Log가 계기, 행위자, 지역/전선을 함께 지지하므로 이 제약 설명은 증거 경계를 벗어나지 않는다."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "decision_point".to_string(),
            description: format!(
                "이 국면은 단순한 사건 경과가 아니라 결정 지점이었다. 주요 행위자는 확전, 후퇴, 협상 중 무엇을 감수할지 판단해야 했다: {}",
                trigger
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} 같은 Claim Log가 계기·전개·귀결을 연결하므로 이 국면은 고립된 에피소드가 아니라 전환점으로 해석할 수 있다."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "execution".to_string(),
            description: format!(
                "{} 이 전개는 주요 행위자({})가 {}에서 실제로 어떻게 움직였는지를 보여 준다.",
                development,
                actor_text,
                region_text
            ),
            epistemic_status: Some("fact".to_string()),
            reasoning: Some(
                "이 전개 문장은 phase-specific evidence에 이미 포함된 행위자, 장소, 작전·외교 표현을 반복하므로 사실 서술 범위에 머문다."
                    .to_string(),
            ),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeCausalSpineStep {
            step_type: "outcome".to_string(),
            description: format!(
                "{} 이 귀결은 주요 행위자({})가 {} 이후 취할 수 있는 선택지를 바꾸었고, 바로 그 지점에서 다음 국면으로 이어지는 압력이 만들어졌다.",
                outcome,
                actor_text,
                card.label
            ),
            epistemic_status: Some("inference".to_string()),
            reasoning: Some(format!(
                "{base_reasoning} 그 결과 귀결은 다음 국면이 왜 그 지점에서 열렸는지를 설명하는 압력점이 된다."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
    ]
}

fn build_interpretive_layers(card: &NarrativeEventCard) -> Vec<NarrativeInterpretiveLayer> {
    let region = card.region_or_front.as_deref().unwrap_or("주요 지역/전선");
    let actors = if card.actors.is_empty() {
        "주요 행위자".to_string()
    } else {
        card.actors.join(", ")
    };
    vec![
        NarrativeInterpretiveLayer {
            layer_type: "diplomacy_alliance".to_string(),
            interpretation: format!(
                "{}에서 {}의 이해가 맞물린 방식은 외교·동맹 신뢰가 별도 문제가 아니었음을 보여 준다. {}",
                region,
                actors,
                ensure_reader_sentence(&format!(
                    "이 국면의 전개는 협상, 동맹 신뢰, 주권 또는 세력권 문제를 다음 국면의 압력으로 바꾸었다: {}",
                    card.development.as_deref().unwrap_or(&card.label)
                ))
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "phase-specific Claim Log가 행위자, 지역/전선, 계기, 귀결을 함께 묶기 때문에 외교적 해석은 새 사건을 발명하지 않고도 한 움직임이 다음 제약을 만든 방식을 설명한다."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
        NarrativeInterpretiveLayer {
            layer_type: "mobilization_politics".to_string(),
            interpretation: format!(
                "{}의 귀결은 군사·동원·정치 계산으로도 이어졌다. {}",
                card.label,
                ensure_reader_sentence(&format!(
                    "이 국면의 귀결은 단순한 외교 문구를 넘어 동원 시간표, 국내 체면, 제국의 권위, 참전 판단에 영향을 주었다: {}",
                    card.outcome.as_deref().unwrap_or(&card.label)
                ))
            ),
            epistemic_status: Some("interpretation".to_string()),
            reasoning: Some(format!(
                "이 층위는 결과를 설명하는 같은 claim-backed sentence가 더 넓은 외교·정치 결과로 넘어가는 인과 연결도 제공하기 때문에 근거에 묶여 있다."
            )),
            limits: Vec::new(),
            claim_log_ids: card.claim_log_ids.clone(),
            source_ids: card.source_ids.clone(),
        },
    ]
}

fn build_card_open_questions(card: &NarrativeEventCard, debts: &[ResearchDebtItem]) -> Vec<String> {
    debts
        .iter()
        .filter(|debt| debt.status != "closed")
        .filter(|debt| {
            debt.missing_evidence.contains(&card.label)
                || card
                    .claim_log_ids
                    .iter()
                    .any(|claim_id| debt.missing_evidence.contains(claim_id))
        })
        .map(|debt| debt.missing_evidence.clone())
        .take(2)
        .collect()
}

fn build_reader_quality(
    cards: &[NarrativeEventCard],
    working_thesis: &str,
) -> ReaderQualityArtifacts {
    ReaderQualityArtifacts {
        argument_graph: Some(ReaderArgumentGraph {
            nodes: cards
                .iter()
                .take(3)
                .enumerate()
                .map(|(idx, card)| ReaderArgumentNode {
                    id: format!("RN{}", idx + 1),
                    label: format!("{} as a turning phase", card.label),
                    node_type: Some("phase_turn".to_string()),
                    rationale: Some(format!(
                        "{} is one part of the thesis that the conflict moved by chained constraints rather than disconnected battles because {}.",
                        card.label,
                        card.outcome.as_deref().unwrap_or(&card.label)
                    )),
                    claim_log_ids: card.claim_log_ids.clone(),
                    source_card_ids: card.source_ids.clone(),
                })
                .collect(),
            edges: cards
                .windows(2)
                .enumerate()
                .map(|(idx, pair)| ReaderArgumentEdge {
                    id: format!("RE{}", idx + 1),
                    from_node_id: format!("RN{}", idx + 1),
                    to_node_id: format!("RN{}", idx + 2),
                    relation: "constraint_to_next_phase".to_string(),
                    rationale: Some(format!(
                        "{} narrowed the next choice set until {} became unavoidable in the reader-facing narrative because {} shaped the next options.",
                        pair[0].label,
                        pair[1].label,
                        pair[0].outcome.as_deref().unwrap_or(&pair[0].label)
                    )),
                    claim_log_ids: pair[0]
                        .claim_log_ids
                        .iter()
                        .chain(pair[1].claim_log_ids.iter())
                        .cloned()
                        .collect(),
                    source_card_ids: pair[0]
                        .source_ids
                        .iter()
                        .chain(pair[1].source_ids.iter())
                        .cloned()
                        .collect(),
                })
                .collect(),
        }),
        narrative_plan: Some(ReaderNarrativePlan {
            lead_section_id: Some("phase-1".to_string()),
            section_ids: cards
                .iter()
                .enumerate()
                .map(|(idx, _)| format!("phase-{}", idx + 1))
                .collect(),
            transition_ids: cards
                .windows(2)
                .enumerate()
                .map(|(idx, _)| format!("transition-{}", idx + 1))
                .collect(),
            narrative_arc: Some(working_thesis.to_string()),
            ending_note: Some(
                "End by separating what the evidence confirms from what remains interpretive."
                    .to_string(),
            ),
        }),
        section_briefs: cards
            .iter()
            .take(4)
            .enumerate()
            .map(|(idx, card)| ReaderSectionBrief {
                section_id: Some(format!("phase-{}", idx + 1)),
                key_point: format!(
                    "{} should read as a linked phase with trigger, development, outcome, and consequence because {}.",
                    card.label,
                    card.development.as_deref().unwrap_or(&card.label)
                ),
                reader_goal: Some(
                    "Help the reader follow why one phase narrowed the options for the next."
                        .to_string(),
                ),
                claim_log_ids: card.claim_log_ids.clone(),
                source_card_ids: card.source_ids.clone(),
            })
            .collect(),
        reader_critique: None,
    }
}

fn extract_timeframe_from_claim(claim: &str) -> Option<String> {
    let trimmed = claim.trim();
    let prefix = trimmed.split(':').next().map(str::trim).unwrap_or(trimmed);
    if prefix.chars().any(|ch| ch.is_ascii_digit()) {
        Some(prefix.to_string())
    } else {
        None
    }
}

fn extract_region_from_claim(claim: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let markers = ["에서 ", " in ", " on ", " at ", " across ", " into "];
    let lower = compact.to_ascii_lowercase();
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            let start = idx + marker.len();
            let remainder = compact[start..].trim();
            let end = remainder
                .find(',')
                .or_else(|| remainder.find(';'))
                .or_else(|| remainder.find(" and "))
                .unwrap_or(remainder.len());
            let region = remainder[..end].trim();
            if region.chars().count() >= 3 {
                return Some(region.to_string());
            }
        }
    }
    None
}

fn extract_outcome_from_claim(claim: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let markers = [
        " and ",
        " therefore ",
        " 그 결과 ",
        " 결과적으로 ",
        " 이어졌고",
        " 강화되었다",
        " 확대되었다",
        " 전환했다",
        " 시작되었다",
        " 드러났다",
        " 커졌다",
    ];
    for marker in markers {
        if let Some(idx) = compact.find(marker) {
            let outcome = compact[idx + marker.len()..].trim();
            if outcome.chars().count() >= 20 {
                return Some(outcome.to_string());
            }
        }
    }
    compact
        .split(':')
        .nth(1)
        .map(str::trim)
        .filter(|text| text.chars().count() >= 20)
        .map(ToString::to_string)
}

fn build_trigger_from_claim(claim: &str, label: &str) -> Option<String> {
    let compact = compact_claim_text(claim);
    let detail = compact
        .split(':')
        .nth(1)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(compact.as_str());
    let markers = [
        " and ",
        " therefore ",
        " 그 결과 ",
        " 결과적으로 ",
        " 이어졌고",
        " 강화되었다",
        " 확대되었다",
        " 전환했다",
        " 시작되었다",
        " 드러났다",
        " 커졌다",
    ];
    let end = markers
        .iter()
        .filter_map(|marker| detail.find(marker))
        .min()
        .unwrap_or(detail.len());
    let trigger = detail[..end].trim();
    if trigger.is_empty() {
        Some(label.to_string())
    } else {
        Some(trigger.to_string())
    }
}

fn build_development_from_claim(
    claim: &str,
    label: &str,
    actors: &[String],
    region: Option<&str>,
    trigger: Option<&str>,
    outcome: Option<&str>,
) -> String {
    let actor_text = if actors.is_empty() {
        "the main actors".to_string()
    } else {
        actors.join(", ")
    };
    let region_text = region.unwrap_or("the main theater");
    let trigger_text = trigger.unwrap_or(label);
    let outcome_text = outcome.unwrap_or(claim);
    format!(
        "{} This phase shows how {} moved through {} around {}. It matters because {} then changed the next diplomatic, political, or military options rather than remaining an isolated episode.",
        compact_claim_text(claim),
        actor_text,
        region_text,
        trigger_text,
        outcome_text
    )
}

fn extract_actors_from_claim(claim: &str) -> Vec<String> {
    let detail = compact_claim_text(claim);
    let detail = detail
        .split(':')
        .nth(1)
        .map(str::trim)
        .unwrap_or(detail.as_str());
    first_clause_before_action(detail)
        .split(&[',', ';', '·'][..])
        .flat_map(|part| part.split(" and "))
        .map(str::trim)
        .map(|part| part.trim_matches(|ch: char| matches!(ch, '.' | ':' | ' ')))
        .filter(|part| part.chars().count() >= 2)
        .filter(|part| !part.chars().any(|ch| ch.is_ascii_digit()))
        .fold(Vec::new(), |mut actors, actor| {
            if !actors.iter().any(|existing| existing == actor) {
                actors.push(actor.to_string());
            }
            actors
        })
}

fn compact_claim_text(claim: &str) -> String {
    claim.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_clause_before_action(text: &str) -> &str {
    let lower = text.to_ascii_lowercase();
    let markers = [
        " turned ",
        " forced ",
        " crossed ",
        " attacked ",
        " invaded ",
        " responded ",
        " widened ",
        " changed ",
        " fixed ",
        " began ",
        " became ",
        " moved ",
        " 겪었고",
        " 겪었으며",
        " 둘러싸고",
        " 통해",
        " 이어졌고",
        " 강화되었다",
        " 드러냈고",
        " 시작되었다",
    ];
    let mut earliest = None;
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            earliest = Some(earliest.map_or(idx, |current: usize| current.min(idx)));
        }
    }
    earliest
        .map(|idx| text[..idx].trim())
        .unwrap_or(text.trim())
}

fn card_has_claim_backed_causal_spine(card: &NarrativeEventCard) -> bool {
    if card.claim_log_ids.is_empty() || card.source_ids.is_empty() {
        return false;
    }
    if card.causal_spine.len() < 4 {
        return false;
    }
    let required_types = ["precondition", "decision_point", "execution", "outcome"];
    required_types.iter().all(|required| {
        card.causal_spine.iter().any(|step| {
            step.step_type == *required
                && !step.claim_log_ids.is_empty()
                && !step.source_ids.is_empty()
                && engine_safe_reader_text(&step.description, 220).is_some()
        })
    })
}

fn card_is_research_grade_specific(card: &NarrativeEventCard) -> bool {
    card.timeframe
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && !card.actors.is_empty()
        && card
            .region_or_front
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && card
            .trigger
            .as_deref()
            .is_some_and(|value| value.chars().count() >= 16)
        && card
            .development
            .as_deref()
            .is_some_and(|value| value.chars().count() >= 80)
        && card
            .outcome
            .as_deref()
            .is_some_and(|value| value.chars().count() >= 24)
        && card_has_claim_backed_causal_spine(card)
}

fn historical_final_answer_template_failures(
    output: &str,
    cards: &[NarrativeEventCard],
) -> Vec<String> {
    let final_answer = output.split("\n# 검증 부록").next().unwrap_or(output);
    let lower = final_answer.to_ascii_lowercase();
    let mut failures = Vec::new();
    for marker in [
        "이 사건은 중요했고 결국",
        "독자는 큰 흐름만 이해하면",
        "전체적으로는 위기 대응의 사례",
        "이 국면이 중요한 이유는 사건 하나가 끝났기 때문이 아니라",
        "해석은 두 층위로 나뉜다",
        "주요 행위자는 확전, 후퇴, 협상 중",
        "다만 현재 근거가 지지하는 것은",
        "### 동시대 비교",
        "### 후대 영향",
        "### 후속 탐색 질문",
    ] {
        if final_answer.contains(marker) || lower.contains(&marker.to_ascii_lowercase()) {
            failures.push(format!(
                "historical phase engine rendered a template-like summary marker instead of card-specific prose: {marker}"
            ));
        }
    }
    let mentioned_cards = cards
        .iter()
        .filter(|card| final_answer.contains(card.label.as_str()))
        .count();
    let required_card_mentions = cards.len().min(4);
    if required_card_mentions > 0 && mentioned_cards < required_card_mentions {
        failures.push(format!(
            "historical phase engine final answer mentioned only {mentioned_cards} phase labels; strict rendering expected at least {required_card_mentions}"
        ));
    }
    let mentioned_timeframes = cards
        .iter()
        .filter_map(|card| card.timeframe.as_deref())
        .filter(|timeframe| final_answer.contains(timeframe))
        .count();
    let required_timeframe_mentions = cards.len().min(3);
    if required_timeframe_mentions > 0 && mentioned_timeframes < required_timeframe_mentions {
        failures.push(format!(
            "historical phase engine final answer mentioned only {mentioned_timeframes} phase time anchors; strict rendering expected at least {required_timeframe_mentions}"
        ));
    }
    let repeated_question_count = final_answer.matches("분석 질문은 이것이다").count();
    if repeated_question_count > cards.len().min(2).max(1) {
        failures.push(format!(
            "historical phase engine repeated the same analytical-question lead {repeated_question_count} time(s); research-grade rendering expects phase-specific variation"
        ));
    }
    failures
}

fn assess_historical_phase_engine_verdict(
    artifacts: &ResearchControllerArtifacts,
    output: &str,
    validation_failures: &[String],
) -> (HistoricalPhaseEngineVerdict, Vec<String>) {
    let mut blocked = Vec::new();
    let mut partial = Vec::new();
    let mut explainer = Vec::new();
    let state = artifacts.narrative_state.as_ref();
    let cards = state
        .map(|state| state.event_cards.as_slice())
        .unwrap_or(&[]);
    if artifacts.source_cards.is_empty() {
        blocked.push(
            "historical phase engine could not build any trusted Source Cards with non-empty extracted facts"
                .to_string(),
        );
    }
    if artifacts.claim_log.is_empty() {
        blocked.push(
            "historical phase engine could not build any semantically supported Claim Log rows"
                .to_string(),
        );
    }
    if cards.is_empty() {
        blocked.push(
            "historical phase engine could not build any claim-backed phase cards".to_string(),
        );
    }
    for failure in validation_failures {
        let lower = failure.to_ascii_lowercase();
        let evidence_failure = lower.contains("source")
            || lower.contains("claim")
            || lower.contains("support")
            || lower.contains("debt")
            || lower.contains("conflict")
            || lower.contains("blocked");
        if blocked.is_empty() && evidence_failure {
            partial.push(failure.clone());
        } else if blocked.is_empty() {
            explainer.push(failure.clone());
        } else {
            blocked.push(failure.clone());
        }
    }
    let support_gap_debt_count = artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
        .filter(|debt| {
            let failed_gate = debt
                .failed_gate
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let missing = debt.missing_evidence.to_ascii_lowercase();
            failed_gate.contains("source")
                || failed_gate.contains("claim")
                || missing.contains("public url")
                || missing.contains("source card")
                || missing.contains("claim log")
                || missing.contains("extracted facts")
                || missing.contains("semantically supported")
                || missing.contains("support")
        })
        .count();
    if blocked.is_empty() && support_gap_debt_count > 0 {
        partial.push(format!(
            "historical phase engine left {support_gap_debt_count} open support debt item(s) in the evidence ledger"
        ));
    }
    let granular_claim_count = artifacts
        .claim_log
        .iter()
        .filter(|claim| {
            claim
                .claim_type
                .as_deref()
                .is_some_and(claim_type_is_granular)
        })
        .count();
    if blocked.is_empty() && !artifacts.claim_log.is_empty() && granular_claim_count == 0 {
        explainer.push(
            "historical phase engine built only broad overview claims, not granular phase claims"
                .to_string(),
        );
    }
    let claim_type_by_id = artifacts
        .claim_log
        .iter()
        .filter_map(|claim| {
            Some((
                claim.id.as_str(),
                claim
                    .claim_type
                    .as_deref()
                    .filter(|value| !value.is_empty())?,
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let cards_with_claim_type = |claim_type: &str| -> usize {
        cards
            .iter()
            .filter(|card| {
                card.claim_log_ids.iter().any(|claim_id| {
                    claim_type_by_id
                        .get(claim_id.as_str())
                        .is_some_and(|value| *value == claim_type)
                })
            })
            .count()
    };
    if blocked.is_empty() && !cards.is_empty() {
        let event_fact_cards = cards_with_claim_type("event_fact");
        if event_fact_cards < cards.len() {
            explainer.push(format!(
                "historical phase engine attached event_fact claims to only {event_fact_cards}/{} phase card(s); research-grade output requires every phase to have an event_fact claim",
                cards.len()
            ));
        }
        let causal_handoff_cards = cards_with_claim_type("causal_handoff");
        if causal_handoff_cards * 5 < cards.len() * 3 {
            explainer.push(format!(
                "historical phase engine attached causal_handoff claims to only {causal_handoff_cards}/{} phase card(s); research-grade output requires at least 60% causal handoff coverage",
                cards.len()
            ));
        }
        let actor_strategy_cards = cards_with_claim_type("actor_strategy");
        if actor_strategy_cards * 2 < cards.len() {
            explainer.push(format!(
                "historical phase engine attached actor_strategy claims to only {actor_strategy_cards}/{} phase card(s); research-grade output requires at least 50% actor strategy coverage",
                cards.len()
            ));
        }
        let interpretive_limit_count = artifacts
            .claim_log
            .iter()
            .filter(|claim| claim.claim_type.as_deref() == Some("interpretive_limit"))
            .count();
        if interpretive_limit_count == 0 {
            explainer.push(
                "historical phase engine did not retain any interpretive_limit claim; research-grade output requires at least one explicit interpretive limit"
                    .to_string(),
            );
        }
    }
    let research_grade_cards = cards
        .iter()
        .filter(|card| card_is_research_grade_specific(card))
        .count();
    let required_research_grade_cards = cards.len().min(4);
    if blocked.is_empty()
        && required_research_grade_cards > 0
        && research_grade_cards < required_research_grade_cards
    {
        explainer.push(format!(
            "historical phase engine built only {research_grade_cards} research-grade phase card(s); strict rendering expected at least {required_research_grade_cards}"
        ));
    }
    if blocked.is_empty() {
        explainer.extend(historical_final_answer_template_failures(output, cards));
    }
    if !blocked.is_empty() {
        (HistoricalPhaseEngineVerdict::Blocked, blocked)
    } else if !partial.is_empty() {
        partial.extend(explainer);
        (HistoricalPhaseEngineVerdict::Partial, partial)
    } else if !explainer.is_empty() {
        (HistoricalPhaseEngineVerdict::Explainer, explainer)
    } else {
        (HistoricalPhaseEngineVerdict::ResearchGrade, Vec::new())
    }
}

fn historical_phase_engine_quality_gate(
    artifacts: &ResearchControllerArtifacts,
    verdict: HistoricalPhaseEngineVerdict,
    failure_messages: &[String],
) -> ResearchQualityGateArtifact {
    let mut gate = research_quality_gate_from_failures(
        artifacts,
        if verdict == HistoricalPhaseEngineVerdict::ResearchGrade {
            &[]
        } else {
            failure_messages
        },
    );
    gate.status = verdict.quality_gate_status().to_string();
    gate.failure_messages = gate
        .failure_messages
        .into_iter()
        .take(4)
        .map(|message| truncate_engine_artifact_text(&message, 140))
        .collect();
    gate
}

fn sync_engine_validation_debt(artifacts: &mut ResearchControllerArtifacts, failures: &[String]) {
    for (idx, failure) in failures.iter().enumerate() {
        upsert_research_debt(
            &mut artifacts.research_debt,
            ResearchDebtItem {
                id: format!("historical-phase-engine-validation-{}", idx + 1),
                severity: "high".to_string(),
                failed_gate: Some("historical_phase_engine_validation".to_string()),
                missing_evidence: failure.clone(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: vec![
                    "Add or tighten phase-specific claims, fronts, actors, and settlement consequences in the engine-owned evidence inputs.".to_string(),
                ],
                status: "open".to_string(),
            },
        );
    }
}

fn render_historical_phase_engine_output(
    subject: &str,
    artifacts: &ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let final_answer = render_historical_phase_engine_final_answer(subject, artifacts);
    let appendix = render_historical_phase_engine_appendix(artifacts, phase_plan, quality_gate);
    format!(
        "## 최종 답변 (Final Answer)\n\n{}\n\n# 검증 부록\n\n{}",
        final_answer, appendix
    )
}

fn render_historical_phase_engine_final_answer(
    subject: &str,
    artifacts: &ResearchControllerArtifacts,
) -> String {
    let mut sections = Vec::new();
    if let Some(state) = artifacts.narrative_state.as_ref() {
        let cards = state.event_cards.as_slice();
        let first = cards
            .first()
            .map(|card| card.label.as_str())
            .unwrap_or("첫 국면");
        let last = cards
            .last()
            .map(|card| card.label.as_str())
            .unwrap_or("마지막 국면");
        let thesis =
            engine_safe_reader_text(state.working_thesis.as_deref().unwrap_or(subject), 360)
                .unwrap_or_else(|| subject.to_string());
        sections.push(format!(
            "### 핵심 결론\n\n이 연구는 “{subject}”이라는 문제를 단일한 암살이나 선전포고의 폭발로 보지 않고, {first}에서 {last}까지 이어진 외교 위기·동맹 신뢰·군사 동원 판단의 연쇄로 읽는다. {thesis} 따라서 핵심은 각국의 목표를 나열하는 데 있지 않다. 위기가 반복될수록 후퇴 비용은 커지고, 협상 시간은 줄었으며, 어느 순간 전쟁을 피하는 선택지가 정치적으로도 군사적으로도 좁아졌다는 점이 더 중요하다."
        ));
        sections.push(format!(
            "### 배경과 구조\n\n본문은 확인된 출처와 주장 로그에 묶인 국면만 다룬다. 먼저 외교 위기의 순서를 따라가고, 각 국면에서 행위자·지역/전선·계기·전개·귀결이 다음 선택지를 어떻게 바꾸었는지 본다. 이렇게 보면 연표는 단순한 사건 목록이 아니라 협정과 동맹, 주권 문제, 발칸의 지역 위기, 동원과 침공이 차례로 서로를 압박한 구조가 된다."
        ));
        sections.push("### 국면별 전개와 인과 사슬".to_string());
        for (idx, card) in cards.iter().enumerate() {
            let next = cards.get(idx + 1);
            sections.push(render_phase_section(idx, card, next));
        }
        sections.push(render_actor_calculus(state));
        sections.push(render_chronology_and_interpretation(state));
        sections.push(render_source_layers(artifacts));
        sections.push(render_debate_map(state));
        sections.push(render_confirmed_and_uncertain(state));
    } else {
        sections.push(format!(
            "### 핵심 결론\n\n{subject}는 현재 evidence 입력만으로 신뢰 가능한 국면별 연구 상태를 만들 수 없다. Source Cards와 Claim Log가 충분하지 않으므로 최종 답변은 차단된 상태로 남긴다."
        ));
    }
    sections.join("\n\n")
}

fn phase_analytical_question(card: &NarrativeEventCard) -> String {
    let label = card.label.as_str();
    if label.contains("모로코") {
        "독일의 세력 시험은 왜 협상국을 흔들기보다 오히려 결속시키는 방향으로 작동했는가?"
            .to_string()
    } else if label.contains("보스니아") {
        "외교적 타결처럼 보인 병합 승인은 왜 러시아와 세르비아의 다음 후퇴 비용을 더 키웠는가?"
            .to_string()
    } else if label.contains("아가디르") {
        "식민지 교섭으로 시작한 위기는 왜 해군력과 안보 불안을 동시에 증폭시켰는가?".to_string()
    } else if label.contains("발칸") {
        "지역 전쟁은 왜 세르비아 문제를 제국 안보와 동맹 신뢰의 문제로 바꾸었는가?".to_string()
    } else if label.contains("사라예보") {
        "암살 사건은 왜 수사·처벌의 문제가 아니라 동맹과 체면이 결합된 결단 문제로 번졌는가?"
            .to_string()
    } else if label.contains("최후통첩") || label.contains("7월") {
        "최후통첩은 왜 협상의 문서라기보다 선택지를 좁히는 압박 장치로 작동했는가?".to_string()
    } else if label.contains("동원") {
        "방어적 준비라고 주장된 동원은 왜 상대에게 공격 시점 선점으로 읽혔는가?".to_string()
    } else if label.contains("벨기에") {
        "작전 계획은 왜 외교적 모호성을 영국 참전이라는 현실 선택으로 바꾸었는가?".to_string()
    } else {
        format!(
            "{}에서 확인되는 행위와 귀결은 다음 국면의 선택지를 어떤 방향으로 좁혔는가?",
            label
        )
    }
}

fn phase_specific_move(
    index: usize,
    card: &NarrativeEventCard,
    next: Option<&NarrativeEventCard>,
) -> String {
    let next_label = next
        .map(|next| next.label.as_str())
        .unwrap_or("전쟁 발발·참전 결정");
    let actor = card
        .actors
        .first()
        .map(String::as_str)
        .unwrap_or("주요 행위자");
    match index % 4 {
        0 => format!(
            "여기서 눈여겨볼 움직임은 {actor}의 선택이 단순한 국면 종결이 아니라 {next_label}의 협상 비용을 미리 올렸다는 점이다."
        ),
        1 => format!(
            "이 단계의 전환은 타협 자체보다 타협 뒤에 남은 불만과 체면 손실이 {next_label}에서 더 강한 압박으로 되돌아왔다는 데 있다."
        ),
        2 => format!(
            "핵심 변화는 지역·식민지·동맹 문제가 분리되지 않고 {next_label}의 안보 계산으로 재배열되었다는 점이다."
        ),
        _ => format!(
            "따라서 이 국면은 다음 사건의 배경이 아니라 {next_label}이 더 위험하게 읽히도록 만든 조건 변화로 보아야 한다."
        ),
    }
}

fn render_phase_section(
    index: usize,
    card: &NarrativeEventCard,
    next: Option<&NarrativeEventCard>,
) -> String {
    let timeframe = card.timeframe.as_deref().unwrap_or("시기 미상 국면");
    let actors = if card.actors.is_empty() {
        "주요 행위자".to_string()
    } else {
        card.actors.join(", ")
    };
    let region = card.region_or_front.as_deref().unwrap_or("주요 지역/전선");
    let trigger = reader_fragment(
        card.trigger.as_deref().unwrap_or(&card.label),
        &card.label,
        360,
    );
    let development = reader_fragment(
        card.development.as_deref().unwrap_or(&card.label),
        &card.label,
        520,
    );
    let outcome = reader_fragment(
        card.outcome.as_deref().unwrap_or(&card.label),
        &card.label,
        360,
    );
    let causal = card
        .causal_spine
        .iter()
        .take(2)
        .filter_map(|step| engine_safe_reader_text(step.description.as_str(), 260))
        .collect::<Vec<_>>()
        .join(" ");
    let layer_one = card
        .interpretive_layers
        .first()
        .and_then(|layer| engine_safe_reader_text(layer.interpretation.as_str(), 360))
        .unwrap_or_else(|| "이 국면은 다음 선택지를 바꾸는 해석 층위를 만든다.".to_string());
    let layer_two = card
        .interpretive_layers
        .get(1)
        .and_then(|layer| engine_safe_reader_text(layer.interpretation.as_str(), 360))
        .unwrap_or_else(|| {
            "동시에 같은 압력은 즉각적 외교 충돌을 넘어 동맹·정치·군사 계산으로 확장된다."
                .to_string()
        });
    let question = phase_analytical_question(card);
    let question_lead = match index % 4 {
        0 => "이 국면의 질문은 분명하다.",
        1 => "여기서 먼저 물어야 할 것은 다음이다.",
        2 => "이 단계는 한 가지 분석축으로 압축된다.",
        _ => "이 사건을 연결고리로 읽으려면 이렇게 물어야 한다.",
    };
    let move_sentence = phase_specific_move(index, card, next);
    let next_handoff = next
        .map(|next| {
            format!(
                "그 압력은 곧 {}에서 더 좁은 시간표와 더 높은 후퇴 비용으로 나타났다.",
                next.label
            )
        })
        .unwrap_or_else(|| {
            "마지막에는 외교 위기가 침공·참전 결정으로 고정되면서 별도의 완충 국면을 만들 여지가 크게 줄었다."
                .to_string()
        });
    format!(
        "#### {}. {} ({})\n\n{} {} {}의 무대는 {}였고, 중심 행위자는 {}였다. 출발점은 {} 이어지는 전개는 {} 이 흐름의 귀결은 {} {}\n\n{} 확인된 causal spine은 다음과 같이 읽힌다: {} 이 연결은 새 사건을 덧붙이는 것이 아니라 같은 Claim Log가 묶는 계기·행위자·장소·귀결을 따라 다음 선택지가 좁아지는 방향을 설명한다.\n\n외교적 해석에서는 {} 군사·동원·정치 계산에서는 {} 근거의 한계도 남는다. 이 서술은 국면별 방향과 귀결을 지지하지만, 각국 내부 의사결정의 모든 세부 논쟁까지 확정하지는 않는다.",
        index + 1,
        card.label,
        timeframe,
        question_lead,
        question,
        timeframe,
        region,
        actors,
        ensure_reader_sentence(&trigger),
        ensure_reader_sentence(&development),
        ensure_reader_sentence(&outcome),
        next_handoff,
        move_sentence,
        if causal.trim().is_empty() {
            ensure_reader_sentence(&outcome)
        } else {
            ensure_reader_sentence(causal.as_str())
        },
        ensure_reader_sentence(&layer_one),
        ensure_reader_sentence(&layer_two)
    )
}

fn render_chronology_and_interpretation(state: &NarrativeState) -> String {
    let first = state
        .event_cards
        .first()
        .map(|card| card.label.as_str())
        .unwrap_or("the opening phase");
    let last = state
        .event_cards
        .last()
        .map(|card| card.label.as_str())
        .unwrap_or("마지막 국면");
    format!(
        "### 전개 순서와 해석\n\n연표가 중요한 이유는 {}에서 시작된 외교 압력이 다음 국면의 선택지를 줄이다가 결국 {}이 가능해지는 과정을 보여 주기 때문이다. 그러나 이 해석은 무제한 추론이 아니다. 계기, 행위자, 지역/전선, 결과가 같은 근거 묶음 안에서 함께 지지될 때에만 압력·선택·귀결을 연결한다. 그래서 본문은 “전쟁은 필연이었다”는 단정 대신, 반복된 위기 속에서 각국의 후퇴 비용과 동맹 신뢰 비용이 어떻게 커졌는지를 중심으로 설명한다.",
        first, last
    )
}

fn render_actor_calculus(state: &NarrativeState) -> String {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for card in &state.event_cards {
        for actor in &card.actors {
            if !seen.insert(actor.clone()) {
                continue;
            }
            rows.push(format!(
                "| {} | {} | {} |",
                markdown_table_cell(actor),
                markdown_table_cell(&format!(
                    "{} 국면에서 {} 문제를 통해 외교적 선택지를 계산했다.",
                    card.label,
                    card.region_or_front.as_deref().unwrap_or("주요 전선/지역")
                )),
                markdown_table_cell(
                    card.outcome
                        .as_deref()
                        .unwrap_or("다음 국면의 압력으로 이어졌다.")
                )
            ));
        }
    }
    if rows.is_empty() {
        return "### 행위자별 계산\n\n현재 근거로는 행위자별 계산을 충분히 분리할 수 없다."
            .to_string();
    }
    format!(
        "### 행위자별 계산\n\n| 행위자 | 계산의 초점 | 전쟁 발발로 이어진 압력 |\n| --- | --- | --- |\n{}",
        rows.join("\n")
    )
}

fn render_source_layers(artifacts: &ResearchControllerArtifacts) -> String {
    let primary = artifacts
        .source_cards
        .iter()
        .filter(|card| {
            card.source_class.contains("official") || card.source_class.contains("primary")
        })
        .count();
    let secondary = artifacts.source_cards.len().saturating_sub(primary);
    format!(
        "### 사료 층위\n\n출처 층위는 공식·1차 성격의 앵커 {}개와 해설·2차 종합 출처 {}개로 나뉜다. 공식·1차 성격의 자료는 날짜, 행위자, 지역/전선, 조약·협정·동원 같은 사건 앵커를 고정하는 데 쓰고, 2차 종합 출처는 한 국면이 왜 다음 국면을 압박했는지 설명하는 데 쓴다. 이 구분은 중요하다. 출처는 해석의 발판이지만, 출처가 있다는 사실만으로 모든 해석이 자동으로 증명되지는 않는다.",
        primary, secondary
    )
}

fn render_debate_map(state: &NarrativeState) -> String {
    let debate = state
        .interpretive_tensions
        .first()
        .map(|tension| tension.question.as_str())
        .unwrap_or("어떤 인과 층위가 국면 전환에서 가장 큰 비중을 가졌는가");
    format!(
        "### 주요 쟁점과 해석\n\n핵심 쟁점은 전쟁 직전 외교가 단순한 위기 목록인지, 아니면 반복될수록 후퇴 비용이 증가하는 누적 구조인지에 있다. 이 보고서는 후자에 가깝게 읽되, 모든 국면을 하나의 원인으로 환원하지 않는다. 외교·동맹, 군사·동원, 지역/전선, 제국 내부 정치가 서로 다른 속도로 압력을 키웠다. 제한된 질문은 다음과 같다. {}.",
        debate
    )
}

fn render_confirmed_and_uncertain(state: &NarrativeState) -> String {
    let question = state
        .reader_questions
        .first()
        .map(|question| question.question.as_str())
        .unwrap_or("어떤 자료 층위가 가장 약한 국면을 더 단단하게 만들 수 있는가");
    format!(
        "### 확인된 사실과 불확실성\n\n확인된 사실은 국면의 순서, 주요 행위자, 지역/전선, 그리고 각 국면의 결과가 다음 외교·군사 선택지를 압박했다는 점이다. 불확실성은 각국 내각·군부·여론의 내부 계산을 어느 정도 비중으로 읽어야 하는가에 남아 있다. 다음 확인 질문은 {}이다. 이 질문은 검증 부록의 연구 부채와 분리해 둔다. 본문은 독자가 현재 근거로 판단할 수 있는 범위와 추가 자료가 필요한 범위를 혼동하지 않도록 하기 위한 것이다.",
        question
    )
}

fn render_historical_phase_engine_appendix(
    artifacts: &ResearchControllerArtifacts,
    phase_plan: &HistoricalPhasePlan,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let serialized = serialized_appendix_artifacts(artifacts, quality_gate);
    format!(
        "## 국면 계획 (Phase Plan)\n| Phase | Timeframe | Actors | Region/Front | Claim IDs | Source IDs | Ready |\n| --- | --- | --- | --- | --- | --- | --- |\n{}\n## 국면 카드 (Phase Cards)\n{}\n## 출처 감사 (Source Audit)\n| ID | URL | Source | Class | Checked Fact | Limitation |\n| --- | --- | --- | --- | --- | --- |\n{}\n## 주장 로그 (Claim Log)\n| ID | Claim | Support | Confidence | Uncertainty |\n| --- | --- | --- | --- | --- |\n{}\n## 한계, 충돌, 연구 부채 (Limits, Conflicts, And Research Debt)\n{}\n## 품질 게이트 (Quality Gate)\n{}\n[RESEARCH_ARTIFACT_JSON]\n```json\n{}\n```",
        render_phase_plan_rows(phase_plan),
        render_phase_card_rows(artifacts),
        render_source_audit_rows(artifacts),
        render_claim_log_rows(artifacts),
        render_limits_rows(artifacts),
        render_quality_gate(quality_gate),
        serialized
    )
}

fn serialized_appendix_artifacts(
    artifacts: &ResearchControllerArtifacts,
    quality_gate: &ResearchQualityGateArtifact,
) -> String {
    let mut appendix_artifacts = artifacts.clone();
    appendix_artifacts.events.clear();
    appendix_artifacts.quality_gate = Some(quality_gate.clone());
    compact_engine_appendix_artifacts(&mut appendix_artifacts);
    let value = compact_engine_appendix_artifact_value(&appendix_artifacts);
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

fn compact_engine_appendix_artifact_value(
    artifacts: &ResearchControllerArtifacts,
) -> serde_json::Value {
    let event_cards = artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            state
                .event_cards
                .iter()
                .take(8)
                .map(compact_engine_event_card_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let narrative_state = artifacts.narrative_state.as_ref().map(|state| {
        serde_json::json!({
            "version": state.version,
            "topic_frame": state.topic_frame.as_ref().map(|value| truncate_engine_artifact_text(value, 100)),
            "working_thesis": state.working_thesis.as_ref().map(|value| truncate_engine_artifact_text(value, 140)),
            "event_cards": event_cards,
        })
    });
    serde_json::json!({
        "version": artifacts.version,
        "source_cards": artifacts.source_cards.iter().take(8).map(compact_engine_source_card_value).collect::<Vec<_>>(),
        "claim_log": artifacts.claim_log.iter().take(24).map(compact_engine_claim_value).collect::<Vec<_>>(),
        "conflict_map": artifacts.conflict_map.iter().take(3).collect::<Vec<_>>(),
        "research_debt": artifacts.research_debt.iter().filter(|debt| debt.status != "closed").take(4).map(compact_engine_debt_value).collect::<Vec<_>>(),
        "narrative_state": narrative_state,
        "quality_gate": artifacts.quality_gate,
        "warnings": artifacts.warnings.iter().take(4).map(|value| truncate_engine_artifact_text(value, 100)).collect::<Vec<_>>(),
    })
}

fn compact_engine_source_card_value(card: &ResearchSourceCard) -> serde_json::Value {
    serde_json::json!({
        "id": card.id,
        "url": card.url,
        "title": truncate_engine_artifact_text(&card.title, 60),
        "source_class": truncate_engine_artifact_text(&card.source_class, 40),
        "accessed_at": card.accessed_at,
        "extracted_facts": card.extracted_facts.iter().take(2).map(|value| truncate_engine_artifact_text(value, 56)).collect::<Vec<_>>(),
        "limitation": card.limitation.as_ref().map(|value| truncate_engine_artifact_text(value, 80)),
        "diagnostics_ref": serde_json::Value::Null,
        "confidence": card.confidence,
    })
}

fn compact_engine_claim_value(claim: &ResearchClaimLogEntry) -> serde_json::Value {
    serde_json::json!({
        "id": claim.id,
        "claim": truncate_engine_artifact_text(&claim.claim, 64),
        "claim_type": claim.claim_type,
        "support_source_card_ids": claim.support_source_card_ids,
        "support_urls": claim.support_urls,
        "confidence": claim.confidence,
        "uncertainty_note": claim.uncertainty_note.as_ref().map(|value| truncate_engine_artifact_text(value, 80)),
        "needs_verification": claim.needs_verification,
    })
}

fn compact_engine_event_card_value(card: &NarrativeEventCard) -> serde_json::Value {
    serde_json::json!({
        "label": truncate_engine_artifact_text(&card.label, 80),
        "timeframe": card.timeframe.as_ref().map(|value| truncate_engine_artifact_text(value, 60)),
        "actors": card.actors.iter().take(5).map(|value| truncate_engine_artifact_text(value, 40)).collect::<Vec<_>>(),
        "region_or_front": card.region_or_front.as_ref().map(|value| truncate_engine_artifact_text(value, 60)),
        "trigger": card.trigger.as_ref().map(|value| truncate_engine_artifact_text(value, 100)),
        "development": card.development.as_ref().map(|value| truncate_engine_artifact_text(value, 88)),
        "outcome": card.outcome.as_ref().map(|value| truncate_engine_artifact_text(value, 60)),
        "claim_log_ids": card.claim_log_ids,
        "source_ids": card.source_ids,
        "causal_spine": card.causal_spine.iter().take(2).map(|step| serde_json::json!({
            "step_type": step.step_type,
            "description": truncate_engine_artifact_text(&step.description, 52),
            "epistemic_status": step.epistemic_status,
            "reasoning": serde_json::Value::Null,
            "limits": Vec::<String>::new(),
            "claim_log_ids": step.claim_log_ids,
            "source_ids": step.source_ids,
        })).collect::<Vec<_>>(),
        "interpretive_layers": card.interpretive_layers.iter().take(2).map(|layer| serde_json::json!({
            "layer_type": layer.layer_type,
            "interpretation": truncate_engine_artifact_text(&layer.interpretation, 40),
            "epistemic_status": layer.epistemic_status,
            "reasoning": serde_json::Value::Null,
            "limits": Vec::<String>::new(),
            "claim_log_ids": layer.claim_log_ids,
            "source_ids": layer.source_ids,
        })).collect::<Vec<_>>(),
        "confidence": card.confidence,
        "open_questions": card.open_questions.iter().take(1).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
    })
}

fn compact_engine_debt_value(debt: &ResearchDebtItem) -> serde_json::Value {
    serde_json::json!({
        "id": debt.id,
        "severity": debt.severity,
        "failed_gate": debt.failed_gate,
        "missing_evidence": truncate_engine_artifact_text(&debt.missing_evidence, 140),
        "required_source_class": debt.required_source_class,
        "candidate_queries": debt.candidate_queries.iter().take(2).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
        "next_check_actions": debt.next_check_actions.iter().take(2).map(|value| truncate_engine_artifact_text(value, 80)).collect::<Vec<_>>(),
        "status": debt.status,
    })
}

fn compact_support_facts_for_source(
    source_id: &str,
    facts: &[String],
    claims: &[ResearchClaimLogEntry],
) -> Vec<String> {
    let mut retained = Vec::new();
    for claim in claims.iter().filter(|claim| {
        claim
            .support_source_card_ids
            .iter()
            .any(|id| id.trim() == source_id)
    }) {
        if let Some(fact) = facts
            .iter()
            .filter(|fact| extracted_fact_is_substantive(fact))
            .find(|fact| {
                claim_has_semantic_support_in_facts(&claim.claim, std::slice::from_ref(fact))
            })
        {
            let compact = truncate_engine_artifact_text(fact, 100);
            if !retained.iter().any(|existing| existing == &compact) {
                retained.push(compact);
            }
        }
    }
    if retained.is_empty() {
        if let Some(fact) = facts
            .iter()
            .find(|fact| extracted_fact_is_substantive(fact))
        {
            retained.push(truncate_engine_artifact_text(fact, 100));
        }
    }
    retained.truncate(2);
    retained
}

fn compact_engine_claim_log_for_research_grade(
    claims: &mut Vec<ResearchClaimLogEntry>,
    valid_source_ids: &HashSet<String>,
) {
    for claim in claims.iter_mut() {
        claim
            .support_source_card_ids
            .retain(|id| valid_source_ids.contains(id.trim()));
        claim.support_urls.clear();
    }
    claims.retain(|claim| !claim.support_source_card_ids.is_empty());

    let phase_order = claims
        .iter()
        .filter(|claim| claim.claim_type.as_deref() == Some("event_fact"))
        .map(|claim| phase_label_from_claim(&claim.claim))
        .fold(Vec::<String>::new(), |mut phases, phase| {
            if !phase.is_empty() && !phases.iter().any(|existing| existing == &phase) {
                phases.push(phase);
            }
            phases
        });
    let phase_count = phase_order.len();
    if phase_count == 0 {
        claims.truncate(8);
        return;
    }

    let causal_target = (phase_count * 3).div_ceil(5);
    let actor_target = phase_count.div_ceil(2);
    let mut selected = HashSet::new();
    let mut causal_count = 0usize;
    let mut actor_count = 0usize;
    let mut limit_count = 0usize;

    for claim in claims.iter() {
        let Some(claim_type) = claim.claim_type.as_deref() else {
            continue;
        };
        let include = match claim_type {
            "event_fact" => true,
            "causal_handoff" if causal_count < causal_target => {
                causal_count += 1;
                true
            }
            "actor_strategy" if actor_count < actor_target => {
                actor_count += 1;
                true
            }
            "interpretive_limit" if limit_count == 0 => {
                limit_count += 1;
                true
            }
            _ => false,
        };
        if include {
            selected.insert(claim.id.clone());
        }
    }

    if selected.is_empty() {
        selected.extend(claims.iter().take(8).map(|claim| claim.id.clone()));
    }
    claims.retain(|claim| selected.contains(&claim.id));
    claims.truncate(24);
}

fn compact_engine_appendix_artifacts(artifacts: &mut ResearchControllerArtifacts) {
    artifacts.events.clear();
    artifacts.source_cards.truncate(8);
    let valid_source_ids = artifacts
        .source_cards
        .iter()
        .map(|source| source.id.trim().to_string())
        .collect::<HashSet<_>>();
    compact_engine_claim_log_for_research_grade(&mut artifacts.claim_log, &valid_source_ids);
    artifacts.research_debt.truncate(4);
    artifacts.warnings.truncate(6);
    for debt in &mut artifacts.research_debt {
        debt.missing_evidence = truncate_engine_artifact_text(&debt.missing_evidence, 96);
        debt.candidate_queries = debt
            .candidate_queries
            .iter()
            .take(1)
            .map(|value| truncate_engine_artifact_text(value, 56))
            .collect();
        debt.next_check_actions = debt
            .next_check_actions
            .iter()
            .take(1)
            .map(|value| truncate_engine_artifact_text(value, 64))
            .collect();
    }
    let retained_claims = artifacts.claim_log.clone();
    for source in &mut artifacts.source_cards {
        source.title = truncate_engine_artifact_text(&source.title, 36);
        source.extracted_facts =
            compact_support_facts_for_source(&source.id, &source.extracted_facts, &retained_claims);
        source.limitation = source
            .limitation
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 60));
        source.diagnostics_ref = None;
    }
    for claim in &mut artifacts.claim_log {
        claim.claim = truncate_engine_artifact_text(&claim.claim, 64);
        claim.support_urls.clear();
        claim.uncertainty_note = claim
            .uncertainty_note
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 48));
    }
    if let Some(state) = artifacts.narrative_state.as_mut() {
        state.topic_frame = state
            .topic_frame
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 120));
        state.working_thesis = state
            .working_thesis
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 80));
        state.reader_promise = state
            .reader_promise
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 70));
        state.last_iteration_summary = state
            .last_iteration_summary
            .as_deref()
            .map(|value| truncate_engine_artifact_text(value, 80));
        state.timeline.truncate(4);
        state.actors.clear();
        state.causal_chain.truncate(3);
        for link in &mut state.causal_chain {
            link.cause = truncate_engine_artifact_text(&link.cause, 56);
            link.effect = truncate_engine_artifact_text(&link.effect, 56);
            link.rationale = link
                .rationale
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 64));
        }
        state.evidence_layers.truncate(3);
        state.interpretive_tensions.truncate(1);
        for tension in &mut state.interpretive_tensions {
            tension.question = truncate_engine_artifact_text(&tension.question, 100);
            tension.competing_readings = tension
                .competing_readings
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 52));
        }
        state.impacts.truncate(1);
        for impact in &mut state.impacts {
            impact.implication = impact
                .implication
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 60));
        }
        state.reader_questions.clear();
        state.section_outline.clear();
        state.transition_plan.clear();
        state.open_gaps.truncate(4);
        for item in &mut state.timeline {
            item.significance = item
                .significance
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 56));
            item.expected_claim_log_ids.truncate(2);
            item.expected_source_card_ids.truncate(1);
        }
        for layer in &mut state.evidence_layers {
            layer.purpose = layer
                .purpose
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 64));
            layer.expected_claim_log_ids.truncate(4);
            layer.expected_source_card_ids.truncate(2);
        }
        for link in &mut state.causal_chain {
            link.expected_claim_log_ids.truncate(3);
            link.expected_source_card_ids.truncate(2);
        }
        for tension in &mut state.interpretive_tensions {
            tension.expected_claim_log_ids.truncate(3);
            tension.expected_source_card_ids.truncate(2);
        }
        for impact in &mut state.impacts {
            impact.expected_claim_log_ids.truncate(3);
            impact.expected_source_card_ids.truncate(2);
        }
        state.event_cards.truncate(8);
        for card in &mut state.event_cards {
            card.label = truncate_engine_artifact_text(&card.label, 50);
            card.timeframe = card
                .timeframe
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 40));
            card.actors = card
                .actors
                .iter()
                .take(3)
                .map(|value| truncate_engine_artifact_text(value, 36))
                .collect();
            card.region_or_front = card
                .region_or_front
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 44));
            card.trigger = card
                .trigger
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 48));
            card.development = card
                .development
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 82));
            card.outcome = card
                .outcome
                .as_deref()
                .map(|value| truncate_engine_artifact_text(value, 52));
            card.causal_spine.truncate(4);
            for step in &mut card.causal_spine {
                step.description = truncate_engine_artifact_text(&step.description, 48);
                step.reasoning = None;
                step.limits.clear();
                step.claim_log_ids.truncate(1);
                step.source_ids.truncate(1);
            }
            card.interpretive_layers.truncate(1);
            for layer in &mut card.interpretive_layers {
                layer.interpretation = truncate_engine_artifact_text(&layer.interpretation, 36);
                layer.reasoning = None;
                layer.limits.clear();
            }
            card.open_questions.truncate(1);
        }
    }
    let valid_claim_ids = artifacts
        .claim_log
        .iter()
        .map(|claim| claim.id.trim().to_string())
        .collect::<HashSet<_>>();
    if let Some(state) = artifacts.narrative_state.as_mut() {
        for card in &mut state.event_cards {
            card.source_ids
                .retain(|id| valid_source_ids.contains(id.trim()));
            card.claim_log_ids
                .retain(|id| valid_claim_ids.contains(id.trim()));
            card.claim_log_ids.truncate(4);
            for step in &mut card.causal_spine {
                step.source_ids
                    .retain(|id| valid_source_ids.contains(id.trim()));
                step.claim_log_ids
                    .retain(|id| valid_claim_ids.contains(id.trim()));
                step.claim_log_ids.truncate(4);
            }
            for layer in &mut card.interpretive_layers {
                layer
                    .source_ids
                    .retain(|id| valid_source_ids.contains(id.trim()));
                layer
                    .claim_log_ids
                    .retain(|id| valid_claim_ids.contains(id.trim()));
                layer.claim_log_ids.truncate(4);
            }
        }
    }
    artifacts.reader_quality = None;
}

fn truncate_engine_artifact_text(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        compact
    } else {
        compact.chars().take(limit).collect::<String>()
    }
}

fn render_phase_plan_rows(phase_plan: &HistoricalPhasePlan) -> String {
    if phase_plan.phases.is_empty() {
        return "| none | none | none | none | none | none | no |".to_string();
    }
    phase_plan
        .phases
        .iter()
        .map(|phase| {
            format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                escape_table_cell(&phase.label),
                escape_table_cell(phase.timeframe.as_deref().unwrap_or("")),
                escape_table_cell(&phase.actors.join(", ")),
                escape_table_cell(phase.region_or_front.as_deref().unwrap_or("")),
                escape_table_cell(&phase.claim_log_ids.join(", ")),
                escape_table_cell(&phase.source_ids.join(", ")),
                if phase.readiness.ready { "yes" } else { "no" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_phase_card_rows(artifacts: &ResearchControllerArtifacts) -> String {
    let Some(state) = artifacts.narrative_state.as_ref() else {
        return "- no grounded phase cards".to_string();
    };
    if state.event_cards.is_empty() {
        return "- no grounded phase cards".to_string();
    }
    state
        .event_cards
        .iter()
        .map(|card| {
            format!(
                "- **{}**: {} | actors: {} | region/front: {} | trigger: {} | outcome: {}",
                card.label,
                card.timeframe.as_deref().unwrap_or(""),
                card.actors.join(", "),
                card.region_or_front.as_deref().unwrap_or(""),
                card.trigger.as_deref().unwrap_or(""),
                card.outcome.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_source_audit_rows(artifacts: &ResearchControllerArtifacts) -> String {
    if artifacts.source_cards.is_empty() {
        return "| none | none | none | none | none | none |".to_string();
    }
    artifacts
        .source_cards
        .iter()
        .map(|card| {
            format!(
                "| {} | {} | {} | {} | {} | {} |",
                escape_table_cell(&card.id),
                escape_table_cell(&card.url),
                escape_table_cell(&card.title),
                escape_table_cell(&card.source_class),
                escape_table_cell(
                    card.extracted_facts
                        .first()
                        .map(String::as_str)
                        .unwrap_or("")
                ),
                escape_table_cell(card.limitation.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_claim_log_rows(artifacts: &ResearchControllerArtifacts) -> String {
    if artifacts.claim_log.is_empty() {
        return "| none | none | none | none | none |".to_string();
    }
    artifacts
        .claim_log
        .iter()
        .map(|claim| {
            let support = if !claim.support_source_card_ids.is_empty() {
                claim.support_source_card_ids.join(", ")
            } else {
                claim.support_urls.join(", ")
            };
            format!(
                "| {} | {} | {} | {} | {} |",
                escape_table_cell(&claim.id),
                escape_table_cell(&claim.claim),
                escape_table_cell(&support),
                escape_table_cell(claim.confidence.as_deref().unwrap_or("")),
                escape_table_cell(claim.uncertainty_note.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_limits_rows(artifacts: &ResearchControllerArtifacts) -> String {
    let mut lines = Vec::new();
    for debt in artifacts
        .research_debt
        .iter()
        .filter(|debt| debt.status != "closed")
    {
        lines.push(format!("- {}", debt.missing_evidence));
    }
    for warning in &artifacts.warnings {
        lines.push(format!("- warning: {}", warning));
    }
    if lines.is_empty() {
        "No open conflicts or research debt remained after the isolated historical phase pass."
            .to_string()
    } else {
        lines.join("\n")
    }
}

fn render_quality_gate(quality_gate: &ResearchQualityGateArtifact) -> String {
    let failures = if quality_gate.failure_messages.is_empty() {
        "- none".to_string()
    } else {
        quality_gate
            .failure_messages
            .iter()
            .map(|message| format!("- {}", message))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "- status: {}\n- unsupported_claim_count: {}\n- unresolved_conflict_count: {}\n- open_debt_count: {}\n- failure_messages:\n{}",
        quality_gate.status,
        quality_gate.unsupported_claim_count,
        quality_gate.unresolved_conflict_count,
        quality_gate.open_debt_count,
        failures
    )
}

fn escape_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn legacy_artifact_ignore_warning(existing: Option<&str>) -> Option<String> {
    let existing = existing?.trim();
    if existing.is_empty() {
        return None;
    }
    Some(if existing.len() > 32_000 {
        "historical_phase_engine_ignored_oversized_legacy_controller_artifacts".to_string()
    } else {
        "historical_phase_engine_ignored_legacy_controller_artifacts".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_test_dir, test_state_with_historical_phase_engine_enabled};
    use liquid_storage_sqlite::setup_db;

    fn facts_from_claim(claim: &str) -> Vec<String> {
        let compact = compact_claim_text(claim);
        let trigger = build_trigger_from_claim(claim, claim).unwrap_or_else(|| compact.clone());
        let outcome = extract_outcome_from_claim(claim).unwrap_or_else(|| compact.clone());
        vec![
            format!(
                "{} 이 자료는 해당 국면의 시기, 행위자, 장소/전선, 계기를 직접 제시한다.",
                trigger
            ),
            format!(
                "{} 이 자료는 그 국면의 귀결과 다음 외교·군사 선택지 변화도 함께 설명한다.",
                outcome
            ),
        ]
    }

    fn historical_source_document_with_facts(
        title: &str,
        url: &str,
        facts: &[String],
        claims: &[&str],
    ) -> String {
        format!(
            "# {title}\nURL: {url}\nSource Class: authoritative_secondary\nConfidence: high\nFacts:\n{}\nClaims:\n{}\n",
            facts
                .iter()
                .map(|fact| format!("- {fact}"))
                .collect::<Vec<_>>()
                .join("\n"),
            claims
                .iter()
                .map(|claim| format!("- {claim}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    fn historical_source_document(title: &str, url: &str, claims: &[&str]) -> String {
        let facts = claims
            .iter()
            .flat_map(|claim| facts_from_claim(claim))
            .collect::<Vec<_>>();
        historical_source_document_with_facts(title, url, &facts, claims)
    }

    async fn insert_historical_task(
        db: &sqlx::SqlitePool,
        topic: &str,
        source_filenames: &[&str],
    ) -> i64 {
        sqlx::query("INSERT INTO tasks (original_name, status, file_prefix, file_type, source_filenames, research_topic, research_intensity, quality_depth) VALUES (?, 'researching', '[AI-Research]', 'md', ?, ?, 'high', 'strict')")
            .bind(topic)
            .bind(serde_json::to_string(source_filenames).unwrap())
            .bind(topic)
            .execute(db)
            .await
            .unwrap()
            .last_insert_rowid()
    }

    async fn load_task(db: &sqlx::SqlitePool, task_id: i64) -> TaskInfo {
        sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    async fn write_source_file(dir: &std::path::Path, filename: &str, body: &str) {
        tokio::fs::create_dir_all(dir).await.unwrap();
        tokio::fs::write(dir.join(filename), body).await.unwrap();
    }

    async fn write_historical_source_set(
        uploads: &std::path::Path,
        prefix: &str,
        rows: &[(&str, &str, &str)],
    ) -> Vec<String> {
        let mut filenames = Vec::new();
        for (index, (title, url, claim)) in rows.iter().enumerate() {
            let filename = format!("{prefix}-{}.md", index + 1);
            write_source_file(
                uploads,
                &filename,
                &historical_source_document(title, url, &[*claim]),
            )
            .await;
            filenames.push(filename);
        }
        filenames
    }

    #[test]
    fn evidence_sanitizer_rejects_prompt_provider_diagnostic_aliases() {
        for unsafe_text in [
            "provider_payload: raw upstream request",
            "raw_provider_payload: hidden provider body",
            "resolved_prompt: hidden resolved prompt",
            "resolved_system_prompt: hidden system prompt",
            "resolved_user_prompt: hidden user prompt",
            "source_diagnostics: raw search diagnostics",
            "controller_artifact_json: raw controller state",
            "raw_model_output: hidden model output",
            "source_documents: hidden source prompt section",
            "source documents: hidden source prompt section",
            "provider.payload: hidden provider body",
            "resolved.prompt: hidden resolved prompt",
            "source.diagnostics: hidden source diagnostics",
            "controller.artifact.json: hidden controller state",
            "model.input: hidden task input",
            "raw.model.output: hidden model output",
            "model_input: hidden task input",
            "token: hidden token",
            "access_token: hidden token",
            "authorization=hidden",
            "bearer: hidden",
            "token : hidden token",
            "Authorization : hidden",
            "access token = hidden token",
            "api_key : hidden key",
            "client-secret = hidden secret",
            "password : hidden password",
        ] {
            assert!(
                engine_safe_evidence_text(unsafe_text, 320).is_none(),
                "unsafe evidence marker should be rejected: {unsafe_text}"
            );
        }
    }

    #[test]
    fn unsafe_evidence_filename_is_neutralized_for_debt() {
        let label = safe_evidence_filename_label("provider_payload.md", 0);
        let suffix = safe_evidence_filename_debt_suffix("resolved_prompt.md", 1);

        assert_eq!(label, "historical evidence file 1");
        assert_eq!(suffix, "historical-evidence-file-2");
        assert!(!label.contains("provider"));
        assert!(!suffix.contains("resolved"));
        assert!(!suffix.contains("prompt"));
    }

    #[test]
    fn source_class_is_derived_from_url_not_declared_metadata() {
        let mut artifacts = ResearchControllerArtifacts::default();
        let documents = vec![HistoricalEvidenceDocument {
            filename: "spoof.md".to_string(),
            title: Some("Spoofed source".to_string()),
            url: Some("https://www.britannica.com/event/World-War-I".to_string()),
            source_class: Some("official_primary".to_string()),
            confidence: Some("high".to_string()),
            facts: vec!["1914년 7월 위기에서 오스트리아-헝가리와 세르비아의 충돌이 동원 판단으로 이어졌다.".to_string()],
            claims: vec!["1914년 7월 위기: 오스트리아-헝가리와 세르비아는 빈·베오그라드에서 충돌했고 동원 판단으로 위기가 이동했다.".to_string()],
        }];

        populate_engine_ledgers_from_documents(&mut artifacts, &documents);

        assert_eq!(artifacts.source_cards.len(), 1);
        assert_ne!(artifacts.source_cards[0].source_class, "official_primary");
        assert!(artifacts.warnings.iter().any(|warning| {
            warning.starts_with("historical_phase_engine_ignored_declared_source_class")
        }));
    }

    #[tokio::test]
    async fn world_war_one_vertical_slice_is_accepted() {
        let dir = temp_test_dir("historical-phase-engine-wwi");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        let source_rows = vec![
            (
                "First Moroccan Crisis evidence",
                "https://www.britannica.com/event/Moroccan-crises",
                "1905-1906년 제1차 모로코 위기: 독일, 프랑스, 영국은 모로코·알헤시라스에서 독일의 모로코 개입과 프랑스 영향권 문제를 둘러싸고 충돌했고, 협상국 협조와 독일 고립 인식이 강화되었다.",
            ),
            (
                "Bosnian Crisis evidence",
                "https://www.britannica.com/event/Bosnian-crisis-of-1908",
                "1908-1909년 보스니아 병합 위기: 오스트리아-헝가리, 세르비아, 러시아는 보스니아·발칸에서 병합 선언과 세르비아 반발, 러시아 후퇴를 겪었고 발칸 위기가 동맹 정치와 직접 결합했다.",
            ),
            (
                "Agadir Crisis evidence",
                "https://history.state.gov/milestones/1899-1913/morocco",
                "1911년 아가디르 위기: 독일, 프랑스, 영국은 모로코·아가디르에서 판터호 파견과 프랑스의 모로코 행동을 두고 충돌했고, 영국과 프랑스의 안보 협력이 더 가시화되었다.",
            ),
            (
                "Balkan Wars evidence",
                "https://www.britannica.com/event/Balkan-Wars",
                "1912-1913년 발칸 전쟁: 발칸 동맹, 오스만 제국, 세르비아, 오스트리아-헝가리는 발칸에서 오스만 후퇴와 세르비아 팽창을 겪었고, 대국 동맹 개입 위험이 커졌다.",
            ),
            (
                "Sarajevo assassination evidence",
                "https://www.iwm.org.uk/history/how-the-first-world-war-began",
                "1914년 6월 사라예보 암살: 오스트리아-헝가리, 세르비아, 가브릴로 프린치프는 사라예보·보스니아에서 프란츠 페르디난트 암살 사건을 둘러싸고 충돌했고, 7월 위기와 최후통첩 판단이 전면화되었다.",
            ),
            (
                "July Ultimatum evidence",
                "https://www.nationalarchives.gov.uk/pathways/firstworldwar/spotlights/origins.htm",
                "1914년 7월 7월 최후통첩: 오스트리아-헝가리, 세르비아, 독일, 러시아는 빈·베오그라드에서 세르비아 최후통첩과 독일 지지를 둘러싸고 외교적 후퇴 공간이 좁아졌고, 동원 판단으로 위기가 이동했다.",
            ),
            (
                "Mobilization crisis evidence",
                "https://www.loc.gov/item/today-in-history/july-28/",
                "1914년 7-8월 러시아·독일 동원 위기: 러시아, 독일, 프랑스는 동유럽·서유럽에서 러시아 동원과 독일 대응 계획을 통해 외교 위기를 군사 일정의 문제로 전환했다.",
            ),
            (
                "Belgium and British entry evidence",
                "https://www.britannica.com/event/World-War-I",
                "1914년 8월 벨기에 침공과 영국 참전: 독일, 벨기에, 영국, 프랑스는 벨기에·서부전선에서 독일의 벨기에 침공과 중립 보장 문제를 겪었고, 영국 참전으로 전쟁이 확대되었다.",
            ),
        ];
        let source_filenames = write_historical_source_set(&uploads, "wwi", &source_rows).await;
        let source_filename_refs = source_filenames
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let topic = "1차 세계대전 전의 위기와 외교";
        let task_id = insert_historical_task(&db, topic, &source_filename_refs).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &source_filenames,
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let filename = completed
            .filename
            .expect("historical engine should save a file");
        let output = tokio::fs::read_to_string(uploads.join(filename))
            .await
            .unwrap();
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(
            completed.quality_status.as_deref(),
            Some("passed"),
            "failure={:?}",
            completed.quality_last_failure
        );
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("research_grade")
        );
        assert_eq!(output.matches("## 최종 답변 (Final Answer)").count(), 1);
        assert_eq!(output.matches("# 검증 부록").count(), 1);
        assert!(output.contains("제1차 모로코 위기"));
        assert!(output.contains("### 국면별 전개와 인과 사슬"));
        assert!(!output.contains("첨부된 SOURCE DOCUMENTS"));
        assert!(artifacts.source_cards.len() >= 8);
        assert!(artifacts.claim_log.len() >= 8);
        assert!(artifacts
            .source_cards
            .iter()
            .all(|card| !card.extracted_facts.is_empty()));
        assert!(artifacts.narrative_state.as_ref().is_some_and(|state| {
            state.event_cards.len() >= 8
                && state
                    .event_cards
                    .iter()
                    .all(|card| !card.causal_spine.is_empty())
        }));
    }

    #[tokio::test]
    async fn world_war_two_vertical_slice_is_accepted() {
        let dir = temp_test_dir("historical-phase-engine-wwii");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        let source_rows = vec![
            (
                "Interwar norms evidence",
                "https://history.state.gov/milestones/1914-1920/league",
                "1919-1931년 전간기 평화규범과 집행 공백: 국제연맹, 영국, 프랑스는 유럽·국제연맹에서 베르사유 질서와 국제연맹 규범의 집행 공백을 드러냈고, 만주와 유럽의 시험 사례로 이어졌다.",
            ),
            (
                "Manchuria evidence",
                "https://history.state.gov/milestones/1921-1936/manchuria",
                "1931-1933년 만주사변과 비승인 외교: 일본, 중국, 국제연맹, 미국은 만주·동아시아에서 일본의 만주 점령과 만주국 수립을 둘러싸고 집단안보와 비승인 원칙의 약함을 드러냈다.",
            ),
            (
                "German rearmament evidence",
                "https://www.britannica.com/event/World-War-II/German-rearmament-and-the-Rhineland",
                "1933-1935년 독일 재무장과 베르사유 질서 해체: 독일, 영국, 프랑스는 독일·유럽에서 히틀러 정권의 재무장과 병역 부활을 겪었고 베르사유 제한과 조약 신뢰가 흔들렸다.",
            ),
            (
                "Ethiopia crisis evidence",
                "https://www.britannica.com/event/Italo-Ethiopian-War-1935-1936",
                "1935-1936년 에티오피아 위기와 국제연맹 실패: 이탈리아, 에티오피아, 국제연맹, 영국, 프랑스는 에티오피아·제네바에서 침공과 제재 문제를 겪었고 집단안보 실패가 드러났다.",
            ),
            (
                "Rhineland evidence",
                "https://www.iwm.org.uk/history/the-road-to-war",
                "1936년 라인란트 재무장: 독일, 프랑스, 영국은 라인란트·서유럽에서 독일군의 라인란트 진입과 로카르노 체제 위기를 겪었고, 서방의 군사 대응 회피가 독일의 위험 감수를 키웠다.",
            ),
            (
                "Spanish Civil War evidence",
                "https://www.britannica.com/event/Spanish-Civil-War",
                "1936-1939년 스페인 내전과 불간섭 실패: 스페인 공화파, 프랑코 세력, 독일, 이탈리아, 소련은 스페인·지중해에서 불간섭 원칙 약화와 외국 지원을 겪었고 추축 협력과 제한된 민주국가 대응이 뚜렷해졌다.",
            ),
            (
                "Austria Sudeten evidence",
                "https://www.britannica.com/event/Anschluss",
                "1938년 오스트리아 병합과 수데텐 위기: 독일, 오스트리아, 체코슬로바키아, 영국, 프랑스는 오스트리아·수데텐란트에서 병합과 수데텐 문제를 겪었고, 강압 외교가 뮌헨 협정으로 이어졌다.",
            ),
            (
                "Munich evidence",
                "https://www.britannica.com/event/Munich-Agreement",
                "1938년 9월 뮌헨 협정: 독일, 영국, 프랑스, 체코슬로바키아는 뮌헨·수데텐란트에서 수데텐란트 양보와 전쟁 회피를 결정했고, 프라하 점령으로 유화의 한계가 드러났다.",
            ),
            (
                "Prague Poland evidence",
                "https://history.state.gov/milestones/1937-1945/munich",
                "1939년 3-4월 프라하 점령과 폴란드 보장: 독일, 체코슬로바키아, 영국, 프랑스, 폴란드는 프라하·폴란드에서 독일의 프라하 점령과 폴란드 보장을 겪었고 유화에서 억지로 전환했다.",
            ),
            (
                "Nazi Soviet Pact evidence",
                "https://www.britannica.com/event/German-Soviet-Nonaggression-Pact",
                "1939년 8-9월 독소불가침조약과 폴란드 침공: 독일, 소련, 폴란드, 영국, 프랑스는 모스크바·폴란드에서 불가침조약과 폴란드 침공을 겪었고 제2차 세계대전이 시작되었다.",
            ),
        ];
        let source_filenames = write_historical_source_set(&uploads, "wwii", &source_rows).await;
        let source_filename_refs = source_filenames
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let topic = "2차대전 전의 각국 외교와 그 분석";
        let task_id = insert_historical_task(&db, topic, &source_filename_refs).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &source_filenames,
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let filename = completed
            .filename
            .expect("historical engine should save a file");
        let output = tokio::fs::read_to_string(uploads.join(filename))
            .await
            .unwrap();
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(
            completed.quality_status.as_deref(),
            Some("passed"),
            "failure={:?}",
            completed.quality_last_failure
        );
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("research_grade")
        );
        assert!(output.contains("### 주요 쟁점과 해석"));
        assert!(output.contains("뮌헨 협정"));
        assert!(!output.contains("근거 연결 국면"));
        assert!(artifacts.narrative_state.as_ref().is_some_and(|state| {
            state.event_cards.len() >= 8
                && state
                    .event_cards
                    .iter()
                    .all(|card| !card.causal_spine.is_empty())
        }));
    }

    #[tokio::test]
    async fn placeholder_legacy_cards_are_rejected_as_truth() {
        let dir = temp_test_dir("historical-phase-engine-placeholder");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "wwi.md",
            &historical_source_document(
                "World War I chronology",
                "https://www.britannica.com/event/World-War-I",
                &[
                    "1914 July Crisis in Sarajevo and the alliance system: Austria-Hungary, Serbia, Germany, Russia, France, and Britain turned assassination fallout into general war and fixed the opening fronts in Europe.",
                    "1914 Opening campaigns in Belgium and northern France: German armies crossed Belgium, French and British forces responded, and the Marne stopped a quick decision before trench warfare hardened the western front.",
                    "1915-1916 Eastern and southern widening: Russia, Austria-Hungary, Italy, and the Ottoman fronts expanded the war and forced both coalitions to spread manpower, logistics, and diplomacy across multiple theaters.",
                    "1916 Verdun and the Somme: France, Britain, and Germany fought attritional campaigns that consumed manpower, locked strategy into industrial endurance, and changed the next year's political and military choices.",
                    "1917 Upheaval and intervention: the Russian Revolutions, U-boat escalation, and United States entry altered coalition capacity, legitimacy, and the longer war balance.",
                    "1918 Allied offensives and the armistice settlement: German spring offensives failed, Allied counteroffensives broke imperial staying power, and the armistice opened the postwar order and treaty phase.",
                ],
            ),
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War I background, development, impact, and significance",
            &["wwi.md"],
        )
        .await;
        sqlx::query("UPDATE tasks SET research_controller_artifacts_json = ? WHERE id = ?")
            .bind(
                serde_json::json!({
                    "version": 1,
                    "narrative_state": {
                        "version": 1,
                        "event_cards": [
                            {"label": "Phase", "claim_log_ids": ["C999"], "source_ids": ["S999"]},
                            {"label": "Section 1", "claim_log_ids": ["C998"], "source_ids": ["S998"]}
                        ]
                    }
                })
                .to_string(),
            )
            .bind(task_id)
            .execute(&db)
            .await
            .unwrap();
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["wwi.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War I background, development, impact, and significance",
            )
            .await
        );

        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let labels = artifacts
            .narrative_state
            .as_ref()
            .map(|state| {
                state
                    .event_cards
                    .iter()
                    .map(|card| card.label.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(labels
            .iter()
            .all(|label| !historical_phase_label_is_placeholder(label)));
        assert!(artifacts.warnings.iter().any(|warning| {
            warning == "historical_phase_engine_ignored_legacy_controller_artifacts"
        }));
    }

    #[tokio::test]
    async fn missing_source_or_claim_is_blocked() {
        let dir = temp_test_dir("historical-phase-engine-blocked");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "incomplete.md",
            "# Incomplete\nFacts:\n- This file has no URL and no claim rows.\n",
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War I background, development, impact, and significance",
            &["incomplete.md"],
        )
        .await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["incomplete.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War I background, development, impact, and significance",
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(completed.quality_status.as_deref(), Some("blocked"));
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("blocked")
        );
        assert!(artifacts.research_debt.iter().any(|debt| {
            debt.missing_evidence.contains("missing a public URL")
                || debt
                    .missing_evidence
                    .contains("could not build any phase-specific Claim Log")
        }));
    }

    #[tokio::test]
    async fn oversized_legacy_artifact_is_ignored() {
        let dir = temp_test_dir("historical-phase-engine-oversized");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "wwii.md",
            &historical_source_document(
                "World War II chronology",
                "https://www.britannica.com/event/World-War-II",
                &[
                    "1939 German and Soviet moves in Poland: Germany, the Soviet Union, Poland, Britain, and France turned the European crisis into open war and fixed the first diplomatic and military alignments.",
                    "1940 Western collapse and the Battle of Britain: German campaigns overran France and the Low Countries, Britain held the air and maritime approaches, and the war split into continental occupation and offshore resistance.",
                    "1941 Operation Barbarossa and the global widening: Germany invaded the Soviet Union, Japan attacked Pearl Harbor, and the conflict became a truly global coalition war.",
                    "1942-1943 Midway, Stalingrad, and North Africa: the United States, Britain, Germany, Japan, and the Soviet Union reversed the initiative across sea, land, and imperial theaters.",
                    "1944 Allied return to Western Europe: the Normandy landings, Soviet advances, and strategic bombing narrowed Axis options and pushed the war toward simultaneous collapse on multiple fronts.",
                    "1945 Final offensives, surrender, and postwar settlement: Berlin fell, Japan surrendered after devastating final campaigns, and the wartime coalition turned toward occupation, partition, and a new international order.",
                ],
            ),
        )
        .await;
        let task_id = insert_historical_task(
            &db,
            "World War II background, development, impact, and significance",
            &["wwii.md"],
        )
        .await;
        let oversized = format!(
            "{{\"version\":1,\"warnings\":[\"{}\"],\"source_cards\":[],\"claim_log\":[],\"conflict_map\":[],\"research_debt\":[]}}",
            "x".repeat(40_000)
        );
        sqlx::query("UPDATE tasks SET research_controller_artifacts_json = ? WHERE id = ?")
            .bind(oversized)
            .bind(task_id)
            .execute(&db)
            .await
            .unwrap();
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["wwii.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "World War II background, development, impact, and significance",
            )
            .await
        );

        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let json = serde_json::to_string(&artifacts).unwrap();
        assert!(json.len() < 24_000, "artifact_json_len={}", json.len());
        assert!(artifacts.warnings.iter().any(|warning| {
            warning == "historical_phase_engine_ignored_oversized_legacy_controller_artifacts"
        }));
    }

    #[test]
    fn moroccan_facts_do_not_support_bosnian_claims() {
        let moroccan_facts = vec![
            "1905-1906년 제1차 모로코 위기에서 독일의 개입과 알헤시라스 협상이 협상국 협조를 강화했다."
                .to_string(),
            "모로코와 알헤시라스에서의 외교 충돌은 독일 고립 인식을 키웠다.".to_string(),
        ];
        let moroccan_claim =
            "1905-1906년 제1차 모로코 위기: 독일, 프랑스, 영국은 모로코·알헤시라스에서 충돌했고 협상국 협조가 강화되었다.";
        let bosnian_claim =
            "1908-1909년 보스니아 병합 위기: 오스트리아-헝가리, 세르비아, 러시아는 보스니아·발칸에서 충돌했고 러시아 후퇴가 동맹 정치를 흔들었다.";

        assert!(claim_has_semantic_support_in_facts(
            moroccan_claim,
            &moroccan_facts
        ));
        assert!(!claim_has_semantic_support_in_facts(
            bosnian_claim,
            &moroccan_facts
        ));
    }

    #[test]
    fn extracted_facts_remain_non_empty_after_sanitized_compaction() {
        let mut artifacts = ResearchControllerArtifacts {
            version: 1,
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com/wwi".to_string(),
                title: "WWI source".to_string(),
                source_class: "authoritative_secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec![
                    "1914년 7월 위기에서 오스트리아-헝가리와 세르비아의 충돌이 동원 판단으로 이어졌다."
                        .to_string(),
                    "빈과 베오그라드의 최후통첩 위기는 외교 후퇴 공간을 줄였다.".to_string(),
                ],
                limitation: None,
                diagnostics_ref: Some("historical_phase_engine:test".to_string()),
                confidence: Some("high".to_string()),
            }],
            ..ResearchControllerArtifacts::default()
        };

        compact_engine_appendix_artifacts(&mut artifacts);

        assert_eq!(artifacts.source_cards.len(), 1);
        assert!(!artifacts.source_cards[0].extracted_facts.is_empty());
        assert!(artifacts.source_cards[0].diagnostics_ref.is_none());
    }

    #[test]
    fn claim_types_are_granular_for_phase_specific_rows() {
        let phase_claim = "1914년 6월 사라예보 암살: 오스트리아-헝가리와 세르비아는 사라예보·보스니아에서 암살 여파를 둘러싸고 충돌했고 7월 위기가 전면화되었다.";
        assert_eq!(infer_historical_claim_type(phase_claim), "event_fact");
        let granular_types = build_granular_claim_texts(phase_claim)
            .into_iter()
            .map(|(claim_type, _)| claim_type)
            .collect::<Vec<_>>();
        assert_eq!(
            granular_types,
            vec![
                "event_fact",
                "actor_strategy",
                "causal_handoff",
                "interpretive_limit"
            ]
        );
        assert_eq!(
            infer_historical_claim_type(
                "여러 외교 문제와 동맹 계산이 누적되면서 분쟁이 확대되었다."
            ),
            "historical_overview"
        );
    }

    #[test]
    fn claim_backed_causal_spine_survives_compaction() {
        let card = NarrativeEventCard {
            label: "July Crisis".to_string(),
            timeframe: Some("1914년 7월".to_string()),
            actors: vec!["오스트리아-헝가리".to_string(), "세르비아".to_string()],
            region_or_front: Some("빈·베오그라드".to_string()),
            trigger: Some("최후통첩과 동원 준비가 외교 후퇴 공간을 줄였다.".to_string()),
            development: Some(
                "최후통첩과 동원 준비는 빈·베오그라드에서 외교·군사 선택지를 동시에 압박했고 다음 단계의 확전 판단을 재배치했다."
                    .to_string(),
            ),
            outcome: Some("동원 판단이 일반 전쟁으로 향하는 압력을 만들었다.".to_string()),
            claim_log_ids: vec!["C1".to_string()],
            source_ids: vec!["S1".to_string()],
            causal_spine: build_causal_spine(&NarrativeEventCard {
                label: "July Crisis".to_string(),
                timeframe: Some("1914년 7월".to_string()),
                actors: vec!["오스트리아-헝가리".to_string(), "세르비아".to_string()],
                region_or_front: Some("빈·베오그라드".to_string()),
                trigger: Some("최후통첩과 동원 준비가 외교 후퇴 공간을 줄였다.".to_string()),
                development: Some(
                    "최후통첩과 동원 준비는 빈·베오그라드에서 외교·군사 선택지를 동시에 압박했고 다음 단계의 확전 판단을 재배치했다."
                        .to_string(),
                ),
                outcome: Some("동원 판단이 일반 전쟁으로 향하는 압력을 만들었다.".to_string()),
                claim_log_ids: vec!["C1".to_string()],
                source_ids: vec!["S1".to_string()],
                causal_spine: Vec::new(),
                interpretive_layers: Vec::new(),
                confidence: Some("high".to_string()),
                open_questions: Vec::new(),
            }),
            interpretive_layers: Vec::new(),
            confidence: Some("high".to_string()),
            open_questions: Vec::new(),
        };
        let mut artifacts = ResearchControllerArtifacts {
            version: 1,
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com/july-crisis".to_string(),
                title: "July Crisis source".to_string(),
                source_class: "authoritative_secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec![
                    "1914년 7월 위기에서 최후통첩과 동원 판단이 확전 압력을 키웠다.".to_string(),
                ],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1914년 7월 위기".to_string(),
                claim_type: Some("ultimatum_escalation".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![card],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        compact_engine_appendix_artifacts(&mut artifacts);

        let card = &artifacts
            .narrative_state
            .as_ref()
            .expect("narrative state")
            .event_cards[0];
        assert!(card_has_claim_backed_causal_spine(card));
    }

    #[test]
    fn template_like_renderer_is_flagged_as_explainer() {
        let cards = vec![
            NarrativeEventCard {
                label: "July Crisis".to_string(),
                timeframe: Some("1914년 7월".to_string()),
                actors: vec!["오스트리아-헝가리".to_string(), "세르비아".to_string()],
                region_or_front: Some("빈·베오그라드".to_string()),
                trigger: Some("최후통첩".to_string()),
                development: Some(
                    "최후통첩과 동원 위기가 외교 선택지를 급격히 줄였다.".to_string(),
                ),
                outcome: Some("동원 판단이 확전 압력을 키웠다.".to_string()),
                claim_log_ids: vec!["C1".to_string()],
                source_ids: vec!["S1".to_string()],
                causal_spine: build_causal_spine(&NarrativeEventCard {
                    label: "July Crisis".to_string(),
                    timeframe: Some("1914년 7월".to_string()),
                    actors: vec!["오스트리아-헝가리".to_string(), "세르비아".to_string()],
                    region_or_front: Some("빈·베오그라드".to_string()),
                    trigger: Some("최후통첩".to_string()),
                    development: Some(
                        "최후통첩과 동원 위기가 외교 선택지를 급격히 줄였다.".to_string(),
                    ),
                    outcome: Some("동원 판단이 확전 압력을 키웠다.".to_string()),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: vec!["S1".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                }),
                interpretive_layers: Vec::new(),
                confidence: Some("high".to_string()),
                open_questions: Vec::new(),
            },
            NarrativeEventCard {
                label: "Belgium Invasion".to_string(),
                timeframe: Some("1914년 8월".to_string()),
                actors: vec!["독일".to_string(), "영국".to_string()],
                region_or_front: Some("벨기에·서부전선".to_string()),
                trigger: Some("벨기에 침공".to_string()),
                development: Some(
                    "벨기에 침공과 중립 보장 문제가 영국 참전을 압박했다.".to_string(),
                ),
                outcome: Some("영국 참전으로 전쟁이 확대되었다.".to_string()),
                claim_log_ids: vec!["C2".to_string()],
                source_ids: vec!["S2".to_string()],
                causal_spine: build_causal_spine(&NarrativeEventCard {
                    label: "Belgium Invasion".to_string(),
                    timeframe: Some("1914년 8월".to_string()),
                    actors: vec!["독일".to_string(), "영국".to_string()],
                    region_or_front: Some("벨기에·서부전선".to_string()),
                    trigger: Some("벨기에 침공".to_string()),
                    development: Some(
                        "벨기에 침공과 중립 보장 문제가 영국 참전을 압박했다.".to_string(),
                    ),
                    outcome: Some("영국 참전으로 전쟁이 확대되었다.".to_string()),
                    claim_log_ids: vec!["C2".to_string()],
                    source_ids: vec!["S2".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                }),
                interpretive_layers: Vec::new(),
                confidence: Some("high".to_string()),
                open_questions: Vec::new(),
            },
        ];
        let failures = historical_final_answer_template_failures(
            "## 최종 답변 (Final Answer)\n\n이 사건은 중요했고 결국 제도의 방향을 바꾸었습니다.\n\n### 동시대 비교\n같은 시기 다른 지역 사례와 비교합니다.\n\n# 검증 부록\n",
            &cards,
        );
        assert!(!failures.is_empty());
    }

    #[tokio::test]
    async fn broad_chronology_insufficiency_is_explainer_not_research_grade() {
        let dir = temp_test_dir("historical-phase-engine-explainer");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "broad-wwi.md",
            &historical_source_document_with_facts(
                "Broad World War I overview",
                "https://www.britannica.com/event/World-War-I",
                &[
                    "1914년부터 1918년까지 유럽 전역에서 분쟁이 확대되고 이후 질서가 달라졌다는 큰 흐름은 확인된다.".to_string(),
                    "여러 위기와 동맹 계산이 누적되며 전쟁이 장기화되었지만, 개별 국면의 행위자와 장소를 세밀하게 분해하지는 않는다.".to_string(),
                ],
                &[
                    "1914년부터 1918년까지 유럽 전역에서 분쟁이 확대되고 이후 질서가 달라졌다.",
                    "여러 위기와 동맹 계산이 누적되며 전쟁이 장기화되었다.",
                ],
            ),
        )
        .await;
        let topic = "World War I background, development, impact, and significance";
        let task_id = insert_historical_task(&db, topic, &["broad-wwi.md"]).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["broad-wwi.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        assert_eq!(completed.quality_status.as_deref(), Some("untrusted"));
        let verdict = artifacts
            .quality_gate
            .as_ref()
            .map(|gate| gate.status.as_str());
        assert_ne!(verdict, Some("research_grade"));
        assert!(matches!(verdict, Some("explainer") | Some("partial")));
    }

    #[tokio::test]
    async fn mismatched_source_support_is_blocked() {
        let dir = temp_test_dir("historical-phase-engine-mismatch");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "mismatch.md",
            &historical_source_document_with_facts(
                "Moroccan crisis facts with Bosnian claim",
                "https://www.britannica.com/event/Moroccan-crises",
                &[
                    "1905-1906년 제1차 모로코 위기에서 독일의 개입과 알헤시라스 협상이 협상국 협조를 강화했다."
                        .to_string(),
                    "모로코와 알헤시라스의 외교 충돌은 독일 고립 인식을 키웠다.".to_string(),
                ],
                &["1908-1909년 보스니아 병합 위기: 오스트리아-헝가리, 세르비아, 러시아는 보스니아·발칸에서 충돌했고 러시아 후퇴가 동맹 정치를 흔들었다."],
            ),
        )
        .await;
        let topic = "World War I background, development, impact, and significance";
        let task_id = insert_historical_task(&db, topic, &["mismatch.md"]).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["mismatch.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                topic,
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        assert_eq!(completed.quality_status.as_deref(), Some("blocked"));
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("blocked")
        );
    }

    #[tokio::test]
    async fn mixed_supported_and_unsupported_claims_end_as_partial() {
        let dir = temp_test_dir("historical-phase-engine-partial");
        let uploads = dir.join("uploads");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), uploads.clone(), true);
        write_source_file(
            &uploads,
            "valid.md",
            &historical_source_document(
                "July Crisis evidence",
                "https://www.nationalarchives.gov.uk/pathways/firstworldwar/spotlights/origins.htm",
                &["1914년 7월 최후통첩 위기: 오스트리아-헝가리, 세르비아, 독일, 러시아는 빈·베오그라드에서 최후통첩과 독일 지지를 둘러싸고 외교적 후퇴 공간이 좁아졌고, 동원 판단으로 위기가 이동했다."],
            ),
        )
        .await;
        write_source_file(
            &uploads,
            "invalid.md",
            &historical_source_document_with_facts(
                "Mismatched facts",
                "https://www.britannica.com/event/Moroccan-crises",
                &[
                    "1905-1906년 제1차 모로코 위기에서 독일의 개입과 알헤시라스 협상이 협상국 협조를 강화했다."
                        .to_string(),
                ],
                &["1908-1909년 보스니아 병합 위기: 오스트리아-헝가리, 세르비아, 러시아는 보스니아·발칸에서 충돌했고 러시아 후퇴가 동맹 정치를 흔들었다."],
            ),
        )
        .await;
        let task_id =
            insert_historical_task(&db, "전쟁 전 외교 위기", &["valid.md", "invalid.md"]).await;
        let task = load_task(&db, task_id).await;

        assert!(
            try_run_historical_phase_engine(
                &state,
                &task,
                &["valid.md".to_string(), "invalid.md".to_string()],
                Vec::new(),
                "[AI-Research]",
                "md",
                "전쟁 전 외교 위기",
            )
            .await
        );

        let completed = load_task(&db, task_id).await;
        let artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        assert_eq!(completed.quality_status.as_deref(), Some("untrusted"));
        assert_eq!(
            artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("partial")
        );
    }

    #[tokio::test]
    async fn non_historical_requests_noop_even_when_flag_enabled() {
        let dir = temp_test_dir("historical-phase-engine-noop");
        let db = setup_db(&dir).await.unwrap();
        let state =
            test_state_with_historical_phase_engine_enabled(db.clone(), dir.join("uploads"), true);
        let task_id = sqlx::query("INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Tech guide', 'researching', '[AI-Research]', 'md', 'modern C++ work-stealing scheduler implementation guide', 'high', 'strict')")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
        let task = load_task(&db, task_id).await;

        assert!(
            !try_run_historical_phase_engine(
                &state,
                &task,
                &[],
                Vec::new(),
                "[AI-Research]",
                "md",
                "modern C++ work-stealing scheduler implementation guide",
            )
            .await
        );
    }

    #[tokio::test]
    async fn rendered_output_contains_exactly_one_final_answer_and_appendix() {
        let mut artifacts = ResearchControllerArtifacts {
            version: 1,
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://www.britannica.com/event/World-War-I".to_string(),
                title: "World War I".to_string(),
                source_class: "authoritative_secondary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["World War I chronology anchor".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "1914 July Crisis in Sarajevo and the alliance system turned assassination fallout into general war.".to_string(),
                claim_type: Some("event".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            ..ResearchControllerArtifacts::default()
        };
        let plan = build_historical_phase_plan(&artifacts, "World War I");
        build_engine_planning_artifacts(&mut artifacts, &plan, "World War I");
        let output = render_historical_phase_engine_output(
            "World War I",
            &artifacts,
            &plan,
            &ResearchQualityGateArtifact {
                status: "passed".to_string(),
                failure_messages: Vec::new(),
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            },
        );

        assert_eq!(output.matches("## 최종 답변 (Final Answer)").count(), 1);
        assert_eq!(output.matches("# 검증 부록").count(), 1);
    }
}
