use chrono::Utc;
use std::{
    backtrace::Backtrace,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process, thread,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub(crate) fn install_panic_hook(crash_dir: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let summary = format_panic_summary(info);
        let report = format_panic_report(info, &Backtrace::force_capture());
        match write_crash_report(&crash_dir, &report) {
            Ok(path) => eprintln!(
                "{}",
                format_lifecycle_event(
                    "panic",
                    &format!("{} crash_report={}", summary, path.display())
                )
            ),
            Err(err) => eprintln!(
                "{}",
                format_lifecycle_event(
                    "panic",
                    &format!(
                        "{} failed to write crash report: {}",
                        summary,
                        sanitize_log_field(&err.to_string())
                    )
                )
            ),
        }
    }));
}

pub(crate) fn log_lifecycle_event(event: &str, detail: &str) {
    eprintln!("{}", format_lifecycle_event(event, detail));
}

pub(crate) async fn shutdown_signal() {
    let signal = wait_for_shutdown_signal().await;
    log_lifecycle_event("shutdown_signal", &format!("received {}", signal));
}

async fn wait_for_shutdown_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut interrupt =
            signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        let mut terminate =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = interrupt.recv() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        "Ctrl+C"
    }
}

fn format_panic_report(info: &std::panic::PanicHookInfo<'_>, backtrace: &Backtrace) -> String {
    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|value| (*value).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string());

    format!(
        "[{}] panic captured\npid: {}\nthread: {}\nlocation: {}\npayload: {}\n\nbacktrace:\n{}\n",
        Utc::now().to_rfc3339(),
        process::id(),
        thread_name,
        location,
        payload,
        backtrace
    )
}

pub(crate) fn format_lifecycle_event(event: &str, detail: &str) -> String {
    format!(
        "[{}] lifecycle event={} pid={} {}",
        Utc::now().to_rfc3339(),
        event,
        process::id(),
        detail
    )
}

fn format_panic_summary(info: &std::panic::PanicHookInfo<'_>) -> String {
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    format!("location={}", sanitize_log_field(&location))
}

fn sanitize_log_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_control() && ch != '\t' {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn write_crash_report(crash_dir: &Path, report: &str) -> io::Result<PathBuf> {
    create_private_crash_dir(crash_dir)?;
    let filename = format!(
        "crash-{}-pid{}.log",
        Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        process::id()
    );
    let path = crash_dir.join(filename);
    write_private_file(&path, report)?;
    Ok(path)
}

fn create_private_crash_dir(crash_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(crash_dir)?;
    #[cfg(unix)]
    fs::set_permissions(crash_dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_private_file(path: &Path, report: &str) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(path)?;
    file.write_all(report.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn lifecycle_event_contains_event_detail_and_pid() {
        let event = format_lifecycle_event("startup", "listening on 127.0.0.1:3000");

        assert!(event.contains("lifecycle event=startup"));
        assert!(event.contains("pid="));
        assert!(event.contains("listening on 127.0.0.1:3000"));
    }

    #[test]
    fn sanitize_log_field_removes_multiline_and_control_chars() {
        let sanitized = sanitize_log_field("hello\n[forged]\x1b[31m\tworld");

        assert_eq!(sanitized, "hello [forged] [31m world");
    }

    #[test]
    fn write_crash_report_creates_file_under_crash_dir() {
        let dir = std::env::temp_dir().join(format!("liquid-crash-test-{}", Uuid::new_v4()));
        let path = write_crash_report(&dir, "panic report").expect("write crash report");

        assert!(path.starts_with(&dir));
        assert_eq!(fs::read_to_string(&path).unwrap(), "panic report");

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_crash_report_uses_private_unix_permissions() {
        let dir = std::env::temp_dir().join(format!("liquid-crash-test-{}", Uuid::new_v4()));
        let path = write_crash_report(&dir, "panic report").expect("write crash report");

        let dir_mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        let file_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);

        let _ = fs::remove_dir_all(dir);
    }
}
