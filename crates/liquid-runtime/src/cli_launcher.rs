use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliLaunchMode {
    Auto,
    SandboxExec,
    Unsandboxed,
    Disabled,
}

impl CliLaunchMode {
    pub fn resolved(self) -> ResolvedCliLauncher {
        match self {
            CliLaunchMode::Auto if cfg!(target_os = "macos") => ResolvedCliLauncher::SandboxExec,
            CliLaunchMode::Auto => ResolvedCliLauncher::Disabled,
            CliLaunchMode::SandboxExec if cfg!(target_os = "macos") => {
                ResolvedCliLauncher::SandboxExec
            }
            CliLaunchMode::SandboxExec => ResolvedCliLauncher::Disabled,
            CliLaunchMode::Unsandboxed => ResolvedCliLauncher::Unsandboxed,
            CliLaunchMode::Disabled => ResolvedCliLauncher::Disabled,
        }
    }

    pub fn can_launch_cli(self) -> bool {
        self.resolved().can_launch()
    }
}

impl Default for CliLaunchMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl fmt::Display for CliLaunchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::SandboxExec => "sandbox-exec",
            Self::Unsandboxed => "unsandboxed",
            Self::Disabled => "disabled",
        })
    }
}

impl FromStr for CliLaunchMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "sandbox-exec" => Ok(Self::SandboxExec),
            "unsandboxed" => Ok(Self::Unsandboxed),
            "disabled" => Ok(Self::Disabled),
            _ => Err(format!(
                "invalid CLI launch mode '{value}'; expected auto, sandbox-exec, unsandboxed, or disabled"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedCliLauncher {
    SandboxExec,
    Unsandboxed,
    Disabled,
}

impl ResolvedCliLauncher {
    pub fn can_launch(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

pub fn launcher_unavailable_message(mode: CliLaunchMode) -> String {
    format!(
        "CLI launch mode '{}' is disabled on this platform; set LIQUID_CLI_LAUNCH_MODE=unsandboxed to run CLI tools without sandboxing on WSL/Linux.",
        mode
    )
}

#[derive(Clone, Debug)]
pub struct CliInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub envs: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub sandbox_profile: String,
}

impl CliInvocation {
    pub fn new(program: impl Into<String>, current_dir: impl AsRef<Path>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: current_dir.as_ref().to_path_buf(),
            envs: Vec::new(),
            env_remove: Vec::new(),
            sandbox_profile: String::new(),
        }
    }

    pub fn arg(&mut self, arg: impl Into<String>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    pub fn env(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    pub fn env_remove(&mut self, key: impl Into<String>) -> &mut Self {
        self.env_remove.push(key.into());
        self
    }
}

pub fn build_command(
    mode: CliLaunchMode,
    invocation: &CliInvocation,
) -> Result<tokio::process::Command, String> {
    let mut cmd = match mode.resolved() {
        ResolvedCliLauncher::SandboxExec => {
            let mut cmd = tokio::process::Command::new("sandbox-exec");
            cmd.arg("-p")
                .arg(&invocation.sandbox_profile)
                .arg(&invocation.program);
            cmd
        }
        ResolvedCliLauncher::Unsandboxed => tokio::process::Command::new(&invocation.program),
        ResolvedCliLauncher::Disabled => return Err(launcher_unavailable_message(mode)),
    };
    cmd.args(&invocation.args);
    cmd.current_dir(&invocation.current_dir);
    for (key, value) in &invocation.envs {
        cmd.env(key, value);
    }
    for key in &invocation.env_remove {
        cmd.env_remove(key);
    }
    Ok(cmd)
}

pub fn generic_sandbox_profile(home: &str, data_dir: &str) -> String {
    format!(
        r#"(version 1)
           (allow default)
           (deny file-write*
             (require-all
               (subpath "{0}")
               (require-not (literal "{0}/.codex"))
               (require-not (subpath "{0}/.codex"))
               (require-not (literal "{0}/.gemini"))
               (require-not (subpath "{0}/.gemini"))
               (require-not (literal "{0}/Library/Caches"))
               (require-not (subpath "{0}/Library/Caches"))
               (require-not (literal "{0}/.cache"))
               (require-not (subpath "{0}/.cache"))
               (require-not (literal "{1}"))
               (require-not (subpath "{1}"))))
           (allow file-read* (subpath "{1}"))
           (allow file-write* (subpath "{1}"))
           (allow network-outbound)"#,
        home, data_dir
    )
}

pub fn pi_sandbox_profile(pi_home: &str, agent_dir: &str, session_dir: &str) -> String {
    format!(
        r#"(version 1)
                   (deny default)
                   (allow file-read-data (literal "/"))
                   (allow process-exec)
                   (allow process-fork)
                   (allow sysctl-read)
                   (allow mach-lookup)
                   (allow network-outbound)
                   (allow file-read-metadata)
                   (allow file-read* (literal "/dev/null"))
                   (allow file-read* (literal "/bin"))
                   (allow file-read* (subpath "/bin"))
                   (allow file-read* (literal "/usr"))
                   (allow file-read* (subpath "/usr"))
                   (allow file-read* (literal "/opt/homebrew"))
                   (allow file-read* (subpath "/opt/homebrew"))
                   (allow file-read* (literal "{0}"))
                   (allow file-read* (subpath "{0}"))
                   (allow file-read* (literal "{1}"))
                   (allow file-read* (subpath "{1}"))
                   (allow file-read* (literal "{2}"))
                   (allow file-read* (subpath "{2}"))
                   (allow file-write* (literal "{0}"))
                   (allow file-write* (subpath "{0}"))
                   (allow file-write* (literal "{1}"))
                   (allow file-write* (subpath "{1}"))
                   (allow file-write* (literal "{2}"))
                   (allow file-write* (subpath "{2}"))"#,
        pi_home, agent_dir, session_dir
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn program_name(cmd: &tokio::process::Command) -> &OsStr {
        cmd.as_std().get_program()
    }

    #[test]
    fn test_launch_mode_parse_and_display() {
        assert_eq!(
            "auto".parse::<CliLaunchMode>().unwrap(),
            CliLaunchMode::Auto
        );
        assert_eq!(
            "sandbox-exec".parse::<CliLaunchMode>().unwrap(),
            CliLaunchMode::SandboxExec
        );
        assert_eq!(
            "unsandboxed".parse::<CliLaunchMode>().unwrap(),
            CliLaunchMode::Unsandboxed
        );
        assert_eq!(
            "disabled".parse::<CliLaunchMode>().unwrap(),
            CliLaunchMode::Disabled
        );
        assert!("other".parse::<CliLaunchMode>().is_err());
        assert_eq!(CliLaunchMode::Unsandboxed.to_string(), "unsandboxed");
    }

    #[test]
    fn test_auto_resolves_by_platform() {
        let expected = if cfg!(target_os = "macos") {
            ResolvedCliLauncher::SandboxExec
        } else {
            ResolvedCliLauncher::Disabled
        };
        assert_eq!(CliLaunchMode::Auto.resolved(), expected);
    }

    #[test]
    fn test_unsandboxed_command_uses_tool_program() {
        let mut invocation = CliInvocation::new("gemini", "/tmp");
        invocation.arg("-p").arg("prompt").env("PAGER", "cat");
        let cmd = build_command(CliLaunchMode::Unsandboxed, &invocation).unwrap();

        assert_eq!(program_name(&cmd), OsStr::new("gemini"));
        let args = cmd.as_std().get_args().collect::<Vec<_>>();
        assert_eq!(args, vec![OsStr::new("-p"), OsStr::new("prompt")]);
    }

    #[test]
    fn test_sandbox_command_wraps_program_when_available() {
        let mut invocation = CliInvocation::new("codex", "/tmp");
        invocation.sandbox_profile = "(version 1)".to_string();
        invocation.arg("exec");
        let result = build_command(CliLaunchMode::SandboxExec, &invocation);
        if cfg!(target_os = "macos") {
            let cmd = result.unwrap();
            assert_eq!(program_name(&cmd), OsStr::new("sandbox-exec"));
            let args = cmd.as_std().get_args().collect::<Vec<_>>();
            assert_eq!(args[0], OsStr::new("-p"));
            assert_eq!(args[2], OsStr::new("codex"));
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_disabled_mode_rejects_invocation() {
        let invocation = CliInvocation::new("claude", "/tmp");

        assert!(build_command(CliLaunchMode::Disabled, &invocation).is_err());
    }

    #[test]
    fn test_pi_invocation_preserves_env_policy() {
        let mut invocation = CliInvocation::new("pi", "/tmp/pi-home");
        invocation
            .env("HOME", "/tmp/pi-home")
            .env("PI_CODING_AGENT_DIR", "/tmp/agent")
            .env_remove("PI_CODING_AGENT")
            .env_remove("PNPM_HOME")
            .env_remove("NODE_PATH");

        let cmd = build_command(CliLaunchMode::Unsandboxed, &invocation).unwrap();
        assert_eq!(program_name(&cmd), OsStr::new("pi"));
        assert_eq!(
            cmd.as_std()
                .get_envs()
                .filter(|(key, _)| *key == OsStr::new("HOME"))
                .count(),
            1
        );
    }
}
