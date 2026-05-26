use crate::cli_launcher::{launcher_unavailable_message, CliLaunchMode};
use crate::models::{EnginePreset, EngineResolution, TaskMetadata};
use crate::pi_runtime::ensure_pi_ollama_models_config;
use axum::http::StatusCode;
use chrono::Utc;
use sqlx::sqlite::SqlitePool;
use std::path::Path as StdPath;
use std::process::Command;

pub(crate) struct DefaultEnginePreset {
    id: i64,
    name: &'static str,
    engine_kind: &'static str,
    provider: &'static str,
    model: Option<&'static str>,
    command: Option<&'static str>,
    default_intensity: &'static str,
    web_search_enabled: &'static str,
    fallback_execution: &'static str,
    install_hint: &'static str,
}

pub(crate) fn default_engine_presets() -> Vec<DefaultEnginePreset> {
    vec![
        DefaultEnginePreset {
            id: -1,
            name: "Pi+Ollama 검증 조사",
            engine_kind: "pi_ollama",
            provider: "pi-ollama",
            model: Some("llama3"),
            command: Some("pi"),
            default_intensity: "high",
            web_search_enabled: "true",
            fallback_execution: "false",
            install_hint: "Pi runtime isolation is planned. Configure Liquid Pi runtime before using this preset.",
        },
        DefaultEnginePreset {
            id: -2,
            name: "Claude CLI 검증 조사",
            engine_kind: "cli",
            provider: "claude-cli",
            model: Some("claude"),
            command: Some("claude"),
            default_intensity: "high",
            web_search_enabled: "true",
            fallback_execution: "false",
            install_hint: "Install and authenticate the Claude CLI.",
        },
        DefaultEnginePreset {
            id: -3,
            name: "Gemini CLI 조사",
            engine_kind: "cli",
            provider: "gemini-cli",
            model: Some("gemini"),
            command: Some("gemini"),
            default_intensity: "medium",
            web_search_enabled: "true",
            fallback_execution: "false",
            install_hint: "Install and authenticate the Gemini CLI.",
        },
        DefaultEnginePreset {
            id: -4,
            name: "Codex CLI 검증 조사",
            engine_kind: "cli",
            provider: "codex-cli",
            model: Some("codex"),
            command: Some("codex"),
            default_intensity: "high",
            web_search_enabled: "true",
            fallback_execution: "false",
            install_hint: "Install and authenticate the Codex CLI.",
        },
        DefaultEnginePreset {
            id: -5,
            name: "direct Ollama fallback",
            engine_kind: "ollama_legacy",
            provider: "direct-ollama",
            model: Some("llama3"),
            command: None,
            default_intensity: "low",
            web_search_enabled: "false",
            fallback_execution: "true",
            install_hint: "Run Ollama locally and pull the configured model.",
        },
    ]
}

pub(crate) fn default_engine_preset_by_id(id: i64) -> Option<EnginePreset> {
    default_engine_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .map(default_engine_preset_to_engine_preset)
}

pub(crate) fn default_engine_preset_for_model(model: Option<&str>) -> EnginePreset {
    let preset_name = match model.unwrap_or_default() {
        "cli:claude" => "Claude CLI 검증 조사",
        "cli:gemini" => "Gemini CLI 조사",
        "cli:codex" => "Codex CLI 검증 조사",
        _ => "direct Ollama fallback",
    };
    default_engine_presets()
        .into_iter()
        .find(|preset| preset.name == preset_name)
        .map(default_engine_preset_to_engine_preset)
        .expect("default engine preset names are code-defined")
}

pub(crate) fn default_engine_preset_to_engine_preset(preset: DefaultEnginePreset) -> EnginePreset {
    let now = Utc::now();
    EnginePreset {
        id: preset.id,
        name: preset.name.to_string(),
        engine_kind: preset.engine_kind.to_string(),
        provider: preset.provider.to_string(),
        model: preset.model.map(str::to_string),
        command: preset.command.map(str::to_string),
        args_json: None,
        base_url: None,
        runtime_profile: None,
        default_intensity: preset.default_intensity.to_string(),
        web_search_enabled: preset.web_search_enabled.to_string(),
        fallback_execution: preset.fallback_execution.to_string(),
        enabled: "true".to_string(),
        is_default: "true".to_string(),
        install_hint: Some(preset.install_hint.to_string()),
        limits_json: None,
        allowed_tools_json: None,
        allowed_skills_json: None,
        last_test_status: "unverified".to_string(),
        last_test_at: None,
        last_test_message: None,
        created_at: now,
        updated_at: now,
    }
}

pub(crate) fn supported_cli_command(command: &str) -> bool {
    matches!(command, "gemini" | "claude" | "codex")
}

pub(crate) fn valid_research_intensity(intensity: &str) -> bool {
    matches!(intensity, "low" | "medium" | "high")
}

pub(crate) fn has_reserved_model_prefix(model: &str) -> bool {
    model.starts_with("cli:") || model.starts_with("pi:")
}

pub(crate) async fn test_engine_preset_status(
    preset: &EnginePreset,
    data_dir: &StdPath,
    cli_launch_mode: CliLaunchMode,
) -> (String, String) {
    match preset.engine_kind.as_str() {
        "cli" => {
            if !cli_launch_mode.can_launch_cli() {
                return (
                    "needs_setup".to_string(),
                    launcher_unavailable_message(cli_launch_mode),
                );
            }
            let command = preset
                .command
                .as_deref()
                .or(preset.model.as_deref())
                .unwrap_or("");
            if command.is_empty() {
                return (
                    "failed".to_string(),
                    "CLI 명령이 비어 있습니다.".to_string(),
                );
            }
            if !supported_cli_command(command) {
                return (
                    "failed".to_string(),
                    format!(
                        "{command} 명령은 지원하지 않습니다. 지원 명령: gemini, claude, codex."
                    ),
                );
            }
            if executable_exists(command) {
                (
                    "available".to_string(),
                    format!("{command} 실행 파일을 찾았습니다."),
                )
            } else {
                (
                    "needs_setup".to_string(),
                    format!("{command} 실행 파일을 찾지 못했습니다."),
                )
            }
        }
        "ollama_legacy" => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build();
            match client {
                Ok(client) => match client.get("http://localhost:11434/api/tags").send().await {
                    Ok(resp) if resp.status().is_success() => (
                        "available".to_string(),
                        "Ollama 모델 목록 엔드포인트가 응답했습니다.".to_string(),
                    ),
                    Ok(resp) => (
                        "needs_setup".to_string(),
                        format!("Ollama가 HTTP {}로 응답했습니다.", resp.status()),
                    ),
                    Err(e) => (
                        "needs_setup".to_string(),
                        format!("Ollama에 연결할 수 없습니다: {e}"),
                    ),
                },
                Err(e) => (
                    "failed".to_string(),
                    format!("테스트 클라이언트를 만들지 못했습니다: {e}"),
                ),
            }
        }
        "pi_ollama" => {
            if !cli_launch_mode.can_launch_cli() {
                return (
                    "needs_setup".to_string(),
                    launcher_unavailable_message(cli_launch_mode),
                );
            }
            let command = preset.command.as_deref().unwrap_or("pi");
            if !executable_exists(command) {
                return (
                    "needs_setup".to_string(),
                    format!("{command} 실행 파일을 찾지 못했습니다."),
                );
            }
            let model = preset.model.as_deref().unwrap_or("llama3");
            match ensure_pi_ollama_models_config(data_dir, model).await {
                Ok(models) if models.iter().any(|listed| listed == model) => (
                    "available".to_string(),
                    if preset.web_search_enabled == "true" {
                        format!("pi가 Ollama 모델 {model}과 Liquid 웹검색 도구 설정을 확인했습니다.")
                    } else {
                        format!("pi가 Ollama 모델 {model}을 확인했습니다.")
                    },
                ),
                Ok(_) => (
                    "needs_setup".to_string(),
                    format!("pi는 실행되지만 Ollama 모델 {model}이 목록에 없습니다."),
                ),
                Err(message) => ("needs_setup".to_string(), message),
            }
        }
        _ => (
            "failed".to_string(),
            "지원하지 않는 실행 방식입니다.".to_string(),
        ),
    }
}

pub(crate) fn executable_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(crate) async fn resolve_engine_for_research(
    _db: &SqlitePool,
    engine_preset_id: Option<i64>,
    model: Option<String>,
    requested_intensity: Option<String>,
) -> Result<EngineResolution, StatusCode> {
    let preset = if let Some(id) = engine_preset_id {
        if id < 0 {
            default_engine_preset_by_id(id).ok_or(StatusCode::NOT_FOUND)?
        } else {
            sqlx::query_as::<_, EnginePreset>(
                "SELECT * FROM engine_presets WHERE id = ? AND enabled = 'true' AND is_default != 'true'",
            )
            .bind(id)
            .fetch_one(_db)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            })?
        }
    } else {
        compatibility_preset_for_model(model.as_deref())
    };

    let intensity = requested_intensity
        .filter(|value| valid_research_intensity(value))
        .unwrap_or_else(|| preset.default_intensity.clone());
    if !valid_research_intensity(&intensity) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let legacy_model = model
        .as_deref()
        .filter(|value| !has_reserved_model_prefix(value))
        .map(str::to_string);
    let fallback_used = preset.fallback_execution == "true";
    let fallback_reason = if fallback_used {
        Some("Preset is marked as fallback execution.".to_string())
    } else {
        None
    };

    let model_input = match preset.engine_kind.as_str() {
        "cli" => {
            let command = preset
                .command
                .clone()
                .or_else(|| preset.model.clone())
                .ok_or(StatusCode::BAD_REQUEST)?;
            if !supported_cli_command(&command) {
                return Err(StatusCode::BAD_REQUEST);
            }
            format!("cli:{command}")
        }
        "ollama_legacy" => legacy_model
            .or_else(|| preset.model.clone())
            .unwrap_or_else(|| "llama3".to_string()),
        "pi_ollama" => format!(
            "pi:{}",
            legacy_model
                .or_else(|| preset.model.clone())
                .unwrap_or_else(|| "llama3".to_string())
        ),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let resolved_model = model_input
        .strip_prefix("cli:")
        .or_else(|| model_input.strip_prefix("pi:"))
        .unwrap_or(model_input.as_str())
        .to_string();
    let metadata = TaskMetadata {
        engine_preset_id: Some(preset.id),
        engine_preset_name: Some(preset.name.clone()),
        engine_kind: Some(preset.engine_kind.clone()),
        resolved_model: Some(resolved_model),
        research_intensity: Some(intensity),
        fallback_used: Some(fallback_used.to_string()),
        fallback_reason,
        web_search_requested: Some((preset.web_search_enabled == "true").to_string()),
        web_search_provider: None,
        ..TaskMetadata::default()
    };

    Ok(EngineResolution {
        model_input,
        metadata,
    })
}

pub(crate) fn compatibility_preset_for_model(model: Option<&str>) -> EnginePreset {
    default_engine_preset_for_model(model)
}

use crate::models::{EnginePresetCreatePayload, EnginePresetUpdatePayload};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

pub(crate) async fn list_engine_presets(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut presets: Vec<EnginePreset> = default_engine_presets()
        .into_iter()
        .map(default_engine_preset_to_engine_preset)
        .collect();
    match sqlx::query_as::<_, EnginePreset>(
        "SELECT * FROM engine_presets WHERE is_default != 'true' ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(user_presets) => {
            presets.extend(user_presets);
            Json(presets).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn create_engine_preset(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EnginePresetCreatePayload>,
) -> impl IntoResponse {
    let name = payload.name.trim();
    let Some(template) = default_engine_preset_by_id(payload.template_id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if name.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let model = if template.engine_kind == "cli" {
        template.model.clone()
    } else {
        let model_override = payload
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if model_override
            .as_deref()
            .is_some_and(has_reserved_model_prefix)
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
        model_override.or_else(|| template.model.clone())
    };

    match sqlx::query("INSERT INTO engine_presets (name, engine_kind, provider, model, command, args_json, base_url, runtime_profile, default_intensity, web_search_enabled, fallback_execution, enabled, is_default, install_hint, limits_json, allowed_tools_json, allowed_skills_json, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'true', 'false', ?, ?, ?, ?, CURRENT_TIMESTAMP)")
        .bind(name)
        .bind(&template.engine_kind)
        .bind(&template.provider)
        .bind(model)
        .bind(&template.command)
        .bind(&template.args_json)
        .bind(&template.base_url)
        .bind(&template.runtime_profile)
        .bind(&template.default_intensity)
        .bind(&template.web_search_enabled)
        .bind(&template.fallback_execution)
        .bind(format!("Created from built-in preset: {}", template.name))
        .bind(&template.limits_json)
        .bind(&template.allowed_tools_json)
        .bind(&template.allowed_skills_json)
        .execute(&state.db)
        .await
    {
        Ok(result) => (StatusCode::CREATED, Json(serde_json::json!({ "id": result.last_insert_rowid() }))).into_response(),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => StatusCode::CONFLICT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn update_engine_preset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<EnginePresetUpdatePayload>,
) -> impl IntoResponse {
    if id < 0 {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let name = payload.name.trim();
    if name.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let preset = match sqlx::query_as::<_, EnginePreset>(
        "SELECT * FROM engine_presets WHERE id = ? AND is_default != 'true'",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    {
        Ok(preset) => preset,
        Err(sqlx::Error::RowNotFound) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let model = if preset.engine_kind == "cli" {
        preset.model
    } else {
        let model_override = payload
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if model_override
            .as_deref()
            .is_some_and(has_reserved_model_prefix)
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
        model_override.or(preset.model)
    };
    match sqlx::query("UPDATE engine_presets SET name = ?, model = ?, last_test_status = 'unverified', last_test_at = NULL, last_test_message = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND is_default != 'true'")
        .bind(name)
        .bind(model)
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => StatusCode::CONFLICT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn delete_engine_preset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if id < 0 {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    match sqlx::query("DELETE FROM engine_presets WHERE id = ? AND is_default != 'true'")
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn test_engine_preset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let preset = if id < 0 {
        let Some(preset) = default_engine_preset_by_id(id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        preset
    } else {
        match sqlx::query_as::<_, EnginePreset>(
            "SELECT * FROM engine_presets WHERE id = ? AND enabled = 'true' AND is_default != 'true'",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await
        {
            Ok(preset) => preset,
            Err(sqlx::Error::RowNotFound) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    };
    let (status, message) =
        test_engine_preset_status(&preset, &state.data_dir, state.cli_launch_mode).await;
    if id > 0 {
        match sqlx::query("UPDATE engine_presets SET last_test_status = ?, last_test_at = CURRENT_TIMESTAMP, last_test_message = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&status)
            .bind(&message)
            .bind(id)
            .execute(&state.db)
            .await
        {
            Ok(result) if result.rows_affected() > 0 => {}
            Ok(_) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
    Json(serde_json::json!({ "status": status, "message": message })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_launcher::CliLaunchMode;
    use crate::db::setup_db;
    use crate::models::{EnginePreset, EnginePresetCreatePayload, EnginePresetUpdatePayload};
    use crate::test_support::{response_status, temp_test_dir, test_state};
    use axum::{
        extract::{Path, State},
        http::StatusCode,
        Json,
    };
    use std::fs as std_fs;
    use std::sync::Arc;

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

        let presets_response = list_engine_presets(State(Arc::clone(&state))).await;
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
            test_engine_preset(State(Arc::clone(&state)), Path(claude_id)).await;
        assert_eq!(response_status(custom_test_response), StatusCode::OK);

        let test_response = test_engine_preset(State(Arc::clone(&state)), Path(-2)).await;
        assert_eq!(response_status(test_response), StatusCode::OK);

        let created_response = create_engine_preset(
            State(Arc::clone(&state)),
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
            State(Arc::clone(&state)),
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
            State(Arc::clone(&state)),
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
            State(Arc::clone(&state)),
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
            State(Arc::clone(&state)),
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
            delete_engine_preset(State(Arc::clone(&state)), Path(-5)).await;
        assert_eq!(
            response_status(builtin_delete_response),
            StatusCode::METHOD_NOT_ALLOWED
        );
        let delete_response =
            delete_engine_preset(State(Arc::clone(&state)), Path(copied.id)).await;
        assert_eq!(response_status(delete_response), StatusCode::OK);
        let copied_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_presets WHERE id = ?")
                .bind(copied.id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(copied_count, 0);

        let invalid_model_response = create_engine_preset(
            State(Arc::clone(&state)),
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
            State(Arc::clone(&state)),
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
}
