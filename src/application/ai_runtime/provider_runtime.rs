use crate::contracts::{OllamaGenerateRequest, OllamaGenerateResponse};
use crate::state::AppState;
use liquid_runtime::cli_launcher::{
    build_command, generic_sandbox_profile, launcher_unavailable_message, pi_sandbox_profile,
    CliInvocation, CliLaunchMode,
};
use liquid_runtime::pi_runtime::{
    clear_pi_session_jsonl, contains_http_url, ensure_pi_ollama_models_config,
    ensure_pi_web_search_extension, format_cli_failure, pi_agent_dir, pi_home_dir, pi_session_dir,
    pi_tool_args, pi_web_tool_failure,
};
use std::path::Path;
use tokio::fs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProviderFailureKind {
    UnknownEngine,
    LauncherUnavailable,
    PiModelConfig,
    PiModelMissing,
    PiSessionDirectory,
    PiWebSearchExtension,
    BuildCommand,
    Spawn,
    CliWait,
    CliExit,
    Timeout,
    PiWebTool,
    PiMissingVerifiableUrl,
    OllamaHttp,
    OllamaMalformedResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProviderFailure {
    pub(super) kind: ProviderFailureKind,
    pub(super) message: String,
}

impl ProviderFailure {
    fn new(kind: ProviderFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(super) fn unknown_engine() -> Self {
        Self::new(ProviderFailureKind::UnknownEngine, "Unknown engine")
    }

    pub(super) fn launcher_unavailable(message: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::LauncherUnavailable, message)
    }

    pub(super) fn pi_model_config(message: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::PiModelConfig, message)
    }

    pub(super) fn pi_model_missing(model: &str) -> Self {
        Self::new(
            ProviderFailureKind::PiModelMissing,
            format!("pi는 실행되지만 Ollama 모델 {model}이 목록에 없습니다."),
        )
    }

    pub(super) fn pi_session_directory(error: impl std::fmt::Display) -> Self {
        Self::new(
            ProviderFailureKind::PiSessionDirectory,
            format!("Failed to create Pi session directory: {error}"),
        )
    }

    pub(super) fn pi_web_search_extension(message: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::PiWebSearchExtension, message)
    }

    pub(super) fn build_command(message: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::BuildCommand, message)
    }

    pub(super) fn spawn(error: impl std::fmt::Display) -> Self {
        Self::new(ProviderFailureKind::Spawn, error.to_string())
    }

    pub(super) fn cli_wait(error: impl std::fmt::Display) -> Self {
        Self::new(ProviderFailureKind::CliWait, error.to_string())
    }

    pub(super) fn cli_exit(output: &std::process::Output) -> Self {
        Self::new(ProviderFailureKind::CliExit, format_cli_failure(output))
    }

    pub(super) fn timeout(task_timeout_secs: u64) -> Self {
        Self::new(
            ProviderFailureKind::Timeout,
            format_task_timeout_message(task_timeout_secs),
        )
    }

    pub(super) fn pi_web_tool(message: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::PiWebTool, message)
    }

    pub(super) fn pi_missing_verifiable_url() -> Self {
        Self::new(
            ProviderFailureKind::PiMissingVerifiableUrl,
            "Pi+Ollama 웹검색이 요청되었지만 결과에 검증 가능한 http/https URL이 없습니다.",
        )
    }

    pub(super) fn ollama_http(error: impl std::fmt::Display) -> Self {
        Self::new(ProviderFailureKind::OllamaHttp, error.to_string())
    }

    pub(super) fn ollama_malformed_response(text: impl Into<String>) -> Self {
        Self::new(ProviderFailureKind::OllamaMalformedResponse, text)
    }
}

async fn mark_task_failed(state: &AppState, task_id: i64, failure: ProviderFailure) {
    let _ = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ?")
        .bind(failure.message)
        .bind(task_id)
        .execute(&state.db)
        .await;
}

pub(super) async fn execute_ollama_task(
    state: &AppState,
    task_id: i64,
    model_name: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1200))
        .build()
        .ok()?;
    let ollama_req = build_ollama_generate_request(model_name, system_prompt, user_prompt);
    match client
        .post("http://localhost:11434/api/generate")
        .json(&ollama_req)
        .send()
        .await
    {
        Ok(resp) => {
            let text = resp.text().await.unwrap_or_default();
            match parse_ollama_generate_response(text) {
                Ok(response) => Some(response),
                Err(failure) => {
                    mark_task_failed(state, task_id, failure).await;
                    None
                }
            }
        }
        Err(e) => {
            mark_task_failed(state, task_id, ProviderFailure::ollama_http(e)).await;
            None
        }
    }
}

pub(super) async fn execute_cli_task(
    state: &AppState,
    task_id: i64,
    model_name: &str,
    source: &str,
    system_prompt: &str,
    user_prompt: &str,
    allow_web_search: bool,
) -> Option<String> {
    let data_dir = state.data_dir.clone();

    let invocation = match (source, model_name) {
        ("cli", "gemini") => build_gemini_invocation(&data_dir, system_prompt, user_prompt),
        ("cli", "claude") => {
            build_claude_invocation(&data_dir, system_prompt, user_prompt, allow_web_search)
        }
        ("cli", "codex") => {
            build_codex_invocation(&data_dir, system_prompt, user_prompt, allow_web_search)
        }
        ("pi", model) => {
            if let Err(message) = ensure_pi_launcher_allowed(state.cli_launch_mode) {
                mark_task_failed(
                    state,
                    task_id,
                    ProviderFailure::launcher_unavailable(message),
                )
                .await;
                return None;
            }
            let session_dir = pi_session_dir(&data_dir, task_id);
            let available_models = match ensure_pi_ollama_models_config(&data_dir, model).await {
                Ok(models) => models,
                Err(message) => {
                    mark_task_failed(state, task_id, ProviderFailure::pi_model_config(message))
                        .await;
                    return None;
                }
            };
            if !available_models.iter().any(|available| available == model) {
                mark_task_failed(state, task_id, ProviderFailure::pi_model_missing(model)).await;
                return None;
            }
            if let Err(e) = fs::create_dir_all(&session_dir).await {
                mark_task_failed(state, task_id, ProviderFailure::pi_session_directory(e)).await;
                return None;
            }
            clear_pi_session_jsonl(&session_dir).await;
            let web_search_extension = if allow_web_search {
                match ensure_pi_web_search_extension(&data_dir).await {
                    Ok(path) => Some(path),
                    Err(message) => {
                        mark_task_failed(
                            state,
                            task_id,
                            ProviderFailure::pi_web_search_extension(message),
                        )
                        .await;
                        return None;
                    }
                }
            } else {
                None
            };
            build_pi_invocation(
                &data_dir,
                task_id,
                model,
                system_prompt,
                user_prompt,
                allow_web_search,
                web_search_extension.as_deref(),
            )
        }
        _ => {
            mark_task_failed(state, task_id, ProviderFailure::unknown_engine()).await;
            return None;
        }
    };

    let mut cmd = match build_command(state.cli_launch_mode, &invocation) {
        Ok(cmd) => cmd,
        Err(message) => {
            mark_task_failed(state, task_id, ProviderFailure::build_command(message)).await;
            return None;
        }
    };

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            mark_task_failed(state, task_id, ProviderFailure::spawn(e)).await;
            return None;
        }
    };

    let task_timeout_secs = state.ai_task_timeout_secs;
    match tokio::time::timeout(
        std::time::Duration::from_secs(task_timeout_secs),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = provider_stdout_text(&output);
            if source == "pi" && allow_web_search {
                let session_dir = pi_session_dir(&data_dir, task_id);
                if let Some(message) = pi_web_tool_failure(&session_dir).await {
                    mark_task_failed(state, task_id, ProviderFailure::pi_web_tool(message)).await;
                    return None;
                }
                if !contains_http_url(&stdout) {
                    mark_task_failed(state, task_id, ProviderFailure::pi_missing_verifiable_url())
                        .await;
                    return None;
                }
            }
            Some(stdout)
        }
        Ok(Ok(output)) => {
            mark_task_failed(state, task_id, ProviderFailure::cli_exit(&output)).await;
            None
        }
        Ok(Err(e)) => {
            mark_task_failed(state, task_id, ProviderFailure::cli_wait(e)).await;
            None
        }
        Err(_) => {
            mark_task_failed(state, task_id, ProviderFailure::timeout(task_timeout_secs)).await;
            None
        }
    }
}

pub(super) fn build_ollama_generate_request(
    model_name: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> OllamaGenerateRequest {
    OllamaGenerateRequest {
        model: model_name.to_string(),
        prompt: user_prompt.to_string(),
        stream: false,
        system: Some(system_prompt.to_string()),
    }
}

pub(super) fn parse_ollama_generate_response(text: String) -> Result<String, ProviderFailure> {
    serde_json::from_str::<OllamaGenerateResponse>(&text)
        .map(|json_res| json_res.response)
        .map_err(|_| ProviderFailure::ollama_malformed_response(text))
}

pub(super) fn provider_stdout_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub(super) fn build_provider_full_prompt(system_prompt: &str, user_prompt: &str) -> String {
    format!(
        "<<< SYSTEM INSTRUCTION >>>\n{}\n\n<<< USER REQUEST >>>\n{}",
        system_prompt, user_prompt
    )
}

pub(super) fn build_gemini_invocation(
    data_dir: &Path,
    system_prompt: &str,
    user_prompt: &str,
) -> CliInvocation {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_dir_str = data_dir.to_string_lossy().to_string();
    let full_prompt = build_provider_full_prompt(system_prompt, user_prompt);
    let mut invocation = CliInvocation::new("gemini", data_dir);
    invocation.sandbox_profile = generic_sandbox_profile(&home, &data_dir_str);
    invocation
        .arg("--approval-mode")
        .arg("plan")
        .arg("-p")
        .arg(&full_prompt)
        .env("PAGER", "cat");
    invocation
}

pub(super) fn build_claude_invocation(
    data_dir: &Path,
    system_prompt: &str,
    user_prompt: &str,
    allow_web_search: bool,
) -> CliInvocation {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_dir_str = data_dir.to_string_lossy().to_string();
    let full_prompt = build_provider_full_prompt(system_prompt, user_prompt);
    let claude_prompt = format!(
        "{}\n\n[OUTPUT RULES]\n- Output only the requested final translated text or report.\n- Do not mention tools, plans, attempts, file creation, or internal steps.",
        full_prompt
    );
    let mut invocation = CliInvocation::new("claude", data_dir);
    invocation.sandbox_profile = generic_sandbox_profile(&home, &data_dir_str);
    invocation.arg("-p").arg("--permission-mode").arg("dontAsk");
    if allow_web_search {
        invocation
            .arg("--tools")
            .arg("WebFetch,WebSearch")
            .arg("--allowed-tools")
            .arg("WebFetch,WebSearch");
    } else {
        invocation.arg("--tools").arg("");
    }
    invocation
        .arg("--disallowed-tools")
        .arg("Bash,Edit,Write")
        .arg("--no-session-persistence")
        .arg(&claude_prompt);
    invocation
}

pub(super) fn build_codex_invocation(
    data_dir: &Path,
    system_prompt: &str,
    user_prompt: &str,
    allow_web_search: bool,
) -> CliInvocation {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_dir_str = data_dir.to_string_lossy().to_string();
    let full_prompt = build_provider_full_prompt(system_prompt, user_prompt);
    let mut invocation = CliInvocation::new("codex", data_dir);
    invocation.sandbox_profile = generic_sandbox_profile(&home, &data_dir_str);
    invocation
        .arg("--sandbox")
        .arg("read-only")
        .arg("--ask-for-approval")
        .arg("never");
    if allow_web_search {
        invocation.arg("--search");
    }
    invocation
        .arg("exec")
        .arg("--ephemeral")
        .arg("--skip-git-repo-check")
        .arg(&full_prompt);
    invocation
}

pub(super) fn build_pi_invocation(
    data_dir: &Path,
    task_id: i64,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    allow_web_search: bool,
    web_search_extension: Option<&Path>,
) -> CliInvocation {
    let agent_dir = pi_agent_dir(data_dir);
    let pi_home = pi_home_dir(data_dir);
    let session_dir = pi_session_dir(data_dir, task_id);
    let pi_home_str = pi_home.to_string_lossy().to_string();
    let agent_dir_str = agent_dir.to_string_lossy().to_string();
    let session_dir_str = session_dir.to_string_lossy().to_string();
    let full_prompt = build_provider_full_prompt(system_prompt, user_prompt);
    let mut invocation = CliInvocation::new("pi", &pi_home);
    invocation.sandbox_profile = pi_sandbox_profile(&pi_home_str, &agent_dir_str, &session_dir_str);
    invocation
        .arg("--provider")
        .arg("ollama")
        .arg("--model")
        .arg(model)
        .arg("--session-dir")
        .arg(session_dir_str)
        .arg("--no-context-files")
        .arg("--no-extensions");
    for arg in pi_tool_args(allow_web_search, web_search_extension) {
        invocation.arg(arg);
    }
    invocation
        .arg("--mode")
        .arg("text")
        .arg("-p")
        .arg(&full_prompt)
        .env("HOME", pi_home_str)
        .env("PI_CODING_AGENT_DIR", agent_dir_str)
        .env_remove("PI_CODING_AGENT")
        .env_remove("PNPM_HOME")
        .env_remove("NODE_PATH");
    invocation
}

pub(super) fn format_task_timeout_message(task_timeout_secs: u64) -> String {
    if task_timeout_secs >= 60 && task_timeout_secs % 60 == 0 {
        let minutes = task_timeout_secs / 60;
        let unit = if minutes == 1 { "minute" } else { "minutes" };
        format!("Task timed out after {minutes} {unit} and was killed")
    } else {
        let unit = if task_timeout_secs == 1 {
            "second"
        } else {
            "seconds"
        };
        format!("Task timed out after {task_timeout_secs} {unit} and was killed")
    }
}

pub(super) fn ensure_pi_launcher_allowed(mode: CliLaunchMode) -> Result<(), String> {
    if mode.can_launch_cli() {
        Ok(())
    } else {
        Err(launcher_unavailable_message(mode))
    }
}
