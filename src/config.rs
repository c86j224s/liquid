use crate::cli_launcher::CliLaunchMode;
use clap::Parser;
use std::path::{Path as StdPath, PathBuf};

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
}
