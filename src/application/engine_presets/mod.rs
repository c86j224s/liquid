mod catalog;
mod repository;
mod resolver;
mod status_check;

pub(crate) use catalog::{
    default_engine_preset_by_id, default_engine_preset_to_engine_preset, default_engine_presets,
};
pub(crate) use repository::SqliteEnginePresetRepository;
pub(crate) use resolver::{resolve_engine_for_research, EngineResolutionError};
pub(crate) use status_check::{executable_exists, test_engine_preset_status};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::EnginePreset;
    use crate::server::engine_presets::{
        create_engine_preset, delete_engine_preset, list_engine_presets, test_engine_preset,
        update_engine_preset,
    };
    use crate::test_support::{
        response_status, temp_test_dir, test_server_context, test_state,
        test_state_with_cli_launch_mode,
    };
    use axum::{
        body::to_bytes,
        extract::{Path, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    };
    use liquid_protocol::{EnginePresetCreatePayload, EnginePresetUpdatePayload};
    use liquid_runtime::cli_launcher::CliLaunchMode;
    use liquid_storage_sqlite::setup_db;
    use serde_json::Value;
    use std::fs as std_fs;
    use std::sync::Arc;

    fn route_snapshot_fingerprint(entries: &[(&str, String)]) -> u64 {
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

    async fn json_body(response: impl IntoResponse) -> Value {
        let response = response.into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn stable_preset_snapshot(value: &Value) -> String {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|preset| {
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    preset["id"].as_i64().unwrap(),
                    preset["name"].as_str().unwrap(),
                    preset["engine_kind"].as_str().unwrap(),
                    preset["provider"].as_str().unwrap(),
                    preset["model"].as_str().unwrap_or(""),
                    preset["command"].as_str().unwrap_or(""),
                    preset["default_intensity"].as_str().unwrap(),
                    preset["web_search_enabled"].as_str().unwrap(),
                    preset["fallback_execution"].as_str().unwrap(),
                    preset["is_default"].as_str().unwrap(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn test_cli_and_pi_presets_need_setup_when_launcher_disabled() {
        let dir = temp_test_dir("launcher-disabled-presets");
        let cli_preset = default_engine_preset_by_id(-2).unwrap();
        let pi_preset = default_engine_preset_by_id(-1).unwrap();

        let (cli_status, cli_message) =
            test_engine_preset_status(&cli_preset, &dir, CliLaunchMode::Disabled).await;
        let (pi_status, pi_message) =
            test_engine_preset_status(&pi_preset, &dir, CliLaunchMode::Disabled).await;

        assert_eq!(cli_status, "needs_setup");
        assert_eq!(pi_status, "needs_setup");
        assert!(cli_message.contains("LIQUID_CLI_LAUNCH_MODE=unsandboxed"));
        assert!(pi_message.contains("LIQUID_CLI_LAUNCH_MODE=unsandboxed"));
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_engine_preset_resolution_and_test_endpoint() {
        let dir = temp_test_dir("engine-presets");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));
        let server = test_server_context(Arc::clone(&state));

        let presets_response = list_engine_presets(State(Arc::clone(&server))).await;
        assert_eq!(response_status(presets_response), StatusCode::OK);

        let gemini = resolve_engine_for_research(
            &db,
            None,
            Some("cli:gemini".to_string()),
            Some("high".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(gemini.model_input, "cli:gemini");
        assert_eq!(
            gemini.metadata.engine_preset_name.as_deref(),
            Some("Gemini CLI 조사")
        );
        assert_eq!(gemini.metadata.research_intensity.as_deref(), Some("high"));

        let fallback = resolve_engine_for_research(&db, None, Some("llama3.1".to_string()), None)
            .await
            .unwrap();
        assert_eq!(fallback.model_input, "llama3.1");
        assert_eq!(
            fallback.metadata.engine_kind.as_deref(),
            Some("ollama_legacy")
        );
        assert_eq!(fallback.metadata.fallback_used.as_deref(), Some("true"));
        let reserved_pi_model =
            resolve_engine_for_research(&db, None, Some("pi:llama3.1".to_string()), None)
                .await
                .unwrap();
        assert_eq!(reserved_pi_model.model_input, "llama3");

        let pi = resolve_engine_for_research(&db, Some(-1), None, None)
            .await
            .unwrap();
        assert_eq!(pi.model_input, "pi:llama3");
        assert_eq!(pi.metadata.engine_kind.as_deref(), Some("pi_ollama"));
        assert_eq!(pi.metadata.fallback_used.as_deref(), Some("false"));
        assert_eq!(pi.metadata.web_search_provider.as_deref(), None);

        sqlx::query(
            "INSERT INTO engine_presets (name, engine_kind, provider, model, command, web_search_enabled, enabled, is_default) VALUES ('Custom Claude', 'cli', 'claude-cli', 'claude', 'claude', 'true', 'true', 'false')",
        )
        .execute(&db)
        .await
        .unwrap();
        let claude_id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM engine_presets WHERE name = 'Custom Claude'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let custom_resolution = resolve_engine_for_research(&db, Some(claude_id), None, None)
            .await
            .unwrap();
        assert_eq!(custom_resolution.model_input, "cli:claude");
        let custom_test_response =
            test_engine_preset(State(Arc::clone(&server)), Path(claude_id)).await;
        assert_eq!(response_status(custom_test_response), StatusCode::OK);

        let test_response = test_engine_preset(State(Arc::clone(&server)), Path(-2)).await;
        assert_eq!(response_status(test_response), StatusCode::OK);

        let created_response = create_engine_preset(
            State(Arc::clone(&server)),
            Json(EnginePresetCreatePayload {
                name: "Copied Ollama".to_string(),
                template_id: -5,
                model: Some("llama3.1".to_string()),
            }),
        )
        .await;
        assert_eq!(response_status(created_response), StatusCode::CREATED);
        let copied = sqlx::query_as::<_, EnginePreset>(
            "SELECT * FROM engine_presets WHERE name = 'Copied Ollama'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(copied.provider, "direct-ollama");
        assert_eq!(copied.command, None);
        assert_eq!(copied.default_intensity, "low");
        assert_eq!(copied.model.as_deref(), Some("llama3.1"));
        let copied_resolution = resolve_engine_for_research(&db, Some(copied.id), None, None)
            .await
            .unwrap();
        assert_eq!(copied_resolution.model_input, "llama3.1");

        let update_response = update_engine_preset(
            State(Arc::clone(&server)),
            Path(copied.id),
            Json(EnginePresetUpdatePayload {
                name: "Renamed Ollama".to_string(),
                model: Some("mistral".to_string()),
            }),
        )
        .await;
        assert_eq!(response_status(update_response), StatusCode::OK);
        let renamed =
            sqlx::query_as::<_, EnginePreset>("SELECT * FROM engine_presets WHERE id = ?")
                .bind(copied.id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(renamed.name, "Renamed Ollama");
        assert_eq!(renamed.model.as_deref(), Some("mistral"));

        let invalid_update_model_response = update_engine_preset(
            State(Arc::clone(&server)),
            Path(copied.id),
            Json(EnginePresetUpdatePayload {
                name: "Still Renamed Ollama".to_string(),
                model: Some("pi:llama3.1".to_string()),
            }),
        )
        .await;
        assert_eq!(
            response_status(invalid_update_model_response),
            StatusCode::BAD_REQUEST
        );

        let builtin_update_response = update_engine_preset(
            State(Arc::clone(&server)),
            Path(-5),
            Json(EnginePresetUpdatePayload {
                name: "Nope".to_string(),
                model: Some("mistral".to_string()),
            }),
        )
        .await;
        assert_eq!(
            response_status(builtin_update_response),
            StatusCode::METHOD_NOT_ALLOWED
        );

        sqlx::query("UPDATE engine_presets SET is_default = 'true' WHERE id = ?")
            .bind(claude_id)
            .execute(&db)
            .await
            .unwrap();
        let stale_default_update_response = update_engine_preset(
            State(Arc::clone(&server)),
            Path(claude_id),
            Json(EnginePresetUpdatePayload {
                name: "Still Nope".to_string(),
                model: Some("claude".to_string()),
            }),
        )
        .await;
        assert_eq!(
            response_status(stale_default_update_response),
            StatusCode::NOT_FOUND
        );

        let builtin_delete_response =
            delete_engine_preset(State(Arc::clone(&server)), Path(-5)).await;
        assert_eq!(
            response_status(builtin_delete_response),
            StatusCode::METHOD_NOT_ALLOWED
        );
        let delete_response =
            delete_engine_preset(State(Arc::clone(&server)), Path(copied.id)).await;
        assert_eq!(response_status(delete_response), StatusCode::OK);
        let copied_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_presets WHERE id = ?")
                .bind(copied.id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(copied_count, 0);

        let invalid_model_response = create_engine_preset(
            State(Arc::clone(&server)),
            Json(EnginePresetCreatePayload {
                name: "Invalid Model Override".to_string(),
                template_id: -5,
                model: Some("cli:claude".to_string()),
            }),
        )
        .await;
        assert_eq!(
            response_status(invalid_model_response),
            StatusCode::BAD_REQUEST
        );
        let invalid_pi_model_response = create_engine_preset(
            State(Arc::clone(&server)),
            Json(EnginePresetCreatePayload {
                name: "Invalid Pi Override".to_string(),
                template_id: -5,
                model: Some("pi:llama3".to_string()),
            }),
        )
        .await;
        assert_eq!(
            response_status(invalid_pi_model_response),
            StatusCode::BAD_REQUEST
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn engine_preset_route_contract_snapshot_is_frozen() {
        let dir = temp_test_dir("engine-presets-snapshot");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state_with_cli_launch_mode(
            db.clone(),
            dir.join("uploads"),
            liquid_runtime::cli_launcher::CliLaunchMode::Disabled,
        );
        let server = test_server_context(Arc::clone(&state));

        let initial_list = json_body(list_engine_presets(State(Arc::clone(&server))).await).await;
        let created = json_body(
            create_engine_preset(
                State(Arc::clone(&server)),
                Json(EnginePresetCreatePayload {
                    name: "Snapshot Ollama".to_string(),
                    template_id: -5,
                    model: Some("llama3.1".to_string()),
                }),
            )
            .await,
        )
        .await;
        let after_create = json_body(list_engine_presets(State(Arc::clone(&server))).await).await;
        let builtin_test =
            json_body(test_engine_preset(State(Arc::clone(&server)), Path(-2)).await).await;
        let entries = vec![
            ("list.initial", stable_preset_snapshot(&initial_list)),
            ("create.response", serde_json::to_string(&created).unwrap()),
            ("list.after_create", stable_preset_snapshot(&after_create)),
            (
                "builtin.test",
                serde_json::to_string(&builtin_test).unwrap(),
            ),
        ];
        let fingerprint = route_snapshot_fingerprint(&entries);
        assert_eq!(
            fingerprint, 0x5c34112df6c1f2fb,
            "engine preset route snapshot fingerprint changed: {fingerprint:#018x}"
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }
}
