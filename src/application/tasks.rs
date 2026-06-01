use crate::application::ai_runtime::{execute_task_logic, load_research_source_diagnostics};
use crate::application::file_tags::{assign_file_tag_labels, system_tag_labels_for_task};
use crate::application::ports::{
    ArtifactFinalizer, ArtifactValidator, ModelRuntime, ModelRuntimeRequest,
    ResearchDiagnosticsRepository, SourceAcquisition, TaskRepository,
};
use crate::application::scraping::{
    friendly_scrape_failure_message, parse_scrape_task_input, scrape_url_to_markdown, ScrapeResult,
};
use crate::config::setup_data_dir;
#[cfg(test)]
use crate::contracts::{
    NarrativeCausalLink, NarrativeEventCard, NarrativeOpenGap, NarrativeState,
    ResearchClaimLogEntry, ResearchConflictMapEntry, ResearchSourceCard,
};
use crate::contracts::{
    ResearchControllerArtifacts, ResearchControllerEvent, ResearchDebtItem,
    ResearchQualityGateArtifact, ResearchSourceCoverageMiss, ResearchSourceDiagnosticsEnvelope,
    TaskInfo, TaskMetadata, TaskUpdateEvent, PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING,
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING,
};
use crate::state::{AppState, BenchmarkFixture};
#[cfg(test)]
use liquid_acquisition::normalize_result_url;
use liquid_acquisition::{collect_transient_repair_search_hints, RepairSearchHint};
use liquid_research_core::{
    finalize_research_output, normalize_ai_output,
    parse_research_artifact_block_with_budget_repair,
    repair_historical_planning_scaffold_from_visible_output, validate_research_artifacts,
    validate_research_output, validate_transient_repair_hint_evidence_provenance,
    ResearchQualityContext,
};
use liquid_runtime::cli_launcher::CliLaunchMode;
use liquid_storage_sqlite::{setup_db, strip_legacy_title_metadata};
use std::collections::HashSet;
use std::{path::PathBuf, sync::Arc};
use tokio::fs;
use tokio::sync::{broadcast, Notify};
use uuid::Uuid;

mod benchmark_fixture;
mod benchmark_runtime;
mod completion_shell;
mod execution_shell;
mod helpers;
mod lifecycle_policy;
mod narrative_enrichment;
mod narrative_merge;
mod phase_state;
mod queue_workflow;
mod repair_helpers;
mod retry_policy;
mod runtime_adapters;
use self::benchmark_fixture::{
    benchmark_engine_kind, benchmark_model_name, benchmark_queue_lane, benchmark_research_mode,
    build_benchmark_fixture,
};
#[cfg(test)]
use self::benchmark_runtime::benchmark_ai_task_timeout_secs;
#[allow(unused_imports)]
pub use self::benchmark_runtime::{
    replay_research_benchmark_case, replay_research_benchmark_case_debug,
    run_research_benchmark_case, run_research_benchmark_case_debug,
    ResearchBenchmarkDebugCaseResult,
};
#[allow(unused_imports)]
use self::completion_shell::*;
#[allow(unused_imports)]
use self::execution_shell::*;
use self::helpers::{
    scrape_task_identity_name, task_update_event, task_update_event_from_progress,
    TaskProgressSnapshot,
};
use self::lifecycle_policy::lifecycle_target_status;
pub(crate) use self::lifecycle_policy::{
    is_delete_cancellable_task_status, DELETE_CANCELLABLE_TASK_STATUS_SQL_LIST,
    TASK_CANCELLED_MESSAGE,
};
#[allow(unused_imports)]
use self::narrative_enrichment::*;
#[allow(unused_imports)]
use self::narrative_merge::*;
#[allow(unused_imports)]
use self::phase_state::*;
#[allow(unused_imports)]
use self::queue_workflow::*;
#[allow(unused_imports)]
pub(crate) use self::queue_workflow::{run_ai_task, spawn_ai_queue_workers};
#[allow(unused_imports)]
use self::repair_helpers::*;
pub(crate) use self::retry_policy::{
    is_retryable_task, merge_retry_engine_metadata, retry_task_metadata,
};
use self::runtime_adapters::{AppModelRuntime, DefaultArtifactProcessor, DefaultSourceAcquisition};

const RESEARCH_CONTROLLER_ARTIFACT_VERSION: u8 = 1;
const RESEARCH_CONTROLLER_ARTIFACT_LIMIT: usize = 40;
const RESEARCH_STAGE_PLAN: &str = "plan";
const RESEARCH_STAGE_SEARCH: &str = "search";
const RESEARCH_STAGE_SOURCE_CARDS: &str = "source_cards";
const RESEARCH_STAGE_CLAIM_LOG: &str = "claim_log";
const RESEARCH_STAGE_DRAFT: &str = "draft";
const RESEARCH_STAGE_QUALITY_GATE: &str = "quality_gate";
const RESEARCH_STAGE_PHASE_STATE: &str = "phase_state";
const RESEARCH_STAGE_NARRATIVE_ENRICHMENT: &str = "narrative_enrichment";
const RESEARCH_STAGE_REPAIR_PLANNING: &str = "repair_planning";
const RESEARCH_STAGE_EVIDENCE_REPAIR: &str = "evidence_repair";
const RESEARCH_STAGE_FINAL: &str = "final";
const RESEARCH_STAGE_UNTRUSTED: &str = "untrusted";
const RESEARCH_CONTROLLER_STATUS_RUNNING: &str = "running";
const RESEARCH_CONTROLLER_STATUS_COMPLETED: &str = "completed";
const RESEARCH_CONTROLLER_STATUS_FAILED: &str = "failed";
#[cfg(test)]
const MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR: &str =
    "missing machine-readable research artifact JSON block";
const MAX_REPAIR_SEARCH_QUERIES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueLane {
    Local,
    Cloud,
}

pub use liquid_protocol::{
    ResearchBenchmarkCaseInput, ResearchBenchmarkCaseResult, ResearchBenchmarkMode,
    ResearchReplayCaseInput,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::tasks::lifecycle_policy::is_delete_cancellable_task_status;
    use crate::contracts::{
        ResearchSourceCandidateReport, ResearchSourcePackReport, ResearchSourceQueryReport,
    };
    use crate::server::tasks::{delete_task, list_tasks, retry_task};
    use crate::test_support::{response_status, temp_test_dir, test_server_context, test_state};
    use axum::{
        body::to_bytes,
        extract::{Path, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    };
    use liquid_protocol::RetryTaskPayload;
    use liquid_storage_sqlite::setup_db;
    use tokio::time::{sleep, Duration};

    fn task_snapshot_fingerprint(entries: &[(&str, String)]) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for (label, value) in entries {
            for byte in label
                .as_bytes()
                .iter()
                .chain(b"\0".iter())
                .chain(value.len().to_string().as_bytes().iter())
                .chain(b"\0".iter())
                .chain(value.as_bytes().iter())
            {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    fn scaffold_test_source_pack(count: usize) -> ResearchSourcePackReport {
        ResearchSourcePackReport {
            subject: Some("local pi scaffold subject".to_string()),
            status: "success".to_string(),
            reason: None,
            queries: Vec::new(),
            seeded_source_count: 0,
            discovered_source_count: count,
            adopted_source_count: count,
            adopted_candidates: (1..=count)
                .map(|idx| ResearchSourceCandidateReport {
                    title: format!("Official Source {idx}"),
                    url: format!("https://example{idx}.gov/source/{idx}"),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: None,
                    rejection_reason: None,
                })
                .collect(),
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: None,
        }
    }

    fn scaffolded_local_pi_source_card(idx: usize) -> ResearchSourceCard {
        ResearchSourceCard {
            id: format!("SP{idx}"),
            url: format!("https://example{idx}.gov/source/{idx}"),
            title: format!("Official Source {idx}"),
            source_class: "official_or_primary".to_string(),
            accessed_at: None,
            extracted_facts: vec![
                crate::contracts::PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT.to_string(),
            ],
            limitation: Some(
                crate::contracts::PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION.to_string(),
            ),
            diagnostics_ref: Some(
                crate::contracts::PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF
                    .to_string(),
            ),
            confidence: Some("high".to_string()),
        }
    }

    #[test]
    fn task_lifecycle_status_policy_is_frozen() {
        assert_eq!(
            lifecycle_policy::DELETE_CANCELLABLE_TASK_STATUSES,
            &[
                "queued",
                "processing",
                "translating",
                "researching",
                "scraping"
            ]
        );
        assert_eq!(
            lifecycle_policy::ABORT_RECOVERY_TASK_STATUSES,
            &["processing", "translating", "researching", "scraping"]
        );
        assert!(is_delete_cancellable_task_status("queued"));
        assert!(!lifecycle_policy::is_abort_recovery_task_status("queued"));
        assert!(!is_delete_cancellable_task_status("completed"));
        assert_eq!(target_status_for_prefix("[KO]"), "translating");
        assert_eq!(target_status_for_prefix("[Research]"), "researching");
        assert_eq!(target_status_for_prefix("[AI-Research]"), "researching");
        assert_eq!(target_status_for_prefix("[Scrape]"), "scraping");
        assert_eq!(target_status_for_prefix("[Scrape+KO]"), "scraping");
        assert_eq!(target_status_for_prefix("[Other]"), "processing");
    }

    #[tokio::test]
    async fn abort_recovery_marks_only_active_execution_statuses_interrupted() {
        let dir = temp_test_dir("abort-recovery-contract");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let statuses = [
            "queued",
            "processing",
            "translating",
            "researching",
            "scraping",
            "completed",
            "failed",
            "interrupted",
        ];
        let mut task_ids = Vec::new();
        for status in statuses {
            let task_id = sqlx::query(
                "INSERT INTO tasks (original_name, status, file_prefix) VALUES (?, ?, '[AI-Research]')",
            )
            .bind(format!("Task {status}"))
            .bind(status)
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
            task_ids.push((task_id, status));
        }

        for (task_id, status) in &task_ids {
            queue_workflow::mark_task_interrupted_if_present(&state, *task_id, status).await;
        }

        for (task_id, original_status) in task_ids {
            let (status, error_message) = sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT status, error_message FROM tasks WHERE id = ?",
            )
            .bind(task_id)
            .fetch_one(&db)
            .await
            .unwrap();
            if lifecycle_policy::is_abort_recovery_task_status(original_status) {
                assert_eq!(status, "interrupted");
                assert_eq!(error_message.as_deref(), Some(TASK_CANCELLED_MESSAGE));
            } else {
                assert_eq!(status, original_status);
                assert_eq!(error_message, None);
            }
        }

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn delete_task_cancels_running_task_and_keeps_interrupted_row() {
        let dir = temp_test_dir("cancel-running-task");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let server = test_server_context(Arc::clone(&state));
        let mut rx = state.tx.subscribe();
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix) VALUES ('Running', 'researching', '[Research]')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let active_task = tokio::spawn(async {
            sleep(Duration::from_secs(60)).await;
        });
        state
            .active_tasks
            .lock()
            .await
            .insert(task_id, active_task.abort_handle());

        let status = response_status(delete_task(State(Arc::clone(&server)), Path(task_id)).await);
        let (status_text, error_message) = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, error_message FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_one(&db)
        .await
        .unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(status_text, "interrupted");
        assert_eq!(error_message.as_deref(), Some(TASK_CANCELLED_MESSAGE));
        assert!(active_task.await.unwrap_err().is_cancelled());
        let event = rx.recv().await.unwrap();
        assert_eq!(event.id, task_id);
        assert_eq!(event.status, "interrupted");
        assert_eq!(event.original_name, "Running");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn delete_task_soft_deletes_terminal_task() {
        let dir = temp_test_dir("delete-terminal-task");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let server = test_server_context(Arc::clone(&state));
        let task_id =
            sqlx::query("INSERT INTO tasks (original_name, status) VALUES ('Done', 'completed')")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();

        let status = response_status(delete_task(State(server), Path(task_id)).await);
        let (remaining, visible, deleted_at) = sqlx::query_as::<_, (i64, i64, Option<String>)>(
            "SELECT COUNT(*), SUM(CASE WHEN deleted_at IS NULL THEN 1 ELSE 0 END), MAX(deleted_at) FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_one(&db)
        .await
        .unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(remaining, 1);
        assert_eq!(visible, 0);
        assert!(deleted_at.is_some());

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn research_controller_progress_persists_stage_and_artifacts() {
        let dir = temp_test_dir("research-controller-progress");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix) VALUES ('Research task', 'researching', '[AI-Research]')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut events = Vec::new();

        update_research_controller_progress(
            &state,
            task_id,
            "Research task",
            "quality_gate",
            2,
            5,
            "running",
            Some("checking evidence"),
            &mut events,
        )
        .await;

        let row = sqlx::query_as::<_, (Option<String>, Option<i64>, Option<i64>, Option<String>)>(
            "SELECT research_controller_stage, research_controller_iteration, research_controller_max_iterations, research_controller_artifacts_json FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(row.0.as_deref(), Some("quality_gate"));
        assert_eq!(row.1, Some(2));
        assert_eq!(row.2, Some(5));
        let artifacts: serde_json::Value = serde_json::from_str(&row.3.unwrap()).unwrap();
        assert_eq!(artifacts["version"], RESEARCH_CONTROLLER_ARTIFACT_VERSION);
        assert_eq!(artifacts["events"][0]["stage"], RESEARCH_STAGE_QUALITY_GATE);
        assert_eq!(artifacts["events"][0]["detail"], "checking evidence");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn untrusted_research_output_is_saved_as_low_confidence_completion() {
        let dir = temp_test_dir("untrusted-research-output");
        let uploads = dir.join("uploads");
        std::fs::create_dir_all(&uploads).unwrap();
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), uploads.clone());
        let source_id = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status) VALUES ('source.md', 'Source', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, source_file_ids, source_filenames, file_prefix, file_type, research_type, quality_current_iteration, quality_max_iterations, quality_status, research_controller_stage, research_controller_iteration, research_controller_max_iterations) VALUES ('Bad research', 'researching', ?, '[\"source.md\"]', '[AI-Research]', 'md', 'initial', 2, 2, 'untrusted', 'untrusted', 2, 2)",
        )
        .bind(format!("[{source_id}]"))
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        handle_untrusted_research_completion(
            &state,
            task_id,
            "NO CONFIDENCE\n\nDraft below required evidence threshold.".to_string(),
            "Bad research".to_string(),
            "[AI-Research]",
            "md",
            vec![],
            "source URL count 0 is below required minimum 7",
        )
        .await;

        let task = sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(task.status, "completed");
        assert_eq!(task.quality_status.as_deref(), Some("untrusted"));
        assert_eq!(
            task.research_controller_stage.as_deref(),
            Some(RESEARCH_STAGE_UNTRUSTED)
        );
        assert!(task.error_message.is_none());
        assert!(task
            .quality_last_failure
            .unwrap()
            .contains("source URL count"));
        let filename = task.filename.unwrap();
        assert!(uploads.join(&filename).exists());

        let file_title =
            sqlx::query_scalar::<_, String>("SELECT original_name FROM files WHERE filename = ?")
                .bind(&filename)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(file_title, "Bad research");
        let tags = sqlx::query_as::<_, (String, String)>(
            "SELECT tags.label, file_tags.source
             FROM tags
             JOIN file_tags ON file_tags.tag_id = tags.id
             JOIN files ON files.id = file_tags.file_id
             WHERE files.filename = ?
             ORDER BY tags.label",
        )
        .bind(&filename)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            tags,
            vec![
                ("Low Confidence".to_string(), "task".to_string()),
                ("Research".to_string(), "task".to_string()),
            ]
        );
        let link = sqlx::query_as::<_, (i64, i64, String, Option<i64>)>(
            "SELECT from_file_id, to_file_id, relation_type, created_by_task_id
             FROM document_links",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let output_file_id =
            sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
                .bind(&filename)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            link,
            (
                output_file_id,
                source_id,
                "derived_from".to_string(),
                Some(task_id)
            )
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn completed_synthesis_task_creates_one_document_link_per_source() {
        let dir = temp_test_dir("future-synthesis-links");
        let uploads = dir.join("uploads");
        std::fs::create_dir_all(&uploads).unwrap();
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), uploads.clone());
        let source_one = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status) VALUES ('one.md', 'One', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let source_two = sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status) VALUES ('two.md', 'Two', 'md', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, source_file_ids, source_filenames, file_prefix, file_type, research_type) VALUES ('Synthesis', 'researching', ?, '[\"one.md\",\"two.md\"]', '[Research]', 'md', 'synthesis')",
        )
        .bind(format!("[{source_one},{source_two}]"))
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        handle_task_completion(
            &state,
            task_id,
            Some("Synthesis result with enough body.".to_string()),
            "Synthesis".to_string(),
            "[Research]",
            "md",
            vec![],
            false,
        )
        .await;

        let links = sqlx::query_as::<_, (i64, String)>(
            "SELECT to_file_id, relation_type FROM document_links ORDER BY to_file_id",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            links,
            vec![
                (source_one, "synthesized_from".to_string()),
                (source_two, "synthesized_from".to_string()),
            ]
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn retry_task_can_create_derived_research_with_option_overrides() {
        let dir = temp_test_dir("retry-derived-research");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let server = test_server_context(Arc::clone(&state));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, model, system_prompt, user_prompt, file_prefix, file_type, research_type, research_mode, research_format, research_topic, engine_kind, quality_status, quality_max_iterations, quality_depth) VALUES ('Original research', 'completed', 'cli:codex', 'system', 'user', '[AI-Research]', 'md', 'initial', 'general', 'md', 'topic', 'cli', 'untrusted', 2, 'standard')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let status = response_status(
            retry_task(
                State(Arc::clone(&server)),
                Path(task_id),
                Some(Json(RetryTaskPayload {
                    derive_task: Some(true),
                    engine_preset_id: Some(-1),
                    research_intensity: Some("high".to_string()),
                    research_quality_max_iterations: Some(5),
                    research_quality_depth: Some("strict".to_string()),
                })),
            )
            .await,
        );

        assert_eq!(status, StatusCode::ACCEPTED);
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<String>,
            ),
        >(
            "SELECT original_name, status, model, engine_kind, quality_max_iterations, quality_depth FROM tasks ORDER BY id ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "Original research");
        assert_eq!(rows[0].1, "completed");
        assert_eq!(rows[1].0, "Original research");
        assert_eq!(rows[1].1, "queued");
        assert_eq!(rows[1].2.as_deref(), Some("pi:llama3"));
        assert_eq!(rows[1].3.as_deref(), Some("pi_ollama"));
        assert_eq!(rows[1].4, Some(5));
        assert_eq!(rows[1].5.as_deref(), Some("strict"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn queue_claim_routes_plain_scrape_to_cloud_lane() {
        let dir = temp_test_dir("queue-plain-scrape");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix) VALUES ('https://example.com/article', 'queued', '', '[Scrape]')",
        )
        .execute(&db)
        .await
        .unwrap();

        assert!(claim_next_ai_task(&state, QueueLane::Local).await.is_none());
        let cloud = claim_next_ai_task(&state, QueueLane::Cloud).await.unwrap();
        assert_eq!(cloud.original_name, "https://example.com/article");

        let (status,) = sqlx::query_as::<_, (String,)>("SELECT status FROM tasks WHERE id = ?")
            .bind(cloud.id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(status, "scraping");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn queue_claim_separates_local_and_cloud_lanes() {
        let dir = temp_test_dir("queue-lanes");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES ('Local old', 'queued', 'pi:llama3', '[AI-Research]', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES ('Cloud newer', 'queued', 'cli:codex', '[AI-Research]', 'cli')",
        )
        .execute(&db)
        .await
        .unwrap();

        let cloud = claim_next_ai_task(&state, QueueLane::Cloud).await.unwrap();
        assert_eq!(cloud.original_name, "Cloud newer");
        let local = claim_next_ai_task(&state, QueueLane::Local).await.unwrap();
        assert_eq!(local.original_name, "Local old");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn queue_claim_finds_lane_task_beyond_many_opposite_lane_tasks() {
        let dir = temp_test_dir("queue-lane-starvation");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        for idx in 0..55 {
            sqlx::query(
                "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES (?, 'queued', 'cli:codex', '[AI-Research]', 'cli')",
            )
            .bind(format!("Cloud {idx}"))
            .execute(&db)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES ('Local after cloud burst', 'queued', 'pi:llama3', '[AI-Research]', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap();

        let local = claim_next_ai_task(&state, QueueLane::Local).await.unwrap();
        assert_eq!(local.original_name, "Local after cloud burst");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn queue_claim_marks_missing_model_failed_and_continues_lane_scan() {
        let dir = temp_test_dir("queue-missing-model-contract");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let mut rx = state.tx.subscribe();
        let missing_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES ('Missing model', 'queued', '', '[AI-Research]', NULL)",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO tasks (original_name, status, model, file_prefix, engine_kind) VALUES ('Cloud valid', 'queued', 'cli:codex', '[AI-Research]', 'cli')",
        )
        .execute(&db)
        .await
        .unwrap();

        let cloud = claim_next_ai_task(&state, QueueLane::Cloud).await.unwrap();
        assert_eq!(cloud.original_name, "Cloud valid");
        let (missing_status, missing_error) = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, error_message FROM tasks WHERE id = ?",
        )
        .bind(missing_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(missing_status, "failed");
        assert_eq!(
            missing_error.as_deref(),
            Some("Missing model for queued task")
        );
        let failed_event = rx.recv().await.unwrap();
        assert_eq!(failed_event.id, missing_id);
        assert_eq!(failed_event.status, "failed");
        let claimed_event = rx.recv().await.unwrap();
        assert_eq!(claimed_event.id, cloud.id);
        assert_eq!(claimed_event.status, "researching");

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn list_tasks_omits_raw_prompts_and_sanitizes_public_failure_fields() {
        let dir = temp_test_dir("list-tasks-redaction");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let server = test_server_context(Arc::clone(&state));
        sqlx::query(
            "INSERT INTO tasks (
                original_name, status, file_prefix, error_message, quality_last_failure,
                system_prompt, user_prompt, resolved_system_prompt, resolved_user_prompt,
                research_instructions, research_controller_artifacts_json, research_source_diagnostics_json
            ) VALUES (
                'Scrape task', 'failed', '[Scrape]',
                'provider payload Authorization: Bearer secret-token at /Users/allthatcode/private https://example.com/secret?token=abc http://169.254.169.254/latest/meta-data',
                'resolved user prompt raw_provider_payload={\"secret\":\"abc\"} source:metadata.google.internal',
                'system secret', 'user secret', 'resolved system secret', 'resolved user secret',
                'private research instructions',
                '{\"version\":1,\"source_cards\":[{\"id\":\"S1\",\"url\":\"https://example.com/secret?token=abc\"}]}',
                '{\"version\":1,\"subject\":\"https://example.com/secret?token=abc\",\"scrapes\":[{\"original_url\":\"https://example.com/secret?token=abc\"}]}'
            )",
        )
        .execute(&db)
        .await
        .unwrap();

        let response = list_tasks(State(server)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();

        assert_eq!(first["original_name"], "Scrape task");
        assert!(first.get("system_prompt").is_none());
        assert!(first.get("user_prompt").is_none());
        assert!(first.get("resolved_system_prompt").is_none());
        assert!(first.get("resolved_user_prompt").is_none());
        assert!(first.get("research_instructions").is_none());
        assert!(first.get("research_controller_artifacts_json").is_none());
        assert!(first.get("research_source_diagnostics_json").is_none());
        let serialized = serde_json::to_string(first).unwrap();
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("allthatcode/private"));
        assert!(!serialized.contains("https://example.com/secret?token=abc"));
        assert!(!serialized.contains("169.254.169.254"));
        assert!(!serialized.contains("metadata.google.internal"));
        assert!(!serialized.contains("raw_provider_payload"));
        assert!(serialized.contains("[redacted-payload]"));
        assert!(serialized.contains("[redacted-header]"));
        assert!(serialized.contains("[redacted-local-path]"));
        assert!(serialized.contains("<redacted-url:example.com>"));
        assert!(serialized.contains("[redacted-private-url]"));
        assert!(serialized.contains("<redacted-private-url>"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn scrape_task_persists_fetch_failure_diagnostics_and_actionable_error() {
        let dir = temp_test_dir("scrape-task-fetch-failure");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix) VALUES ('https://nonexistent.invalid/article', 'scraping', '[Scrape]')",
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

        execute_scrape_task(&state, task).await;

        let stored = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT status, error_message, research_source_diagnostics_json FROM tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_one(&db)
        .await
        .unwrap();
        let diagnostics: ResearchSourceDiagnosticsEnvelope =
            serde_json::from_str(stored.2.as_deref().unwrap()).unwrap();

        assert_eq!(stored.0, "failed");
        let error_message = stored.1.expect("expected actionable scrape error message");
        assert!(error_message.contains("페이지를 가져오지 못했습니다."));
        assert!(!error_message.contains("HTTP 상태:"));
        assert!(error_message.contains("진단 분류: fetch_failed"));
        assert!(error_message.contains("기술 세부: Failed to resolve scrape target"));
        assert_eq!(
            diagnostics.subject.as_deref(),
            Some("https://nonexistent.invalid/article")
        );
        assert_eq!(diagnostics.scrapes.len(), 1);
        assert_eq!(diagnostics.scrapes[0].status_class, "fetch_failed");
        assert_eq!(
            diagnostics.scrapes[0].failure_reason.as_deref(),
            Some("Failed to resolve scrape target")
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn normalized_quality_iterations_allow_experiment_depth() {
        assert_eq!(
            normalized_quality_max_iterations("[AI-Research]", Some(15), Some("high")),
            15
        );
        assert_eq!(
            normalized_quality_max_iterations("[AI-Research]", Some(99), Some("high")),
            15
        );
        assert_eq!(
            normalized_quality_max_iterations("[KO]", Some(15), Some("high")),
            1
        );
    }

    #[test]
    fn research_controller_iterations_follow_research_tasks_only() {
        assert_eq!(
            research_controller_max_iterations("[AI-Research]", 15),
            Some(15)
        );
        assert_eq!(research_controller_max_iterations("[Research]", 2), Some(2));
        assert_eq!(research_controller_max_iterations("[KO]", 15), None);
    }

    #[test]
    fn repaired_artifacts_replace_stale_claims_and_close_resolved_debt() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::contracts::ResearchSourceCard {
                id: "S-old".to_string(),
                url: "https://example.com/old".to_string(),
                title: "Old source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["stale fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("low".to_string()),
            }],
            claim_log: vec![crate::contracts::ResearchClaimLogEntry {
                id: "C-old".to_string(),
                claim: "unsupported stale claim".to_string(),
                claim_type: None,
                support_source_card_ids: Vec::new(),
                support_urls: Vec::new(),
                confidence: Some("low".to_string()),
                uncertainty_note: None,
                needs_verification: Some(true),
            }],
            conflict_map: vec![crate::contracts::ResearchConflictMapEntry {
                id: "X-old".to_string(),
                topic: "stale conflict".to_string(),
                conflicting_claim_ids: vec!["C-old".to_string()],
                source_card_ids: vec!["S-old".to_string()],
                resolution_status: Some("unresolved".to_string()),
                resolution_note: None,
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![ResearchDebtItem {
                id: "D-old".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("artifact_quality".to_string()),
                missing_evidence: "claim C-old missing support".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["old source".to_string()],
                next_check_actions: vec!["replace stale claim".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let repaired = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::contracts::ResearchSourceCard {
                id: "S-new".to_string(),
                url: "https://example.com/new".to_string(),
                title: "New source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["supported fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![crate::contracts::ResearchClaimLogEntry {
                id: "C-new".to_string(),
                claim: "supported repaired claim".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S-new".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "passed".to_string(),
                failure_messages: Vec::new(),
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            }),
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, repaired);

        assert_eq!(current.source_cards.len(), 1);
        assert_eq!(current.source_cards[0].id, "S-new");
        assert_eq!(current.claim_log.len(), 1);
        assert_eq!(current.claim_log[0].id, "C-new");
        assert!(current.conflict_map.is_empty());
        assert!(current
            .research_debt
            .iter()
            .any(|debt| debt.id == "D-old" && debt.status == "closed"));
        assert!(liquid_research_core::validate_research_artifacts(
            &current,
            Some("high"),
            Some("strict"),
        )
        .is_ok());
    }

    #[test]
    fn merged_artifacts_preserve_existing_narrative_state_when_new_iteration_omits_it() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::contracts::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com/source".to_string(),
                title: "Source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![crate::contracts::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported claim".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("역사적 전개".to_string()),
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "배경 형성".to_string(),
                    timeframe: Some("초기".to_string()),
                    actors: vec!["궁정".to_string()],
                    region_or_front: None,
                    trigger: Some("계승 문제".to_string()),
                    development: Some("초기 국면이 형성되었다.".to_string()),
                    outcome: Some("다음 국면의 대립이 심화되었다.".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: vec!["S1".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("medium".to_string()),
                    open_questions: vec!["후속 조약 확인".to_string()],
                }],
                timeline: vec![crate::contracts::NarrativeTimelineEvent {
                    id: "NE1".to_string(),
                    label: "배경 형성".to_string(),
                    date_anchor: None,
                    significance: None,
                    expected_claim_log_ids: vec!["C1".to_string()],
                    expected_source_card_ids: vec!["S1".to_string()],
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: current.source_cards.clone(),
            claim_log: current.claim_log.clone(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.topic_frame.as_deref()),
            Some("역사적 전개")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len()),
            Some(1)
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.timeline.len()),
            Some(1)
        );
    }

    #[test]
    fn merged_artifacts_merge_reader_quality_and_preserve_existing_when_omitted() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: Some(crate::contracts::ReaderQualityArtifacts {
                argument_graph: Some(crate::contracts::ReaderArgumentGraph {
                    nodes: vec![crate::contracts::ReaderArgumentNode {
                        id: "AQN1".to_string(),
                        label: "Existing reader graph".to_string(),
                        node_type: Some("support".to_string()),
                        rationale: Some("keep the existing graph".to_string()),
                        claim_log_ids: Vec::new(),
                        source_card_ids: Vec::new(),
                    }],
                    edges: Vec::new(),
                }),
                narrative_plan: Some(crate::contracts::ReaderNarrativePlan {
                    lead_section_id: Some("SEC1".to_string()),
                    section_ids: vec!["SEC1".to_string()],
                    transition_ids: vec!["TR1".to_string()],
                    narrative_arc: Some("existing arc".to_string()),
                    ending_note: None,
                }),
                section_briefs: vec![crate::contracts::ReaderSectionBrief {
                    section_id: Some("SEC1".to_string()),
                    key_point: "existing brief".to_string(),
                    reader_goal: Some("preserve context".to_string()),
                    claim_log_ids: Vec::new(),
                    source_card_ids: Vec::new(),
                }],
                reader_critique: Some(crate::contracts::ReaderCritique {
                    summary: Some("existing critique".to_string()),
                    strengths: vec!["good chronology".to_string()],
                    weaknesses: Vec::new(),
                    improvement_priorities: Vec::new(),
                    metrics: vec![crate::contracts::ReaderCritiqueMetric {
                        key: "clarity".to_string(),
                        label: "Clarity".to_string(),
                        status: "passed".to_string(),
                        rationale: None,
                    }],
                }),
            }),
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(
            &mut current,
            ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: None,
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            },
        );

        assert_eq!(
            current
                .reader_quality
                .as_ref()
                .and_then(|reader_quality| reader_quality.argument_graph.as_ref())
                .map(|graph| graph.nodes.len()),
            Some(1)
        );
        assert_eq!(
            current
                .reader_quality
                .as_ref()
                .map(|reader_quality| reader_quality.section_briefs.len()),
            Some(1)
        );

        merge_research_controller_artifacts(
            &mut current,
            ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: None,
                reader_quality: Some(crate::contracts::ReaderQualityArtifacts {
                    argument_graph: None,
                    narrative_plan: None,
                    section_briefs: vec![crate::contracts::ReaderSectionBrief {
                        section_id: Some("SEC2".to_string()),
                        key_point: "incoming brief".to_string(),
                        reader_goal: Some("update the lead".to_string()),
                        claim_log_ids: Vec::new(),
                        source_card_ids: Vec::new(),
                    }],
                    reader_critique: None,
                }),
                quality_gate: None,
                warnings: Vec::new(),
            },
        );

        let reader_quality = current
            .reader_quality
            .as_ref()
            .expect("reader quality should persist");
        assert_eq!(
            reader_quality
                .argument_graph
                .as_ref()
                .map(|graph| graph.nodes[0].label.as_str()),
            Some("Existing reader graph")
        );
        assert_eq!(
            reader_quality
                .section_briefs
                .iter()
                .map(|brief| brief.key_point.as_str())
                .collect::<Vec<_>>(),
            vec!["incoming brief"]
        );
        assert_eq!(
            reader_quality
                .reader_critique
                .as_ref()
                .and_then(|critique| critique.summary.as_deref()),
            Some("existing critique")
        );
    }

    #[test]
    fn merged_artifacts_preserve_existing_event_scaffold_when_partial_narrative_state_has_empty_event_cards(
    ) {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("기존 틀".to_string()),
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "배경 형성".to_string(),
                    timeframe: Some("초기".to_string()),
                    actors: vec!["궁정".to_string()],
                    region_or_front: Some("국경 지대".to_string()),
                    trigger: Some("계승 문제와 동맹 갈등".to_string()),
                    development: Some("초기 국면의 대립이 전선 충돌로 확대되었다.".to_string()),
                    outcome: Some("다음 국면에서 참전 세력이 늘어났다.".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: Vec::new(),
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: None,
                    open_questions: Vec::new(),
                }],
                timeline: vec![crate::contracts::NarrativeTimelineEvent {
                    id: "NE1".to_string(),
                    label: "배경 형성".to_string(),
                    date_anchor: None,
                    significance: None,
                    expected_claim_log_ids: Vec::new(),
                    expected_source_card_ids: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("업데이트된 틀".to_string()),
                event_cards: Vec::new(),
                timeline: Vec::new(),
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.topic_frame.as_deref()),
            Some("업데이트된 틀")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len()),
            Some(1)
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.event_cards.first())
                .and_then(|card| card.region_or_front.as_deref()),
            Some("국경 지대")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.timeline.len()),
            Some(1)
        );
    }

    #[test]
    fn merged_artifacts_preserve_richer_existing_event_scaffold_when_new_iteration_cards_are_shallower(
    ) {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("제2차 포에니 전쟁".to_string()),
                event_cards: vec![
                    crate::contracts::NarrativeEventCard {
                        label: "사군툼 위기".to_string(),
                        timeframe: Some("219-218 BCE".to_string()),
                        actors: vec!["한니발".to_string(), "로마 원로원".to_string()],
                        region_or_front: Some("이베리아".to_string()),
                        trigger: Some("동맹 도시 분쟁과 조약 해석 충돌".to_string()),
                        development: Some(
                            "사군툼 포위가 외교 결렬과 전면전 직전 국면으로 이어졌다.".to_string(),
                        ),
                        outcome: Some(
                            "알프스 원정과 이탈리아 전선 개시의 계기가 되었다.".to_string(),
                        ),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::contracts::NarrativeEventCard {
                        label: "이탈리아 전환".to_string(),
                        timeframe: Some("218-216 BCE".to_string()),
                        actors: vec!["한니발".to_string(), "로마 집정관".to_string()],
                        region_or_front: Some("알프스와 북부 이탈리아".to_string()),
                        trigger: Some("해상 우세를 피하려는 전략 전환".to_string()),
                        development: Some(
                            "알프스 돌파와 연속 승전으로 전쟁 중심이 이탈리아 본토로 이동했다."
                                .to_string(),
                        ),
                        outcome: Some(
                            "로마가 장기 소모전 체제로 적응하는 다음 국면이 열렸다.".to_string(),
                        ),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::contracts::NarrativeEventCard {
                        label: "역전과 종결".to_string(),
                        timeframe: Some("212-201 BCE".to_string()),
                        actors: vec!["스키피오".to_string(), "카르타고 원로회".to_string()],
                        region_or_front: Some("이베리아와 북아프리카".to_string()),
                        trigger: Some("로마의 재정비와 다전선 압박 전략".to_string()),
                        development: Some(
                            "이베리아 회복과 북아프리카 침공이 전쟁 축을 카르타고 본토로 돌렸다."
                                .to_string(),
                        ),
                        outcome: Some("자마 전투와 강화가 전쟁을 마무리했다.".to_string()),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                causal_chain: vec![NarrativeCausalLink {
                    id: "NC-authored".to_string(),
                    cause: "사군툼 위기가 외교적 선택지를 좁혔다.".to_string(),
                    effect: "이탈리아 전환과 로마의 장기 적응으로 이어졌다.".to_string(),
                    rationale: Some(
                        "이 링크는 모델이 작성한 해석이므로 얕은 후속 카드 때문에 사라지면 안 된다."
                            .to_string(),
                    ),
                    derived_from: None,
                    expected_claim_log_ids: Vec::new(),
                    expected_source_card_ids: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("업데이트된 틀".to_string()),
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "전쟁 전개".to_string(),
                    timeframe: Some("전기".to_string()),
                    actors: vec!["한니발".to_string()],
                    region_or_front: None,
                    trigger: Some("갈등 심화".to_string()),
                    development: Some("전개가 이어졌다.".to_string()),
                    outcome: None,
                    claim_log_ids: Vec::new(),
                    source_ids: Vec::new(),
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: None,
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.topic_frame.as_deref()),
            Some("업데이트된 틀")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.event_cards.len()),
            Some(3)
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.event_cards.first())
                .map(|card| card.label.as_str()),
            Some("사군툼 위기")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.causal_chain.first())
                .map(|link| link.id.as_str()),
            Some("NC-authored")
        );
        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .and_then(|state| state.causal_chain.first())
                .and_then(|link| link.derived_from.as_deref()),
            None
        );
    }

    #[test]
    fn merged_artifacts_prefers_claim_linked_event_cards_over_source_only_richer_cards() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "사군툼 위기".to_string(),
                    timeframe: Some("219-218 BCE".to_string()),
                    actors: vec!["한니발".to_string(), "로마 원로원".to_string()],
                    region_or_front: Some("이베리아".to_string()),
                    trigger: Some("동맹 도시 분쟁과 조약 해석 충돌".to_string()),
                    development: Some(
                        "사군툼 포위가 외교 결렬과 전면전 직전 국면으로 이어졌고, 카르타고와 로마의 선택지를 좁혔다."
                            .to_string(),
                    ),
                    outcome: Some("알프스 원정과 이탈리아 전선 개시의 계기가 되었다.".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: vec!["S1".to_string(), "S2".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "사군툼 위기".to_string(),
                    timeframe: Some("219-218 BCE".to_string()),
                    actors: vec!["한니발".to_string()],
                    region_or_front: Some("이베리아".to_string()),
                    trigger: Some("사군툼 포위".to_string()),
                    development: Some("사군툼 위기가 전쟁 명분으로 바뀌었다.".to_string()),
                    outcome: Some("전쟁이 시작됐다.".to_string()),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: Vec::new(),
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("medium".to_string()),
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        let card = current
            .narrative_state
            .as_ref()
            .and_then(|state| state.event_cards.first())
            .expect("merged card");
        assert_eq!(card.claim_log_ids, vec!["C1"]);
        assert!(card.source_ids.is_empty());
    }

    #[test]
    fn merged_artifacts_merge_event_scaffold_so_later_repairs_do_not_drop_prior_scope_anchors() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("프랑스 혁명".to_string()),
                event_cards: vec![
                    crate::contracts::NarrativeEventCard {
                        label: "공화정 수립".to_string(),
                        timeframe: Some("1792".to_string()),
                        actors: vec!["국민공회".to_string(), "파리 민중".to_string()],
                        region_or_front: Some("파리와 프랑스 전역".to_string()),
                        trigger: Some("왕권 불신과 전쟁 위기가 군주제 폐지 요구로 모였다.".to_string()),
                        development: Some(
                            "입법의회와 민중 압박이 왕정 폐지와 국민공회 소집으로 이어졌다."
                                .to_string(),
                        ),
                        outcome: Some("공화정이 선포되고 혁명 전쟁의 정치적 성격이 바뀌었다.".to_string()),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::contracts::NarrativeEventCard {
                        label: "테르미도르 반동".to_string(),
                        timeframe: Some("1794-1795".to_string()),
                        actors: vec!["국민공회".to_string(), "로베스피에르 반대파".to_string()],
                        region_or_front: Some("파리".to_string()),
                        trigger: Some("공포정치 피로와 정치적 생존 계산이 결합했다.".to_string()),
                        development: Some(
                            "로베스피에르 실각 이후 혁명정부의 동원 체제가 완화되고 권력이 재편됐다."
                                .to_string(),
                        ),
                        outcome: Some("총재정부로 이어지는 보수적 공화정 질서가 열렸다.".to_string()),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("업데이트된 프랑스 혁명 틀".to_string()),
                event_cards: vec![
                    crate::contracts::NarrativeEventCard {
                        label: "왕정 위기와 삼부회".to_string(),
                        timeframe: Some("1788-1789".to_string()),
                        actors: vec!["왕실".to_string(), "제3신분".to_string()],
                        region_or_front: Some("베르사유와 파리".to_string()),
                        trigger: Some("재정 위기와 대표성 갈등이 정치 위기로 확대됐다.".to_string()),
                        development: Some(
                            "삼부회 소집과 국민의회 선언이 구체제 개혁 요구를 제도 투쟁으로 바꿨다."
                                .to_string(),
                        ),
                        outcome: Some("바스티유 함락과 봉건제 폐지 국면으로 이어졌다.".to_string()),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    crate::contracts::NarrativeEventCard {
                        label: "유럽 질서 재편".to_string(),
                        timeframe: Some("1799-1815".to_string()),
                        actors: vec!["프랑스".to_string(), "대프랑스 동맹".to_string()],
                        region_or_front: Some("유럽 대륙".to_string()),
                        trigger: Some("혁명 전쟁과 나폴레옹 전쟁이 국제 질서를 흔들었다.".to_string()),
                        development: Some(
                            "동맹전쟁과 제국 확장이 세력균형, 민족주의, 복고 체제 논쟁을 낳았다."
                                .to_string(),
                        ),
                        outcome: Some("빈 체제와 19세기 유럽 정치 질서의 전제가 형성됐다.".to_string()),
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        let labels = current
            .narrative_state
            .as_ref()
            .map(|state| {
                state
                    .event_cards
                    .iter()
                    .map(|card| card.label.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(
            labels,
            vec![
                "공화정 수립",
                "테르미도르 반동",
                "왕정 위기와 삼부회",
                "유럽 질서 재편"
            ]
        );
    }

    #[test]
    fn merged_event_scaffold_replaces_cross_language_duplicate_phase_with_newer_card() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("프랑스 혁명".to_string()),
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "입헌군주정 실험".to_string(),
                    timeframe: Some("1789~1791년".to_string()),
                    actors: vec!["국민의회".to_string(), "루이 16세".to_string()],
                    region_or_front: Some("프랑스".to_string()),
                    trigger: Some("헌법 제정".to_string()),
                    development: Some("교회 개혁, 행정 개편, 1791년 헌법".to_string()),
                    outcome: Some("왕권 불신 심화".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: Vec::new(),
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: None,
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                topic_frame: Some("Updated French Revolution frame".to_string()),
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "Constitutional monarchy".to_string(),
                    timeframe: Some("1790-1791".to_string()),
                    actors: vec!["National Constituent Assembly".to_string(), "Louis XVI".to_string()],
                    region_or_front: Some("France".to_string()),
                    trigger: Some("need to institutionalize new sovereignty".to_string()),
                    development: Some(
                        "1791 constitution, church reorganization, restricted suffrage, and royal mistrust made the settlement unstable."
                            .to_string(),
                    ),
                    outcome: Some("the regime remained fragile and fed the 1792 crisis".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: Vec::new(),
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: None,
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        let cards = &current.narrative_state.as_ref().unwrap().event_cards;
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].label, "Constitutional monarchy");
        assert!(cards[0]
            .development
            .as_deref()
            .is_some_and(|development| development.contains("restricted suffrage")));
    }

    #[test]
    fn merged_artifacts_carry_open_gaps_until_explicitly_closed_or_deferred() {
        let mut current = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                open_gaps: vec![NarrativeOpenGap {
                    id: "NG1".to_string(),
                    gap_type: "chronology".to_string(),
                    description: "초기 전개 순서 확인 필요".to_string(),
                    status: Some("open".to_string()),
                    expected_claim_log_ids: Vec::new(),
                    expected_source_card_ids: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let incoming = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: Some(NarrativeState {
                version: 1,
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, incoming);

        assert_eq!(
            current
                .narrative_state
                .as_ref()
                .map(|state| state.open_gaps.len()),
            Some(1)
        );

        let deferred = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: Vec::new(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: vec![ResearchDebtItem {
                id: "D-gap".to_string(),
                severity: "medium".to_string(),
                failed_gate: Some("quality_gate".to_string()),
                missing_evidence: "NG1 초기 전개 순서 확인 필요".to_string(),
                required_source_class: None,
                candidate_queries: vec!["초기 전개 공식 연표".to_string()],
                next_check_actions: vec!["연표 공백을 연구 부채로 유지".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };

        merge_research_controller_artifacts(&mut current, deferred);

        assert!(current
            .narrative_state
            .as_ref()
            .is_some_and(|state| state.open_gaps.is_empty()));
    }

    #[test]
    fn quality_repair_prompt_converts_failures_to_search_debt() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![crate::contracts::ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com/spec".to_string(),
                title: "Spec".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["price listed".to_string()],
                limitation: Some("single region".to_string()),
                diagnostics_ref: Some("scrape-1".to_string()),
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![crate::contracts::ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "official price exists".to_string(),
                claim_type: Some("price".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            conflict_map: vec![crate::contracts::ResearchConflictMapEntry {
                id: "X1".to_string(),
                topic: "regional price mismatch".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("unresolved".to_string()),
                resolution_note: None,
                promoted_to_debt: Some(true),
            }],
            research_debt: vec![ResearchDebtItem {
                id: "D1".to_string(),
                severity: "high".to_string(),
                failed_gate: Some("artifact_quality".to_string()),
                missing_evidence: "official price missing".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["official price page".to_string()],
                next_check_actions: vec!["find region-specific official pricing".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![crate::contracts::NarrativeEventCard {
                    label: "초기 가격 구조".to_string(),
                    timeframe: Some("초기 국면".to_string()),
                    actors: vec!["지역 판매자".to_string()],
                    region_or_front: Some("도심 상권".to_string()),
                    trigger: Some("원가 상승".to_string()),
                    development: Some("판매 채널별 가격 차이가 벌어졌다.".to_string()),
                    outcome: Some("비교 기준이 달라졌다.".to_string()),
                    claim_log_ids: Vec::new(),
                    source_ids: vec!["S1".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("medium".to_string()),
                    open_questions: vec!["지방 상권 데이터 추가 필요".to_string()],
                }],
                evidence_layers: vec![crate::contracts::NarrativeEvidenceLayer {
                    id: "NL1".to_string(),
                    label: "공식 가격 근거".to_string(),
                    purpose: Some("사실 확인 후 해석".to_string()),
                    derived_from: None,
                    expected_claim_log_ids: vec!["C1".to_string()],
                    expected_source_card_ids: vec!["S1".to_string()],
                }],
                open_gaps: vec![NarrativeOpenGap {
                    id: "NG1".to_string(),
                    gap_type: "impact".to_string(),
                    description: "지역별 가격 차이의 실제 영향 확인 필요".to_string(),
                    status: Some("open".to_string()),
                    expected_claim_log_ids: vec!["C1".to_string()],
                    expected_source_card_ids: vec!["S1".to_string()],
                }],
                ..NarrativeState::default()
            }),
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("price comparison".to_string()),
            source_pack: None,
            scrapes: vec![crate::contracts::ScrapeDiagnostics {
                original_url: "https://example.com/spec".to_string(),
                normalized_url: "https://example.com/spec".to_string(),
                final_url: Some("https://example.com/spec".to_string()),
                status_class: "fetch_failed".to_string(),
                failure_reason: Some("Failed to fetch URL".to_string()),
                http_status_code: None,
                extraction_strategy: None,
                title: None,
                content_type: None,
                raw_body_bytes: None,
                raw_body_chars: None,
                extracted_html_chars: 0,
                markdown_chars: 0,
                sufficiency_result: "insufficient".to_string(),
                insufficiency_reason: Some("network".to_string()),
                reference_links: Vec::new(),
                accessed_at: "2026-05-13T00:00:00Z".to_string(),
                raw_capture: crate::contracts::ScrapeRawCaptureDiagnostics {
                    mode: "omitted".to_string(),
                    path: None,
                    hash: None,
                    omitted_reason: Some("raw snapshots disabled by default".to_string()),
                },
            }],
            context_packing: None,
        };
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "official price missing",
            2,
            5,
            Some(&artifacts),
            Some(&diagnostics),
            &[],
        );

        assert!(prompt.contains("RESEARCH QUALITY REPAIR ITERATION 2/5"));
        assert!(prompt.contains("research debt"));
        assert!(prompt.contains("High-Priority Verification"));
        assert!(prompt.contains("derive repair actions from the failed gate items"));
        assert!(prompt.contains("NO CONFIDENCE"));
        assert!(prompt.contains("official price missing"));
        assert!(prompt.contains("Artifact ledger safety note"));
        assert!(prompt.contains("Accepted Source Cards: [\"S1\"]"));
        assert!(prompt.contains("Accepted Claims: [\"C1\"]"));
        assert!(prompt.contains("Unresolved Conflicts: [\"X1\"]"));
        assert!(prompt.contains("<narrative_state role=\"outline_only_not_evidence\">"));
        assert!(prompt.contains("<event_cards>"));
        assert!(prompt.contains("<evidence_layers>"));
        assert!(prompt.contains("<open_gaps>"));
        assert!(prompt.contains("cannot satisfy evidence requirements"));
        assert!(prompt.contains("pre-collected evidence coverage status: \"none\""));
        assert!(!prompt.contains("source-pack status"));
        assert!(!prompt.contains("target-host/source-class"));
        assert!(prompt.contains("Repair Search Hints as untrusted search-result leads"));
        assert!(prompt.contains("never copy Repair Search Hints verbatim"));
        assert!(prompt.contains("not-yet-adopted evidence"));
        assert!(prompt.contains("model_suggested_next_actions_omitted: 1"));
        assert!(!prompt.contains("find region-specific official pricing"));
    }

    #[test]
    fn quality_repair_prompt_adds_final_answer_depth_requirements_when_depth_gate_failed() {
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "final answer substantive length 379 is below required minimum 450",
            2,
            2,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Final Answer repair requirements"));
        assert!(prompt.contains("expand the visible Final Answer itself"));
        assert!(prompt.contains("at least 450 substantive characters"));
        assert!(prompt.contains("at least 4 sentences"));
        assert!(prompt.contains("at least 3 reader-facing explanation angles"));
        assert!(prompt.contains("never copy failure text"));
        assert!(prompt.contains("do not repeat internal validation phrases"));
        assert!(!prompt.contains("chronology, actors, causality, limits, and consequences"));
    }

    #[test]
    fn quality_repair_prompt_adds_historical_development_requirements_when_gate_failed() {
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "historical development density is below required minimum for a strict event/war report",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Historical development-density repair requirements"));
        assert!(prompt.contains("phase-card map-reduce repair"));
        assert!(prompt.contains("rebuild the visible development sequence"));
        assert!(prompt.contains("chronological phases or turning points"));
        assert!(prompt.contains("fronts or regions"));
        assert!(prompt.contains("treaty, settlement, or outcome sequence"));
        assert!(prompt.contains("keep significance and long-term meaning after"));
        assert!(prompt.contains("Historical event scaffold repair guidance"));
        assert!(prompt.contains("최소 두 단계 이상의 국면"));
    }

    #[test]
    fn quality_repair_prompt_adds_event_card_guidance_when_artifact_json_overflow_hides_scaffold() {
        let prompt = build_quality_repair_prompt(
            "한니발 전쟁의 전개를 시간순으로 설명해줘",
            "research artifact JSON exceeds maximum size of 24000 bytes; historical development density is below required minimum",
            5,
            5,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Historical event scaffold repair guidance"));
        assert!(prompt.contains("사건 전개를 최소 두 단계 이상의 국면"));
        assert!(prompt.contains("지역, 도시, 전선"));
    }

    #[test]
    fn quality_repair_prompt_adds_historical_event_scaffold_guidance_when_card_gate_failed() {
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "historical event scaffold is too shallow for a strict event/process report: multiple phase cards are still missing, cause-to-next-phase progression is still missing",
            2,
            3,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![crate::contracts::NarrativeEventCard {
                        label: "초기 국면".to_string(),
                        timeframe: Some("초기".to_string()),
                        actors: Vec::new(),
                        region_or_front: None,
                        trigger: None,
                        development: Some("갈등이 커졌다.".to_string()),
                        outcome: None,
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    }],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Historical event scaffold repair guidance"));
        assert!(prompt.contains("최소 두 단계 이상의 국면"));
        assert!(prompt.contains("짧은 단락 수준으로 다시 확장"));
    }

    #[test]
    fn quality_repair_prompt_includes_claim_text_context_for_event_card_grounding() {
        let prompt = build_quality_repair_prompt(
            "러일전쟁의 중심 해석 줄기를 세워 설명해줘",
            "historical event scaffold is too shallow for a strict event/process report: broad historical event/process topics still need at least 6 distinct phase cards",
            2,
            2,
            Some(&ResearchControllerArtifacts {
                version: 1,
                source_cards: vec![ResearchSourceCard {
                    id: "SC1".to_string(),
                    url: "https://history.state.gov/milestones/1899-1913/portsmouth-treaty"
                        .to_string(),
                    title: "The Treaty of Portsmouth and the Russo-Japanese War".to_string(),
                    source_class: "official_secondary".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: Some("high".to_string()),
                }],
                claim_log: vec![ResearchClaimLogEntry {
                    id: "C8".to_string(),
                    claim: "Tsushima crippled Russia's naval recovery path and raised pressure for peace."
                        .to_string(),
                    claim_type: Some("supported".to_string()),
                    support_source_card_ids: vec!["SC1".to_string()],
                    support_urls: Vec::new(),
                    confidence: Some("high".to_string()),
                    uncertainty_note: None,
                    needs_verification: Some(false),
                }],
                ..ResearchControllerArtifacts::default()
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Accepted Claim Context"));
        assert!(prompt.contains("C8"));
        assert!(prompt.contains("Tsushima crippled Russia"));
        assert!(prompt.contains("naval recovery path"));
        assert!(prompt
            .contains("SC1<https://history.state.gov/milestones/1899-1913/portsmouth-treaty>"));
        assert!(prompt.contains("use this exact claim text when grounding event_cards"));
    }

    #[test]
    fn quality_repair_prompt_does_not_normalize_relative_support_refs_as_public_urls() {
        let prompt = build_quality_repair_prompt(
            "러일전쟁의 중심 해석 줄기를 세워 설명해줘",
            "historical event scaffold is too shallow for a strict event/process report",
            2,
            2,
            Some(&ResearchControllerArtifacts {
                version: 1,
                source_cards: vec![ResearchSourceCard {
                    id: "SC1".to_string(),
                    url: "/relative-source".to_string(),
                    title: "Relative Source".to_string(),
                    source_class: "secondary".to_string(),
                    accessed_at: None,
                    extracted_facts: Vec::new(),
                    limitation: None,
                    diagnostics_ref: None,
                    confidence: None,
                }],
                claim_log: vec![ResearchClaimLogEntry {
                    id: "C1".to_string(),
                    claim: "A relative support ref must not become accepted public evidence."
                        .to_string(),
                    claim_type: Some("supported".to_string()),
                    support_source_card_ids: vec!["SC1".to_string()],
                    support_urls: vec!["/relative-claim".to_string()],
                    confidence: None,
                    uncertainty_note: None,
                    needs_verification: Some(true),
                }],
                ..ResearchControllerArtifacts::default()
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Accepted Claim Context"));
        assert!(prompt.contains("C1"));
        assert!(!prompt.contains("https://duckduckgo.com/relative-source"));
        assert!(!prompt.contains("https://duckduckgo.com/relative-claim"));
    }

    #[test]
    fn quality_repair_prompt_adds_historical_narrative_artifact_guidance_when_spine_gate_failed() {
        let prompt = build_quality_repair_prompt(
            "러일전쟁의 중심 해석 줄기를 세워 설명해줘",
            "historical high-intensity strict research must persist useful narrative_state or reader_quality planning artifacts with chronology, source-layer, interpretation, impact, or reader-guidance detail; historical high-intensity strict research must persist a grounded central interpretive spine",
            2,
            2,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![crate::contracts::NarrativeEventCard {
                        label: "개전".to_string(),
                        timeframe: Some("1904".to_string()),
                        actors: vec!["일본".to_string(), "러시아".to_string()],
                        region_or_front: Some("뤼순".to_string()),
                        trigger: Some("협상 결렬".to_string()),
                        development: Some("전쟁이 시작되었다.".to_string()),
                        outcome: Some("만주 전선으로 이어졌다.".to_string()),
                        claim_log_ids: vec!["C1".to_string()],
                        source_ids: vec!["S1".to_string()],
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: Some("medium".to_string()),
                        open_questions: Vec::new(),
                    }],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Historical narrative artifact repair requirements"));
        assert!(prompt.contains("causal_chain을 최소 3개"));
        assert!(prompt.contains("evidence_layers 최소 2개"));
        assert!(prompt.contains("reader_quality에는 narrative_plan.narrative_arc와 section_briefs"));
        assert!(prompt.contains("기존 국면을 얇게 버리지 말고"));
        assert!(prompt.contains("provider payload"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_historical_event_scaffold_guidance_for_non_historical_failures_with_event_cards(
    ) {
        let prompt = build_quality_repair_prompt(
            "동네 카페 비교",
            "official price missing",
            2,
            3,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![crate::contracts::NarrativeEventCard {
                        label: "초기 국면".to_string(),
                        timeframe: Some("초기".to_string()),
                        actors: Vec::new(),
                        region_or_front: None,
                        trigger: None,
                        development: Some("짧은 메모".to_string()),
                        outcome: None,
                        claim_log_ids: Vec::new(),
                        source_ids: Vec::new(),
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    }],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(!prompt.contains("Historical event scaffold repair guidance"));
        assert!(!prompt.contains("최소 두 단계 이상의 국면"));
    }

    #[test]
    fn quality_repair_prompt_adds_per_phase_expansion_guidance_for_underfilled_event_cards() {
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "historical event scaffold is too shallow for a strict event/process report",
            2,
            3,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![
                        crate::contracts::NarrativeEventCard {
                            label: "초기 국면".to_string(),
                            timeframe: Some("초기".to_string()),
                            actors: vec!["궁정".to_string()],
                            region_or_front: None,
                            trigger: Some("계승 문제".to_string()),
                            development: Some("대립이 커졌다.".to_string()),
                            outcome: Some("다음 단계로 이어졌다.".to_string()),
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                        crate::contracts::NarrativeEventCard {
                            label: "중기 국면".to_string(),
                            timeframe: Some("중기".to_string()),
                            actors: Vec::new(),
                            region_or_front: Some("국경 지대".to_string()),
                            trigger: None,
                            development: Some("전개가 심화되었다.".to_string()),
                            outcome: None,
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                    ],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Historical event scaffold repair guidance"));
        assert!(prompt.contains("직접 계기나 원인"));
        assert!(prompt.contains("지역, 전선, 도시"));
        assert!(prompt.contains("각 국면은 한두 문장 메모가 아니라 읽을 수 있는 짧은 단락 수준"));
        assert!(prompt.contains("다음 국면의 계기와 전환으로 이어졌는지"));
    }

    #[test]
    fn quality_repair_prompt_requests_six_or_more_phases_for_broad_historical_topics() {
        let prompt = build_quality_repair_prompt(
            "제2차 포에니 전쟁의 배경과 전개, 영향과 의의",
            "historical event scaffold is too shallow for a strict event/process report: broad historical event/process topics still need at least 6 distinct phase cards",
            3,
            4,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![
                        crate::contracts::NarrativeEventCard {
                            label: "개시 국면".to_string(),
                            timeframe: Some("전기".to_string()),
                            actors: vec!["한니발".to_string()],
                            region_or_front: Some("이베리아".to_string()),
                            trigger: Some("동맹 분쟁".to_string()),
                            development: Some("전면전이 시작되었다.".to_string()),
                            outcome: Some("이탈리아 전선으로 이어졌다.".to_string()),
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                        crate::contracts::NarrativeEventCard {
                            label: "전환 국면".to_string(),
                            timeframe: Some("중기".to_string()),
                            actors: vec!["로마".to_string()],
                            region_or_front: Some("이탈리아".to_string()),
                            trigger: Some("장기전 적응".to_string()),
                            development: Some("국면이 바뀌었다.".to_string()),
                            outcome: Some("마지막 종결 국면이 열렸다.".to_string()),
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                    ],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(prompt.contains("Historical event scaffold repair guidance"));
        assert!(prompt.contains("최소 여섯 단계 이상의 국면"));
        assert!(prompt.contains("8-12개 compact 카드"));
        assert!(prompt.contains("짧은 단락 수준으로 다시 확장"));
    }

    #[test]
    fn quality_repair_prompt_requests_late_historical_scope_anchors_when_missing() {
        let prompt = build_quality_repair_prompt(
            "프랑스 혁명의 배경과 전개, 공화정 전환, 테르미도르, 유럽 질서 영향",
            "historical event scaffold is too shallow for a strict event/process report: requested republican transition is still missing from phase cards, requested thermidor or later reaction phase is still missing from phase cards, requested later settlement or wider-order impact phase is still missing from phase cards",
            3,
            4,
            Some(&ResearchControllerArtifacts {
                version: 1,
                events: Vec::new(),
                source_cards: Vec::new(),
                claim_log: Vec::new(),
                conflict_map: Vec::new(),
                research_debt: Vec::new(),
                narrative_state: Some(NarrativeState {
                    version: 1,
                    event_cards: vec![
                        crate::contracts::NarrativeEventCard {
                            label: "구체제 위기".to_string(),
                            timeframe: Some("1788-1789".to_string()),
                            actors: vec!["왕실".to_string(), "삼부회".to_string()],
                            region_or_front: Some("베르사유".to_string()),
                            trigger: Some("재정 위기".to_string()),
                            development: Some("대표제 충돌이 혁명 초기를 열었다.".to_string()),
                            outcome: Some("국민의회 국면이 시작되었다.".to_string()),
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                        crate::contracts::NarrativeEventCard {
                            label: "민중 개입".to_string(),
                            timeframe: Some("1789년 7월".to_string()),
                            actors: vec!["파리 군중".to_string()],
                            region_or_front: Some("파리".to_string()),
                            trigger: Some("탄압 공포".to_string()),
                            development: Some("바스티유 사건과 시정 재편이 일어났다.".to_string()),
                            outcome: Some("개혁 압력이 확대되었다.".to_string()),
                            claim_log_ids: Vec::new(),
                            source_ids: Vec::new(),
                            causal_spine: Vec::new(),
                            interpretive_layers: Vec::new(),
                            confidence: None,
                            open_questions: Vec::new(),
                        },
                    ],
                    ..NarrativeState::default()
                }),
                reader_quality: None,
                quality_gate: None,
                warnings: Vec::new(),
            }),
            None,
            &[],
        );

        assert!(prompt.contains("공화정 수립이나 왕정 폐지"));
        assert!(prompt.contains("테르미도르 같은 후반 반동"));
        assert!(prompt.contains("유럽 질서"));
    }

    #[test]
    fn quality_repair_prompt_adds_technology_evidence_requirements_for_authority_failures() {
        let prompt = build_quality_repair_prompt(
            "Explain ephemeral port allocation and port exhaustion across Linux kernel defaults, Windows dynamic port ranges, and cloud container networking behavior.",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology implementation evidence-repair requirements"));
        assert!(prompt.contains("standards, specs, kernel or runtime documentation"));
        assert!(prompt.contains("RFC/IANA style references"));
        assert!(prompt.contains("Linux, Windows, project/runtime, and cloud-provider defaults"));
        assert!(prompt.contains("never copy outline placeholders"));
    }

    #[test]
    fn quality_repair_prompt_adds_technology_concept_guidance_for_ai_concept_topics() {
        let prompt = build_quality_repair_prompt(
            "AI 개념과 머신러닝, 딥러닝, 생성형 AI, LLM, RAG, agent의 차이와 오해, 한계",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology concept evidence-repair requirements"));
        assert!(prompt.contains("precise definitions, concept boundaries"));
        assert!(prompt.contains("concrete operational model"));
        assert!(prompt.contains("examples, non-examples, common misconceptions"));
        assert!(prompt.contains("concept-focused"));
        assert!(prompt.contains("survey/tutorial papers"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_adds_attention_operational_model_guidance_for_transformer_topics() {
        let prompt = build_quality_repair_prompt(
            "Explain transformer attention and why query, key, and value matter",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology concept evidence-repair requirements"));
        assert!(prompt.contains("query/key/value"));
        assert!(prompt.contains("relation scoring"));
        assert!(prompt.contains("value mixing"));
        assert!(prompt.contains("context handling"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_attention_guidance_for_ai_value_alignment_topics() {
        let prompt = build_quality_repair_prompt(
            "AI value alignment concept overview",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology concept evidence-repair requirements"));
        assert!(prompt.contains("concrete operational model"));
        assert!(!prompt.contains("query/key/value"));
        assert!(!prompt.contains("relation scoring"));
        assert!(!prompt.contains("value mixing"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_attention_guidance_for_key_difference_topics() {
        let prompt = build_quality_repair_prompt(
            "key differences between AI and machine learning concepts",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology concept evidence-repair requirements"));
        assert!(prompt.contains("concrete operational model"));
        assert!(!prompt.contains("query/key/value"));
        assert!(!prompt.contains("relation scoring"));
        assert!(!prompt.contains("value mixing"));
    }

    #[test]
    fn quality_repair_prompt_does_not_treat_ai_policy_governance_as_technology_concept() {
        let prompt = build_quality_repair_prompt(
            "AI regulation overview and governance policy explanation",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_port_substrings_in_plain_prose() {
        let prompt = build_quality_repair_prompt(
            "Explain why important support and transportation choices matter for customer trust.",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_port_arthur_history() {
        let prompt = build_quality_repair_prompt(
            "Port Arthur siege history: background, development, impact, and significance",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_local_port_city_trip() {
        let prompt = build_quality_repair_prompt(
            "Best port city breakfast route and cafe stops for a short local trip",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_korean_port_arthur_history() {
        let prompt = build_quality_repair_prompt(
            "포트 아서 공방의 배경과 전개",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_korean_local_cloud_trip() {
        let prompt = build_quality_repair_prompt(
            "클라우드 전망이 보이는 포트 시티 산책 동선과 카페 추천",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_still_adds_technology_guidance_for_korean_port_protocol_topic() {
        let prompt = build_quality_repair_prompt(
            "동적 포트 할당과 네트워크 프로토콜 동작을 Linux와 Windows 기준으로 설명",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_policy_implementation_history() {
        let prompt = build_quality_repair_prompt(
            "policy implementation history and institutional impact",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_local_travel_route_design() {
        let prompt = build_quality_repair_prompt(
            "travel route design for a quiet morning port city walk",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_korean_policy_implementation() {
        let prompt = build_quality_repair_prompt(
            "정책 구현의 배경과 영향",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_does_not_add_technology_guidance_for_korean_local_itinerary_design() {
        let prompt = build_quality_repair_prompt(
            "여행 일정 설계와 포트 시티 동선 추천",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(!prompt.contains("Technology concept evidence-repair requirements"));
        assert!(!prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_prompt_still_adds_technology_guidance_for_cpp_scheduler_implementation() {
        let prompt = build_quality_repair_prompt(
            "modern C++ work-stealing scheduler implementation guide",
            "authoritative evidence URL count 2 is below required minimum 5",
            2,
            3,
            None,
            None,
            &[],
        );

        assert!(prompt.contains("Technology implementation evidence-repair requirements"));
    }

    #[test]
    fn quality_repair_candidate_queries_compact_instruction_heavy_topic_prompts() {
        let queries = candidate_queries_from_failure(
            "source card support missing",
            Some(
                "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates. The output should be useful for actually deciding where to run and where to go afterward.",
            ),
        );

        assert!(!queries.is_empty());
        assert!(!queries[0].contains("Write a Korean reader-facing research report"));
        assert!(!queries[0].contains("The output should be useful"));
        assert!(queries[0].contains("Namsan"));
        assert!(queries[0].contains("Seoul"));
        assert!(queries[0].contains("running"));
        assert!(queries[0].contains("cafe"));
    }

    #[test]
    fn quality_repair_candidate_queries_keep_short_plain_topics() {
        let queries = candidate_queries_from_failure(
            "support missing",
            Some("Aurelian monetary reform and Sol Invictus"),
        );

        assert_eq!(
            queries.first().map(String::as_str),
            Some("Aurelian monetary reform and Sol Invictus")
        );
    }

    #[test]
    fn collect_repair_search_queries_dedupes_prior_diagnostics_and_caps_four() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            research_debt: vec![
                ResearchDebtItem {
                    id: "D1".to_string(),
                    severity: "high".to_string(),
                    failed_gate: None,
                    missing_evidence: "missing".to_string(),
                    required_source_class: None,
                    candidate_queries: vec![
                        "official price page".to_string(),
                        "regional pricing official".to_string(),
                        "shipping policy".to_string(),
                    ],
                    next_check_actions: Vec::new(),
                    status: "open".to_string(),
                },
                ResearchDebtItem {
                    id: "D2".to_string(),
                    severity: "medium".to_string(),
                    failed_gate: None,
                    missing_evidence: "missing".to_string(),
                    required_source_class: None,
                    candidate_queries: vec![
                        "regional pricing official".to_string(),
                        "availability notice".to_string(),
                    ],
                    next_check_actions: Vec::new(),
                    status: "open".to_string(),
                },
            ],
            ..ResearchControllerArtifacts::default()
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("price comparison".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("price comparison".to_string()),
                status: "partial".to_string(),
                reason: None,
                queries: vec![ResearchSourceQueryReport {
                    query: "official price page".to_string(),
                    status: "success".to_string(),
                    provider: Some("naver".to_string()),
                    result_count: 1,
                    adopted_count: 1,
                    skipped_count: 0,
                    error: None,
                }],
                seeded_source_count: 0,
                discovered_source_count: 0,
                adopted_source_count: 0,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: vec![
                    crate::contracts::ResearchSourceCoverageMiss {
                        expected_host: Some("example.com".to_string()),
                        expected_source_class: Some("official_or_primary".to_string()),
                        query: "official price page".to_string(),
                        provider: Some("naver".to_string()),
                        status: "missed".to_string(),
                        reason: None,
                    },
                    crate::contracts::ResearchSourceCoverageMiss {
                        expected_host: Some("example.org".to_string()),
                        expected_source_class: Some("official_or_primary".to_string()),
                        query: "availability notice".to_string(),
                        provider: Some("kakao".to_string()),
                        status: "missed".to_string(),
                        reason: None,
                    },
                    crate::contracts::ResearchSourceCoverageMiss {
                        expected_host: Some("example.net".to_string()),
                        expected_source_class: Some("official_or_primary".to_string()),
                        query: "regional warranty terms".to_string(),
                        provider: Some("duckduckgo".to_string()),
                        status: "missed".to_string(),
                        reason: None,
                    },
                ],
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let queries = collect_repair_search_queries(Some(&artifacts), Some(&diagnostics));

        assert_eq!(queries.len(), 4);
        assert_eq!(queries[0], "regional pricing official");
        assert_eq!(queries[1], "shipping policy");
        assert_eq!(queries[2], "availability notice");
        assert_eq!(
            queries[3],
            "price comparison example.com official or primary"
        );
    }

    #[test]
    fn collect_repair_search_queries_keeps_coverage_miss_repair_opportunity_when_prior_query_matches(
    ) {
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("Austrian Succession War background and consequences".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("Austrian Succession War background and consequences".to_string()),
                status: "partial".to_string(),
                reason: None,
                queries: vec![ResearchSourceQueryReport {
                    query: "austrian succession war overview".to_string(),
                    status: "success".to_string(),
                    provider: Some("naver".to_string()),
                    result_count: 5,
                    adopted_count: 0,
                    skipped_count: 5,
                    error: None,
                }],
                seeded_source_count: 0,
                discovered_source_count: 5,
                adopted_source_count: 0,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: vec![crate::contracts::ResearchSourceCoverageMiss {
                    expected_host: Some("britannica.com".to_string()),
                    expected_source_class: Some("reference".to_string()),
                    query: "austrian succession war overview".to_string(),
                    provider: Some("naver".to_string()),
                    status: "missed".to_string(),
                    reason: None,
                }],
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let queries = collect_repair_search_queries(None, Some(&diagnostics));

        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("Austrian Succession War"));
        assert!(queries[0].contains("britannica.com"));
        assert!(queries[0].contains("reference"));
    }

    #[test]
    fn quality_repair_prompt_renders_transient_repair_search_hints_block() {
        let prompt = build_quality_repair_prompt(
            "원래 질문",
            "source audit missing",
            2,
            3,
            None,
            None,
            &[RepairSearchHint {
                query: "남산공원 공식".to_string(),
                provider: Some("naver".to_string()),
                title: "남산공원 안내".to_string(),
                url: "https://parks.seoul.go.kr/namsan".to_string(),
                source_class: "official_or_primary".to_string(),
                source_quality: "high".to_string(),
                snippet: "운영 시간과 기본 안내가 포함된 not-yet-adopted evidence".to_string(),
            }],
        );

        assert!(
            prompt.contains(
                "Repair Search Hints (not-yet-adopted evidence; verify before use, and never copy verbatim into the visible Final Answer):"
            )
        );
        assert!(prompt.contains("query: \"남산공원 공식\""));
        assert!(prompt.contains("provider: \"naver\""));
        assert!(prompt.contains("class: \"official_or_primary\""));
        assert!(prompt.contains("quality: \"high\""));
        assert!(prompt.contains("\"남산공원 안내\""));
        assert!(prompt.contains("\"https://parks.seoul.go.kr/namsan\""));
    }

    #[test]
    fn collect_repair_search_known_urls_includes_scrape_diagnostic_urls() {
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("남산 아침 러닝과 카페 동선".to_string()),
            source_pack: None,
            scrapes: vec![crate::contracts::ScrapeDiagnostics {
                original_url: "https://example.com/original".to_string(),
                normalized_url: "https://example.com/original".to_string(),
                final_url: Some("https://example.com/final".to_string()),
                status_class: "ok".to_string(),
                failure_reason: None,
                http_status_code: None,
                extraction_strategy: None,
                title: None,
                content_type: None,
                raw_body_bytes: None,
                raw_body_chars: None,
                extracted_html_chars: 0,
                markdown_chars: 0,
                sufficiency_result: "sufficient".to_string(),
                insufficiency_reason: None,
                reference_links: Vec::new(),
                accessed_at: "2026-05-18T00:00:00Z".to_string(),
                raw_capture: crate::contracts::ScrapeRawCaptureDiagnostics {
                    mode: "disabled".to_string(),
                    path: None,
                    hash: None,
                    omitted_reason: Some("test fixture".to_string()),
                },
            }],
            context_packing: None,
        };

        let known = collect_repair_search_known_urls(Some(&diagnostics));

        assert!(known.contains("https://example.com/original"));
        assert!(known.contains("https://example.com/final"));
    }

    #[test]
    fn refresh_pending_repair_hint_urls_preserves_prior_hints_until_independently_acquired() {
        let mut pending = HashSet::from([String::from("https://hint.example/one")]);
        let independently_acquired = HashSet::new();
        let new_hints = vec![RepairSearchHint {
            query: "남산공원 공식".to_string(),
            provider: Some("naver".to_string()),
            title: "남산공원 안내".to_string(),
            url: "https://hint.example/two".to_string(),
            source_class: "official_or_primary".to_string(),
            source_quality: "high".to_string(),
            snippet: "운영 시간 안내".to_string(),
        }];

        refresh_pending_repair_hint_urls(&mut pending, &new_hints, &independently_acquired);
        assert!(pending.contains("https://hint.example/one"));
        assert!(pending.contains("https://hint.example/two"));

        let independently_acquired = HashSet::from([String::from("https://hint.example/two")]);
        refresh_pending_repair_hint_urls(&mut pending, &[], &independently_acquired);
        assert!(pending.contains("https://hint.example/one"));
        assert!(!pending.contains("https://hint.example/two"));
    }

    #[tokio::test]
    async fn benchmark_fixture_case_runs_real_controller_loop_to_completion() {
        let dir = temp_test_dir("research-benchmark-fixture");

        let result = run_research_benchmark_case_debug(ResearchBenchmarkCaseInput {
            case_id: "fixture-case".to_string(),
            title: "Fixture Case".to_string(),
            category: "product-decision".to_string(),
            prompt: "Compare two current developer laptops for a local Rust and AI workflow."
                .to_string(),
            data_dir: dir.clone(),
            mode: ResearchBenchmarkMode::Fixture,
            model_input: None,
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            quality_max_iterations: 2,
            cli_launch_mode: Some("auto".to_string()),
            ai_task_timeout_secs: 1200,
        })
        .await
        .unwrap();

        assert_eq!(result.status, "completed");
        assert_eq!(result.quality_status.as_deref(), Some("passed"));
        assert!(result.final_output.is_some());
        assert!(result
            .research_controller_artifacts_json
            .as_deref()
            .is_some_and(|json| json.contains("\"claim_log\"")));
        assert!(result
            .research_controller_artifacts_json
            .as_deref()
            .is_some_and(|json| json.contains("\"narrative_state\"")));
        assert!(result
            .research_source_diagnostics_json
            .as_deref()
            .is_some_and(|json| json.contains("\"source_pack\"")));
        assert!(result.resolved_system_prompt.is_some());
        assert!(result.resolved_user_prompt.is_some());
        assert_eq!(
            result.resolved_system_prompt.as_deref(),
            Some(
                "[redacted research system prompt; provider/source-pack instructions omitted for storage]"
            )
        );
        let stored_user_prompt = result
            .resolved_user_prompt
            .as_deref()
            .expect("benchmark fixture should persist a redacted user prompt");
        assert!(stored_user_prompt.contains("[redacted source documents for storage]"));
        assert!(stored_user_prompt.contains("[redacted storage-safe reminder only]"));
        assert!(!stored_user_prompt
            .contains("- Source 1 | official_or_primary | Britannica: Justinian I"));
        assert_eq!(result.research_controller_iteration, Some(2));
        assert_eq!(result.research_controller_max_iterations, Some(2));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn task_contract_snapshot_matrix_is_frozen() {
        let dir = temp_test_dir("task-contract-snapshot");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let running_task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix) VALUES ('Running', 'researching', '[Research]')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let active_task = tokio::spawn(async {
            sleep(Duration::from_secs(60)).await;
        });
        state
            .active_tasks
            .lock()
            .await
            .insert(running_task_id, active_task.abort_handle());
        let server = test_server_context(Arc::clone(&state));
        let delete_status =
            response_status(delete_task(State(Arc::clone(&server)), Path(running_task_id)).await);
        let delete_row = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, error_message FROM tasks WHERE id = ?",
        )
        .bind(running_task_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(active_task.await.unwrap_err().is_cancelled());

        let retry_task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, model, system_prompt, user_prompt, file_prefix, file_type, research_type, research_mode, research_format, research_topic, engine_kind, quality_status, quality_max_iterations, quality_depth) VALUES ('Original research', 'completed', 'cli:codex', 'system', 'user', '[AI-Research]', 'md', 'initial', 'general', 'md', 'topic', 'cli', 'untrusted', 2, 'standard')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let retry_status = response_status(
            retry_task(
                State(Arc::clone(&server)),
                Path(retry_task_id),
                Some(Json(RetryTaskPayload {
                    derive_task: Some(true),
                    engine_preset_id: Some(-1),
                    research_intensity: Some("high".to_string()),
                    research_quality_max_iterations: Some(5),
                    research_quality_depth: Some("strict".to_string()),
                })),
            )
            .await,
        );
        let retried_row = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<String>,
            ),
        >(
            "SELECT original_name, status, model, engine_kind, quality_max_iterations, quality_depth FROM tasks WHERE id > ? ORDER BY id DESC LIMIT 1",
        )
        .bind(retry_task_id)
        .fetch_one(&db)
        .await
        .unwrap();

        let benchmark_dir = temp_test_dir("task-contract-snapshot-benchmark");
        let benchmark_result = run_research_benchmark_case_debug(ResearchBenchmarkCaseInput {
            case_id: "fixture-case".to_string(),
            title: "Fixture Case".to_string(),
            category: "product-decision".to_string(),
            prompt: "Compare two current developer laptops for a local Rust and AI workflow."
                .to_string(),
            data_dir: benchmark_dir.clone(),
            mode: ResearchBenchmarkMode::Fixture,
            model_input: None,
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            quality_max_iterations: 2,
            cli_launch_mode: Some("auto".to_string()),
            ai_task_timeout_secs: 1200,
        })
        .await
        .unwrap();
        let entries = vec![
            (
                "delete.running",
                format!(
                    "{}|{}|{}",
                    delete_status.as_u16(),
                    delete_row.0,
                    delete_row.1.unwrap_or_default()
                ),
            ),
            (
                "retry.derived",
                format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    retry_status.as_u16(),
                    retried_row.0,
                    retried_row.1,
                    retried_row.2.unwrap_or_default(),
                    retried_row.3.unwrap_or_default(),
                    retried_row.4.unwrap_or_default(),
                    retried_row.5.unwrap_or_default()
                ),
            ),
            (
                "benchmark.fixture",
                format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    benchmark_result.status,
                    benchmark_result.quality_status.unwrap_or_default(),
                    benchmark_result
                        .research_controller_stage
                        .unwrap_or_default(),
                    benchmark_result
                        .research_controller_iteration
                        .unwrap_or_default(),
                    benchmark_result
                        .research_controller_max_iterations
                        .unwrap_or_default(),
                    benchmark_result.resolved_system_prompt.unwrap_or_default(),
                    benchmark_result.resolved_user_prompt.unwrap_or_default()
                ),
            ),
        ];
        let fingerprint = task_snapshot_fingerprint(&entries);
        assert_eq!(
            fingerprint, 0x8c2e68d91d5e239e,
            "task snapshot fingerprint changed: {fingerprint:#018x}"
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(benchmark_dir);
    }

    #[test]
    fn benchmark_timeout_seconds_honor_sub_minute_values() {
        assert_eq!(benchmark_ai_task_timeout_secs(1), 1);
        assert_eq!(benchmark_ai_task_timeout_secs(7), 7);
        assert_eq!(benchmark_ai_task_timeout_secs(59), 59);
        assert_eq!(benchmark_ai_task_timeout_secs(0), 1);
    }

    #[test]
    fn benchmark_historical_categories_route_to_historical_lens() {
        assert_eq!(
            benchmark_research_mode("historical-explanation"),
            "historical"
        );
        assert_eq!(benchmark_research_mode("historical-research"), "historical");
    }

    #[test]
    fn benchmark_local_recommendation_category_routes_to_local_lens() {
        assert_eq!(benchmark_research_mode("local-recommendation"), "local");
    }

    #[test]
    fn benchmark_technology_categories_route_to_split_lenses() {
        assert_eq!(
            benchmark_research_mode("technology-implementation"),
            "technology_implementation"
        );
        assert_eq!(
            benchmark_research_mode("technology-decision"),
            "technology_implementation"
        );
        assert_eq!(
            benchmark_research_mode("product-decision"),
            "technology_implementation"
        );
        assert_eq!(
            benchmark_research_mode("technology-concept"),
            "technology_concept"
        );
    }

    #[test]
    fn replay_finalization_repairs_missing_visible_source_audit_before_validation() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: (1..=7)
                .map(|idx| ResearchSourceCard {
                    id: format!("S{idx}"),
                    url: format!("https://example{idx}.gov/evidence/{idx}"),
                    title: format!("Official Source {idx}"),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: None,
                    extracted_facts: vec![format!("fact {idx}")],
                    limitation: Some("scope limit".to_string()),
                    diagnostics_ref: None,
                    confidence: Some("high".to_string()),
                })
                .collect(),
            claim_log: (1..=7)
                .map(|idx| ResearchClaimLogEntry {
                    id: format!("C{idx}"),
                    claim: format!("검증된 주장 {idx}"),
                    claim_type: Some("verified_fact".to_string()),
                    support_source_card_ids: vec![format!("S{idx}")],
                    support_urls: Vec::new(),
                    confidence: Some("high".to_string()),
                    uncertainty_note: Some("none".to_string()),
                    needs_verification: Some(false),
                })
                .collect(),
            conflict_map: Vec::new(),
            research_debt: vec![ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: None,
                missing_evidence: "follow-up monitoring".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["official monitoring query".to_string()],
                next_check_actions: vec!["monitor follow-up".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "failed".to_string(),
                failure_messages: vec!["source audit missing".to_string()],
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 1,
            }),
            warnings: Vec::new(),
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("policy replay".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("policy replay".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let result = replay_research_benchmark_case_debug(ResearchReplayCaseInput {
            case_id: "replay-pass".to_string(),
            title: "Replay Pass".to_string(),
            category: "current-policy-regulatory".to_string(),
            prompt: "두 정책 체계의 배경, 책임, 운영 부담, 후속 확인 과제를 독자에게 설명하세요.".to_string(),
            draft_output: "## 최종 답변 (Final Answer)\n\n기존 결론은 남겨 두되, 두 정책 체계가 어떤 배경과 목적에서 갈라지는지 먼저 설명해야 합니다. 같은 규제 목표를 말하더라도 실제 집행 기관과 민간 사업자가 감당하는 책임이 다르기 때문에, 독자는 누가 기준을 정하고 누가 비용과 운영 부담을 떠안는지 분리해서 읽어야 합니다. 예를 들어 한 체계는 사전에 절차와 문서 요건을 촘촘히 요구해 초기 준비 비용이 커질 수 있고, 다른 체계는 사후 감독과 시장 감시에 무게를 두어 운영 중 보고와 시정 의무가 더 커질 수 있습니다. 따라서 독자에게는 어느 제도가 더 엄격한지를 단순 비교하기보다, 실제 도입 일정과 내부 통제 체계, 외부 감사 대응 방식이 어떻게 달라지는지까지 풀어서 설명해야 합니다. 또한 지금 확보된 공식 자료가 어디까지는 직접 뒷받침하고 어디부터는 후속 확인이 필요한지 함께 적어야 실제 의사결정에 쓸 수 있습니다. 마지막으로 당장 채택 가능한 판단과 보수적으로 남겨야 할 판단을 나눠 적어야 독자가 추가 확인 우선순위를 바로 정할 수 있고, 향후 규정 개정이나 해석 지침이 나왔을 때 어느 쟁점을 먼저 다시 확인해야 하는지도 선명해집니다.\n".to_string(),
            controller_artifacts_json: serde_json::to_string(&artifacts).unwrap(),
            research_source_diagnostics_json: serde_json::to_string(&diagnostics).unwrap(),
            model_input: "cli:codex".to_string(),
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
        })
        .unwrap();

        assert_eq!(result.quality_status.as_deref(), Some("passed"));
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("## 출처 감사 (Source Audit)")));
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("| deterministic gate status | passed |")));
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("| open debt count | 1 |")));
        let persisted_artifacts = serde_json::from_str::<ResearchControllerArtifacts>(
            result
                .research_controller_artifacts_json
                .as_deref()
                .expect("replay artifacts should persist"),
        )
        .unwrap();
        assert_eq!(
            persisted_artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("passed")
        );
        assert_eq!(
            persisted_artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.open_debt_count),
            Some(1)
        );
    }

    #[test]
    fn replay_finalization_keeps_insufficient_artifacts_untrusted() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://example.com/one".to_string(),
                title: "Only Source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["one fact".to_string()],
                limitation: Some("insufficient breadth".to_string()),
                diagnostics_ref: None,
                confidence: Some("medium".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "하나의 주장".to_string(),
                claim_type: Some("verified_fact".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("medium".to_string()),
                uncertainty_note: Some("needs more sources".to_string()),
                needs_verification: Some(true),
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "failed".to_string(),
                failure_messages: vec!["thin evidence".to_string()],
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            }),
            warnings: Vec::new(),
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("thin replay".to_string()),
            source_pack: None,
            scrapes: Vec::new(),
            context_packing: None,
        };

        let result = replay_research_benchmark_case_debug(ResearchReplayCaseInput {
            case_id: "replay-fail".to_string(),
            title: "Replay Fail".to_string(),
            category: "comparative-product-technical-decision".to_string(),
            prompt: "Compare two current laptops for local AI work.".to_string(),
            draft_output: "## 최종 답변 (Final Answer)\n\n짧은 결론입니다.\n".to_string(),
            controller_artifacts_json: serde_json::to_string(&artifacts).unwrap(),
            research_source_diagnostics_json: serde_json::to_string(&diagnostics).unwrap(),
            model_input: "cli:codex".to_string(),
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
        })
        .unwrap();

        assert_eq!(result.quality_status.as_deref(), Some("untrusted"));
        assert!(result
            .quality_last_failure
            .as_deref()
            .is_some_and(|message| message.contains("source audit URL count")));
    }

    #[test]
    fn replay_scaffold_warning_blocks_pass_without_supported_claim_log() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: (1..=7).map(scaffolded_local_pi_source_card).collect(),
            claim_log: Vec::new(),
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "failed".to_string(),
                failure_messages: vec!["artifact scaffolded".to_string()],
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            }),
            warnings: vec![PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string()],
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("policy replay scaffold".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("policy replay scaffold".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let result = replay_research_benchmark_case_debug(ResearchReplayCaseInput {
            case_id: "replay-scaffold-untrusted".to_string(),
            title: "Replay Scaffold Untrusted".to_string(),
            category: "current-policy-regulatory".to_string(),
            prompt: "Explain a current policy issue using the provided evidence pack.".to_string(),
            draft_output: format!(
                "## 최종 답변 (Final Answer)\n\n{}",
                "This replay explains what the visible provenance can show directly, separates source capture from validated substantive claims, and preserves the evidence boundary while additional claim linkage is still missing. It keeps the narrative focused on traceability, why adopted public URLs remain useful in the appendix, what remains unsupported without a durable claim log, and how a reviewer should interpret the confidence boundary before relying on any conclusion. The discussion also states which procedural facts are established, which substantive conclusions remain blocked, and why the run must remain untrusted until supported claims are emitted with resolvable evidence links.\n\n".repeat(3)
            ),
            controller_artifacts_json: serde_json::to_string(&artifacts).unwrap(),
            research_source_diagnostics_json: serde_json::to_string(&diagnostics).unwrap(),
            model_input: "pi:qwen".to_string(),
            research_intensity: "medium".to_string(),
            quality_depth: "strict".to_string(),
        })
        .expect("replay should complete with untrusted scaffold status");

        assert_eq!(result.quality_status.as_deref(), Some("untrusted"));
        assert_eq!(
            result.research_controller_stage.as_deref(),
            Some(RESEARCH_STAGE_UNTRUSTED)
        );
        assert!(result
            .quality_last_failure
            .as_deref()
            .is_some_and(|message| message.contains(
                "local pi provenance scaffold requires at least one supported Claim Log row before trust can pass"
            )));
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("## 출처 감사 (Source Audit)")));
    }

    #[tokio::test]
    async fn validate_task_research_output_rerenders_visible_quality_gate_from_persisted_gate() {
        let dir = temp_test_dir("research-quality-gate-sync");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_intensity, quality_depth, research_topic, web_search_requested) VALUES ('Synced research', 'researching', '[AI-Research]', 'md', 'high', 'strict', 'policy comparison', 'true')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let artifacts = ResearchControllerArtifacts {
            version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
            events: Vec::new(),
            source_cards: (1..=7)
                .map(|idx| ResearchSourceCard {
                    id: format!("S{idx}"),
                    url: format!("https://example{idx}.gov/evidence/{idx}"),
                    title: format!("Official Source {idx}"),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: None,
                    extracted_facts: vec![format!("fact {idx}")],
                    limitation: Some("scope limit".to_string()),
                    diagnostics_ref: None,
                    confidence: Some("high".to_string()),
                })
                .collect(),
            claim_log: (1..=7)
                .map(|idx| ResearchClaimLogEntry {
                    id: format!("C{idx}"),
                    claim: format!("검증된 주장 {idx}"),
                    claim_type: Some("verified_fact".to_string()),
                    support_source_card_ids: vec![format!("S{idx}")],
                    support_urls: Vec::new(),
                    confidence: Some("high".to_string()),
                    uncertainty_note: Some("none".to_string()),
                    needs_verification: Some(false),
                })
                .collect(),
            conflict_map: Vec::new(),
            research_debt: vec![ResearchDebtItem {
                id: "D1".to_string(),
                severity: "medium".to_string(),
                failed_gate: None,
                missing_evidence: "follow-up monitoring".to_string(),
                required_source_class: Some("official_or_primary".to_string()),
                candidate_queries: vec!["official monitoring query".to_string()],
                next_check_actions: vec!["monitor follow-up".to_string()],
                status: "open".to_string(),
            }],
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "failed".to_string(),
                failure_messages: vec!["stale gate".to_string()],
                unsupported_claim_count: 0,
                unresolved_conflict_count: 0,
                open_debt_count: 0,
            }),
            warnings: Vec::new(),
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("policy comparison".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("policy comparison".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };
        persist_research_controller_artifacts(&state, task_id, &artifacts).await;
        persist_research_source_diagnostics(&state, task_id, diagnostics).await;
        let draft = "## 최종 답변 (Final Answer)\n\n기존 결론은 남겨 두되, 두 정책 체계가 어떤 순서와 배경 속에서 형성되었는지 먼저 설명해야 합니다. 같은 목표를 말하는 제도라도 실제로는 집행 기관, 현장 운영 조직, 적용 대상 사업자가 각각 다른 책임과 비용을 부담하므로 역할 차이를 분리해서 써야 합니다. 또한 지금 확보된 근거가 어느 판단까지는 직접 뒷받침하고 어느 지점부터는 추가 확인이 필요한지 함께 밝혀야 독자가 실제 선택에 바로 활용할 수 있습니다. 마지막으로 당장 채택 가능한 판단과 후속 확인이 필요한 판단을 나눠 적어야 보수적인 의사결정 기준이 선명해집니다.\n";

        let finalized =
            finalize_task_research_output(&state, task_id, draft, "[AI-Research]", "md").await;
        let validated =
            validate_task_research_output(&state, task_id, &finalized, "[AI-Research]", "md", None)
                .await
                .expect("quality validation should pass");
        let persisted_artifacts = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert!(validated.contains("| deterministic gate status | passed |"));
        assert!(validated.contains("| open debt count | 1 |"));
        assert_eq!(
            persisted_artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.status.as_str()),
            Some("passed")
        );
        assert_eq!(
            persisted_artifacts
                .quality_gate
                .as_ref()
                .map(|gate| gate.open_debt_count),
            Some(1)
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn validate_task_research_output_rejects_transient_repair_hint_url_as_evidence() {
        let dir = temp_test_dir("research-repair-hint-provenance");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, research_intensity, quality_depth, research_topic, web_search_requested) VALUES ('Repair hint provenance', 'researching', '[AI-Research]', 'md', 'medium', 'medium', '남산 아침 러닝과 카페 동선', 'false')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let artifacts = ResearchControllerArtifacts {
            version: RESEARCH_CONTROLLER_ARTIFACT_VERSION,
            events: Vec::new(),
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://parks.seoul.go.kr/template/sub/namsan.do".to_string(),
                title: "남산공원 안내".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["운영 정보".to_string()],
                limitation: Some("계절 변동 가능".to_string()),
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "남산공원 운영 정보는 공식 안내로 확인할 수 있다.".to_string(),
                claim_type: Some("verified_fact".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: Some("none".to_string()),
                needs_verification: Some(false),
            }],
            conflict_map: Vec::new(),
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: None,
            warnings: Vec::new(),
        };
        persist_research_controller_artifacts(&state, task_id, &artifacts).await;

        let output = r#"
## 최종 답변 (Final Answer)

남산공원 공식 안내를 참고하면 운영 시간과 접근 동선은 현장 공지 기준으로 다시 확인하는 편이 안전하다.

# Verification Appendix
[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://parks.seoul.go.kr/template/sub/namsan.do",
      "title": "남산공원 안내",
      "source_class": "official_or_primary"
    }
  ],
  "claim_log": [
    {
      "id": "C1",
      "claim": "남산공원 운영 정보는 공식 안내로 확인할 수 있다.",
      "support_source_card_ids": ["S1"]
    }
  ],
  "conflict_map": [],
  "research_debt": []
}
```
"#;
        let prohibited = HashSet::from([String::from(
            "https://parks.seoul.go.kr/template/sub/namsan.do",
        )]);

        let err = validate_task_research_output(
            &state,
            task_id,
            output,
            "[AI-Research]",
            "md",
            Some(&prohibited),
        )
        .await
        .unwrap_err();

        assert!(err
            .message
            .contains("repair search hint URLs cannot be cited as adopted evidence"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn local_pi_source_pack_scaffold_cards_require_local_pi_and_missing_or_empty_artifacts() {
        let current = ResearchControllerArtifacts::default();
        let parsed_empty = ResearchControllerArtifacts::default();
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("subject".to_string()),
            source_pack: Some(scaffold_test_source_pack(2)),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let cards = local_pi_source_pack_scaffold_cards_for_iteration(
            &current,
            Some(&parsed_empty),
            None,
            Some(&diagnostics),
            true,
        )
        .expect("empty parsed artifacts should scaffold source cards");
        assert_eq!(cards.len(), 2);

        assert!(local_pi_source_pack_scaffold_cards_for_iteration(
            &current,
            None,
            Some(MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR),
            Some(&diagnostics),
            false,
        )
        .is_none());
        assert!(local_pi_source_pack_scaffold_cards_for_iteration(
            &current,
            None,
            Some("invalid research artifact JSON"),
            Some(&diagnostics),
            true,
        )
        .is_none());
    }

    #[test]
    fn scaffold_warning_blocks_trust_until_supported_claim_log_exists() {
        let mut scaffolded = ResearchControllerArtifacts {
            source_cards: vec![scaffolded_local_pi_source_card(1)],
            warnings: vec![PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string()],
            ..ResearchControllerArtifacts::default()
        };

        assert!(
            scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict")).is_some()
        );

        scaffolded.claim_log.push(ResearchClaimLogEntry {
            id: "C1".to_string(),
            claim: "unsupported claim".to_string(),
            claim_type: None,
            support_source_card_ids: Vec::new(),
            support_urls: Vec::new(),
            confidence: Some("low".to_string()),
            uncertainty_note: None,
            needs_verification: Some(true),
        });
        assert!(
            scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict")).is_some()
        );

        scaffolded.claim_log[0].support_source_card_ids = vec!["SP1".to_string()];
        assert!(
            scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict")).is_some()
        );

        scaffolded.claim_log[0].support_urls = vec!["https://example1.gov/source/1".to_string()];
        assert!(
            scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict")).is_none()
        );
    }

    #[test]
    fn strict_scaffold_claims_require_direct_public_support_urls() {
        let mut scaffolded = ResearchControllerArtifacts {
            source_cards: vec![scaffolded_local_pi_source_card(1)],
            warnings: vec![PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string()],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "supported only by source card id".to_string(),
                claim_type: None,
                support_source_card_ids: vec!["SP1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("medium".to_string()),
                uncertainty_note: None,
                needs_verification: Some(true),
            }],
            ..ResearchControllerArtifacts::default()
        };

        let failure = scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict"))
            .expect("strict scaffold should require public URL support");
        assert!(failure.contains("every Claim Log row to include at least one public support URL"));

        scaffolded.claim_log[0].support_urls = vec!["https://example1.gov/source/1".to_string()];
        assert!(
            scaffold_trust_block_failure(&scaffolded, Some("medium"), Some("strict")).is_none()
        );
    }

    #[test]
    fn scrape_task_identity_name_prefers_title_and_falls_back_to_url() {
        assert_eq!(
            scrape_task_identity_name("Example Article", "https://example.com/article"),
            "Example Article"
        );
        assert_eq!(
            scrape_task_identity_name("   ", "https://example.com/article"),
            "https://example.com/article"
        );
    }

    #[tokio::test]
    async fn task_update_event_carries_quality_and_controller_progress_from_task() {
        let dir = temp_test_dir("task-update-event-progress");
        let db = setup_db(&dir).await.unwrap();
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, quality_current_iteration, quality_max_iterations, quality_status, research_controller_stage, research_controller_iteration, research_controller_max_iterations) VALUES ('Queued scrape', 'scraping', 1, 3, 'running', 'draft', 2, 4)",
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

        let event = task_update_event(task_id, "queued", "Display Name", Some(&task));

        assert_eq!(event.id, task_id);
        assert_eq!(event.status, "queued");
        assert_eq!(event.original_name, "Display Name");
        assert_eq!(event.quality_current_iteration, Some(1));
        assert_eq!(event.quality_max_iterations, Some(3));
        assert_eq!(event.quality_status.as_deref(), Some("running"));
        assert_eq!(event.research_controller_stage.as_deref(), Some("draft"));
        assert_eq!(event.research_controller_iteration, Some(2));
        assert_eq!(event.research_controller_max_iterations, Some(4));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_promotes_resolved_with_caveat_conflict_to_actionable_debt() {
        let dir = temp_test_dir("persist-conflict-debt-normalization");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type) VALUES ('Conflict normalization', 'researching', '[AI-Research]', 'md')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let artifact_json = serde_json::json!({
            "version": 1,
            "source_cards": [
                {
                    "id": "S1",
                    "url": "https://www.nist.gov/itl/ai-risk-management-framework",
                    "title": "NIST AI RMF",
                    "source_class": "official_or_primary"
                }
            ],
            "claim_log": [
                {
                    "id": "C1",
                    "claim": "NIST AI RMF policy summary",
                    "support_source_card_ids": ["S1"]
                }
            ],
            "conflict_map": [
                {
                    "id": "K3",
                    "topic": "NIST AI RMF enforcement scope ambiguity",
                    "conflicting_claim_ids": ["C1"],
                    "source_card_ids": ["S1"],
                    "resolution_status": "resolved_with_caveat",
                    "resolution_note": "Needs one more official clarification",
                    "promoted_to_debt": false
                }
            ],
            "research_debt": [],
            "warnings": []
        });
        let normalized_output = format!(
            "## 최종 답변 (Final Answer)\n\n충분히 긴 결론입니다.\n\n[RESEARCH_ARTIFACT_JSON]\n```json\n{}\n```",
            serde_json::to_string_pretty(&artifact_json).unwrap()
        );

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            &normalized_output,
            false,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(
            persisted.conflict_map[0].promoted_to_debt,
            Some(true),
            "resolved_with_caveat conflict should be promoted to debt"
        );
        let debt = persisted
            .research_debt
            .iter()
            .find(|debt| debt.id == "conflict-debt-k3")
            .expect("normalized debt should be created");
        assert_eq!(debt.status, "open");
        assert!(!debt.candidate_queries.is_empty());
        assert!(!debt.next_check_actions.is_empty());
        assert!(debt.missing_evidence.contains("K3"));
        assert!(debt.missing_evidence.contains("C1"));
        assert!(debt.missing_evidence.contains("S1"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_scaffolds_provenance_source_cards_for_local_pi_missing_block() {
        let dir = temp_test_dir("persist-local-pi-source-card-scaffold");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind) VALUES ('Local scaffold', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        persist_research_source_diagnostics(
            &state,
            task_id,
            ResearchSourceDiagnosticsEnvelope {
                version: 1,
                subject: Some("subject".to_string()),
                source_pack: Some(scaffold_test_source_pack(3)),
                scrapes: Vec::new(),
                context_packing: None,
            },
        )
        .await;

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            "## 최종 답변 (Final Answer)\n\nArtifact block omitted.",
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(persisted.source_cards.len(), 3);
        assert!(persisted.claim_log.is_empty());
        assert!(persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING));
        assert!(persisted
            .warnings
            .iter()
            .any(|warning| { warning == MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR }));
        assert_eq!(
            persisted.source_cards[0].extracted_facts,
            vec!["pre-collected source-pack provenance only".to_string()]
        );

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_repairs_local_pi_claim_log_from_visible_rows() {
        let dir = temp_test_dir("persist-local-pi-claim-log-repair");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind) VALUES ('Local claim repair', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        persist_research_source_diagnostics(
            &state,
            task_id,
            ResearchSourceDiagnosticsEnvelope {
                version: 1,
                subject: Some("subject".to_string()),
                source_pack: Some(scaffold_test_source_pack(2)),
                scrapes: Vec::new(),
                context_packing: None,
            },
        )
        .await;

        let normalized_output = r#"
## 최종 답변 (Final Answer)

Official Source 1 confirms the rollout keeps a public deployment checklist. Official Source 2 confirms the project keeps a public API reference.

# 검증 부록
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C9 | Official Source 1 confirms the rollout keeps a public deployment checklist. | SP1; https://example1.gov/source/1 | high | none |
| C10 | Official Source 2 confirms the project keeps a public API reference. | https://example2.gov/source/2 | medium | limited |
"#;

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            normalized_output,
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(persisted.source_cards.len(), 2);
        assert_eq!(persisted.claim_log.len(), 2);
        assert_eq!(
            persisted.claim_log[0].support_source_card_ids,
            vec!["SP1".to_string()]
        );
        assert_eq!(
            persisted.claim_log[0].support_urls,
            vec!["https://example1.gov/source/1".to_string()]
        );
        assert_eq!(
            persisted.claim_log[1].support_source_card_ids,
            vec!["SP2".to_string()]
        );
        assert_eq!(
            persisted.claim_log[1].support_urls,
            vec!["https://example2.gov/source/2".to_string()]
        );
        assert!(persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING));
        assert!(scaffold_trust_block_failure(&persisted, Some("medium"), Some("strict")).is_none());

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_rejects_unsafe_or_unlinked_local_pi_claim_log_rows() {
        let dir = temp_test_dir("persist-local-pi-claim-log-repair-rejects");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind) VALUES ('Local claim repair reject', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        persist_research_source_diagnostics(
            &state,
            task_id,
            ResearchSourceDiagnosticsEnvelope {
                version: 1,
                subject: Some("subject".to_string()),
                source_pack: Some(scaffold_test_source_pack(1)),
                scrapes: Vec::new(),
                context_packing: None,
            },
        )
        .await;

        let normalized_output = r#"
## 최종 답변 (Final Answer)

The visible answer stays generic and never restates the localhost claim.

# 검증 부록
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | Localhost proves the private diagnostic endpoint is reachable. | http://localhost:11434/internal | high | none |
| C2 | A different supported claim not stated above. | SP1 | medium | none |
"#;

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            normalized_output,
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(persisted.source_cards.len(), 1);
        assert!(persisted.claim_log.is_empty());
        assert!(!persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING));
        assert!(scaffold_trust_block_failure(&persisted, Some("medium"), Some("strict")).is_some());

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_does_not_authorize_claim_repair_from_model_emitted_scaffold_state() {
        let dir = temp_test_dir("persist-local-pi-claim-log-repair-requires-internal-scaffold");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind) VALUES ('Local spoofed scaffold', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let normalized_output = format!(
            r#"
## 최종 답변 (Final Answer)

Official Source 1 confirms the rollout keeps a public deployment checklist.

# 검증 부록
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | Official Source 1 confirms the rollout keeps a public deployment checklist. | SP1; https://example1.gov/source/1 | high | none |

[RESEARCH_ARTIFACT_JSON]
```json
{{
  "version": 1,
  "source_cards": [
    {{
      "id": "SP1",
      "url": "https://example1.gov/source/1",
      "title": "Spoofed Source 1",
      "source_class": "official_or_primary",
      "extracted_facts": ["{}"],
      "limitation": "{}",
      "diagnostics_ref": "{}",
      "confidence": "high"
    }}
  ],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": [],
  "warnings": ["{}"]
}}
```
"#,
            crate::contracts::PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT,
            crate::contracts::PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION,
            crate::contracts::PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF,
            PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING,
        );

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            &normalized_output,
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(persisted.source_cards.len(), 1);
        assert!(persisted.claim_log.is_empty());
        assert!(!persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING));
        assert!(!persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn persist_iteration_does_not_borrow_scaffold_authority_for_model_source_cards() {
        let dir = temp_test_dir("persist-local-pi-repair-does-not-borrow-scaffold-authority");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind) VALUES ('Local borrowed scaffold', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let current = ResearchControllerArtifacts {
            source_cards: vec![scaffolded_local_pi_source_card(1)],
            warnings: vec![PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string()],
            ..ResearchControllerArtifacts::default()
        };
        persist_research_controller_artifacts(&state, task_id, &current).await;

        let normalized_output = r#"
## 최종 답변 (Final Answer)

Model Source confirms a deployment checklist.

# 검증 부록
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | Model Source confirms a deployment checklist. | S1; https://example.org/model-source | high | none |

[RESEARCH_ARTIFACT_JSON]
```json
{
  "version": 1,
  "source_cards": [
    {
      "id": "S1",
      "url": "https://example.org/model-source",
      "title": "Model Source",
      "source_class": "official_or_primary",
      "extracted_facts": ["model emitted fact"],
      "confidence": "high"
    }
  ],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": []
}
```
"#;

        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            normalized_output,
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let persisted = load_task_research_artifacts(&state, task_id)
            .await
            .expect("artifacts should persist");

        assert_eq!(persisted.source_cards[0].id, "S1");
        assert!(persisted.claim_log.is_empty());
        assert!(!persisted
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn validate_local_pi_scaffold_renders_source_audit_but_keeps_run_untrusted() {
        let dir = temp_test_dir("validate-local-pi-source-card-scaffold");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind, web_search_requested, research_intensity, quality_depth) VALUES ('Local scaffold validation', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama', 'true', 'medium', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        persist_research_source_diagnostics(
            &state,
            task_id,
            ResearchSourceDiagnosticsEnvelope {
                version: 1,
                subject: Some("subject".to_string()),
                source_pack: Some(scaffold_test_source_pack(7)),
                scrapes: Vec::new(),
                context_packing: None,
            },
        )
        .await;
        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            "## 최종 답변 (Final Answer)\n\nArtifact block omitted.",
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let output = format!(
            "## 최종 답변 (Final Answer)\n\n{}",
            "This report explains what the source-pack provenance can show directly, separates procedural evidence capture from validated substantive claims, and keeps the local run narrow where support is incomplete. It also describes the operational limitation created by the missing machine-readable artifact block, the remaining verification work a human reviewer must perform, and why the visible source audit still matters for traceability. The text stays focused on the evidence-handling path, the limits of the local model, the confidence boundary around adopted URLs, and the practical consequence for downstream review. Finally, it states that the appendix preserves provenance while the run remains untrusted until a durable claim log is emitted with resolvable support.\n\n".repeat(3)
        );

        let err =
            validate_task_research_output(&state, task_id, &output, "[AI-Research]", "md", None)
                .await
                .expect_err("empty claim log should remain untrusted");

        assert!(err.output.contains("## 출처 감사 (Source Audit)"));
        assert!(err.output.contains("https://example1.gov/source/1"));
        assert!(!err.message.contains("source audit URL count 0"));
        assert!(err.message.contains(
            "local pi provenance scaffold requires at least one supported Claim Log row before trust can pass"
        ));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn validate_local_pi_scaffold_with_repaired_claim_log_can_pass_medium_strict() {
        let dir = temp_test_dir("validate-local-pi-claim-log-repair");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let task_id = sqlx::query(
            "INSERT INTO tasks (original_name, status, file_prefix, file_type, model, engine_kind, web_search_requested, research_intensity, quality_depth) VALUES ('Local claim repair validation', 'researching', '[AI-Research]', 'md', 'pi:qwen', 'pi_ollama', 'true', 'medium', 'strict')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        persist_research_source_diagnostics(
            &state,
            task_id,
            ResearchSourceDiagnosticsEnvelope {
                version: 1,
                subject: Some("subject".to_string()),
                source_pack: Some(scaffold_test_source_pack(2)),
                scrapes: Vec::new(),
                context_packing: None,
            },
        )
        .await;
        persist_iteration_research_artifacts(
            &state,
            task_id,
            &[],
            "md",
            r#"
## 최종 답변 (Final Answer)

Official Source 1 confirms the rollout keeps a public deployment checklist. Official Source 2 confirms the project keeps a public API reference. The report keeps those two claims visible, explains the operational boundary around provenance-only source cards, and limits the conclusion to what the cited public evidence can support directly without pretending the hidden artifact block succeeded.

# 검증 부록
## 주장 로그 (Claim Log)
| ID | Claim | Support | Confidence | Uncertainty |
| --- | --- | --- | --- | --- |
| C1 | Official Source 1 confirms the rollout keeps a public deployment checklist. | SP1; https://example1.gov/source/1 | high | none |
| C2 | Official Source 2 confirms the project keeps a public API reference. | https://example2.gov/source/2 | medium | limited |
"#,
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        let output = "## 최종 답변 (Final Answer)\n\nOfficial Source 1 confirms the rollout keeps a public deployment checklist. Official Source 2 confirms the project keeps a public API reference. The discussion stays narrow, explains the public evidence boundary, and keeps the conclusion limited to what those cited public materials support without importing hidden diagnostics or unsupported claims.\n\nOfficial Source 1 confirms the rollout keeps a public deployment checklist. Official Source 2 confirms the project keeps a public API reference. The discussion stays narrow, explains the public evidence boundary, and keeps the conclusion limited to what those cited public materials support without importing hidden diagnostics or unsupported claims.";

        let validated =
            validate_task_research_output(&state, task_id, output, "[AI-Research]", "md", None)
                .await
                .expect("supported repaired claim log should allow medium strict validation");

        assert!(validated.contains("## 출처 감사 (Source Audit)"));
        assert!(validated.contains("## 주장 로그 (Claim Log)"));
        assert!(validated.contains("https://example2.gov/source/2"));

        db.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn replay_finalization_promotes_resolved_with_caveat_conflict_to_actionable_debt() {
        let artifacts = ResearchControllerArtifacts {
            version: 1,
            events: Vec::new(),
            source_cards: (1..=7)
                .map(|idx| ResearchSourceCard {
                    id: format!("S{idx}"),
                    url: format!("https://example{idx}.gov/evidence/{idx}"),
                    title: format!("Official Source {idx}"),
                    source_class: "official_or_primary".to_string(),
                    accessed_at: None,
                    extracted_facts: vec![format!("fact {idx}")],
                    limitation: Some("scope limit".to_string()),
                    diagnostics_ref: None,
                    confidence: Some("high".to_string()),
                })
                .collect(),
            claim_log: (1..=7)
                .map(|idx| ResearchClaimLogEntry {
                    id: format!("C{idx}"),
                    claim: format!("검증된 주장 {idx}"),
                    claim_type: Some("verified_fact".to_string()),
                    support_source_card_ids: vec![format!("S{idx}")],
                    support_urls: Vec::new(),
                    confidence: Some("high".to_string()),
                    uncertainty_note: Some("none".to_string()),
                    needs_verification: Some(false),
                })
                .collect(),
            conflict_map: vec![ResearchConflictMapEntry {
                id: "K3".to_string(),
                topic: "NIST AI RMF enforcement scope ambiguity".to_string(),
                conflicting_claim_ids: vec!["C1".to_string()],
                source_card_ids: vec!["S1".to_string()],
                resolution_status: Some("resolved_with_caveat".to_string()),
                resolution_note: Some("Needs one more official clarification".to_string()),
                promoted_to_debt: Some(false),
            }],
            research_debt: Vec::new(),
            narrative_state: None,
            reader_quality: None,
            quality_gate: Some(ResearchQualityGateArtifact {
                status: "failed".to_string(),
                failure_messages: vec!["stale conflict state".to_string()],
                unsupported_claim_count: 0,
                unresolved_conflict_count: 1,
                open_debt_count: 0,
            }),
            warnings: Vec::new(),
        };
        let diagnostics = ResearchSourceDiagnosticsEnvelope {
            version: 1,
            subject: Some("policy replay".to_string()),
            source_pack: Some(ResearchSourcePackReport {
                subject: Some("policy replay".to_string()),
                status: "success".to_string(),
                reason: None,
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 7,
                adopted_source_count: 7,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            }),
            scrapes: Vec::new(),
            context_packing: None,
        };

        let result = replay_research_benchmark_case_debug(ResearchReplayCaseInput {
            case_id: "replay-conflict-debt".to_string(),
            title: "Replay Conflict Debt".to_string(),
            category: "current-policy-regulatory".to_string(),
            prompt: "Compare two policy frameworks and explain the practical differences for a reader.".to_string(),
            draft_output: "## 최종 답변 (Final Answer)\n\n정책 비교 결론은 독자가 실제 선택 기준을 바로 적용할 수 있을 만큼 충분한 설명으로 남겨야 합니다. 먼저 제도 초안 공개, 이해관계자 피드백, 최종 집행 단계가 어떤 순서와 배경 속에서 이어졌는지 적어야 왜 현재 기준이 형성되었는지 이해할 수 있습니다. 다음으로 규제기관, 정책 집행 조직, 사업자, 적용 대상 조직이 서로 다른 책임과 권한을 갖기 때문에 누가 기준을 정하고 누가 실제 준수 비용을 감당하는지 분리해서 써야 합니다. 또 공식 문서가 설명하는 목적과 현장 적용 해석 사이에 왜 간격이 남는지, 그 간격이 사용자의 현재 의사결정에 어떤 보수적 제약을 남기는지 밝혀야 합니다. 마지막으로 지금 당장 확정 가능한 판단과 추가 공식 확인이 필요한 판단을 갈라 적고, 남은 불확실성은 후속 검토 항목으로 공개적으로 남겨야 독자가 안전하게 결론을 사용할 수 있습니다.\n".to_string(),
            controller_artifacts_json: serde_json::to_string(&artifacts).unwrap(),
            research_source_diagnostics_json: serde_json::to_string(&diagnostics).unwrap(),
            model_input: "cli:codex".to_string(),
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
        })
        .expect("replay should normalize deferred conflict debt");

        assert_eq!(result.quality_status.as_deref(), Some("passed"));
        let persisted_artifacts = serde_json::from_str::<ResearchControllerArtifacts>(
            result
                .research_controller_artifacts_json
                .as_deref()
                .expect("replay artifacts should persist"),
        )
        .unwrap();
        assert_eq!(
            persisted_artifacts.conflict_map[0].promoted_to_debt,
            Some(true)
        );
        let debt = persisted_artifacts
            .research_debt
            .iter()
            .find(|debt| debt.id == "conflict-debt-k3")
            .expect("normalized debt should persist");
        assert_eq!(debt.status, "open");
        assert!(!debt.candidate_queries.is_empty());
        assert!(!debt.next_check_actions.is_empty());
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("conflict-debt-k3")));
        assert!(result
            .final_output
            .as_deref()
            .is_some_and(|output| output.contains("NIST AI RMF enforcement scope ambiguity")));
    }

    #[test]
    fn merged_artifacts_derive_planning_layers_from_claim_linked_event_cards() {
        let mut current = ResearchControllerArtifacts::default();
        let incoming = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                working_thesis: Some(
                    "한국과 만주가 하나의 전략권으로 묶이면서 일본과 러시아의 선택지가 좁아졌다."
                        .to_string(),
                ),
                event_cards: vec![
                    NarrativeEventCard {
                        label: "뤼순 조차와 불신".to_string(),
                        timeframe: Some("1898".to_string()),
                        actors: vec!["러시아".to_string(), "일본".to_string()],
                        region_or_front: Some("뤼순".to_string()),
                        trigger: Some("러시아의 뤼순 조차권 확보".to_string()),
                        development: Some(
                            "러시아가 뤼순 조차권과 철도 거점을 결합하면서 일본은 한국 방어와 만주 접근이 동시에 위협받는다고 보았다."
                                .to_string(),
                        ),
                        outcome: Some("일본의 대러 불신이 강화되었다.".to_string()),
                        claim_log_ids: vec!["C1".to_string()],
                        source_ids: vec!["S1".to_string()],
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                    NarrativeEventCard {
                        label: "쓰시마 해전".to_string(),
                        timeframe: Some("1905".to_string()),
                        actors: vec!["일본 해군".to_string(), "러시아 발틱함대".to_string()],
                        region_or_front: Some("대한해협".to_string()),
                        trigger: Some("러시아 발틱함대의 극동 도착 시도".to_string()),
                        development: Some(
                            "일본 해군은 대한해협에서 러시아 함대를 격파했고 러시아는 제해권 회복 가능성을 잃었다."
                                .to_string(),
                        ),
                        outcome: Some("포츠머스 강화 압력이 커졌다.".to_string()),
                        claim_log_ids: vec!["C2".to_string()],
                        source_ids: vec!["S2".to_string()],
                        causal_spine: Vec::new(),
                        interpretive_layers: Vec::new(),
                        confidence: None,
                        open_questions: Vec::new(),
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        merge_research_controller_artifacts(&mut current, incoming);

        let state = current.narrative_state.as_ref().unwrap();
        assert!(!state.causal_chain.is_empty());
        assert!(!state.section_outline.is_empty());
        assert!(!state.evidence_layers.is_empty());
        assert!(!state.impacts.is_empty());
        assert_eq!(
            state.causal_chain[0].derived_from.as_deref(),
            Some("event_cards")
        );
        assert!(state.causal_chain[0].rationale.is_none());
        assert_eq!(
            state.section_outline[0].derived_from.as_deref(),
            Some("event_cards")
        );
        assert!(state.section_outline[0].purpose.is_none());
        assert!(current
            .research_debt
            .iter()
            .any(|debt| debt.id == "derived-narrative-causal-chain"));
        assert_eq!(
            state.causal_chain[0].expected_claim_log_ids,
            vec!["C1".to_string(), "C2".to_string()]
        );
    }

    #[test]
    fn merge_trust_boundary_strips_fresh_model_ledgers_from_historical_event_grounding() {
        let mut current = ResearchControllerArtifacts {
            source_cards: vec![ResearchSourceCard {
                id: "S-accepted".to_string(),
                url: "https://trusted.example.org/accepted".to_string(),
                title: "Accepted source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["accepted fact".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C-accepted".to_string(),
                claim: "Accepted claim".to_string(),
                claim_type: Some("verified_fact".to_string()),
                support_source_card_ids: vec!["S-accepted".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            ..ResearchControllerArtifacts::default()
        };
        let incoming = ResearchControllerArtifacts {
            source_cards: vec![ResearchSourceCard {
                id: "S-fabricated".to_string(),
                url: "https://fabricated.example.org/alps-crossing".to_string(),
                title: "Fabricated source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["218 BCE Alps crossing".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C-fabricated".to_string(),
                claim: "In 218 BCE Hannibal crossed the Alps into Italy.".to_string(),
                claim_type: Some("verified_fact".to_string()),
                support_source_card_ids: vec!["S-fabricated".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![NarrativeEventCard {
                    label: "Alpine crossing".to_string(),
                    timeframe: Some("218 BCE".to_string()),
                    actors: vec!["Hannibal".to_string()],
                    region_or_front: Some("Alps".to_string()),
                    trigger: Some("Saguntum crisis escalated".to_string()),
                    development: Some("Hannibal crossed into Italy.".to_string()),
                    outcome: Some("The Italian campaign opened.".to_string()),
                    claim_log_ids: vec!["C-fabricated".to_string()],
                    source_ids: vec!["S-fabricated".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let trusted_source_urls = trusted_artifact_merge_source_urls(&current, None);

        merge_research_controller_artifacts_with_trusted_source_urls(
            &mut current,
            incoming,
            &trusted_source_urls,
        );

        assert_eq!(current.source_cards[0].id, "S-fabricated");
        assert_eq!(current.claim_log[0].id, "C-fabricated");
        let card = current
            .narrative_state
            .as_ref()
            .and_then(|state| state.event_cards.first())
            .expect("event card should persist");
        assert!(card.claim_log_ids.is_empty());
        assert!(card.source_ids.is_empty());
    }

    #[test]
    fn merge_trust_boundary_keeps_event_grounding_for_acquired_source_urls() {
        let mut current = ResearchControllerArtifacts::default();
        let incoming = ResearchControllerArtifacts {
            source_cards: vec![ResearchSourceCard {
                id: "S1".to_string(),
                url: "https://trusted.example.org/alps-crossing".to_string(),
                title: "Acquired source".to_string(),
                source_class: "official_or_primary".to_string(),
                accessed_at: None,
                extracted_facts: vec!["218 BCE Alps crossing".to_string()],
                limitation: None,
                diagnostics_ref: None,
                confidence: Some("high".to_string()),
            }],
            claim_log: vec![ResearchClaimLogEntry {
                id: "C1".to_string(),
                claim: "In 218 BCE Hannibal crossed the Alps into Italy.".to_string(),
                claim_type: Some("verified_fact".to_string()),
                support_source_card_ids: vec!["S1".to_string()],
                support_urls: Vec::new(),
                confidence: Some("high".to_string()),
                uncertainty_note: None,
                needs_verification: Some(false),
            }],
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![NarrativeEventCard {
                    label: "Alpine crossing".to_string(),
                    timeframe: Some("218 BCE".to_string()),
                    actors: vec!["Hannibal".to_string()],
                    region_or_front: Some("Alps".to_string()),
                    trigger: Some("Saguntum crisis escalated".to_string()),
                    development: Some("Hannibal crossed into Italy.".to_string()),
                    outcome: Some("The Italian campaign opened.".to_string()),
                    claim_log_ids: vec!["C1".to_string()],
                    source_ids: vec!["S1".to_string()],
                    causal_spine: Vec::new(),
                    interpretive_layers: Vec::new(),
                    confidence: Some("high".to_string()),
                    open_questions: Vec::new(),
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let trusted_source_urls =
            HashSet::from([
                normalize_result_url("https://trusted.example.org/alps-crossing")
                    .expect("public url"),
            ]);

        merge_research_controller_artifacts_with_trusted_source_urls(
            &mut current,
            incoming,
            &trusted_source_urls,
        );

        let card = current
            .narrative_state
            .as_ref()
            .and_then(|state| state.event_cards.first())
            .expect("event card should persist");
        assert_eq!(card.claim_log_ids, vec!["C1".to_string()]);
        assert_eq!(card.source_ids, vec!["S1".to_string()]);
    }

    #[test]
    fn merged_artifacts_regenerate_derived_planning_when_event_cards_change() {
        let mut current = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    NarrativeEventCard {
                        label: "뤼순 조차".to_string(),
                        timeframe: Some("1898".to_string()),
                        trigger: Some("러시아의 조차권 확보".to_string()),
                        outcome: Some("일본의 대러 불신".to_string()),
                        claim_log_ids: vec!["C1".to_string()],
                        source_ids: vec!["S1".to_string()],
                        ..NarrativeEventCard::default()
                    },
                    NarrativeEventCard {
                        label: "쓰시마 해전".to_string(),
                        timeframe: Some("1905".to_string()),
                        trigger: Some("발틱함대의 극동 항해".to_string()),
                        outcome: Some("러시아의 제해권 상실".to_string()),
                        claim_log_ids: vec!["C2".to_string()],
                        source_ids: vec!["S2".to_string()],
                        ..NarrativeEventCard::default()
                    },
                ],
                causal_chain: vec![NarrativeCausalLink {
                    id: "derived-link-1".to_string(),
                    cause: "일본의 대러 불신".to_string(),
                    effect: "발틱함대의 극동 항해".to_string(),
                    rationale: None,
                    derived_from: Some("event_cards".to_string()),
                    expected_claim_log_ids: vec!["C1".to_string(), "C2".to_string()],
                    expected_source_card_ids: vec!["S1".to_string(), "S2".to_string()],
                }],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };
        let incoming = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    NarrativeEventCard {
                        label: "뤼순 조차".to_string(),
                        timeframe: Some("1898".to_string()),
                        trigger: Some("러시아의 조차권 확보와 철도 거점화".to_string()),
                        outcome: Some("한국과 만주를 묶는 일본의 위협 인식".to_string()),
                        claim_log_ids: vec!["C3".to_string()],
                        source_ids: vec!["S3".to_string()],
                        ..NarrativeEventCard::default()
                    },
                    NarrativeEventCard {
                        label: "쓰시마 해전".to_string(),
                        timeframe: Some("1905".to_string()),
                        trigger: Some("러시아 발틱함대의 대한해협 접근".to_string()),
                        outcome: Some("포츠머스 강화 압력".to_string()),
                        claim_log_ids: vec!["C4".to_string()],
                        source_ids: vec!["S4".to_string()],
                        ..NarrativeEventCard::default()
                    },
                ],
                ..NarrativeState::default()
            }),
            ..ResearchControllerArtifacts::default()
        };

        merge_research_controller_artifacts(&mut current, incoming);

        let state = current.narrative_state.as_ref().unwrap();
        assert_eq!(
            state.causal_chain[0].expected_claim_log_ids,
            vec!["C3".to_string(), "C4".to_string()]
        );
        assert_eq!(
            state.causal_chain[0].derived_from.as_deref(),
            Some("event_cards")
        );
    }

    #[test]
    fn merged_artifacts_keep_derived_narrative_debt_when_later_artifact_omits_narrative_state() {
        let mut current = ResearchControllerArtifacts {
            narrative_state: Some(NarrativeState {
                version: 1,
                event_cards: vec![
                    NarrativeEventCard {
                        label: "뤼순 조차".to_string(),
                        timeframe: Some("1898".to_string()),
                        trigger: Some("러시아의 조차권 확보".to_string()),
                        outcome: Some("일본의 대러 불신".to_string()),
                        claim_log_ids: vec!["C1".to_string()],
                        source_ids: vec!["S1".to_string()],
                        ..NarrativeEventCard::default()
                    },
                    NarrativeEventCard {
                        label: "쓰시마 해전".to_string(),
                        timeframe: Some("1905".to_string()),
                        trigger: Some("발틱함대의 극동 항해".to_string()),
                        outcome: Some("러시아의 제해권 상실".to_string()),
                        claim_log_ids: vec!["C2".to_string()],
                        source_ids: vec!["S2".to_string()],
                        ..NarrativeEventCard::default()
                    },
                ],
                causal_chain: vec![NarrativeCausalLink {
                    id: "derived-link-1".to_string(),
                    cause: "일본의 대러 불신".to_string(),
                    effect: "발틱함대의 극동 항해".to_string(),
                    rationale: None,
                    derived_from: Some("event_cards".to_string()),
                    expected_claim_log_ids: vec!["C1".to_string(), "C2".to_string()],
                    expected_source_card_ids: vec!["S1".to_string(), "S2".to_string()],
                }],
                ..NarrativeState::default()
            }),
            research_debt: vec![ResearchDebtItem {
                id: "derived-narrative-causal-chain".to_string(),
                severity: "medium".to_string(),
                failed_gate: Some("narrative_planning".to_string()),
                missing_evidence: "derived causal chain needs authored rationale".to_string(),
                required_source_class: None,
                candidate_queries: Vec::new(),
                next_check_actions: vec!["write causal rationale".to_string()],
                status: "open".to_string(),
            }],
            ..ResearchControllerArtifacts::default()
        };
        let incoming = ResearchControllerArtifacts {
            narrative_state: None,
            research_debt: Vec::new(),
            ..ResearchControllerArtifacts::default()
        };

        merge_research_controller_artifacts(&mut current, incoming);

        assert!(current
            .research_debt
            .iter()
            .any(|debt| debt.id == "derived-narrative-causal-chain" && debt.status == "open"));
    }
}
