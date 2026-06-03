use super::*;
use liquid_protocol::{
    NarrativeCausalLink, NarrativeCausalSpineStep, NarrativeEventCard, NarrativeInterpretiveLayer,
};
use liquid_research_classic::{
    historical_event_card_enrichment_applies, historical_phase_state_should_run,
    select_evidence_ready_weak_historical_event_cards,
};
use liquid_research_core::normalize_public_evidence_url;
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::application::scraping::{is_blocked_ip, is_blocked_ipv4, parse_ipv4_style_host};

const ARTIFACT_STABILIZATION_WORK_ITEM_ID: &str = "artifact_stabilization";
const SOURCE_CARD_REPAIR_WORK_ITEM_ID: &str = "source_card_repair";
const CLAIM_LOG_REPAIR_WORK_ITEM_ID: &str = "claim_log_repair";
const PHASE_PLAN_BUILD_WORK_ITEM_ID: &str = "phase_plan_build";
const PHASE_PLAN_REVIEW_WORK_ITEM_ID: &str = "phase_plan_review";
const PHASE_CLAIM_READINESS_WORK_ITEM_ID: &str = "phase_claim_readiness";
const EVENT_CARD_ENRICHMENT_WORK_ITEM_ID: &str = "event_card_enrichment";
const CAUSAL_CONTINUITY_REVIEW_WORK_ITEM_ID: &str = "causal_continuity_review";
const FINAL_ANSWER_RENDER_WORK_ITEM_ID: &str = "final_answer_render";
const RESEARCH_ACCEPTANCE_REVIEW_WORK_ITEM_ID: &str = "research_acceptance_review";

const MAX_QUEUE_CHECKPOINTS: usize = 24;
const DEFAULT_MAX_WORK_ITEMS_PER_WAVE: usize = 3;
const DEFAULT_MAX_ATTEMPTS_PER_WAVE: usize = 2;
const DEFAULT_MAX_ATTEMPTS_PER_WORK_ITEM: i64 = 2;
const DEFAULT_MAX_NO_PROGRESS_WAVES: i64 = 2;

#[derive(Debug, Default)]
struct QueueDebtIndex {
    ids_by_kind: HashMap<ResearchWorkItemKind, Vec<String>>,
    created_from_by_kind: HashMap<ResearchWorkItemKind, Vec<String>>,
    next_action_by_kind: HashMap<ResearchWorkItemKind, String>,
}

#[derive(Debug, Clone)]
struct WorkItemDraft {
    id: &'static str,
    kind: ResearchWorkItemKind,
    target: &'static str,
    blocking: bool,
    applicable: bool,
    needs_attention: bool,
    needs_run: bool,
    max_attempts: i64,
    debt_ids: Vec<String>,
    dependencies: Vec<String>,
    phase_ids: Vec<String>,
    claim_log_ids: Vec<String>,
    source_card_ids: Vec<String>,
    created_from: Vec<String>,
    next_action: Option<String>,
    detail: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_bounded_research_work_queue(
    state: &AppState,
    task: &TaskInfo,
    runtime: &dyn ModelRuntime,
    model_name: &str,
    source: &str,
    file_prefix: &str,
    file_type: &str,
    normalized_output: &str,
    user_prompt: &str,
    iteration: i64,
    max_iterations: i64,
    controller_events: &mut Vec<ResearchControllerEvent>,
) -> ResearchIterationState {
    let mut artifacts = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    let mut iteration_state =
        refresh_research_iteration_state(task, file_prefix, user_prompt, &artifacts);
    if iteration_state.work_items.is_empty() && iteration_state.checkpoints.is_empty() {
        return iteration_state;
    }
    persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state).await;

    loop {
        if has_failed_work_item(&iteration_state) {
            iteration_state.terminal_status = Some(ResearchRunTerminalStatus::Failed);
            iteration_state.summary = Some(queue_state_summary(&iteration_state));
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }

        let pending_ids = iteration_state
            .work_items
            .iter()
            .filter(|item| item.status == ResearchWorkItemStatus::Pending)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        if pending_ids.is_empty() {
            iteration_state.terminal_status =
                Some(terminal_status_without_pending(&iteration_state));
            iteration_state.summary = Some(queue_state_summary(&iteration_state));
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }

        let mut budget = iteration_state
            .budget
            .clone()
            .unwrap_or_else(|| research_run_budget_for(task));
        if budget.waves_used >= budget.max_waves
            || budget.model_calls_used >= budget.max_model_calls
            || budget.attempts_used >= budget.max_total_work_item_attempts
        {
            iteration_state.terminal_status = Some(ResearchRunTerminalStatus::BudgetExhausted);
            iteration_state.budget = Some(budget);
            iteration_state.summary = Some(queue_state_summary(&iteration_state));
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }

        budget.waves_used += 1;
        iteration_state.current_wave = budget.waves_used;
        iteration_state.max_waves = budget.max_waves;
        iteration_state.budget = Some(budget.clone());

        let before_signature = research_artifact_progress_signature(&artifacts);
        let runnable_ids = pending_ids
            .into_iter()
            .take(
                budget
                    .max_work_items_per_wave
                    .min(budget.max_attempts_per_wave.max(1)),
            )
            .collect::<Vec<_>>();
        let mut wave_checkpoint_statuses = Vec::new();

        for item_id in runnable_ids {
            if budget.attempts_used >= budget.max_total_work_item_attempts
                || budget.model_calls_used >= budget.max_model_calls
            {
                break;
            }

            mark_work_item_running(&mut iteration_state, &item_id);
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;

            let Some(work_item) = iteration_state
                .work_items
                .iter()
                .find(|item| item.id == item_id)
                .cloned()
            else {
                continue;
            };
            let stage_name = routed_stage_for_kind(&work_item.kind)
                .unwrap_or(RESEARCH_STAGE_NARRATIVE_ENRICHMENT);
            let queue_detail = work_queue_running_detail(&iteration_state, &item_id);
            update_research_controller_progress(
                state,
                task.id,
                &task.original_name,
                stage_name,
                iteration,
                max_iterations,
                RESEARCH_CONTROLLER_STATUS_RUNNING,
                Some(&queue_detail),
                controller_events,
            )
            .await;

            let input_fingerprint = work_item.input_fingerprint.clone().unwrap_or_else(|| {
                work_item_fingerprint(
                    &work_item.kind,
                    &artifacts,
                    &work_item.phase_ids,
                    &work_item.claim_log_ids,
                    &work_item.source_card_ids,
                    &work_item.debt_ids,
                )
            });
            let remaining_model_calls =
                (budget.max_model_calls - budget.model_calls_used).max(0) as usize;
            let (status, detail, next_action, model_calls_used) = execute_routed_work_item(
                state,
                task,
                runtime,
                model_name,
                source,
                file_prefix,
                file_type,
                normalized_output,
                user_prompt,
                iteration,
                max_iterations,
                controller_events,
                &work_item.kind,
                &work_item.debt_ids,
                remaining_model_calls,
            )
            .await;

            artifacts = load_task_research_artifacts(state, task.id)
                .await
                .unwrap_or_default();
            budget.work_items_run += 1;
            budget.attempts_used += 1;
            budget.model_calls_used += model_calls_used;
            iteration_state.budget = Some(budget.clone());
            let output_fingerprint = work_item_fingerprint(
                &work_item.kind,
                &artifacts,
                &work_item.phase_ids,
                &work_item.claim_log_ids,
                &work_item.source_card_ids,
                &work_item.debt_ids,
            );
            finalize_work_item_run(
                &mut iteration_state,
                &item_id,
                status.clone(),
                detail.clone(),
                input_fingerprint.clone(),
                output_fingerprint.clone(),
                next_action,
            );
            append_work_checkpoint(
                &mut iteration_state,
                &artifacts,
                &item_id,
                work_item.kind,
                status.clone(),
                detail,
                input_fingerprint,
                output_fingerprint,
            );
            wave_checkpoint_statuses.push(status);
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
        }

        artifacts = load_task_research_artifacts(state, task.id)
            .await
            .unwrap_or_default();
        let after_signature = research_artifact_progress_signature(&artifacts);
        if before_signature == after_signature {
            budget.no_progress_waves += 1;
        } else {
            budget.no_progress_waves = 0;
        }
        iteration_state.budget = Some(budget.clone());
        iteration_state =
            refresh_research_iteration_state(task, file_prefix, user_prompt, &artifacts)
                .with_budget_and_history(&iteration_state, budget.clone());
        iteration_state.summary = Some(queue_state_summary(&iteration_state));

        if has_failed_work_item(&iteration_state) {
            iteration_state.terminal_status = Some(ResearchRunTerminalStatus::Failed);
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }
        if budget.no_progress_waves >= budget.max_no_progress_waves {
            iteration_state.terminal_status = Some(terminal_status_for_stalled_wave(
                &iteration_state,
                &wave_checkpoint_statuses,
            ));
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }
        if (budget.attempts_used >= budget.max_total_work_item_attempts
            || budget.model_calls_used >= budget.max_model_calls)
            && iteration_state
                .work_items
                .iter()
                .any(|item| item.status == ResearchWorkItemStatus::Pending)
        {
            iteration_state.terminal_status = Some(ResearchRunTerminalStatus::BudgetExhausted);
            persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state)
                .await;
            return iteration_state;
        }

        persist_research_iteration_state(state, task.id, &mut artifacts, &iteration_state).await;
    }
}

trait ResearchIterationStateBudgetExt {
    fn with_budget_and_history(
        self,
        previous: &ResearchIterationState,
        budget: ResearchRunBudget,
    ) -> ResearchIterationState;
}

impl ResearchIterationStateBudgetExt for ResearchIterationState {
    fn with_budget_and_history(
        mut self,
        previous: &ResearchIterationState,
        budget: ResearchRunBudget,
    ) -> ResearchIterationState {
        self.current_wave = previous.current_wave;
        self.max_waves = budget.max_waves;
        self.budget = Some(budget);
        self.checkpoints = previous.checkpoints.clone();
        self.terminal_status = None;
        self
    }
}

fn refresh_research_iteration_state(
    task: &TaskInfo,
    file_prefix: &str,
    user_prompt: &str,
    artifacts: &ResearchControllerArtifacts,
) -> ResearchIterationState {
    let existing = artifacts.research_iteration_state.as_ref();
    let prior_items = existing
        .map(|state| {
            state
                .work_items
                .iter()
                .map(|item| (item.id.clone(), item.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let budget = existing
        .and_then(|state| state.budget.clone())
        .unwrap_or_else(|| research_run_budget_for(task));

    let evidence_subject = research_source_subject_for_task(task, user_prompt);
    let phase_applicable = historical_phase_state_should_run(
        file_prefix,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(evidence_subject),
    );
    let narrative_applicable = historical_event_card_enrichment_applies(
        file_prefix,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(evidence_subject),
        artifacts,
    );
    let historical_context = phase_applicable
        || narrative_applicable
        || artifacts
            .research_debt
            .iter()
            .any(research_debt_is_historical_queue_relevant)
        || existing
            .is_some_and(|state| !state.work_items.is_empty() || !state.checkpoints.is_empty());

    if !historical_context {
        return ResearchIterationState {
            current_wave: existing.map(|state| state.current_wave).unwrap_or(0),
            max_waves: budget.max_waves,
            terminal_status: None,
            summary: None,
            budget: Some(budget),
            work_items: Vec::new(),
            checkpoints: existing
                .map(|state| state.checkpoints.clone())
                .unwrap_or_default(),
        };
    }

    let debt_index = build_queue_debt_index(&artifacts.research_debt);
    let phase_ids = collect_phase_ids(artifacts);
    let phase_claim_log_ids = collect_phase_claim_log_ids(artifacts);
    let phase_source_card_ids = collect_phase_source_card_ids(artifacts);
    let weak_ready_card_count =
        select_evidence_ready_weak_historical_event_cards(artifacts, 8).len();
    let narrative_readiness_blocked = artifacts.source_cards.is_empty()
        || artifacts.claim_log.is_empty()
        || artifacts
            .narrative_state
            .as_ref()
            .is_none_or(|state| state.event_cards.is_empty());
    let phase_card_count = artifacts
        .narrative_state
        .as_ref()
        .map(|state| state.event_cards.len())
        .unwrap_or(0);
    let ready_phase_card_count = count_ready_phase_cards(artifacts);

    let mut drafts = Vec::new();
    let mut add_draft = |draft: WorkItemDraft| {
        if draft.applicable || draft.needs_attention || prior_items.contains_key(draft.id) {
            drafts.push(draft);
        }
    };

    let phase_plan_debt_ids = debt_ids_for_kinds(
        &debt_index,
        &[
            ResearchWorkItemKind::PhasePlanBuild,
            ResearchWorkItemKind::PhasePlanReview,
            ResearchWorkItemKind::PhaseClaimReadiness,
        ],
    );
    let event_enrichment_debt_ids =
        debt_ids_for_kind(&debt_index, ResearchWorkItemKind::EventCardEnrichment);
    let causal_review_debt_ids =
        debt_ids_for_kind(&debt_index, ResearchWorkItemKind::CausalContinuityReview);

    add_draft(WorkItemDraft {
        id: ARTIFACT_STABILIZATION_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::ArtifactStabilization,
        target: "research_controller_artifacts_json",
        blocking: true,
        applicable: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ArtifactStabilization)
            .is_empty(),
        needs_attention: !debt_ids_for_kind(
            &debt_index,
            ResearchWorkItemKind::ArtifactStabilization,
        )
        .is_empty(),
        needs_run: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ArtifactStabilization)
            .is_empty(),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ArtifactStabilization),
        dependencies: Vec::new(),
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::ArtifactStabilization,
            vec![],
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::ArtifactStabilization,
            "Stabilize compact controller artifacts before continuing the historical queue.",
        ),
        detail: format!(
            "artifact_debt={} warnings={}",
            debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ArtifactStabilization).len(),
            artifacts.warnings.len()
        ),
    });

    let source_card_repair_debt_ids =
        debt_ids_for_kind(&debt_index, ResearchWorkItemKind::SourceCardRepair);
    let source_card_repair_needed =
        artifacts.source_cards.is_empty() || !source_card_repair_debt_ids.is_empty();
    add_draft(WorkItemDraft {
        id: SOURCE_CARD_REPAIR_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::SourceCardRepair,
        target: "source_cards",
        blocking: true,
        applicable: source_card_repair_needed,
        needs_attention: source_card_repair_needed,
        needs_run: source_card_repair_needed,
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: source_card_repair_debt_ids.clone(),
        dependencies: Vec::new(),
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::SourceCardRepair,
            if artifacts.source_cards.is_empty() {
                vec!["queue_readiness:no_source_cards".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::SourceCardRepair,
            "Repair Source Cards before phase review or event-card enrichment.",
        ),
        detail: format!(
            "source_cards={} debt={}",
            artifacts.source_cards.len(),
            source_card_repair_debt_ids.len()
        ),
    });

    let claim_log_repair_debt_ids =
        debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ClaimLogRepair);
    let claim_log_repair_needed =
        artifacts.claim_log.is_empty() || !claim_log_repair_debt_ids.is_empty();
    add_draft(WorkItemDraft {
        id: CLAIM_LOG_REPAIR_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::ClaimLogRepair,
        target: "claim_log",
        blocking: true,
        applicable: claim_log_repair_needed,
        needs_attention: claim_log_repair_needed,
        needs_run: claim_log_repair_needed,
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: claim_log_repair_debt_ids.clone(),
        dependencies: Vec::new(),
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::ClaimLogRepair,
            if artifacts.claim_log.is_empty() {
                vec!["queue_readiness:no_claim_log".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::ClaimLogRepair,
            "Repair phase-specific supported Claim Log rows before event-card enrichment.",
        ),
        detail: format!(
            "claim_log={} debt={}",
            artifacts.claim_log.len(),
            claim_log_repair_debt_ids.len()
        ),
    });

    add_draft(WorkItemDraft {
        id: PHASE_PLAN_BUILD_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::PhasePlanBuild,
        target: "narrative_state.event_cards",
        blocking: true,
        applicable: phase_applicable || !phase_plan_debt_ids.is_empty(),
        needs_attention: phase_card_count == 0 || !phase_plan_debt_ids.is_empty(),
        needs_run: phase_applicable && (phase_card_count == 0 || !phase_plan_debt_ids.is_empty()),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: phase_plan_debt_ids.clone(),
        dependencies: Vec::new(),
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhasePlanBuild,
            if phase_applicable {
                vec!["historical_phase_state_applicable".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhasePlanBuild,
            "Construct grounded historical phase cards from existing Source Cards and Claim Log rows.",
        ),
        detail: format!(
            "applicable={} event_cards={} ready_cards={} debt={}",
            phase_applicable,
            phase_card_count,
            ready_phase_card_count,
            phase_plan_debt_ids.len()
        ),
    });

    add_draft(WorkItemDraft {
        id: PHASE_PLAN_REVIEW_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::PhasePlanReview,
        target: "narrative_state.section_outline",
        blocking: true,
        applicable: phase_applicable || !phase_plan_debt_ids.is_empty(),
        needs_attention: phase_card_count == 0 || !phase_plan_debt_ids.is_empty(),
        needs_run: phase_applicable && (phase_card_count == 0 || !phase_plan_debt_ids.is_empty()),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: debt_ids_for_kind(&debt_index, ResearchWorkItemKind::PhasePlanReview),
        dependencies: vec![PHASE_PLAN_BUILD_WORK_ITEM_ID.to_string()],
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhasePlanReview,
            if phase_applicable {
                vec!["historical_phase_state_applicable".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhasePlanReview,
            "Review phase scaffold coverage, placeholder removal, and phase segmentation before acceptance.",
        ),
        detail: format!(
            "phase_ids={} event_cards={} debt={}",
            phase_ids.len(),
            phase_card_count,
            debt_ids_for_kind(&debt_index, ResearchWorkItemKind::PhasePlanReview).len()
        ),
    });

    add_draft(WorkItemDraft {
        id: PHASE_CLAIM_READINESS_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::PhaseClaimReadiness,
        target: "narrative_state.event_cards.claim_log_ids",
        blocking: true,
        applicable: phase_applicable || !phase_plan_debt_ids.is_empty(),
        needs_attention: ready_phase_card_count < phase_card_count || !phase_plan_debt_ids.is_empty(),
        needs_run: phase_applicable
            && (ready_phase_card_count < phase_card_count || !phase_plan_debt_ids.is_empty()),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: debt_ids_for_kind(&debt_index, ResearchWorkItemKind::PhaseClaimReadiness),
        dependencies: vec![PHASE_PLAN_BUILD_WORK_ITEM_ID.to_string()],
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhaseClaimReadiness,
            if phase_applicable {
                vec!["historical_phase_state_applicable".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::PhaseClaimReadiness,
            "Ground each phase with phase-specific Claim Log and Source Card support before enrichment.",
        ),
        detail: format!(
            "ready_cards={} total_cards={} debt={}",
            ready_phase_card_count,
            phase_card_count,
            debt_ids_for_kind(&debt_index, ResearchWorkItemKind::PhaseClaimReadiness).len()
        ),
    });

    add_draft(WorkItemDraft {
        id: EVENT_CARD_ENRICHMENT_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::EventCardEnrichment,
        target: "narrative_state.event_cards",
        blocking: true,
        applicable: narrative_applicable
            || !event_enrichment_debt_ids.is_empty()
            || weak_ready_card_count > 0,
        needs_attention: narrative_readiness_blocked
            || !event_enrichment_debt_ids.is_empty()
            || weak_ready_card_count > 0,
        needs_run: narrative_applicable
            && !narrative_readiness_blocked
            && (!event_enrichment_debt_ids.is_empty() || weak_ready_card_count > 0),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: event_enrichment_debt_ids.clone(),
        dependencies: vec![
            PHASE_PLAN_BUILD_WORK_ITEM_ID.to_string(),
            SOURCE_CARD_REPAIR_WORK_ITEM_ID.to_string(),
            CLAIM_LOG_REPAIR_WORK_ITEM_ID.to_string(),
        ],
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::EventCardEnrichment,
            if narrative_applicable {
                vec!["historical_event_card_enrichment_applicable".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::EventCardEnrichment,
            "Enrich weak grounded event cards without adding new evidence or new IDs.",
        ),
        detail: format!(
            "applicable={} readiness_blocked={} weak_ready_cards={} debt={}",
            narrative_applicable,
            narrative_readiness_blocked,
            weak_ready_card_count,
            event_enrichment_debt_ids.len()
        ),
    });

    add_draft(WorkItemDraft {
        id: CAUSAL_CONTINUITY_REVIEW_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::CausalContinuityReview,
        target: "narrative_state.causal_chain",
        blocking: true,
        applicable: narrative_applicable
            || !causal_review_debt_ids.is_empty()
            || weak_ready_card_count > 0,
        needs_attention: !has_grounded_causal_depth(artifacts)
            || !causal_review_debt_ids.is_empty(),
        needs_run: narrative_applicable
            && (!has_grounded_causal_depth(artifacts) || !causal_review_debt_ids.is_empty()),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: causal_review_debt_ids.clone(),
        dependencies: vec![EVENT_CARD_ENRICHMENT_WORK_ITEM_ID.to_string()],
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::CausalContinuityReview,
            if narrative_applicable {
                vec!["historical_event_card_enrichment_applicable".to_string()]
            } else {
                Vec::new()
            },
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::CausalContinuityReview,
            "Review causal continuity and interpretive coverage after event-card enrichment.",
        ),
        detail: format!(
            "causal_depth={} debt={}",
            has_grounded_causal_depth(artifacts),
            causal_review_debt_ids.len()
        ),
    });

    add_draft(WorkItemDraft {
        id: FINAL_ANSWER_RENDER_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::FinalAnswerRender,
        target: "final_answer",
        blocking: true,
        applicable: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::FinalAnswerRender)
            .is_empty(),
        needs_attention: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::FinalAnswerRender)
            .is_empty(),
        needs_run: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::FinalAnswerRender)
            .is_empty(),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: debt_ids_for_kind(&debt_index, ResearchWorkItemKind::FinalAnswerRender),
        dependencies: vec![EVENT_CARD_ENRICHMENT_WORK_ITEM_ID.to_string()],
        phase_ids: phase_ids.clone(),
        claim_log_ids: phase_claim_log_ids.clone(),
        source_card_ids: phase_source_card_ids.clone(),
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::FinalAnswerRender,
            vec![],
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::FinalAnswerRender,
            "Regenerate reader-facing final output from persisted artifacts after repairs land.",
        ),
        detail: format!(
            "debt={}",
            debt_ids_for_kind(&debt_index, ResearchWorkItemKind::FinalAnswerRender).len()
        ),
    });

    add_draft(WorkItemDraft {
        id: RESEARCH_ACCEPTANCE_REVIEW_WORK_ITEM_ID,
        kind: ResearchWorkItemKind::ResearchAcceptanceReview,
        target: "quality_gate",
        blocking: true,
        applicable: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ResearchAcceptanceReview)
            .is_empty(),
        needs_attention: !debt_ids_for_kind(
            &debt_index,
            ResearchWorkItemKind::ResearchAcceptanceReview,
        )
        .is_empty(),
        needs_run: !debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ResearchAcceptanceReview)
            .is_empty(),
        max_attempts: budget.max_attempts_per_work_item.max(1),
        debt_ids: debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ResearchAcceptanceReview),
        dependencies: vec![FINAL_ANSWER_RENDER_WORK_ITEM_ID.to_string()],
        phase_ids,
        claim_log_ids: phase_claim_log_ids,
        source_card_ids: phase_source_card_ids,
        created_from: created_from_for_kind(
            &debt_index,
            ResearchWorkItemKind::ResearchAcceptanceReview,
            vec![],
        ),
        next_action: next_action_for_kind(
            &debt_index,
            ResearchWorkItemKind::ResearchAcceptanceReview,
            "Re-run acceptance checks after artifact and final-answer repair work completes.",
        ),
        detail: format!(
            "debt={}",
            debt_ids_for_kind(&debt_index, ResearchWorkItemKind::ResearchAcceptanceReview).len()
        ),
    });

    let mut work_items = Vec::new();
    for draft in drafts {
        let previous = prior_items.get(draft.id);
        let dependency_blocked = draft.dependencies.iter().any(|dependency_id| {
            work_items
                .iter()
                .find(|item: &&ResearchWorkItem| item.id == *dependency_id)
                .map(|item| item.status != ResearchWorkItemStatus::Completed)
                .unwrap_or(false)
        });
        work_items.push(realize_work_item(
            draft,
            previous,
            artifacts,
            dependency_blocked,
        ));
    }

    ResearchIterationState {
        current_wave: existing.map(|state| state.current_wave).unwrap_or(0),
        max_waves: budget.max_waves,
        terminal_status: None,
        summary: None,
        budget: Some(budget),
        work_items,
        checkpoints: existing
            .map(|state| state.checkpoints.clone())
            .unwrap_or_default(),
    }
}

fn realize_work_item(
    draft: WorkItemDraft,
    previous: Option<&ResearchWorkItem>,
    artifacts: &ResearchControllerArtifacts,
    dependency_blocked: bool,
) -> ResearchWorkItem {
    let attempt_count = previous.map(|item| item.attempt_count).unwrap_or(0);
    let mut created_from = draft.created_from;
    if dependency_blocked {
        created_from.push("dependency_blocked".to_string());
    }
    let input_fingerprint = work_item_fingerprint(
        &draft.kind,
        artifacts,
        &draft.phase_ids,
        &draft.claim_log_ids,
        &draft.source_card_ids,
        &draft.debt_ids,
    );
    let previously_completed_same_input = previous.is_some_and(|item| {
        item.status == ResearchWorkItemStatus::Completed
            && item
                .input_fingerprint
                .as_deref()
                .is_some_and(|fingerprint| fingerprint == input_fingerprint)
    });
    let attempt_budget_exhausted = draft.max_attempts > 0 && attempt_count >= draft.max_attempts;
    let status = if previously_completed_same_input {
        ResearchWorkItemStatus::Completed
    } else if draft.needs_run && !dependency_blocked {
        if draft.max_attempts > 0 && attempt_count >= draft.max_attempts {
            ResearchWorkItemStatus::Blocked
        } else {
            ResearchWorkItemStatus::Pending
        }
    } else if draft.needs_attention || dependency_blocked {
        ResearchWorkItemStatus::Blocked
    } else {
        ResearchWorkItemStatus::Completed
    };
    let last_error = match status {
        ResearchWorkItemStatus::Blocked | ResearchWorkItemStatus::Failed => {
            Some(compact_detail(if attempt_budget_exhausted {
                "work item attempt budget exhausted"
            } else {
                &draft.detail
            }))
        }
        _ => previous.and_then(|item| item.last_error.clone()),
    };
    let output_fingerprint = match status {
        ResearchWorkItemStatus::Completed => previous
            .and_then(|item| item.output_fingerprint.clone())
            .or_else(|| Some(input_fingerprint.clone())),
        _ => previous.and_then(|item| item.output_fingerprint.clone()),
    };

    ResearchWorkItem {
        id: draft.id.to_string(),
        kind: draft.kind,
        status,
        target: Some(draft.target.to_string()),
        blocking: draft.blocking,
        debt_ids: draft.debt_ids,
        dependencies: draft.dependencies,
        phase_ids: draft.phase_ids,
        claim_log_ids: draft.claim_log_ids,
        source_card_ids: draft.source_card_ids,
        created_from: unique_compact_values(created_from),
        max_attempts: draft.max_attempts,
        attempt_count,
        last_wave: previous.and_then(|item| item.last_wave),
        last_error,
        next_action: draft.next_action.map(|action| compact_detail(&action)),
        input_fingerprint: Some(input_fingerprint),
        output_fingerprint,
        detail: Some(compact_detail(&draft.detail)),
    }
}

pub(super) async fn mark_research_work_queue_accepted(state: &AppState, task_id: i64) {
    let Some(mut artifacts) = load_task_research_artifacts(state, task_id).await else {
        return;
    };
    let Some(mut iteration_state) = artifacts.research_iteration_state.clone() else {
        return;
    };
    for item in &mut iteration_state.work_items {
        if item.status == ResearchWorkItemStatus::Pending
            || item.status == ResearchWorkItemStatus::Running
            || item.status == ResearchWorkItemStatus::Blocked
        {
            item.status = ResearchWorkItemStatus::Skipped;
            item.last_error = None;
            item.detail = Some(compact_detail("superseded_by_quality_gate_acceptance"));
            item.next_action = None;
            item.output_fingerprint = item
                .output_fingerprint
                .clone()
                .or_else(|| item.input_fingerprint.clone());
        }
    }
    iteration_state.terminal_status = Some(ResearchRunTerminalStatus::Accepted);
    iteration_state.summary = Some(queue_state_summary(&iteration_state));
    persist_research_iteration_state(state, task_id, &mut artifacts, &iteration_state).await;
}

pub(super) async fn mark_research_work_queue_budget_exhausted(state: &AppState, task_id: i64) {
    let Some(mut artifacts) = load_task_research_artifacts(state, task_id).await else {
        return;
    };
    let Some(mut iteration_state) = artifacts.research_iteration_state.clone() else {
        return;
    };
    if iteration_state.terminal_status != Some(ResearchRunTerminalStatus::Accepted) {
        iteration_state.terminal_status = Some(ResearchRunTerminalStatus::BudgetExhausted);
        iteration_state.summary = Some(queue_state_summary(&iteration_state));
        persist_research_iteration_state(state, task_id, &mut artifacts, &iteration_state).await;
    }
}

fn build_queue_debt_index(debts: &[ResearchDebtItem]) -> QueueDebtIndex {
    let mut index = QueueDebtIndex::default();

    for debt in debts.iter().filter(|debt| debt.status != "closed") {
        let mapped_kinds = mapped_work_item_kinds_for_debt(debt);
        let safe_debt_id = sanitize_queue_id(&debt.id);
        let created_from = format!("research_debt:{safe_debt_id}");
        for kind in mapped_kinds {
            index
                .ids_by_kind
                .entry(kind.clone())
                .or_default()
                .push(safe_debt_id.clone());
            index
                .created_from_by_kind
                .entry(kind.clone())
                .or_default()
                .push(created_from.clone());
            if let Some(action) = debt.next_check_actions.first() {
                index
                    .next_action_by_kind
                    .entry(kind)
                    .or_insert_with(|| compact_detail(action));
            }
        }
    }

    for ids in index.ids_by_kind.values_mut() {
        *ids = unique_safe_ids(ids.clone());
    }
    for created_from in index.created_from_by_kind.values_mut() {
        *created_from = unique_compact_values(created_from.clone());
    }
    index
}

fn mapped_work_item_kinds_for_debt(debt: &ResearchDebtItem) -> Vec<ResearchWorkItemKind> {
    let gate = debt_gate_label(debt);
    let missing = debt.missing_evidence.to_ascii_lowercase();
    let next_actions = debt.next_check_actions.join(" ").to_ascii_lowercase();
    let mut kinds = Vec::new();

    if gate.contains("artifact_parse") || gate.contains("artifact_quality") {
        kinds.push(ResearchWorkItemKind::ArtifactStabilization);
    }
    if gate.contains("historical_phase_state") {
        kinds.push(ResearchWorkItemKind::PhasePlanBuild);
        kinds.push(ResearchWorkItemKind::PhaseClaimReadiness);
    }
    if gate.contains("narrative_planning") {
        kinds.push(ResearchWorkItemKind::PhasePlanReview);
        kinds.push(ResearchWorkItemKind::CausalContinuityReview);
    }
    if gate.contains("narrative_enrichment_readiness") {
        if missing.contains("source")
            || next_actions.contains("source card")
            || debt.id.contains("source")
        {
            kinds.push(ResearchWorkItemKind::SourceCardRepair);
        }
        if missing.contains("claim")
            || next_actions.contains("claim log")
            || debt.id.contains("claim")
        {
            kinds.push(ResearchWorkItemKind::ClaimLogRepair);
        }
        kinds.push(ResearchWorkItemKind::PhaseClaimReadiness);
    }
    if gate.contains("event_card_enrichment") {
        kinds.push(ResearchWorkItemKind::EventCardEnrichment);
        kinds.push(ResearchWorkItemKind::CausalContinuityReview);
    }
    if gate.contains("historical_richness") {
        kinds.push(ResearchWorkItemKind::CausalContinuityReview);
        kinds.push(ResearchWorkItemKind::ResearchAcceptanceReview);
    }
    if gate.contains("quality_gate") {
        kinds.push(ResearchWorkItemKind::FinalAnswerRender);
        kinds.push(ResearchWorkItemKind::ResearchAcceptanceReview);
    }
    if missing.contains("source card") && !kinds.contains(&ResearchWorkItemKind::SourceCardRepair) {
        kinds.push(ResearchWorkItemKind::SourceCardRepair);
    }
    if missing.contains("claim log") && !kinds.contains(&ResearchWorkItemKind::ClaimLogRepair) {
        kinds.push(ResearchWorkItemKind::ClaimLogRepair);
    }
    unique_kinds(kinds)
}

fn debt_ids_for_kind(debt_index: &QueueDebtIndex, kind: ResearchWorkItemKind) -> Vec<String> {
    debt_index
        .ids_by_kind
        .get(&kind)
        .cloned()
        .unwrap_or_default()
}

fn debt_ids_for_kinds(debt_index: &QueueDebtIndex, kinds: &[ResearchWorkItemKind]) -> Vec<String> {
    let mut ids = Vec::new();
    for kind in kinds {
        ids.extend(debt_ids_for_kind(debt_index, kind.clone()));
    }
    unique_compact_values(ids)
}

fn created_from_for_kind(
    debt_index: &QueueDebtIndex,
    kind: ResearchWorkItemKind,
    mut created_from: Vec<String>,
) -> Vec<String> {
    if let Some(existing) = debt_index.created_from_by_kind.get(&kind) {
        created_from.extend(existing.clone());
    }
    unique_compact_values(created_from)
}

fn next_action_for_kind(
    debt_index: &QueueDebtIndex,
    kind: ResearchWorkItemKind,
    fallback: &str,
) -> Option<String> {
    Some(
        debt_index
            .next_action_by_kind
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| compact_detail(fallback)),
    )
}

fn research_debt_is_historical_queue_relevant(debt: &ResearchDebtItem) -> bool {
    let gate = debt_gate_label(debt);
    gate.contains("historical_phase_state")
        || gate.contains("narrative_enrichment_readiness")
        || gate.contains("event_card_enrichment")
        || gate.contains("historical_richness")
        || gate.contains("narrative_planning")
}

fn research_run_budget_for(task: &TaskInfo) -> ResearchRunBudget {
    let high_rigor = task.research_intensity.as_deref() == Some("high")
        || task.quality_depth.as_deref() == Some("strict");
    let max_waves = if high_rigor { 3 } else { 2 };
    let max_work_items_per_wave = DEFAULT_MAX_WORK_ITEMS_PER_WAVE;
    let max_attempts_per_wave = DEFAULT_MAX_ATTEMPTS_PER_WAVE;
    let max_attempts_per_work_item = DEFAULT_MAX_ATTEMPTS_PER_WORK_ITEM;
    let max_model_calls = if high_rigor { 12 } else { 6 };
    ResearchRunBudget {
        max_waves,
        max_model_calls,
        max_work_items_per_wave,
        max_attempts_per_wave,
        max_attempts_per_work_item,
        max_total_work_item_attempts: max_model_calls,
        max_no_progress_waves: DEFAULT_MAX_NO_PROGRESS_WAVES,
        waves_used: 0,
        model_calls_used: 0,
        work_items_run: 0,
        attempts_used: 0,
        no_progress_waves: 0,
    }
}

fn routed_stage_for_kind(kind: &ResearchWorkItemKind) -> Option<&'static str> {
    match kind {
        ResearchWorkItemKind::ArtifactStabilization => Some(RESEARCH_STAGE_QUALITY_GATE),
        ResearchWorkItemKind::SourceCardRepair | ResearchWorkItemKind::ClaimLogRepair => {
            Some(RESEARCH_STAGE_EVIDENCE_REPAIR)
        }
        ResearchWorkItemKind::PhasePlanBuild
        | ResearchWorkItemKind::PhasePlanReview
        | ResearchWorkItemKind::PhaseClaimReadiness => Some(RESEARCH_STAGE_PHASE_STATE),
        ResearchWorkItemKind::EventCardEnrichment
        | ResearchWorkItemKind::CausalContinuityReview => Some(RESEARCH_STAGE_NARRATIVE_ENRICHMENT),
        ResearchWorkItemKind::FinalAnswerRender => Some(RESEARCH_STAGE_FINAL),
        ResearchWorkItemKind::ResearchAcceptanceReview => Some(RESEARCH_STAGE_QUALITY_GATE),
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_routed_work_item(
    state: &AppState,
    task: &TaskInfo,
    runtime: &dyn ModelRuntime,
    model_name: &str,
    source: &str,
    file_prefix: &str,
    file_type: &str,
    normalized_output: &str,
    user_prompt: &str,
    iteration: i64,
    max_iterations: i64,
    controller_events: &mut Vec<ResearchControllerEvent>,
    work_kind: &ResearchWorkItemKind,
    debt_ids: &[String],
    remaining_model_calls: usize,
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    match work_kind {
        ResearchWorkItemKind::ArtifactStabilization => {
            run_artifact_stabilization_work_item(
                state,
                task,
                file_type,
                normalized_output,
                user_prompt,
                controller_events,
            )
            .await
        }
        ResearchWorkItemKind::SourceCardRepair => {
            run_source_card_repair_work_item(state, task, debt_ids).await
        }
        ResearchWorkItemKind::ClaimLogRepair => {
            run_claim_log_repair_work_item(state, task, normalized_output, debt_ids).await
        }
        ResearchWorkItemKind::PhasePlanBuild => {
            let report = run_phase_state_stage(
                state,
                task,
                file_prefix,
                user_prompt,
                iteration,
                max_iterations,
                controller_events,
            )
            .await;
            (
                phase_state_checkpoint_status(&report),
                phase_state_checkpoint_detail(&report),
                Some("Review grounded phase cards before acceptance.".to_string()),
                0,
            )
        }
        ResearchWorkItemKind::PhasePlanReview => {
            let report = run_phase_state_stage(
                state,
                task,
                file_prefix,
                user_prompt,
                iteration,
                max_iterations,
                controller_events,
            )
            .await;
            let artifacts = load_task_research_artifacts(state, task.id)
                .await
                .unwrap_or_default();
            let card_count = artifacts
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len())
                .unwrap_or(0);
            if card_count > 0 && report.skipped_reason.is_none() {
                (
                    ResearchWorkItemStatus::Completed,
                    format!("phase_plan_reviewed event_cards={card_count}"),
                    Some("Proceed to phase-specific claim readiness.".to_string()),
                    0,
                )
            } else {
                (
                    ResearchWorkItemStatus::Blocked,
                    format!(
                        "phase_plan_review_blocked {}",
                        phase_state_checkpoint_detail(&report)
                    ),
                    Some(
                        "Build a non-placeholder historical phase plan before readiness review."
                            .to_string(),
                    ),
                    0,
                )
            }
        }
        ResearchWorkItemKind::PhaseClaimReadiness => {
            let report = run_phase_state_stage(
                state,
                task,
                file_prefix,
                user_prompt,
                iteration,
                max_iterations,
                controller_events,
            )
            .await;
            let artifacts = load_task_research_artifacts(state, task.id)
                .await
                .unwrap_or_default();
            let total = artifacts
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len())
                .unwrap_or(0);
            let ready = count_ready_phase_cards(&artifacts);
            if total > 0 && ready == total {
                (
                    ResearchWorkItemStatus::Completed,
                    format!("phase_claim_readiness ready={ready} total={total}"),
                    Some("Proceed to event-card enrichment.".to_string()),
                    0,
                )
            } else {
                (
                    ResearchWorkItemStatus::Blocked,
                    format!(
                        "phase_claim_readiness_blocked ready={ready} total={total} {}",
                        phase_state_checkpoint_detail(&report)
                    ),
                    Some("Provide or repair phase-specific supported Claim Log rows before event-card enrichment.".to_string()),
                    0,
                )
            }
        }
        ResearchWorkItemKind::EventCardEnrichment => {
            let report = run_narrative_enrichment_stage(
                state,
                task,
                runtime,
                model_name,
                source,
                file_prefix,
                user_prompt,
                iteration,
                max_iterations,
                controller_events,
                remaining_model_calls,
            )
            .await;
            (
                narrative_checkpoint_status(&report),
                narrative_checkpoint_detail(&report),
                Some(
                    "Review causal continuity and supported interpretive layers after enrichment."
                        .to_string(),
                ),
                report.selected_cards as i64,
            )
        }
        ResearchWorkItemKind::CausalContinuityReview => {
            run_causal_continuity_review_work_item(state, task.id).await
        }
        ResearchWorkItemKind::FinalAnswerRender => {
            run_final_answer_render_work_item(
                state,
                task.id,
                normalized_output,
                file_prefix,
                file_type,
            )
            .await
        }
        ResearchWorkItemKind::ResearchAcceptanceReview => {
            run_research_acceptance_review_work_item(
                state,
                task.id,
                normalized_output,
                file_prefix,
                file_type,
            )
            .await
        }
    }
}

async fn run_artifact_stabilization_work_item(
    state: &AppState,
    task: &TaskInfo,
    file_type: &str,
    normalized_output: &str,
    user_prompt: &str,
    controller_events: &[ResearchControllerEvent],
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let before = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    let before_signature = research_artifact_progress_signature(&before);
    persist_iteration_research_artifacts(
        state,
        task.id,
        controller_events,
        file_type,
        normalized_output,
        task_uses_local_pi(task),
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(research_source_subject_for_task(task, user_prompt)),
    )
    .await;
    let after = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    let after_signature = research_artifact_progress_signature(&after);
    let stabilized = after_signature != before_signature
        || !after.source_cards.is_empty()
        || !after.claim_log.is_empty()
        || after.narrative_state.is_some();
    if stabilized {
        (
            ResearchWorkItemStatus::Completed,
            format!(
                "artifact_stabilized source_cards={} claim_log={} event_cards={} warnings={}",
                after.source_cards.len(),
                after.claim_log.len(),
                after
                    .narrative_state
                    .as_ref()
                    .map(|state| state.event_cards.len())
                    .unwrap_or(0),
                after.warnings.len()
            ),
            Some("Continue with source, claim, phase, and narrative repair routes.".to_string()),
            0,
        )
    } else {
        (
            ResearchWorkItemStatus::Blocked,
            "artifact_stabilization_blocked no safe compact artifact state could be recovered".to_string(),
            Some("Provide a valid machine-readable research artifact JSON block or stronger public source evidence.".to_string()),
            0,
        )
    }
}

async fn run_source_card_repair_work_item(
    state: &AppState,
    task: &TaskInfo,
    debt_ids: &[String],
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let mut artifacts = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    if !artifacts.source_cards.is_empty() && debt_ids.is_empty() {
        return (
            ResearchWorkItemStatus::Completed,
            format!(
                "source_card_repair_not_needed source_cards={}",
                artifacts.source_cards.len()
            ),
            Some("Proceed to Claim Log readiness.".to_string()),
            0,
        );
    }
    if !artifacts.source_cards.is_empty() && !debt_ids.is_empty() {
        return (
            ResearchWorkItemStatus::Blocked,
            format!(
                "source_card_repair_blocked existing Source Cards remain insufficient for queued debt debt={}",
                debt_ids.len()
            ),
            Some("Repair or add public Source Cards that satisfy the queued evidence debt before event-card enrichment.".to_string()),
            0,
        );
    }
    let diagnostics = load_research_source_diagnostics(state, task.id).await;
    if let Some(source_cards) = local_pi_source_pack_scaffold_cards_for_iteration(
        &artifacts,
        None,
        None,
        diagnostics.as_ref(),
        task_uses_local_pi(task),
    ) {
        artifacts.source_cards = source_cards;
        push_unique_warning(
            &mut artifacts.warnings,
            PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string(),
        );
        artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
        persist_research_controller_artifacts(state, task.id, &artifacts).await;
        return (
            ResearchWorkItemStatus::Completed,
            format!(
                "source_card_repair_scaffolded source_cards={}",
                artifacts.source_cards.len()
            ),
            Some("Repair supported Claim Log rows from visible evidence next.".to_string()),
            0,
        );
    }
    upsert_research_debt(
        &mut artifacts.research_debt,
        ResearchDebtItem {
            id: "source-card-repair-blocked".to_string(),
            severity: "high".to_string(),
            failed_gate: Some("narrative_enrichment_readiness".to_string()),
            missing_evidence: "Source Cards are required before bounded event-card enrichment, but no safe deterministic source-card repair path was available.".to_string(),
            required_source_class: Some("public evidence Source Card".to_string()),
            candidate_queries: Vec::new(),
            next_check_actions: vec![
                "Provide public Source Cards or allow source acquisition to gather public evidence before event-card enrichment.".to_string(),
            ],
            status: "open".to_string(),
        },
    );
    persist_research_controller_artifacts(state, task.id, &artifacts).await;
    (
        ResearchWorkItemStatus::Blocked,
        "source_card_repair_blocked no deterministic public Source Card repair available"
            .to_string(),
        Some(
            "Provide public Source Cards or rerun source acquisition before event-card enrichment."
                .to_string(),
        ),
        0,
    )
}

async fn run_claim_log_repair_work_item(
    state: &AppState,
    task: &TaskInfo,
    normalized_output: &str,
    debt_ids: &[String],
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let mut artifacts = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    if !artifacts.claim_log.is_empty() && debt_ids.is_empty() {
        return (
            ResearchWorkItemStatus::Completed,
            format!(
                "claim_log_repair_not_needed claim_log={}",
                artifacts.claim_log.len()
            ),
            Some("Proceed to phase claim readiness.".to_string()),
            0,
        );
    }
    if !artifacts.claim_log.is_empty() && !debt_ids.is_empty() {
        return (
            ResearchWorkItemStatus::Blocked,
            format!(
                "claim_log_repair_blocked existing Claim Log rows remain insufficient for queued debt debt={}",
                debt_ids.len()
            ),
            Some("Repair or add phase-specific Claim Log rows supported by public Source Cards before event-card enrichment.".to_string()),
            0,
        );
    }
    let scaffold_authorized = has_local_pi_source_pack_source_card_scaffold(&artifacts);
    if let Some(claim_log) = local_pi_repaired_claim_log_for_iteration(
        &artifacts,
        true,
        scaffold_authorized,
        normalized_output,
        &artifacts.source_cards,
        task_uses_local_pi(task),
    ) {
        artifacts.claim_log = claim_log;
        push_unique_warning(
            &mut artifacts.warnings,
            PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING.to_string(),
        );
        artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
        persist_research_controller_artifacts(state, task.id, &artifacts).await;
        return (
            ResearchWorkItemStatus::Completed,
            format!("claim_log_repaired claim_log={}", artifacts.claim_log.len()),
            Some("Proceed to phase-specific claim readiness.".to_string()),
            0,
        );
    }
    upsert_research_debt(
        &mut artifacts.research_debt,
        ResearchDebtItem {
            id: "claim-log-repair-blocked".to_string(),
            severity: "high".to_string(),
            failed_gate: Some("narrative_enrichment_readiness".to_string()),
            missing_evidence: "Supported Claim Log rows are required before bounded event-card enrichment, but no safe deterministic Claim Log repair path was available.".to_string(),
            required_source_class: Some("supported Claim Log row".to_string()),
            candidate_queries: Vec::new(),
            next_check_actions: vec![
                "Provide phase-specific Claim Log rows supported by public Source Cards or full public URLs.".to_string(),
            ],
            status: "open".to_string(),
        },
    );
    persist_research_controller_artifacts(state, task.id, &artifacts).await;
    (
        ResearchWorkItemStatus::Blocked,
        "claim_log_repair_blocked no deterministic supported Claim Log repair available".to_string(),
        Some("Provide phase-specific Claim Log rows supported by public Source Cards or full public URLs.".to_string()),
        0,
    )
}

async fn run_causal_continuity_review_work_item(
    state: &AppState,
    task_id: i64,
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    if has_grounded_causal_depth(&artifacts) {
        (
            ResearchWorkItemStatus::Completed,
            "causal_continuity_review_passed grounded causal or interpretive depth is present"
                .to_string(),
            Some("Proceed to final-answer rendering.".to_string()),
            0,
        )
    } else {
        (
            ResearchWorkItemStatus::Blocked,
            "causal_continuity_review_blocked no grounded causal_spine or interpretive_layers present".to_string(),
            Some("Run event-card enrichment or provide supported phase-specific causal/interpretive Claim Log evidence.".to_string()),
            0,
        )
    }
}

async fn run_final_answer_render_work_item(
    state: &AppState,
    task_id: i64,
    normalized_output: &str,
    file_prefix: &str,
    file_type: &str,
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let artifacts = load_task_research_artifacts(state, task_id)
        .await
        .unwrap_or_default();
    if artifacts.source_cards.is_empty()
        || (artifacts.claim_log.is_empty()
            && !has_local_pi_source_pack_source_card_scaffold(&artifacts))
    {
        return (
            ResearchWorkItemStatus::Blocked,
            "final_answer_render_blocked missing Source Cards or supported Claim Log".to_string(),
            Some(
                "Repair Source Cards and supported Claim Log rows before final-answer rendering."
                    .to_string(),
            ),
            0,
        );
    }
    let rendered =
        finalize_task_research_output(state, task_id, normalized_output, file_prefix, file_type)
            .await;
    if rendered.trim().is_empty() {
        (
            ResearchWorkItemStatus::Blocked,
            "final_answer_render_blocked empty rendered output".to_string(),
            Some("Repair artifacts before final-answer rendering.".to_string()),
            0,
        )
    } else {
        (
            ResearchWorkItemStatus::Completed,
            format!("final_answer_rendered chars={}", rendered.chars().count()),
            Some("Run research acceptance review.".to_string()),
            0,
        )
    }
}

async fn run_research_acceptance_review_work_item(
    state: &AppState,
    task_id: i64,
    normalized_output: &str,
    file_prefix: &str,
    file_type: &str,
) -> (ResearchWorkItemStatus, String, Option<String>, i64) {
    let rendered =
        finalize_task_research_output(state, task_id, normalized_output, file_prefix, file_type)
            .await;
    match validate_task_research_output(state, task_id, &rendered, file_prefix, file_type, None)
        .await
    {
        Ok(_) => (
            ResearchWorkItemStatus::Completed,
            "research_acceptance_review_passed".to_string(),
            Some("Research quality gate can accept the current trusted state.".to_string()),
            0,
        ),
        Err(failure) => {
            let safe_message = compact_detail(&failure.message);
            (
                ResearchWorkItemStatus::Blocked,
                format!("research_acceptance_review_blocked {safe_message}"),
                Some(
                    "Repair the remaining quality-gate failures before accepting this research run."
                        .to_string(),
                ),
                0,
            )
        }
    }
}

fn mark_work_item_running(iteration_state: &mut ResearchIterationState, item_id: &str) {
    let current_wave = iteration_state.current_wave;
    if let Some(item) = iteration_state
        .work_items
        .iter_mut()
        .find(|item| item.id == item_id)
    {
        item.status = ResearchWorkItemStatus::Running;
        item.last_wave = Some(current_wave);
    }
}

fn finalize_work_item_run(
    iteration_state: &mut ResearchIterationState,
    item_id: &str,
    status: ResearchWorkItemStatus,
    detail: String,
    input_fingerprint: String,
    output_fingerprint: String,
    next_action: Option<String>,
) {
    let current_wave = iteration_state.current_wave;
    if let Some(item) = iteration_state
        .work_items
        .iter_mut()
        .find(|item| item.id == item_id)
    {
        item.status = status.clone();
        item.attempt_count += 1;
        item.last_wave = Some(current_wave);
        item.detail = Some(compact_detail(&detail));
        item.input_fingerprint = Some(input_fingerprint);
        item.output_fingerprint = Some(output_fingerprint);
        item.next_action = next_action.map(|action| compact_detail(&action));
        item.last_error = match status {
            ResearchWorkItemStatus::Blocked | ResearchWorkItemStatus::Failed => {
                Some(compact_detail(&detail))
            }
            _ => None,
        };
    }
}

fn append_work_checkpoint(
    iteration_state: &mut ResearchIterationState,
    artifacts: &ResearchControllerArtifacts,
    item_id: &str,
    work_kind: ResearchWorkItemKind,
    status: ResearchWorkItemStatus,
    detail: String,
    input_fingerprint: String,
    output_fingerprint: String,
) {
    let checkpoint = ResearchWorkCheckpoint {
        wave: iteration_state.current_wave,
        work_item_id: item_id.to_string(),
        work_kind,
        status,
        attempt_count: iteration_state
            .work_items
            .iter()
            .find(|item| item.id == item_id)
            .map(|item| item.attempt_count)
            .unwrap_or_default(),
        event_card_count: artifacts
            .narrative_state
            .as_ref()
            .map(|state| state.event_cards.len())
            .unwrap_or(0),
        open_debt_count: artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
            .count(),
        input_fingerprint: Some(input_fingerprint),
        output_fingerprint: Some(output_fingerprint),
        detail: Some(compact_detail(&detail)),
    };
    iteration_state.checkpoints.push(checkpoint);
    if iteration_state.checkpoints.len() > MAX_QUEUE_CHECKPOINTS {
        let drain = iteration_state.checkpoints.len() - MAX_QUEUE_CHECKPOINTS;
        iteration_state.checkpoints.drain(0..drain);
    }
}

fn phase_state_checkpoint_status(report: &PhaseStateStageReport) -> ResearchWorkItemStatus {
    if report.skipped_reason.is_some() {
        ResearchWorkItemStatus::Blocked
    } else {
        ResearchWorkItemStatus::Completed
    }
}

fn phase_state_checkpoint_detail(report: &PhaseStateStageReport) -> String {
    match &report.skipped_reason {
        Some(reason) => format!("skipped:{reason}"),
        None => format!(
            "strategy={} phases={} ready={} repaired={}",
            report.strategy.unwrap_or("historical_phase_state"),
            report.phase_count,
            report.ready_phase_count,
            report.repaired_event_cards
        ),
    }
}

fn narrative_checkpoint_status(report: &NarrativeEnrichmentStageReport) -> ResearchWorkItemStatus {
    if report.skipped_reason.is_some() {
        ResearchWorkItemStatus::Blocked
    } else {
        ResearchWorkItemStatus::Completed
    }
}

fn narrative_checkpoint_detail(report: &NarrativeEnrichmentStageReport) -> String {
    match &report.skipped_reason {
        Some(reason) => format!("skipped:{reason}"),
        None => format!(
            "strategy={} selected={} accepted={} debt={}",
            report.strategy.unwrap_or("historical_event_card"),
            report.selected_cards,
            report.accepted_cards,
            report.debt_items
        ),
    }
}

fn terminal_status_without_pending(
    iteration_state: &ResearchIterationState,
) -> ResearchRunTerminalStatus {
    if has_failed_work_item(iteration_state) {
        return ResearchRunTerminalStatus::Failed;
    }
    let blocked_items = iteration_state
        .work_items
        .iter()
        .filter(|item| item.blocking && item.status == ResearchWorkItemStatus::Blocked)
        .collect::<Vec<_>>();
    if iteration_state
        .work_items
        .iter()
        .any(work_item_attempt_budget_exhausted)
    {
        ResearchRunTerminalStatus::BudgetExhausted
    } else if blocked_items.is_empty() {
        ResearchRunTerminalStatus::PartialTrusted
    } else if !blocked_items.is_empty() && blocked_items.iter().all(|item| item_requires_user(item))
    {
        ResearchRunTerminalStatus::BlockedNeedsUser
    } else {
        ResearchRunTerminalStatus::NoProgress
    }
}

fn terminal_status_for_stalled_wave(
    iteration_state: &ResearchIterationState,
    statuses: &[ResearchWorkItemStatus],
) -> ResearchRunTerminalStatus {
    if has_failed_work_item(iteration_state) {
        return ResearchRunTerminalStatus::Failed;
    }
    if !statuses.is_empty()
        && statuses
            .iter()
            .all(|status| *status == ResearchWorkItemStatus::Blocked)
        && iteration_state
            .work_items
            .iter()
            .filter(|item| item.blocking && item.status == ResearchWorkItemStatus::Blocked)
            .all(item_requires_user)
    {
        ResearchRunTerminalStatus::BlockedNeedsUser
    } else {
        ResearchRunTerminalStatus::NoProgress
    }
}

fn has_failed_work_item(iteration_state: &ResearchIterationState) -> bool {
    iteration_state
        .work_items
        .iter()
        .any(|item| item.status == ResearchWorkItemStatus::Failed)
}

fn work_item_attempt_budget_exhausted(item: &ResearchWorkItem) -> bool {
    item.max_attempts > 0
        && item.attempt_count >= item.max_attempts
        && item.status == ResearchWorkItemStatus::Blocked
}

fn item_requires_user(item: &ResearchWorkItem) -> bool {
    item.next_action.as_deref().is_some_and(|action| {
        let lower = action.to_ascii_lowercase();
        lower.contains("user")
            || lower.contains("manual")
            || lower.contains("provide")
            || lower.contains("add source")
    })
}

fn queue_state_summary(iteration_state: &ResearchIterationState) -> String {
    let pending = iteration_state
        .work_items
        .iter()
        .filter(|item| item.status == ResearchWorkItemStatus::Pending)
        .count();
    let completed = iteration_state
        .work_items
        .iter()
        .filter(|item| item.status == ResearchWorkItemStatus::Completed)
        .count();
    let blocked = iteration_state
        .work_items
        .iter()
        .filter(|item| item.status == ResearchWorkItemStatus::Blocked)
        .count();
    let failed = iteration_state
        .work_items
        .iter()
        .filter(|item| item.status == ResearchWorkItemStatus::Failed)
        .count();
    format!(
        "wave {}/{} pending={} completed={} blocked={} failed={}",
        iteration_state.current_wave,
        iteration_state.max_waves,
        pending,
        completed,
        blocked,
        failed
    )
}

fn work_queue_running_detail(iteration_state: &ResearchIterationState, item_id: &str) -> String {
    let index = iteration_state
        .work_items
        .iter()
        .position(|item| item.id == item_id)
        .map(|idx| idx + 1)
        .unwrap_or(1);
    format!(
        "queue_wave={}/{} work_item={}/{} id={item_id}",
        iteration_state.current_wave,
        iteration_state.max_waves,
        index,
        iteration_state.work_items.len()
    )
}

fn collect_phase_ids(artifacts: &ResearchControllerArtifacts) -> Vec<String> {
    unique_safe_ids(
        artifacts
            .narrative_state
            .as_ref()
            .map(|state| {
                state
                    .section_outline
                    .iter()
                    .map(|section| section.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    )
}

fn collect_phase_claim_log_ids(artifacts: &ResearchControllerArtifacts) -> Vec<String> {
    let mut ids = artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            let mut values = Vec::new();
            for section in &state.section_outline {
                values.extend(section.expected_claim_log_ids.clone());
            }
            for card in &state.event_cards {
                values.extend(card.claim_log_ids.clone());
            }
            values
        })
        .unwrap_or_default();
    if ids.is_empty() {
        ids.extend(artifacts.claim_log.iter().map(|claim| claim.id.clone()));
    }
    unique_safe_ids(ids)
}

fn collect_phase_source_card_ids(artifacts: &ResearchControllerArtifacts) -> Vec<String> {
    let mut ids = artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            let mut values = Vec::new();
            for section in &state.section_outline {
                values.extend(section.expected_source_card_ids.clone());
            }
            for card in &state.event_cards {
                values.extend(card.source_ids.clone());
            }
            values
        })
        .unwrap_or_default();
    if ids.is_empty() {
        ids.extend(
            artifacts
                .source_cards
                .iter()
                .map(|source| source.id.clone()),
        );
    }
    unique_safe_ids(ids)
}

#[derive(Debug, Default)]
struct QueueGroundingIndex {
    public_source_ids: HashSet<String>,
    supported_claim_ids: HashSet<String>,
}

fn queue_grounding_index(artifacts: &ResearchControllerArtifacts) -> QueueGroundingIndex {
    let public_source_ids = artifacts
        .source_cards
        .iter()
        .filter(|source| normalize_public_evidence_url(&source.url).is_some())
        .map(|source| source.id.clone())
        .collect::<HashSet<_>>();
    let supported_claim_ids = artifacts
        .claim_log
        .iter()
        .filter(|claim| claim.needs_verification != Some(true))
        .filter(|claim| {
            claim
                .support_source_card_ids
                .iter()
                .any(|source_id| public_source_ids.contains(source_id))
                || claim
                    .support_urls
                    .iter()
                    .any(|url| normalize_public_evidence_url(url).is_some())
        })
        .map(|claim| claim.id.clone())
        .collect::<HashSet<_>>();
    QueueGroundingIndex {
        public_source_ids,
        supported_claim_ids,
    }
}

fn all_ids_resolve(ids: &[String], valid_ids: &HashSet<String>) -> bool {
    !ids.is_empty() && ids.iter().all(|id| valid_ids.contains(id))
}

fn optional_ids_resolve(ids: &[String], valid_ids: &HashSet<String>) -> bool {
    ids.is_empty() || ids.iter().all(|id| valid_ids.contains(id))
}

fn phase_card_has_grounded_refs(card: &NarrativeEventCard, index: &QueueGroundingIndex) -> bool {
    all_ids_resolve(&card.claim_log_ids, &index.supported_claim_ids)
        && all_ids_resolve(&card.source_ids, &index.public_source_ids)
}

fn count_ready_phase_cards(artifacts: &ResearchControllerArtifacts) -> usize {
    let index = queue_grounding_index(artifacts);
    artifacts
        .narrative_state
        .as_ref()
        .map(|state| {
            state
                .event_cards
                .iter()
                .filter(|card| phase_card_has_grounded_refs(card, &index))
                .count()
        })
        .unwrap_or(0)
}

fn causal_link_is_grounded(link: &NarrativeCausalLink, index: &QueueGroundingIndex) -> bool {
    all_ids_resolve(&link.expected_claim_log_ids, &index.supported_claim_ids)
        && all_ids_resolve(&link.expected_source_card_ids, &index.public_source_ids)
}

fn causal_spine_step_is_grounded(
    step: &NarrativeCausalSpineStep,
    index: &QueueGroundingIndex,
) -> bool {
    all_ids_resolve(&step.claim_log_ids, &index.supported_claim_ids)
        && optional_ids_resolve(&step.source_ids, &index.public_source_ids)
}

fn interpretive_layer_is_grounded(
    layer: &NarrativeInterpretiveLayer,
    index: &QueueGroundingIndex,
) -> bool {
    all_ids_resolve(&layer.claim_log_ids, &index.supported_claim_ids)
        && optional_ids_resolve(&layer.source_ids, &index.public_source_ids)
}

fn has_grounded_causal_depth(artifacts: &ResearchControllerArtifacts) -> bool {
    let index = queue_grounding_index(artifacts);
    artifacts.narrative_state.as_ref().is_some_and(|state| {
        state
            .causal_chain
            .iter()
            .any(|link| causal_link_is_grounded(link, &index))
            || state.event_cards.iter().any(|card| {
                card.causal_spine
                    .iter()
                    .any(|step| causal_spine_step_is_grounded(step, &index))
                    || card
                        .interpretive_layers
                        .iter()
                        .any(|layer| interpretive_layer_is_grounded(layer, &index))
            })
    })
}

fn research_artifact_progress_signature(artifacts: &ResearchControllerArtifacts) -> String {
    let mut sanitized = artifacts.clone();
    sanitized.research_iteration_state = None;
    serde_json::to_string(&sanitized).unwrap_or_default()
}

fn work_item_fingerprint(
    kind: &ResearchWorkItemKind,
    artifacts: &ResearchControllerArtifacts,
    phase_ids: &[String],
    claim_log_ids: &[String],
    source_card_ids: &[String],
    debt_ids: &[String],
) -> String {
    let snapshot = json!({
        "kind": kind,
        "phase_ids": phase_ids,
        "claim_log_ids": claim_log_ids,
        "source_card_ids": source_card_ids,
        "debt_ids": debt_ids,
        "source_cards": artifacts.source_cards.len(),
        "claim_log": artifacts.claim_log.len(),
        "event_cards": artifacts
            .narrative_state
            .as_ref()
            .map(|state| state.event_cards.len())
            .unwrap_or(0),
        "open_debt_count": artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
            .count(),
        "warnings": artifacts.warnings.len(),
    });
    stable_fingerprint(&serde_json::to_string(&snapshot).unwrap_or_default())
}

fn stable_fingerprint(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a:{hash:016x}")
}

async fn persist_research_iteration_state(
    state: &AppState,
    task_id: i64,
    artifacts: &mut ResearchControllerArtifacts,
    iteration_state: &ResearchIterationState,
) {
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.research_iteration_state = Some(iteration_state.clone());
    persist_research_controller_artifacts(state, task_id, artifacts).await;
}

fn debt_gate_label(debt: &ResearchDebtItem) -> String {
    debt.failed_gate
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn unique_kinds(kinds: Vec<ResearchWorkItemKind>) -> Vec<ResearchWorkItemKind> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for kind in kinds {
        if seen.insert(kind.clone()) {
            deduped.push(kind);
        }
    }
    deduped
}

fn unique_compact_values(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        let compact = compact_detail(&value);
        if !compact.is_empty() && seen.insert(compact.clone()) {
            deduped.push(compact);
        }
    }
    deduped
}

fn unique_safe_ids(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        let safe = sanitize_queue_id(&value);
        if !safe.is_empty() && seen.insert(safe.clone()) {
            deduped.push(safe);
        }
    }
    deduped
}

fn sanitize_queue_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let safe_shape = trimmed.len() <= 80
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.');
    let lowered = trimmed.to_ascii_lowercase();
    let unsafe_semantics = lowered.contains("http")
        || lowered.contains("prompt")
        || lowered.contains("provider")
        || lowered.contains("payload")
        || lowered.contains("diagnostic")
        || contains_secret_marker(&lowered)
        || queue_text_contains_private_host_fragment(trimmed)
        || queue_token_contains_private_host(trimmed);
    if safe_shape && !unsafe_semantics {
        trimmed.to_string()
    } else {
        stable_fingerprint(trimmed).replace("fnv1a:", "ref-")
    }
}

fn contains_secret_marker(lowered: &str) -> bool {
    [
        "api_key",
        "apikey",
        "access_key",
        "secret_key",
        "client_secret",
        "authorization",
        "bearer",
        "password",
        "passwd",
        "token",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn compact_detail(detail: &str) -> String {
    sanitize_queue_detail(detail).chars().take(240).collect()
}

fn sanitize_queue_detail(detail: &str) -> String {
    let flattened = detail
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let lower = flattened.to_ascii_lowercase();
    let sensitive_markers = [
        "resolved_prompt",
        "resolved prompt",
        "system prompt",
        "user prompt",
        "developer prompt",
        "provider_payload",
        "provider payload",
        "raw_model_output",
        "raw model output",
        "source_documents",
        "source documents",
        "source_diagnostics",
        "source diagnostics",
        "source pack",
        "diagnostics_json",
        "diagnostics json",
        "controller_artifacts_json",
        "controller artifacts json",
        "research_controller_artifacts",
        "research controller artifacts",
    ];
    if sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker))
        || contains_secret_marker(&lower)
    {
        return "[redacted-controller-detail]".to_string();
    }

    if queue_detail_contains_private_host(&flattened) {
        return "[redacted-private-host]".to_string();
    }

    flattened
        .split_whitespace()
        .map(|token| {
            if queue_token_contains_private_host(token) {
                "[redacted-private-host]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn queue_detail_contains_private_host(detail: &str) -> bool {
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("::1")
        || lowered.contains("[::1]")
        || lowered.contains("fd00:")
        || lowered.contains("[fd")
        || lowered.contains("fc00:")
        || lowered.contains("[fc")
        || lowered.contains("fe80:")
        || lowered.contains("[fe80")
    {
        return true;
    }

    queue_text_contains_private_host_fragment(detail)
        || queue_detail_contains_host_marker_private_host(detail)
        || queue_token_contains_private_host(detail)
        || detail
            .split(|ch: char| {
                ch.is_whitespace()
                    || matches!(
                        ch,
                        ',' | ';'
                            | '"'
                            | '\''
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '<'
                            | '>'
                            | '\\'
                            | '='
                            | ':'
                    )
            })
            .filter(|part| !part.trim().is_empty())
            .any(queue_token_contains_private_host)
}

fn queue_text_contains_private_host_fragment(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered
        .split(|ch: char| {
            !(ch.is_ascii_alphanumeric()
                || ch == '.'
                || ch == '-'
                || ch == ':'
                || ch == '['
                || ch == ']')
        })
        .filter(|part| !part.trim().is_empty())
        .any(|part| {
            let part = part.trim_matches(|ch| ch == '[' || ch == ']');
            let hostish_prefix = ["source", "host", "endpoint", "url", "path", "metadata"]
                .iter()
                .any(|marker| part.contains(marker));
            queue_token_contains_private_host(part)
                || part
                    .char_indices()
                    .filter_map(|(idx, ch)| (ch == '.').then_some(idx + 1))
                    .filter(|idx| *idx < part.len())
                    .any(|idx| {
                        queue_token_contains_private_host(&part[idx..])
                            || (hostish_prefix
                                && queue_token_contains_private_host_in_host_context(&part[idx..]))
                    })
        })
}

fn queue_token_contains_private_host(token: &str) -> bool {
    queue_token_contains_private_host_with_context(token, false)
}

fn queue_token_contains_private_host_in_host_context(token: &str) -> bool {
    queue_token_contains_private_host_with_context(token, true)
}

fn queue_detail_contains_host_marker_private_host(detail: &str) -> bool {
    let markers = [
        "host=",
        "host:",
        "source=",
        "source:",
        "endpoint=",
        "endpoint:",
        "url=",
        "url:",
        "\\\"host\\\":",
        "\\\"source\\\":",
        "\\\"endpoint\\\":",
        "\\\"url\\\":",
    ];
    let lowered = detail.to_ascii_lowercase();
    markers.iter().any(|marker| {
        let mut search_start = 0;
        while let Some(offset) = lowered[search_start..].find(marker) {
            let value_start = search_start + offset + marker.len();
            let value = detail[value_start..].trim_start_matches(|ch: char| {
                ch.is_whitespace() || ch == '"' || ch == '\'' || ch == '[' || ch == '{'
            });
            let candidate = value
                .split(|ch: char| {
                    ch.is_whitespace()
                        || matches!(ch, ',' | ';' | '"' | '\'' | ')' | ']' | '}' | '<' | '>')
                })
                .next()
                .unwrap_or_default();
            if queue_token_contains_private_host_in_host_context(candidate) {
                return true;
            }
            search_start = value_start;
        }
        false
    })
}

fn queue_token_contains_private_host_with_context(
    token: &str,
    allow_bare_decimal_ipv4: bool,
) -> bool {
    let lowered = token
        .trim_matches(|ch: char| {
            ch == ','
                || ch == ';'
                || ch == '.'
                || ch == ')'
                || ch == '('
                || ch == '"'
                || ch == '\''
                || ch == ']'
                || ch == '['
        })
        .to_ascii_lowercase();
    let candidate = lowered
        .strip_prefix("http://")
        .or_else(|| lowered.strip_prefix("https://"))
        .unwrap_or(&lowered);
    if candidate == "::1" || candidate == "[::1]" {
        return true;
    }
    let host_candidate = candidate
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(candidate)
        .split('@')
        .next_back()
        .unwrap_or(candidate);
    let bracketless_host = host_candidate.trim_matches(|ch| ch == '[' || ch == ']');
    if bracketless_host.parse().is_ok_and(is_blocked_ip) {
        return true;
    }
    let host = if bracketless_host.contains(':') {
        bracketless_host
    } else {
        bracketless_host
            .split(':')
            .next()
            .unwrap_or(bracketless_host)
    };

    let host_looks_like_ip_literal = allow_bare_decimal_ipv4
        || host.contains('.')
        || host.starts_with("0x")
        || (host.len() > 1 && host.starts_with('0') && host.chars().all(|ch| ch.is_ascii_digit()));
    if host_looks_like_ip_literal && parse_ipv4_style_host(host).is_some_and(is_blocked_ipv4) {
        return true;
    }

    host == "localhost"
        || host == "::1"
        || host == "0.0.0.0"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || queue_host_is_private_172(host)
        || host == "metadata.google.internal"
        || host == "host.docker.internal"
        || host.ends_with(".local")
}

fn queue_host_is_private_172(host: &str) -> bool {
    let mut parts = host.split('.');
    if parts.next() != Some("172") {
        return false;
    }
    parts
        .next()
        .and_then(|octet| octet.parse::<u8>().ok())
        .is_some_and(|octet| (16..=31).contains(&octet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_test_dir, test_state};
    use futures::future::BoxFuture;
    use liquid_protocol::{
        NarrativeCausalSpineStep, NarrativeEventCard, NarrativeSectionOutlineItem, NarrativeState,
        ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchDebtItem, ResearchSourceCard,
    };
    use liquid_storage_sqlite::setup_db;

    struct FakeRuntime;

    impl ModelRuntime for FakeRuntime {
        fn execute<'a>(
            &'a self,
            _request: ModelRuntimeRequest<'a>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async { None })
        }
    }

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

    fn all_work_item_kinds() -> Vec<ResearchWorkItemKind> {
        vec![
            ResearchWorkItemKind::ArtifactStabilization,
            ResearchWorkItemKind::SourceCardRepair,
            ResearchWorkItemKind::ClaimLogRepair,
            ResearchWorkItemKind::PhasePlanBuild,
            ResearchWorkItemKind::PhasePlanReview,
            ResearchWorkItemKind::PhaseClaimReadiness,
            ResearchWorkItemKind::EventCardEnrichment,
            ResearchWorkItemKind::CausalContinuityReview,
            ResearchWorkItemKind::FinalAnswerRender,
            ResearchWorkItemKind::ResearchAcceptanceReview,
        ]
    }

    fn compact_artifact_output() -> String {
        serde_json::json!({
            "version": 1,
            "source_cards": [{
                "id": "S1",
                "url": "https://example.org/s1",
                "title": "Source S1",
                "source_class": "authoritative_secondary",
                "extracted_facts": ["1792-1793 republican transition in Paris abolished the monarchy and declared the republic."]
            }],
            "claim_log": [{
                "id": "C1",
                "claim": "1792-1793 republican transition in Paris abolished the monarchy and declared the republic.",
                "support_source_card_ids": ["S1"],
                "confidence": "medium"
            }],
            "conflict_map": [],
            "research_debt": [],
            "narrative_state": {
                "version": 1,
                "section_outline": [{
                    "id": "SO1",
                    "heading": "Republican transition",
                    "purpose": "1792-1793 monarchy collapse and republic declaration in Paris",
                    "expected_claim_log_ids": ["C1"],
                    "expected_source_card_ids": ["S1"]
                }],
                "event_cards": [{
                    "label": "Republican transition",
                    "timeframe": "1792-1793",
                    "actors": ["Paris revolutionaries", "National Convention"],
                    "region_or_front": "Paris",
                    "trigger": "monarchy collapse",
                    "development": "The political center shifted from monarchy to republic.",
                    "outcome": "The republic was declared.",
                    "claim_log_ids": ["C1"],
                    "source_ids": ["S1"]
                }]
            }
        })
        .to_string()
    }

    fn artifact_markdown_output() -> String {
        format!(
            "## 최종 답변\n\nFrench Revolution republican transition.\n\n[RESEARCH_ARTIFACT_JSON]\n```json\n{}\n```",
            compact_artifact_output()
        )
    }

    #[test]
    fn every_work_item_kind_has_a_routed_stage() {
        for kind in all_work_item_kinds() {
            assert!(
                routed_stage_for_kind(&kind).is_some(),
                "{kind:?} must have a controller stage"
            );
        }
    }

    #[test]
    fn routed_work_item_drafts_have_attempt_budgets_when_runnable() {
        let task = TaskInfo {
            id: 1,
            original_name: "Historical queue".to_string(),
            status: "researching".to_string(),
            file_prefix: Some("[AI-Research]".to_string()),
            file_type: Some("md".to_string()),
            research_topic: Some("French Revolution republican transition".to_string()),
            research_intensity: Some("high".to_string()),
            quality_depth: Some("strict".to_string()),
            ..sample_task()
        };
        let artifacts = ResearchControllerArtifacts {
            research_debt: vec![
                ResearchDebtItem {
                    id: "artifact".to_string(),
                    failed_gate: Some("artifact_quality".to_string()),
                    status: "open".to_string(),
                    ..ResearchDebtItem::default()
                },
                ResearchDebtItem {
                    id: "gate".to_string(),
                    failed_gate: Some("quality_gate".to_string()),
                    status: "open".to_string(),
                    ..ResearchDebtItem::default()
                },
            ],
            ..ResearchControllerArtifacts::default()
        };

        let iteration_state = refresh_research_iteration_state(
            &task,
            "[AI-Research]",
            "French Revolution republican transition",
            &artifacts,
        );

        let runnable = iteration_state
            .work_items
            .iter()
            .filter(|item| item.status == ResearchWorkItemStatus::Pending)
            .collect::<Vec<_>>();
        assert!(!runnable.is_empty());
        assert!(runnable.iter().all(|item| item.max_attempts > 0));
        assert!(runnable
            .iter()
            .any(|item| item.kind == ResearchWorkItemKind::ArtifactStabilization));
        assert!(runnable
            .iter()
            .any(|item| item.kind == ResearchWorkItemKind::SourceCardRepair));
        assert!(runnable
            .iter()
            .any(|item| item.kind == ResearchWorkItemKind::ClaimLogRepair));
    }

    #[test]
    fn source_and_claim_repair_debt_routes_even_when_ledgers_are_non_empty() {
        let task = TaskInfo {
            id: 1,
            original_name: "Historical queue".to_string(),
            status: "researching".to_string(),
            file_prefix: Some("[AI-Research]".to_string()),
            file_type: Some("md".to_string()),
            research_topic: Some("French Revolution republican transition".to_string()),
            research_intensity: Some("high".to_string()),
            quality_depth: Some("strict".to_string()),
            ..sample_task()
        };
        let artifacts = ResearchControllerArtifacts {
            source_cards: vec![source(
                "S1",
                "1792-1793 republican transition in Paris abolished the monarchy.",
            )],
            claim_log: vec![claim(
                "C1",
                "1792-1793 republican transition in Paris abolished the monarchy.",
                "S1",
            )],
            research_debt: vec![
                ResearchDebtItem {
                    id: "source-gap".to_string(),
                    failed_gate: Some("narrative_enrichment_readiness".to_string()),
                    missing_evidence: "source card coverage remains insufficient".to_string(),
                    next_check_actions: vec!["Repair Source Cards.".to_string()],
                    status: "open".to_string(),
                    ..ResearchDebtItem::default()
                },
                ResearchDebtItem {
                    id: "claim-gap".to_string(),
                    failed_gate: Some("narrative_enrichment_readiness".to_string()),
                    missing_evidence: "claim log coverage remains insufficient".to_string(),
                    next_check_actions: vec!["Repair Claim Log rows.".to_string()],
                    status: "open".to_string(),
                    ..ResearchDebtItem::default()
                },
            ],
            ..ResearchControllerArtifacts::default()
        };

        let iteration_state = refresh_research_iteration_state(
            &task,
            "[AI-Research]",
            "French Revolution republican transition",
            &artifacts,
        );
        let source_item = iteration_state
            .work_items
            .iter()
            .find(|item| item.kind == ResearchWorkItemKind::SourceCardRepair)
            .expect("source repair debt should route even with existing source cards");
        let claim_item = iteration_state
            .work_items
            .iter()
            .find(|item| item.kind == ResearchWorkItemKind::ClaimLogRepair)
            .expect("claim repair debt should route even with existing claim rows");

        assert_eq!(source_item.status, ResearchWorkItemStatus::Pending);
        assert_eq!(claim_item.status, ResearchWorkItemStatus::Pending);
        assert_eq!(source_item.debt_ids, vec!["source-gap".to_string()]);
        assert_eq!(claim_item.debt_ids, vec!["claim-gap".to_string()]);
    }

    #[test]
    fn phase_and_causal_readiness_require_resolved_supported_refs() {
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
                    actors: vec!["Paris revolutionaries".to_string()],
                    region_or_front: Some("Paris".to_string()),
                    trigger: Some("monarchy collapse".to_string()),
                    development: Some("The political center shifted from monarchy to republic.".to_string()),
                    outcome: Some("The republic was declared.".to_string()),
                    claim_log_ids: vec!["C999".to_string()],
                    source_ids: vec!["S999".to_string()],
                    causal_spine: vec![NarrativeCausalSpineStep {
                        step_type: "forcing_factor".to_string(),
                        description: "Unsupported causal prose should not count.".to_string(),
                        epistemic_status: Some("interpretation".to_string()),
                        claim_log_ids: vec!["C999".to_string()],
                        source_ids: vec!["S999".to_string()],
                        ..NarrativeCausalSpineStep::default()
                    }],
                    ..NarrativeEventCard::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        assert_eq!(count_ready_phase_cards(&artifacts), 0);
        assert!(!has_grounded_causal_depth(&artifacts));

        let state = artifacts.narrative_state.as_mut().unwrap();
        let card = state.event_cards.first_mut().unwrap();
        card.claim_log_ids = vec!["C1".to_string()];
        card.source_ids = vec!["S1".to_string()];
        card.causal_spine[0].claim_log_ids = vec!["C1".to_string()];
        card.causal_spine[0].source_ids = vec!["S1".to_string()];

        assert_eq!(count_ready_phase_cards(&artifacts), 1);
        assert!(has_grounded_causal_depth(&artifacts));
    }

    #[test]
    fn completed_work_item_with_same_input_is_not_rescheduled() {
        let artifacts = ResearchControllerArtifacts {
            research_debt: vec![ResearchDebtItem {
                id: "quality".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                status: "open".to_string(),
                ..ResearchDebtItem::default()
            }],
            ..ResearchControllerArtifacts::default()
        };
        let draft = WorkItemDraft {
            id: FINAL_ANSWER_RENDER_WORK_ITEM_ID,
            kind: ResearchWorkItemKind::FinalAnswerRender,
            target: "final_answer",
            blocking: true,
            applicable: true,
            needs_attention: true,
            needs_run: true,
            max_attempts: 2,
            debt_ids: vec!["quality".to_string()],
            dependencies: Vec::new(),
            phase_ids: Vec::new(),
            claim_log_ids: Vec::new(),
            source_card_ids: Vec::new(),
            created_from: vec!["research_debt:quality".to_string()],
            next_action: Some("Render final answer.".to_string()),
            detail: "quality debt still open".to_string(),
        };
        let fingerprint = work_item_fingerprint(
            &draft.kind,
            &artifacts,
            &draft.phase_ids,
            &draft.claim_log_ids,
            &draft.source_card_ids,
            &draft.debt_ids,
        );
        let previous = ResearchWorkItem {
            id: FINAL_ANSWER_RENDER_WORK_ITEM_ID.to_string(),
            kind: ResearchWorkItemKind::FinalAnswerRender,
            status: ResearchWorkItemStatus::Completed,
            target: Some("final_answer".to_string()),
            blocking: true,
            debt_ids: vec!["quality".to_string()],
            dependencies: Vec::new(),
            phase_ids: Vec::new(),
            claim_log_ids: Vec::new(),
            source_card_ids: Vec::new(),
            created_from: vec!["research_debt:quality".to_string()],
            max_attempts: 2,
            attempt_count: 1,
            last_wave: Some(1),
            last_error: None,
            next_action: None,
            input_fingerprint: Some(fingerprint),
            output_fingerprint: None,
            detail: Some("completed".to_string()),
        };

        let item = realize_work_item(draft, Some(&previous), &artifacts, false);

        assert_eq!(item.status, ResearchWorkItemStatus::Completed);
        assert_eq!(item.attempt_count, 1);
        assert!(item.last_error.is_none());
    }

    #[test]
    fn debt_mapping_covers_requested_contract_work_item_kinds() {
        let artifact_debt = ResearchDebtItem {
            id: "artifact".to_string(),
            failed_gate: Some("artifact_quality".to_string()),
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };
        let readiness_debt = ResearchDebtItem {
            id: "claim-gap".to_string(),
            failed_gate: Some("narrative_enrichment_readiness".to_string()),
            missing_evidence: "claim log support missing".to_string(),
            next_check_actions: vec![
                "Repair Source Cards and phase-specific supported Claim Log rows before event-card enrichment.".to_string(),
            ],
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };
        let enrichment_debt = ResearchDebtItem {
            id: "event".to_string(),
            failed_gate: Some("event_card_enrichment".to_string()),
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };
        let richness_debt = ResearchDebtItem {
            id: "richness".to_string(),
            failed_gate: Some("historical_richness".to_string()),
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };
        let quality_debt = ResearchDebtItem {
            id: "gate".to_string(),
            failed_gate: Some("quality_gate".to_string()),
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };

        assert!(mapped_work_item_kinds_for_debt(&artifact_debt)
            .contains(&ResearchWorkItemKind::ArtifactStabilization));
        let readiness_kinds = mapped_work_item_kinds_for_debt(&readiness_debt);
        assert!(readiness_kinds.contains(&ResearchWorkItemKind::SourceCardRepair));
        assert!(readiness_kinds.contains(&ResearchWorkItemKind::ClaimLogRepair));
        assert!(readiness_kinds.contains(&ResearchWorkItemKind::PhaseClaimReadiness));
        let enrichment_kinds = mapped_work_item_kinds_for_debt(&enrichment_debt);
        assert!(enrichment_kinds.contains(&ResearchWorkItemKind::EventCardEnrichment));
        assert!(enrichment_kinds.contains(&ResearchWorkItemKind::CausalContinuityReview));
        assert!(mapped_work_item_kinds_for_debt(&richness_debt)
            .contains(&ResearchWorkItemKind::ResearchAcceptanceReview));
        let quality_kinds = mapped_work_item_kinds_for_debt(&quality_debt);
        assert!(quality_kinds.contains(&ResearchWorkItemKind::FinalAnswerRender));
        assert!(quality_kinds.contains(&ResearchWorkItemKind::ResearchAcceptanceReview));
    }

    #[test]
    fn queue_debt_ids_and_created_from_are_sanitized() {
        let unsafe_id =
            "https://example.com/?api_key=secret system prompt http://localhost:11434/internal";
        let debt = ResearchDebtItem {
            id: unsafe_id.to_string(),
            failed_gate: Some("event_card_enrichment".to_string()),
            status: "open".to_string(),
            ..ResearchDebtItem::default()
        };

        let index = build_queue_debt_index(&[debt]);
        let ids = index
            .ids_by_kind
            .get(&ResearchWorkItemKind::EventCardEnrichment)
            .expect("event debt should map");
        let created_from = index
            .created_from_by_kind
            .get(&ResearchWorkItemKind::EventCardEnrichment)
            .expect("created_from should map");

        assert_eq!(ids.len(), 1);
        assert!(ids[0].starts_with("ref-"));
        assert!(!ids[0].contains("http"));
        assert!(created_from[0].starts_with("research_debt:ref-"));
        assert!(!created_from[0].contains("localhost"));
        assert!(!created_from[0].contains("prompt"));
    }

    #[test]
    fn queue_secret_like_ids_are_hashed_without_url_markers() {
        let secret_like_ids = [
            "api_key_sk_live_example",
            "client_secret_xyz",
            "AuthorizationBearerToken",
            "password-reset-token",
        ];

        for id in secret_like_ids {
            let sanitized = sanitize_queue_id(id);
            assert!(
                sanitized.starts_with("ref-"),
                "{id} should hash instead of persisting verbatim"
            );
            assert!(!sanitized.to_ascii_lowercase().contains("token"));
            assert!(!sanitized.to_ascii_lowercase().contains("secret"));
            assert!(!sanitized.to_ascii_lowercase().contains("api_key"));
            assert!(!sanitized.to_ascii_lowercase().contains("password"));
        }
    }

    #[test]
    fn queue_prompt_and_diagnostic_detail_markers_are_redacted() {
        let sensitive_actions = [
            "resolved prompt: include prior source diagnostics",
            "system prompt: obey hidden instructions",
            "source_diagnostics: fetch metadata",
            "controller artifacts json: raw state",
        ];

        for action in sensitive_actions {
            let debt = ResearchDebtItem {
                id: "safe-debt-id".to_string(),
                failed_gate: Some("narrative_enrichment_readiness".to_string()),
                missing_evidence: "claim log".to_string(),
                status: "open".to_string(),
                next_check_actions: vec![action.to_string()],
                ..ResearchDebtItem::default()
            };

            let index = build_queue_debt_index(&[debt]);
            let next_action = index
                .next_action_by_kind
                .get(&ResearchWorkItemKind::ClaimLogRepair)
                .expect("next action should map");

            assert_eq!(next_action, "[redacted-controller-detail]");
        }
    }

    #[test]
    fn queue_dot_delimited_private_host_ids_are_hashed() {
        let unsafe_ids = [
            "source.127.0.0.1",
            "path.metadata.google.internal",
            "endpoint.169.254.169.254",
            "source.2130706433",
            "source.0x7f000001",
            "source.0177.0.0.1",
        ];

        for id in unsafe_ids {
            let sanitized = sanitize_queue_id(id);
            assert!(
                sanitized.starts_with("ref-"),
                "{id} should hash instead of persisting verbatim"
            );
            assert!(!sanitized.contains("127."));
            assert!(!sanitized.contains("metadata"));
            assert!(!sanitized.contains("169.254"));
        }
    }

    #[test]
    fn queue_next_actions_redact_structured_private_hosts() {
        let benign_progress_details = [
            "strategy=historical selected=0 accepted=0 debt=0",
            "source_cards=10 debt=0",
            "wave=2 open_blocking=0 completed=10",
        ];
        for detail in benign_progress_details {
            assert_eq!(sanitize_queue_detail(detail), detail);
        }

        let private_actions = [
            "retry with host=127.0.0.1",
            "inspect source:[::1]",
            "read json={\\\"source\\\":\\\"169.254.169.254/latest\\\"}",
            "check endpoint=metadata.google.internal",
            "retry source:127.0.0.1/latest",
            "retry source:metadata.google.internal",
            "retry source:fd00::1/latest",
            "inspect $.source.127.0.0.1",
            "inspect path.metadata.google.internal",
            "inspect source:2130706433/latest",
            "inspect source:0x7f000001/latest",
            "inspect source:0177.0.0.1/latest",
            "inspect source:[::ffff:127.0.0.1]",
            "retry host=::ffff:7f00:1",
        ];

        for action in private_actions {
            let debt = ResearchDebtItem {
                id: "safe-debt-id".to_string(),
                failed_gate: Some("event_card_enrichment".to_string()),
                status: "open".to_string(),
                next_check_actions: vec![action.to_string()],
                ..ResearchDebtItem::default()
            };

            let index = build_queue_debt_index(&[debt]);
            let next_action = index
                .next_action_by_kind
                .get(&ResearchWorkItemKind::EventCardEnrichment)
                .expect("next action should map");

            assert_eq!(next_action, "[redacted-private-host]");
        }
    }

    #[test]
    fn refresh_research_iteration_state_exposes_richer_contract_fields() {
        let task = TaskInfo {
            id: 1,
            original_name: "Historical queue".to_string(),
            status: "researching".to_string(),
            file_prefix: Some("[AI-Research]".to_string()),
            file_type: Some("md".to_string()),
            research_topic: Some("French Revolution republican transition".to_string()),
            research_intensity: Some("high".to_string()),
            quality_depth: Some("strict".to_string()),
            ..sample_task()
        };
        let artifacts = ResearchControllerArtifacts {
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
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Republican transition".to_string(),
                    purpose: Some("Monarchy collapse and republic declaration".to_string()),
                    expected_claim_log_ids: vec!["C1".to_string()],
                    expected_source_card_ids: vec!["S1".to_string()],
                    ..NarrativeSectionOutlineItem::default()
                }],
                event_cards: vec![NarrativeEventCard {
                    label: "Republican transition".to_string(),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: vec!["S1".to_string()],
                    ..NarrativeEventCard::default()
                }],
                ..NarrativeState::default()
            }),
            research_debt: vec![ResearchDebtItem {
                id: "event-gap".to_string(),
                failed_gate: Some("event_card_enrichment".to_string()),
                missing_evidence: "causal continuity is still thin".to_string(),
                next_check_actions: vec!["Retry bounded event-card enrichment.".to_string()],
                status: "open".to_string(),
                ..ResearchDebtItem::default()
            }],
            ..ResearchControllerArtifacts::default()
        };

        let iteration_state = refresh_research_iteration_state(
            &task,
            "[AI-Research]",
            "French Revolution republican transition",
            &artifacts,
        );
        let event_item = iteration_state
            .work_items
            .iter()
            .find(|item| item.kind == ResearchWorkItemKind::EventCardEnrichment)
            .expect("event-card enrichment item should be present");

        assert_eq!(
            event_item.target.as_deref(),
            Some("narrative_state.event_cards")
        );
        assert!(event_item.blocking);
        assert!(event_item.max_attempts >= 1);
        assert!(!event_item.created_from.is_empty());
        assert!(event_item.input_fingerprint.is_some());
        assert_eq!(event_item.phase_ids, vec!["SO1".to_string()]);
        assert_eq!(event_item.claim_log_ids, vec!["C1".to_string()]);
        assert_eq!(event_item.source_card_ids, vec!["S1".to_string()]);
        assert!(event_item
            .dependencies
            .contains(&PHASE_PLAN_BUILD_WORK_ITEM_ID.to_string()));
        assert!(iteration_state
            .budget
            .as_ref()
            .is_some_and(|budget| budget.max_total_work_item_attempts >= 1));
    }

    #[test]
    fn terminal_status_is_no_progress_when_blocking_non_user_work_remains() {
        let iteration_state = ResearchIterationState {
            work_items: vec![ResearchWorkItem {
                id: SOURCE_CARD_REPAIR_WORK_ITEM_ID.to_string(),
                kind: ResearchWorkItemKind::SourceCardRepair,
                status: ResearchWorkItemStatus::Blocked,
                target: Some("source_cards".to_string()),
                blocking: true,
                debt_ids: vec!["D1".to_string()],
                dependencies: Vec::new(),
                phase_ids: Vec::new(),
                claim_log_ids: Vec::new(),
                source_card_ids: Vec::new(),
                created_from: vec!["research_debt:quality_gate:D1".to_string()],
                max_attempts: 0,
                attempt_count: 0,
                last_wave: None,
                last_error: Some("source cards missing".to_string()),
                next_action: Some("Repair Source Cards before event-card enrichment.".to_string()),
                input_fingerprint: Some("fnv1a:1".to_string()),
                output_fingerprint: None,
                detail: Some("source cards missing".to_string()),
            }],
            ..ResearchIterationState::default()
        };

        assert_eq!(
            terminal_status_without_pending(&iteration_state),
            ResearchRunTerminalStatus::NoProgress
        );
    }

    #[test]
    fn terminal_status_is_partial_trusted_when_only_nonblocking_work_remains() {
        let iteration_state = ResearchIterationState {
            work_items: vec![ResearchWorkItem {
                id: FINAL_ANSWER_RENDER_WORK_ITEM_ID.to_string(),
                kind: ResearchWorkItemKind::FinalAnswerRender,
                status: ResearchWorkItemStatus::Blocked,
                target: Some("final_answer".to_string()),
                blocking: false,
                debt_ids: vec!["D1".to_string()],
                dependencies: Vec::new(),
                phase_ids: Vec::new(),
                claim_log_ids: Vec::new(),
                source_card_ids: Vec::new(),
                created_from: vec!["research_debt:quality_gate:D1".to_string()],
                max_attempts: 0,
                attempt_count: 0,
                last_wave: None,
                last_error: Some("nonblocking render polish remains".to_string()),
                next_action: Some("Optional final prose polish can be retried later.".to_string()),
                input_fingerprint: Some("fnv1a:1".to_string()),
                output_fingerprint: None,
                detail: Some("nonblocking render polish remains".to_string()),
            }],
            ..ResearchIterationState::default()
        };

        assert_eq!(
            terminal_status_without_pending(&iteration_state),
            ResearchRunTerminalStatus::PartialTrusted
        );
    }

    #[test]
    fn terminal_status_is_budget_exhausted_when_attempt_budget_is_spent() {
        let iteration_state = ResearchIterationState {
            work_items: vec![ResearchWorkItem {
                id: EVENT_CARD_ENRICHMENT_WORK_ITEM_ID.to_string(),
                kind: ResearchWorkItemKind::EventCardEnrichment,
                status: ResearchWorkItemStatus::Blocked,
                target: Some("narrative_state.event_cards".to_string()),
                blocking: true,
                debt_ids: vec!["D1".to_string()],
                dependencies: Vec::new(),
                phase_ids: Vec::new(),
                claim_log_ids: Vec::new(),
                source_card_ids: Vec::new(),
                created_from: vec!["historical_event_card_enrichment_applicable".to_string()],
                max_attempts: 2,
                attempt_count: 2,
                last_wave: Some(2),
                last_error: Some("work item attempt budget exhausted".to_string()),
                next_action: Some("Retry bounded event-card enrichment.".to_string()),
                input_fingerprint: Some("fnv1a:1".to_string()),
                output_fingerprint: Some("fnv1a:1".to_string()),
                detail: Some("work item attempt budget exhausted".to_string()),
            }],
            ..ResearchIterationState::default()
        };

        assert_eq!(
            terminal_status_without_pending(&iteration_state),
            ResearchRunTerminalStatus::BudgetExhausted
        );
    }

    #[test]
    fn non_historical_work_queue_refresh_is_noop() {
        let task = TaskInfo {
            id: 1,
            original_name: "Technology comparison".to_string(),
            status: "researching".to_string(),
            file_prefix: Some("[AI-Research]".to_string()),
            file_type: Some("md".to_string()),
            research_topic: Some("Rust web framework implementation tradeoffs".to_string()),
            research_intensity: Some("high".to_string()),
            quality_depth: Some("strict".to_string()),
            ..sample_task()
        };
        let artifacts = ResearchControllerArtifacts::default();

        let iteration_state = refresh_research_iteration_state(
            &task,
            "[AI-Research]",
            "Rust web framework implementation tradeoffs",
            &artifacts,
        );

        assert!(iteration_state.work_items.is_empty());
        assert!(iteration_state.terminal_status.is_none());
    }

    #[tokio::test]
    async fn bounded_work_queue_persists_iteration_state_and_checkpoints() {
        let dir = temp_test_dir("research-work-queue-persists-checkpoints");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Historical queue', 'researching', '[AI-Research]', 'md', 'French Revolution republican transition', 'high', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&db)
            .await
            .unwrap();
        let artifacts = ResearchControllerArtifacts {
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
                section_outline: vec![NarrativeSectionOutlineItem {
                    id: "SO1".to_string(),
                    heading: "Republican transition".to_string(),
                    purpose: Some(
                        "1792-1793 monarchy collapse and republic declaration in Paris"
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
        persist_research_controller_artifacts(&state, task_id, &artifacts).await;

        let mut controller_events = Vec::new();
        let iteration_state = run_bounded_research_work_queue(
            &state,
            &task,
            &FakeRuntime,
            "fake-model",
            "cli",
            "[AI-Research]",
            "md",
            "## 최종 답변
French Revolution republican transition.
",
            "French Revolution republican transition",
            1,
            1,
            &mut controller_events,
        )
        .await;

        assert!(iteration_state.current_wave >= 1);
        assert!(!iteration_state.work_items.is_empty());
        assert!(!iteration_state.checkpoints.is_empty());
        assert!(iteration_state.budget.is_some());
        assert!(iteration_state
            .checkpoints
            .iter()
            .all(|checkpoint| checkpoint.input_fingerprint.is_some()));
        assert_eq!(
            iteration_state
                .budget
                .as_ref()
                .map(|budget| budget.model_calls_used),
            Some(0),
            "phase-state work is deterministic and must not consume model-call budget"
        );

        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let persisted_state = persisted
            .research_iteration_state
            .expect("iteration state should persist");
        assert_eq!(persisted_state.current_wave, iteration_state.current_wave);
        assert_eq!(
            persisted_state.checkpoints.len(),
            iteration_state.checkpoints.len()
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn artifact_stabilization_route_persists_safe_ledgers() {
        let dir = temp_test_dir("research-work-queue-artifact-stabilization-route");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Historical queue', 'researching', '[AI-Research]', 'md', 'French Revolution republican transition', 'high', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&db)
            .await
            .unwrap();
        persist_research_controller_artifacts(
            &state,
            task_id,
            &ResearchControllerArtifacts {
                research_debt: vec![ResearchDebtItem {
                    id: "artifact".to_string(),
                    failed_gate: Some("artifact_quality".to_string()),
                    status: "open".to_string(),
                    ..ResearchDebtItem::default()
                }],
                ..ResearchControllerArtifacts::default()
            },
        )
        .await;

        let (status, detail, next_action, calls) = run_artifact_stabilization_work_item(
            &state,
            &task,
            "md",
            &artifact_markdown_output(),
            "French Revolution republican transition",
            &[],
        )
        .await;

        assert_eq!(status, ResearchWorkItemStatus::Completed);
        assert!(detail.contains("source_cards=1"));
        assert!(next_action.is_some());
        assert_eq!(calls, 0);
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        assert_eq!(persisted.source_cards.len(), 1);
        assert_eq!(persisted.claim_log.len(), 1);
        assert!(persisted.narrative_state.is_some());

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn deterministic_repair_routes_block_or_complete_without_unsupported_failure() {
        let dir = temp_test_dir("research-work-queue-deterministic-routes");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Historical queue', 'researching', '[AI-Research]', 'md', 'French Revolution republican transition', 'high', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&db)
            .await
            .unwrap();

        let (source_status, source_detail, source_next, _) =
            run_source_card_repair_work_item(&state, &task, &[]).await;
        assert_eq!(source_status, ResearchWorkItemStatus::Blocked);
        assert!(!source_detail.contains("unsupported_queue_route"));
        assert!(source_next
            .as_deref()
            .is_some_and(|next| next.contains("Provide public Source Cards")));

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
                    actors: vec!["Paris revolutionaries".to_string()],
                    region_or_front: Some("Paris".to_string()),
                    trigger: Some("monarchy collapse".to_string()),
                    development: Some("The political center shifted from monarchy to republic.".to_string()),
                    outcome: Some("The republic was declared.".to_string()),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: vec!["S1".to_string()],
                    causal_spine: vec![NarrativeCausalSpineStep {
                        step_type: "forcing_factor".to_string(),
                        description: "Monarchy collapse forced republican institutional change.".to_string(),
                        epistemic_status: Some("interpretation".to_string()),
                        reasoning: Some("The supported claim ties the transition to the republic declaration.".to_string()),
                        claim_log_ids: vec!["C1".to_string()],
                        source_ids: vec!["S1".to_string()],
                        ..NarrativeCausalSpineStep::default()
                    }],
                    ..NarrativeEventCard::default()
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        persist_research_controller_artifacts(&state, task_id, &artifacts).await;

        let (claim_status, claim_detail, _, _) =
            run_claim_log_repair_work_item(&state, &task, &artifact_markdown_output(), &[]).await;
        assert_eq!(claim_status, ResearchWorkItemStatus::Completed);
        assert!(!claim_detail.contains("unsupported_queue_route"));

        let (causal_status, causal_detail, _, _) =
            run_causal_continuity_review_work_item(&state, task_id).await;
        assert_eq!(causal_status, ResearchWorkItemStatus::Completed);
        assert!(!causal_detail.contains("unsupported_queue_route"));

        let (render_status, render_detail, _, _) = run_final_answer_render_work_item(
            &state,
            task_id,
            "## 최종 답변\nFrench Revolution republican transition.",
            "[AI-Research]",
            "md",
        )
        .await;
        assert_eq!(render_status, ResearchWorkItemStatus::Completed);
        assert!(!render_detail.contains("unsupported_queue_route"));

        artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist after render");
        assert!(!artifacts.warnings.iter().any(|warning| {
            warning.contains("provider_payload") || warning.contains("resolved_prompt")
        }));

        let (acceptance_status, acceptance_detail, _, _) =
            run_research_acceptance_review_work_item(
                &state,
                task_id,
                "## 최종 답변\nFrench Revolution republican transition.",
                "[AI-Research]",
                "md",
            )
            .await;
        assert_ne!(acceptance_status, ResearchWorkItemStatus::Failed);
        assert!(!acceptance_detail.contains("unsupported_queue_route"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn validation_acceptance_forces_terminal_accepted_and_skips_leftover_work() {
        let dir = temp_test_dir("research-work-queue-accepted-terminal");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Historical queue', 'researching', '[AI-Research]', 'md', 'French Revolution republican transition', 'high', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let artifacts = ResearchControllerArtifacts {
            research_iteration_state: Some(ResearchIterationState {
                terminal_status: Some(ResearchRunTerminalStatus::NoProgress),
                work_items: vec![ResearchWorkItem {
                    id: EVENT_CARD_ENRICHMENT_WORK_ITEM_ID.to_string(),
                    kind: ResearchWorkItemKind::EventCardEnrichment,
                    status: ResearchWorkItemStatus::Blocked,
                    target: Some("narrative_state.event_cards".to_string()),
                    blocking: true,
                    debt_ids: vec!["D1".to_string()],
                    dependencies: Vec::new(),
                    phase_ids: Vec::new(),
                    claim_log_ids: Vec::new(),
                    source_card_ids: Vec::new(),
                    created_from: vec!["historical_event_card_enrichment_applicable".to_string()],
                    max_attempts: 2,
                    attempt_count: 1,
                    last_wave: Some(1),
                    last_error: Some("narrative enrichment blocked".to_string()),
                    next_action: Some("Retry bounded event-card enrichment.".to_string()),
                    input_fingerprint: Some("fnv1a:1".to_string()),
                    output_fingerprint: None,
                    detail: Some("narrative enrichment blocked".to_string()),
                }],
                ..ResearchIterationState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        persist_research_controller_artifacts(&state, task_id, &artifacts).await;

        mark_research_work_queue_accepted(&state, task_id).await;

        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let persisted_state = persisted
            .research_iteration_state
            .expect("iteration state should persist");
        assert_eq!(
            persisted_state.terminal_status,
            Some(ResearchRunTerminalStatus::Accepted)
        );
        assert_eq!(
            persisted_state.work_items[0].status,
            ResearchWorkItemStatus::Skipped
        );
        assert!(persisted_state.work_items[0].last_error.is_none());

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    fn sample_task() -> TaskInfo {
        TaskInfo {
            id: 0,
            file_id: None,
            filename: None,
            original_name: String::new(),
            status: String::new(),
            error_message: None,
            created_at: chrono::Utc::now(),
            deleted_at: None,
            model: None,
            system_prompt: None,
            user_prompt: None,
            source_file_ids: None,
            source_filenames: None,
            file_prefix: None,
            file_type: None,
            cleanup_files: None,
            research_type: None,
            research_mode: None,
            research_format: None,
            research_topic: None,
            research_instructions: None,
            prompt_version: None,
            resolved_system_prompt: None,
            resolved_user_prompt: None,
            web_search_requested: None,
            web_search_provider: None,
            engine_preset_id: None,
            engine_preset_name: None,
            engine_kind: None,
            resolved_model: None,
            research_intensity: None,
            fallback_used: None,
            fallback_reason: None,
            quality_current_iteration: None,
            quality_max_iterations: None,
            quality_status: None,
            quality_depth: None,
            quality_last_failure: None,
            research_controller_stage: None,
            research_controller_iteration: None,
            research_controller_max_iterations: None,
            research_controller_artifacts_json: None,
            research_source_diagnostics_json: None,
        }
    }
}
