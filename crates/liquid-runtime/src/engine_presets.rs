#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultEnginePreset {
    pub id: i64,
    pub name: &'static str,
    pub engine_kind: &'static str,
    pub provider: &'static str,
    pub model: Option<&'static str>,
    pub command: Option<&'static str>,
    pub default_intensity: &'static str,
    pub web_search_enabled: &'static str,
    pub fallback_execution: &'static str,
    pub install_hint: &'static str,
}

const DEFAULT_ENGINE_PRESETS: [DefaultEnginePreset; 5] = [
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
        install_hint:
            "Pi runtime isolation is planned. Configure Liquid Pi runtime before using this preset.",
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
];

pub fn default_engine_presets() -> Vec<DefaultEnginePreset> {
    DEFAULT_ENGINE_PRESETS.to_vec()
}

pub fn default_engine_preset_by_id(id: i64) -> Option<DefaultEnginePreset> {
    DEFAULT_ENGINE_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id)
}

pub fn default_engine_preset_for_model(model: Option<&str>) -> DefaultEnginePreset {
    let preset_name = match model.unwrap_or_default() {
        "cli:claude" => "Claude CLI 검증 조사",
        "cli:gemini" => "Gemini CLI 조사",
        "cli:codex" => "Codex CLI 검증 조사",
        _ => "direct Ollama fallback",
    };
    DEFAULT_ENGINE_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.name == preset_name)
        .expect("default engine preset names are code-defined")
}

pub fn supported_cli_command(command: &str) -> bool {
    matches!(command, "gemini" | "claude" | "codex")
}

pub fn valid_research_intensity(intensity: &str) -> bool {
    liquid_protocol::is_valid_research_intensity(intensity)
}

pub fn has_reserved_model_prefix(model: &str) -> bool {
    model.starts_with("cli:") || model.starts_with("pi:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_engine_preset_for_model_uses_expected_catalog_entries() {
        assert_eq!(default_engine_preset_for_model(Some("cli:claude")).id, -2);
        assert_eq!(default_engine_preset_for_model(Some("cli:gemini")).id, -3);
        assert_eq!(default_engine_preset_for_model(Some("cli:codex")).id, -4);
        assert_eq!(default_engine_preset_for_model(Some("llama3.1")).id, -5);
    }

    #[test]
    fn policy_helpers_match_supported_commands_and_prefixes() {
        assert!(supported_cli_command("claude"));
        assert!(supported_cli_command("gemini"));
        assert!(supported_cli_command("codex"));
        assert!(!supported_cli_command("pi"));

        assert!(valid_research_intensity("low"));
        assert!(valid_research_intensity("medium"));
        assert!(valid_research_intensity("high"));
        assert!(!valid_research_intensity("strict"));

        assert!(has_reserved_model_prefix("cli:claude"));
        assert!(has_reserved_model_prefix("pi:llama3"));
        assert!(!has_reserved_model_prefix("llama3"));
    }
}
