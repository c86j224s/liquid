use crate::contracts::EnginePreset;
use liquid_runtime::cli_launcher::{launcher_unavailable_message, CliLaunchMode};
use liquid_runtime::engine_presets::supported_cli_command;
use liquid_runtime::pi_runtime::ensure_pi_ollama_models_config;
use std::path::Path as StdPath;
use std::process::Command;

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
                Ok(models) if models.iter().any(|listed| listed == model) => {
                    (
                        "available".to_string(),
                        if preset.web_search_enabled == "true" {
                            format!("pi가 Ollama 모델 {model}과 Liquid 웹검색 도구 설정을 확인했습니다.")
                        } else {
                            format!("pi가 Ollama 모델 {model}을 확인했습니다.")
                        },
                    )
                }
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
