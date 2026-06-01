use std::path::PathBuf;

const EXTERNAL_SKILL_ENV: &str = "LIQUID_RESEARCH_HTML_SKILL_PATH";

pub(crate) fn build_html_design_prompt() -> String {
    let configured_path = configured_html_skill_path();
    liquid_html_report::build_html_design_prompt(configured_path.as_deref())
}

pub(crate) fn redact_html_design_prompt_for_storage(system_prompt: &str) -> String {
    liquid_html_report::redact_external_skill_from_prompt(system_prompt)
}

pub(crate) fn hydrate_html_design_prompt_for_execution(system_prompt: &str) -> String {
    let configured_path = configured_html_skill_path();
    liquid_html_report::hydrate_external_skill_in_prompt(system_prompt, configured_path.as_deref())
}

fn expand_home_path(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    liquid_html_report::expand_home_path(path, home.as_deref())
}

fn configured_html_skill_path() -> Option<PathBuf> {
    let raw_path = std::env::var(EXTERNAL_SKILL_ENV).ok()?;
    Some(expand_home_path(raw_path.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Mutex, OnceLock},
    };
    use uuid::Uuid;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn html_design_prompt_falls_back_without_external_skill_path() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var(EXTERNAL_SKILL_ENV);

        let prompt = build_html_design_prompt();

        assert!(prompt.contains("[OUTPUT FORMAT: INTERACTIVE HTML REPORT]"));
        assert!(!prompt.contains("[EXPERIMENTAL HTML DESIGN SKILL]"));
    }

    #[test]
    fn html_design_prompt_includes_external_skill_when_configured() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!("liquid-skill-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let skill_path = dir.join("SKILL.md");
        fs::write(&skill_path, "# Skill\n\nUse component diagrams.").unwrap();
        std::env::set_var(EXTERNAL_SKILL_ENV, &skill_path);

        let prompt = build_html_design_prompt();

        assert!(prompt.contains("[OUTPUT FORMAT: INTERACTIVE HTML REPORT]"));
        assert!(prompt.contains("[EXPERIMENTAL HTML DESIGN SKILL]"));
        assert!(prompt.contains("Use component diagrams."));

        std::env::remove_var(EXTERNAL_SKILL_ENV);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn html_design_prompt_includes_directory_skill_when_configured() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!("liquid-skill-dir-test-{}", Uuid::new_v4()));
        fs::create_dir_all(dir.join("references")).unwrap();
        fs::write(dir.join("SKILL.md"), "# Skill\n\nUse sequence diagrams.").unwrap();
        fs::write(
            dir.join("references/component-patterns.md"),
            "Use evidence markers.",
        )
        .unwrap();
        std::env::set_var(EXTERNAL_SKILL_ENV, &dir);

        let prompt = build_html_design_prompt();

        assert!(prompt.contains("[EXPERIMENTAL HTML DESIGN SKILL]"));
        assert!(prompt.contains("Use sequence diagrams."));
        assert!(prompt.contains("[HTML DESIGN REFERENCE BUNDLE]"));
        assert!(prompt.contains("Use evidence markers."));

        std::env::remove_var(EXTERNAL_SKILL_ENV);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn external_skill_prompt_is_redacted_for_storage_and_hydrated_from_configured_path() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!("liquid-skill-redact-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let skill_path = dir.join("SKILL.md");
        fs::write(&skill_path, "# Skill\n\nUse secret design notes safely.").unwrap();
        std::env::set_var(EXTERNAL_SKILL_ENV, &skill_path);

        let prompt = format!(
            "system prefix\n\n{}\n\n[RESEARCH CONTROLLER CONTRACT v22]\ncontroller",
            build_html_design_prompt()
        );
        let redacted = redact_html_design_prompt_for_storage(&prompt);
        assert!(redacted.contains("[external-html-report-skill-redacted]"));
        assert!(!redacted.contains("Use secret design notes safely."));

        let hydrated = hydrate_html_design_prompt_for_execution(&redacted);
        assert!(hydrated.contains("Use secret design notes safely."));

        std::env::remove_var(EXTERNAL_SKILL_ENV);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn home_paths_are_expanded() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("HOME", "/tmp/liquid-home");

        assert_eq!(
            expand_home_path("~/skills/example/SKILL.md"),
            PathBuf::from("/tmp/liquid-home/skills/example/SKILL.md")
        );
    }
}
