use super::*;
use liquid_research_classic::{
    historical_phase_state_should_run, push_unique_warning, stabilize_historical_phase_state,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PhaseStateStageReport {
    pub(super) strategy: Option<&'static str>,
    pub(super) phase_count: usize,
    pub(super) ready_phase_count: usize,
    pub(super) repaired_event_cards: usize,
    pub(super) skipped_reason: Option<String>,
}

pub(super) async fn run_phase_state_stage(
    state: &AppState,
    task: &TaskInfo,
    file_prefix: &str,
    user_prompt: &str,
    iteration: i64,
    max_iterations: i64,
    controller_events: &mut Vec<ResearchControllerEvent>,
) -> PhaseStateStageReport {
    let evidence_subject = research_source_subject_for_task(task, user_prompt);
    if !historical_phase_state_should_run(
        file_prefix,
        task.research_intensity.as_deref(),
        task.quality_depth.as_deref(),
        task.research_topic.as_deref(),
        task.research_instructions.as_deref(),
        Some(evidence_subject),
    ) {
        return PhaseStateStageReport {
            skipped_reason: Some("strategy_not_applicable".to_string()),
            ..PhaseStateStageReport::default()
        };
    }

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PHASE_STATE,
        iteration,
        max_iterations,
        RESEARCH_CONTROLLER_STATUS_RUNNING,
        Some("Constructing engine-owned historical phase state before bounded enrichment."),
        controller_events,
    )
    .await;

    let mut artifacts = load_task_research_artifacts(state, task.id)
        .await
        .unwrap_or_default();
    let before_debt = artifacts.research_debt.len();
    let report = stabilize_historical_phase_state(&mut artifacts, evidence_subject);
    let debt_delta = artifacts.research_debt.len().saturating_sub(before_debt);
    push_unique_warning(
        &mut artifacts.warnings,
        format!(
            "phase_state_historical:phases={} ready={} repaired={} debt={}",
            report.phase_count, report.ready_phase_count, report.repaired_event_cards, debt_delta
        ),
    );
    artifacts.version = RESEARCH_CONTROLLER_ARTIFACT_VERSION;
    artifacts.events = controller_events.clone();
    persist_research_controller_artifacts(state, task.id, &artifacts).await;

    update_research_controller_progress(
        state,
        task.id,
        &task.original_name,
        RESEARCH_STAGE_PHASE_STATE,
        iteration,
        max_iterations,
        RESEARCH_CONTROLLER_STATUS_COMPLETED,
        Some(&format!(
            "Historical phase state ready: phases={}, ready={}, repaired_cards={}",
            report.phase_count, report.ready_phase_count, report.repaired_event_cards
        )),
        controller_events,
    )
    .await;

    PhaseStateStageReport {
        strategy: Some(report.strategy),
        phase_count: report.phase_count,
        ready_phase_count: report.ready_phase_count,
        repaired_event_cards: report.repaired_event_cards,
        skipped_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_test_dir, test_state};
    use liquid_protocol::{
        NarrativeSectionOutlineItem, NarrativeState, ResearchClaimLogEntry,
        ResearchControllerArtifacts, ResearchSourceCard,
    };
    use liquid_storage_sqlite::setup_db;

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

    #[tokio::test]
    async fn phase_state_stage_persists_ready_historical_phase_cards() {
        let dir = temp_test_dir("phase-state-stage-persists-ready-card");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_topic, research_intensity, quality_depth) VALUES ('Historical phase state', 'researching', '[AI-Research]', 'md', 'French Revolution republican transition', 'high', 'strict')",
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
        let report = run_phase_state_stage(
            &state,
            &task,
            "[AI-Research]",
            "French Revolution republican transition",
            1,
            1,
            &mut controller_events,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");
        let card = persisted
            .narrative_state
            .as_ref()
            .and_then(|state| {
                state
                    .event_cards
                    .iter()
                    .find(|card| card.label == "Republican transition")
            })
            .expect("phase state should create a ready card");

        assert_eq!(report.strategy, Some("historical_phase_state"));
        assert_eq!(report.phase_count, 1);
        assert_eq!(report.ready_phase_count, 1);
        assert_eq!(report.repaired_event_cards, 1);
        assert_eq!(card.timeframe.as_deref(), Some("1792-1793년"));
        assert!(card
            .region_or_front
            .as_deref()
            .is_some_and(|value| value.contains("Paris")));
        assert_eq!(card.claim_log_ids, vec!["C1".to_string()]);
        assert_eq!(card.source_ids, vec!["S1".to_string()]);
        assert!(card.development.is_none());
        assert!(card.outcome.is_none());
        assert!(persisted
            .warnings
            .iter()
            .any(|warning| warning.starts_with("historical_phase_state:phases=1 ready=1")));
        assert!(persisted
            .warnings
            .iter()
            .any(|warning| warning == "historical_phase_state_reasons:ready=1"));
        assert!(persisted.warnings.iter().any(
            |warning| warning.starts_with("phase_state_historical:phases=1 ready=1 repaired=1")
        ));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
