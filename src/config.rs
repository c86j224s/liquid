use clap::Parser;
use liquid_runtime::cli_launcher::CliLaunchMode;
use std::path::{Path as StdPath, PathBuf};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ResearchImplementationSelection {
    #[default]
    Classic,
}

impl ResearchImplementationSelection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
        }
    }
}

impl std::fmt::Display for ResearchImplementationSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ResearchImplementationSelection {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "classic" => Ok(Self::Classic),
            other => Err(format!(
                "unsupported research implementation '{other}'; expected 'classic'"
            )),
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    #[arg(short, long, env = "LIQUID_DATA_DIR", default_value = "./data")]
    pub(crate) data_dir: PathBuf,
    #[arg(long, env = "LIQUID_HOST", default_value = "127.0.0.1")]
    pub(crate) host: String,
    #[arg(short, long, env = "LIQUID_PORT", default_value_t = 3000)]
    pub(crate) port: u16,
    #[arg(long, env = "LIQUID_AI_WORKERS", default_value_t = 1)]
    pub(crate) ai_workers: usize,
    #[arg(long, env = "LIQUID_LOCAL_AI_WORKERS", default_value_t = 1)]
    pub(crate) local_ai_workers: usize,
    #[arg(long, env = "LIQUID_AI_TASK_TIMEOUT_SECS", default_value_t = 3600)]
    pub(crate) ai_task_timeout_secs: u64,
    #[arg(long, env = "LIQUID_CLI_LAUNCH_MODE", default_value_t = CliLaunchMode::Auto)]
    pub(crate) cli_launch_mode: CliLaunchMode,
    #[arg(
        long,
        env = "LIQUID_RESEARCH_IMPLEMENTATION",
        default_value_t = ResearchImplementationSelection::Classic
    )]
    pub(crate) research_implementation: ResearchImplementationSelection,
    #[arg(
        long,
        env = "LIQUID_RESEARCH_HISTORICAL_PHASE_ENGINE",
        default_value_t = false
    )]
    pub(crate) research_historical_phase_engine: bool,
}

pub(crate) fn setup_data_dir(
    data_dir_arg: &StdPath,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let data_dir = if data_dir_arg.to_string_lossy().starts_with("~/") {
        let home = std::env::var("HOME")?;
        PathBuf::from(
            data_dir_arg
                .to_string_lossy()
                .replacen("~/", &format!("{}/", home), 1),
        )
    } else {
        data_dir_arg.to_path_buf()
    };

    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
    }
    Ok(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_launch_mode_defaults_to_auto() {
        assert_eq!(CliLaunchMode::default(), CliLaunchMode::Auto);
    }

    #[test]
    fn test_host_arg_defaults_to_loopback() {
        let command = Args::command();
        let host_arg = command
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "host")
            .expect("host arg should exist");
        let defaults = host_arg
            .get_default_values()
            .iter()
            .map(|value| value.to_str().expect("utf-8 default"))
            .collect::<Vec<_>>();

        assert_eq!(defaults, vec!["127.0.0.1"]);
    }

    #[test]
    fn test_cli_launch_mode_parses_valid_values() {
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
    }

    #[test]
    fn test_cli_launch_mode_rejects_invalid_value() {
        assert!("unsafe".parse::<CliLaunchMode>().is_err());
    }

    #[test]
    fn test_research_implementation_defaults_to_classic() {
        assert_eq!(
            ResearchImplementationSelection::default(),
            ResearchImplementationSelection::Classic
        );
        assert_eq!(
            ResearchImplementationSelection::Classic.to_string(),
            "classic"
        );
    }

    #[test]
    fn test_research_implementation_parses_only_classic() {
        assert_eq!(
            "classic"
                .parse::<ResearchImplementationSelection>()
                .unwrap(),
            ResearchImplementationSelection::Classic
        );
        assert!("next-gen"
            .parse::<ResearchImplementationSelection>()
            .is_err());
    }
}
