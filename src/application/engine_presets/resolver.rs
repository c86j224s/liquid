use super::catalog::{default_engine_preset_by_id, default_engine_preset_for_model};
use super::repository::SqliteEnginePresetRepository;
use crate::contracts::{EnginePreset, EngineResolution, TaskMetadata};
use liquid_runtime::engine_presets::{
    has_reserved_model_prefix, supported_cli_command, valid_research_intensity,
};
use sqlx::sqlite::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineResolutionError {
    NotFound,
    InvalidRequest,
    Storage,
}

pub(crate) async fn resolve_engine_for_research(
    db: &SqlitePool,
    engine_preset_id: Option<i64>,
    model: Option<String>,
    requested_intensity: Option<String>,
) -> Result<EngineResolution, EngineResolutionError> {
    let preset = if let Some(id) = engine_preset_id {
        if id < 0 {
            default_engine_preset_by_id(id).ok_or(EngineResolutionError::NotFound)?
        } else {
            let repo = SqliteEnginePresetRepository::new(db);
            repo.load_enabled_custom_preset_by_id(id)
                .await
                .map_err(|_| EngineResolutionError::Storage)?
                .ok_or(EngineResolutionError::NotFound)?
        }
    } else {
        compatibility_preset_for_model(model.as_deref())
    };

    let intensity = requested_intensity
        .filter(|value| valid_research_intensity(value))
        .unwrap_or_else(|| preset.default_intensity.clone());
    if !valid_research_intensity(&intensity) {
        return Err(EngineResolutionError::InvalidRequest);
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
                .ok_or(EngineResolutionError::InvalidRequest)?;
            if !supported_cli_command(&command) {
                return Err(EngineResolutionError::InvalidRequest);
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
        _ => return Err(EngineResolutionError::InvalidRequest),
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
