mod application;
pub use application::tasks;
mod config;
mod contracts;
mod diagnostics;
mod research_design;
mod server;
mod state;
#[cfg(test)]
mod test_support;

pub use liquid_research_core::{has_visible_final_answer_section, strip_research_artifact_blocks};
pub use tasks::{
    run_research_benchmark_case, ResearchBenchmarkCaseInput, ResearchBenchmarkCaseResult,
    ResearchBenchmarkMode,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_reexports_match_contract_inventory() {
        let _strip: fn(&str) -> String = strip_research_artifact_blocks;
        let _visible: fn(&str) -> bool = has_visible_final_answer_section;
        let _runner = run_research_benchmark_case;

        let input = ResearchBenchmarkCaseInput {
            case_id: "case-1".to_string(),
            title: "Case".to_string(),
            category: "fixture".to_string(),
            prompt: "Prompt".to_string(),
            data_dir: std::path::PathBuf::from("./tmp"),
            mode: ResearchBenchmarkMode::Fixture,
            model_input: None,
            research_intensity: "high".to_string(),
            quality_depth: "strict".to_string(),
            quality_max_iterations: 2,
            cli_launch_mode: None,
            ai_task_timeout_secs: 60,
        };
        let _: crate::application::tasks::ResearchBenchmarkCaseInput = input.clone();
        let _: liquid_protocol::ResearchBenchmarkCaseInput = input.clone();

        let result = ResearchBenchmarkCaseResult {
            case_id: "case-1".to_string(),
            title: "Case".to_string(),
            category: "fixture".to_string(),
            mode: ResearchBenchmarkMode::Fixture,
            task_id: 1,
            status: "completed".to_string(),
            error_message: None,
            quality_status: Some("passed".to_string()),
            quality_last_failure: None,
            research_controller_stage: Some("final".to_string()),
            research_controller_iteration: Some(1),
            research_controller_max_iterations: Some(2),
            output_filename: Some("output.md".to_string()),
            final_output: Some("## Final Answer\nDone".to_string()),
            model_input: "cli:fixture-research-bench".to_string(),
        };
        let _: crate::application::tasks::ResearchBenchmarkCaseResult = result.clone();
        let _: liquid_protocol::ResearchBenchmarkCaseResult = result;

        let mode = ResearchBenchmarkMode::Fixture;
        let _: crate::application::tasks::ResearchBenchmarkMode = mode.clone();
        let _: liquid_protocol::ResearchBenchmarkMode = mode;
    }
    #[test]
    fn architecture_layout_uses_named_layers_not_legacy_root_shims() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for removed in [
            "src/db.rs",
            "src/research_quality.rs",
            "src/research_sources.rs",
            "src/cli_launcher.rs",
            "src/pi_runtime.rs",
            "src/models.rs",
        ] {
            assert!(
                !root.join(removed).exists(),
                "legacy root shim should not be recreated: {removed}"
            );
        }
        for required in [
            "src/server/mod.rs",
            "src/server/app.rs",
            "src/server/context.rs",
            "src/server/files.rs",
            "src/server/presenters.rs",
            "src/server/engine_presets.rs",
            "src/server/scraping.rs",
            "crates/liquid-server/src/lib.rs",
            "crates/liquid-server/src/app.rs",
            "crates/liquid-server/src/context.rs",
            "crates/liquid-server/src/files.rs",
            "crates/liquid-server/src/presenters.rs",
            "crates/liquid-server/src/engine_presets.rs",
            "crates/liquid-server/src/scraping.rs",
            "src/application/mod.rs",
            "src/application/server_adapters.rs",
            "src/application/tasks.rs",
            "src/application/tasks/runtime_adapters.rs",
            "src/application/ai_runtime.rs",
            "src/application/ai_runtime/provider_runtime.rs",
            "src/application/ai_runtime/prompt_storage.rs",
            "src/application/ai_runtime/research_context_workflow.rs",
            "src/application/ai_runtime/translation_workflow.rs",
            "src/application/engine_presets/mod.rs",
            "src/application/engine_presets/catalog.rs",
            "src/application/engine_presets/repository.rs",
            "src/application/engine_presets/resolver.rs",
            "src/application/engine_presets/status_check.rs",
            "crates/liquid-research-artifacts/src/lib.rs",
            "crates/liquid-research-classic/src/lib.rs",
            "crates/liquid-files/src/lib.rs",
            "crates/liquid-workspace/src/lib.rs",
            "src/application/file_tags.rs",
            "src/application/research_prompts.rs",
            "src/application/scraping.rs",
            "src/application/ports.rs",
            "src/contracts.rs",
            "docs/architecture/intuitive-architecture.md",
        ] {
            assert!(
                root.join(required).exists(),
                "architecture layer path is required: {required}"
            );
        }

        for entry in walk_rs_files(&root.join("src/application")) {
            let body = std::fs::read_to_string(&entry).expect("read application file");
            let production_body = strip_cfg_test_modules(&body);
            let filtered_body = if entry.ends_with("src/application/server_adapters.rs") {
                production_body.replace("crate::server::context", "")
            } else {
                production_body
            };
            assert!(
                !filtered_body.contains("crate::server::"),
                "application layer must not depend on server shell: {}",
                entry.display()
            );
        }

        for entry in walk_rs_files(&root.join("src/server")) {
            let body = std::fs::read_to_string(&entry).expect("read server file");
            let production_body = strip_cfg_test_modules(&body);
            assert!(
                !production_body.contains("crate::state::AppState"),
                "server shell should not depend on AppState directly: {}",
                entry.display()
            );
            assert!(
                !server_depends_on_application(production_body.as_str()),
                "server shell should not depend on application adapters directly: {}",
                entry.display()
            );
        }
    }

    #[test]
    fn architecture_guard_detects_grouped_server_application_imports() {
        let grouped = "use crate::{application::scraping::friendly_scrape_failure_message, contracts::TaskInfo};";
        let grouped_multiline = "use crate::{\n    contracts::TaskInfo,\n    application::scraping::friendly_scrape_failure_message,\n};";
        let grouped_spaced = "use crate :: { contracts::TaskInfo, application::scraping::friendly_scrape_failure_message };";
        let direct = "use crate::application::scraping::friendly_scrape_failure_message;";
        let clean = "use crate::{contracts::TaskInfo, server::context::ServerContext};";

        assert!(server_depends_on_application(grouped));
        assert!(server_depends_on_application(grouped_multiline));
        assert!(server_depends_on_application(grouped_spaced));
        assert!(server_depends_on_application(direct));
        assert!(!server_depends_on_application(clean));
    }

    fn strip_cfg_test_modules(body: &str) -> String {
        let mut stripped = String::with_capacity(body.len());
        let mut lines = body.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim() == "#[cfg(test)]" {
                if let Some(next) = lines.peek() {
                    let next_trimmed = next.trim_start();
                    if next_trimmed.starts_with("mod tests")
                        || next_trimmed.starts_with("mod ")
                        || next_trimmed.starts_with("fn ")
                        || next_trimmed.starts_with("struct ")
                        || next_trimmed.starts_with("enum ")
                        || next_trimmed.starts_with("impl ")
                        || next_trimmed.starts_with("const ")
                    {
                        skip_cfg_test_item(&mut lines);
                        continue;
                    }
                    if next_trimmed.starts_with("use ")
                        || next_trimmed.starts_with("pub(crate) use ")
                    {
                        skip_cfg_test_use_item(&mut lines);
                        continue;
                    }
                }
            }
            stripped.push_str(line);
            stripped.push('\n');
        }
        stripped
    }

    fn server_depends_on_application(body: &str) -> bool {
        let compact: String = body.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact.contains("crate::application::") {
            return true;
        }

        for statement in compact.split(';') {
            if (statement.starts_with("usecrate::{")
                || statement.starts_with("pub(crate)usecrate::{"))
                && statement.contains("application::")
            {
                return true;
            }
        }

        false
    }

    fn skip_cfg_test_item<'a, I>(lines: &mut std::iter::Peekable<I>)
    where
        I: Iterator<Item = &'a str>,
    {
        let Some(first_line) = lines.next() else {
            return;
        };
        if first_line.contains('{') {
            let mut depth = brace_delta(first_line);
            if depth == 0 {
                return;
            }
            for line in lines.by_ref() {
                depth += brace_delta(line);
                if depth == 0 {
                    return;
                }
            }
        }
    }

    fn skip_cfg_test_use_item<'a, I>(lines: &mut std::iter::Peekable<I>)
    where
        I: Iterator<Item = &'a str>,
    {
        for line in lines.by_ref() {
            if line.trim_end().ends_with(';') {
                return;
            }
        }
    }

    fn brace_delta(line: &str) -> isize {
        let mut delta = 0isize;
        for ch in line.chars() {
            match ch {
                '{' => delta += 1,
                '}' => delta -= 1,
                _ => {}
            }
        }
        delta
    }

    fn walk_rs_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                let Ok(entries) = std::fs::read_dir(path) else {
                    continue;
                };
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push(path);
            }
        }
        out
    }
}
