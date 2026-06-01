use crate::contracts::EnginePreset;
use chrono::Utc;
use liquid_runtime::{
    default_engine_preset_by_id as runtime_default_engine_preset_by_id,
    default_engine_preset_for_model as runtime_default_engine_preset_for_model,
    default_engine_presets as runtime_default_engine_presets, DefaultEnginePreset,
};

pub(crate) fn default_engine_presets() -> Vec<DefaultEnginePreset> {
    runtime_default_engine_presets()
}

pub(crate) fn default_engine_preset_by_id(id: i64) -> Option<EnginePreset> {
    runtime_default_engine_preset_by_id(id).map(default_engine_preset_to_engine_preset)
}

pub(crate) fn default_engine_preset_for_model(model: Option<&str>) -> EnginePreset {
    default_engine_preset_to_engine_preset(runtime_default_engine_preset_for_model(model))
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
