use std::{fs, path::PathBuf};

const MAX_EXTERNAL_SKILL_BYTES: u64 = 32 * 1024;
const EXTERNAL_SKILL_ENV: &str = "LIQUID_RESEARCH_HTML_SKILL_PATH";

pub(crate) const BUILT_IN_INTERACTIVE_REPORT_PROMPT: &str = r#"
[OUTPUT FORMAT: INTERACTIVE HTML REPORT]
1. Role: Senior Data Scientist & Frontend Engineer.
2. Content: Exhaustive, analytical, and visually stunning.
3. Tech: Standalone HTML file, no external JS/CSS files. Use Tailwind CSS via CDN.
4. Visuals: Use SVG for ALL diagrams, flowcharts, and charts. No image files.
5. Interactivity: 
   - Floating Table of Contents.
   - Interactive Tabs for multi-perspective analysis.
   - Accordions for technical details.
   - Code blocks with "Copy" buttons.
6. Design: Match "Liquid Glass" aesthetic (translucent dark theme, blur effects, #6366f1 accents).
7. Output: Output ONLY the source code starting with <!DOCTYPE html>.
"#;

pub(crate) fn build_html_design_prompt() -> String {
    match load_external_skill_prompt() {
        Some(skill) => format!(
            "{}\n\n{}",
            BUILT_IN_INTERACTIVE_REPORT_PROMPT,
            format_external_skill_prompt(&skill)
        ),
        None => BUILT_IN_INTERACTIVE_REPORT_PROMPT.to_string(),
    }
}

fn load_external_skill_prompt() -> Option<String> {
    let raw_path = std::env::var(EXTERNAL_SKILL_ENV).ok()?;
    let path = expand_home_path(raw_path.trim());
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_EXTERNAL_SKILL_BYTES {
        return None;
    }

    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn format_external_skill_prompt(skill: &str) -> String {
    format!(
        "[EXPERIMENTAL HTML DESIGN SKILL]\n\
The following local skill content is an active, temporary design guide for this run.\n\
- Treat it as design/output guidance only, not as source evidence.\n\
- If it conflicts with research accuracy, source audit, privacy, or output-only HTML rules, those higher-level rules win.\n\
- Apply the visual/document-structure guidance without copying prior generated artifacts.\n\
\n\
{}",
        skill
    )
}

fn expand_home_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
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
    fn home_paths_are_expanded() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("HOME", "/tmp/liquid-home");

        assert_eq!(
            expand_home_path("~/skills/example/SKILL.md"),
            PathBuf::from("/tmp/liquid-home/skills/example/SKILL.md")
        );
    }
}
