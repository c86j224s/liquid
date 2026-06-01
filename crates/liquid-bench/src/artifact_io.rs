use super::*;

const REPLAY_FIXTURE_PROMPT_REDACTED_PLACEHOLDER: &str =
    "[redacted replay fixture prompt: explicit user request marker missing or empty]";

pub(super) fn load_replay_fixture_bundles(
    root: &Path,
    case_plans: &[CaseExecutionPlan],
) -> Result<Vec<ReplayFixtureBundle>, Box<dyn std::error::Error>> {
    if !root.is_dir() {
        return Err(format!("replay fixture root is missing: {}", root.display()).into());
    }
    let case_map = case_plans
        .iter()
        .map(|plan| (plan.case.case_id.clone(), plan.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut bundles = Vec::new();
    let mut summary_paths = fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            !file_name.ends_with("-controller-artifacts.json")
                && !file_name.ends_with("-source-diagnostics.json")
        })
        .collect::<Vec<_>>();
    summary_paths.sort();
    if summary_paths.len() > MAX_REPLAY_FIXTURE_BUNDLES {
        return Err(format!(
            "replay fixture root has too many case summaries: {} > {}",
            summary_paths.len(),
            MAX_REPLAY_FIXTURE_BUNDLES
        )
        .into());
    }
    let mut total_bytes = 0_u64;
    for path in summary_paths {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("invalid replay fixture filename: {}", path.display()))?;
        let summary = serde_json::from_str::<ReplayFixtureSummary>(&read_bounded_text_file(
            &path,
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?)?;
        let resolved_user_prompt = read_bounded_text_file(
            &root.join(format!("{stem}-resolved-user-prompt.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let plan = resolve_replay_fixture_plan(&summary, stem, &case_map, &resolved_user_prompt);
        let final_output = read_bounded_text_file(
            &root.join(format!("{stem}-final-output.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let controller_artifacts_json = read_bounded_text_file(
            &root.join(format!("{stem}-controller-artifacts.json")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let source_diagnostics_json = read_bounded_text_file(
            &root.join(format!("{stem}-source-diagnostics.json")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        let _ = read_bounded_text_file(
            &root.join(format!("{stem}-resolved-system-prompt.md")),
            &mut total_bytes,
            MAX_REPLAY_FIXTURE_FILE_BYTES,
            MAX_REPLAY_FIXTURE_TOTAL_BYTES,
        )?;
        bundles.push(ReplayFixtureBundle {
            plan,
            summary,
            final_output,
            controller_artifacts_json,
            source_diagnostics_json,
        });
    }
    bundles.sort_by(|left, right| left.plan.artifact_stem.cmp(&right.plan.artifact_stem));
    Ok(bundles)
}

pub(super) fn read_bounded_text_file(
    path: &Path,
    total_bytes: &mut u64,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let file_bytes = fs::metadata(path)?.len();
    if file_bytes > max_file_bytes {
        return Err(format!(
            "replay fixture file is too large: {} bytes > {} for {}",
            file_bytes,
            max_file_bytes,
            path.display()
        )
        .into());
    }
    let next_total = total_bytes.saturating_add(file_bytes);
    if next_total > max_total_bytes {
        return Err(format!(
            "replay fixture root exceeds total byte limit: {} bytes > {}",
            next_total, max_total_bytes
        )
        .into());
    }
    *total_bytes = next_total;
    Ok(fs::read_to_string(path)?)
}

fn resolve_replay_fixture_plan(
    summary: &ReplayFixtureSummary,
    stem: &str,
    case_map: &BTreeMap<String, CaseExecutionPlan>,
    resolved_user_prompt: &str,
) -> CaseExecutionPlan {
    if let Some(plan) = case_map.get(&summary.case_id) {
        return plan.clone();
    }

    CaseExecutionPlan {
        case: BenchmarkCase {
            case_id: summary.case_id.clone(),
            filename: format!("{stem}.json"),
            title: summary
                .title
                .clone()
                .unwrap_or_else(|| humanize_replay_case_id(&summary.case_id)),
            category: summary.category.clone(),
            prompt: extract_replay_fixture_prompt(resolved_user_prompt),
            must_pass_checks: replay_fixture_must_pass_checks(&summary.case_id, &summary.category),
            expected_failure_modes: replay_fixture_expected_failure_modes(
                &summary.case_id,
                &summary.category,
            ),
        },
        artifact_stem: stem.to_string(),
    }
}

pub(super) fn extract_replay_fixture_prompt(resolved_user_prompt: &str) -> String {
    let mut capture = false;
    let mut lines = Vec::new();
    for line in resolved_user_prompt.lines() {
        let trimmed = line.trim();
        if !capture && malformed_replay_fixture_section_before_user_request(trimmed) {
            return REPLAY_FIXTURE_PROMPT_REDACTED_PLACEHOLDER.to_string();
        }
        if trimmed.eq_ignore_ascii_case("### User Request:") {
            capture = true;
            continue;
        }
        if !capture {
            continue;
        }
        if trimmed
            .to_ascii_lowercase()
            .starts_with("[research quality repair iteration")
        {
            break;
        }
        if trimmed.starts_with("### ") {
            break;
        }
        lines.push(line);
    }
    let prompt = lines.join("\n").trim().to_string();
    if prompt.is_empty() {
        REPLAY_FIXTURE_PROMPT_REDACTED_PLACEHOLDER.to_string()
    } else {
        prompt
    }
}

fn malformed_replay_fixture_section_before_user_request(trimmed: &str) -> bool {
    if !trimmed.starts_with("### ") {
        return false;
    }
    let normalized = normalize_replay_fixture_section_marker(trimmed);
    [
        "source documents",
        "pre collected evidence bundle",
        "source pack",
        "source diagnostics",
        "source diagnostic",
        "controller",
        "controller artifacts",
        "provider",
        "private",
        "debug",
        "resolved system prompt",
        "resolved user prompt",
        "env",
        "environment",
        "database",
        "db",
        "log",
        "logs",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn normalize_replay_fixture_section_marker(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }
    normalized.trim().to_string()
}

fn replay_fixture_must_pass_checks(case_id: &str, category: &str) -> Vec<String> {
    match (case_id, category) {
        ("02-current-policy-regulatory", _) => vec![
            "distinguishes voluntary NIST framework guidance from binding EU AI Act obligations using official sources".to_string(),
            "makes conflicts, open interpretive points, and role-scope limits visible instead of flattening them".to_string(),
            "keeps visible Source Audit URLs and Claim Log support for each core compliance comparison".to_string(),
        ],
        ("03-comparative-product-technical-decision", _) => vec![
            "cites current official technical specifications for memory ceilings, battery, and repairability or upgradeability".to_string(),
            "separates official specs from reviewer or third-party thermals and sustained-performance claims".to_string(),
            "gives a final recommendation that makes CUDA, unified-memory, and repairability tradeoffs explicit".to_string(),
        ],
        _ if category.contains("policy") || category.contains("regulatory") => vec![
            "uses official sources for the core policy or regulatory obligations".to_string(),
            "states what is verified, what conflicts, and what remains open".to_string(),
            "keeps visible Source Audit URLs and Claim Log support".to_string(),
        ],
        _ if category.contains("product") || category.contains("technical-decision") => vec![
            "cites current official specifications for the main decision criteria".to_string(),
            "separates official facts from third-party performance interpretation".to_string(),
            "gives a recommendation with explicit tradeoffs and uncertainties".to_string(),
        ],
        _ => vec![
            "preserves a visible Final Answer with readable prose".to_string(),
            "keeps visible Source Audit URLs and resolvable Claim Log support".to_string(),
            "states remaining limits or conflicts instead of fabricating certainty".to_string(),
        ],
    }
}

fn replay_fixture_expected_failure_modes(case_id: &str, category: &str) -> Vec<String> {
    match (case_id, category) {
        ("02-current-policy-regulatory", _) => vec![
            "deployer, provider, and downstream-provider roles are blurred into a false single obligation set".to_string(),
            "interpretive or enforcement uncertainty is hidden instead of labeled".to_string(),
            "service-desk summaries are treated as binding law without anchoring the official legal text".to_string(),
        ],
        ("03-comparative-product-technical-decision", _) => vec![
            "official specs and reviewer thermal claims are blended without labeling".to_string(),
            "the recommendation ignores CUDA, unified-memory, or repairability constraints".to_string(),
            "current specifications or battery limits are fabricated or left unstated".to_string(),
        ],
        _ if category.contains("policy") || category.contains("regulatory") => vec![
            "official and interpretive sources are collapsed into one certainty level".to_string(),
            "open policy conflicts are presented as resolved fact".to_string(),
        ],
        _ if category.contains("product") || category.contains("technical-decision") => vec![
            "recommendation is made without explicit tradeoff handling".to_string(),
            "specifications are presented as current without concrete support".to_string(),
        ],
        _ => vec![
            "visible evidence support remains missing".to_string(),
            "the conclusion claims more certainty than the artifacts support".to_string(),
        ],
    }
}

fn humanize_replay_case_id(case_id: &str) -> String {
    case_id
        .trim_matches(|ch: char| ch.is_ascii_digit() || ch == '-')
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn render_failure_markdown_artifact(
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> String {
    let critical_flags = scorecard
        .critical_flags
        .iter()
        .filter(|flag| flag.triggered)
        .map(|flag| format!("- {}: {}", flag.label, flag.rationale))
        .collect::<Vec<_>>();
    format!(
        "# Benchmark Case Failure\n\n- Case ID: {}\n- Title: {}\n- Category: {}\n- Mode: {}\n- Task status: {}\n- Quality status: {}\n- Controller stage: {}\n- Error: {}\n- Structured overall score: {:.2}/5.00\n- Critical failure flags: {}\n\n## Failure Summary\n{}\n\n## Artifact Availability\n- final output captured in task result: {}\n- controller artifacts JSON: {}\n- source diagnostics JSON: {}\n- resolved system prompt: {}\n- resolved user prompt: {}\n",
        result.case_id,
        result.title,
        result.category,
        benchmark_mode_label(result.mode),
        result.status,
        result.quality_status.as_deref().unwrap_or("unknown"),
        result
            .research_controller_stage
            .as_deref()
            .unwrap_or("unknown"),
        sanitize_public_artifact_text(
            result
                .error_message
                .as_deref()
                .or(result.quality_last_failure.as_deref())
                .unwrap_or("unknown failure"),
        ),
        scorecard.overall_score,
        critical_flags.len(),
        if critical_flags.is_empty() {
            "- none recorded".to_string()
        } else {
            critical_flags.join("\n")
        },
        if result.final_output.is_some() {
            "yes"
        } else {
            "no"
        },
        if result.research_controller_artifacts_json.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.research_source_diagnostics_json.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.resolved_system_prompt.is_some() {
            "present"
        } else {
            "missing"
        },
        if result.resolved_user_prompt.is_some() {
            "present"
        } else {
            "missing"
        },
    )
}

pub(super) fn write_case_summary_json(
    path: &Path,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> Result<(), Box<dyn std::error::Error>> {
    write_new_text_file(
        path,
        &serde_json::to_string_pretty(&json!({
            "case_id": result.case_id,
            "title": result.title,
            "category": result.category,
            "mode": match result.mode {
                ResearchBenchmarkMode::Fixture => "fixture",
                ResearchBenchmarkMode::Live => "live",
                ResearchBenchmarkMode::Replay => "replay",
            },
            "task_id": result.task_id,
            "status": result.status,
            "error_message": public_artifact_optional_text(result.error_message.as_deref()),
            "quality_status": result.quality_status,
            "quality_last_failure": public_artifact_optional_text(result.quality_last_failure.as_deref()),
            "research_controller_stage": result.research_controller_stage,
            "research_controller_iteration": result.research_controller_iteration,
            "research_controller_max_iterations": result.research_controller_max_iterations,
            "output_filename": result.output_filename,
            "model_input": public_model_input_label(&result.model_input),
            "structured_scorecard": scorecard,
        }))?,
    )
}

pub(super) fn write_case_failure_fallback_artifacts(
    run_case_dir: &Path,
    artifact_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> FallbackArtifactWriteOutcome {
    let unique_suffix = Uuid::new_v4().simple().to_string();
    let fallback_stem = format!("{artifact_stem}-failure-{unique_suffix}");
    write_case_failure_fallback_artifacts_with_stem(run_case_dir, &fallback_stem, result, scorecard)
}

pub(super) fn write_case_failure_fallback_artifacts_with_stem(
    run_case_dir: &Path,
    fallback_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
) -> FallbackArtifactWriteOutcome {
    let failure_path = run_case_dir.join(format!("{fallback_stem}.md"));
    let mut paths = CaseArtifactPaths::default();
    if let Err(error) = write_new_text_file(
        &failure_path,
        &render_failure_markdown_artifact(result, scorecard),
    ) {
        return FallbackArtifactWriteOutcome {
            paths,
            error: Some(format!("failure markdown write failed: {error}")),
        };
    }
    paths.final_output_path = Some(failure_path);
    let summary_path = run_case_dir.join(format!("{fallback_stem}.json"));
    match write_case_summary_json(&summary_path, result, scorecard) {
        Ok(()) => {
            paths.summary_json_path = Some(summary_path);
            FallbackArtifactWriteOutcome { paths, error: None }
        }
        Err(error) => FallbackArtifactWriteOutcome {
            paths,
            error: Some(format!("failure summary JSON write failed: {error}")),
        },
    }
}

pub(super) fn write_case_artifacts(
    run_case_dir: &Path,
    artifact_stem: &str,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
    include_raw_debug_artifacts: bool,
) -> Result<CaseArtifactPaths, Box<dyn std::error::Error>> {
    let mut paths = CaseArtifactPaths::default();
    let path = run_case_dir.join(format!("{artifact_stem}-final-output.md"));
    let output = case_final_output_artifact_markdown(result)
        .unwrap_or_else(|| render_failure_markdown_artifact(result, scorecard));
    write_new_text_file(&path, &output)?;
    paths.final_output_path = Some(path);
    if include_raw_debug_artifacts {
        if let Some(json_body) = result.research_source_diagnostics_json.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-source-diagnostics.json"));
            write_new_text_file(&path, json_body)?;
            paths.diagnostics_json_path = Some(path);
        }
        if let Some(json_body) = result.research_controller_artifacts_json.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-controller-artifacts.json"));
            write_new_text_file(&path, json_body)?;
            paths.controller_json_path = Some(path);
        }
        if let Some(prompt) = result.resolved_system_prompt.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-resolved-system-prompt.md"));
            write_new_text_file(&path, prompt)?;
            paths.resolved_system_prompt_path = Some(path);
        }
        if let Some(prompt) = result.resolved_user_prompt.as_deref() {
            let path = run_case_dir.join(format!("{artifact_stem}-resolved-user-prompt.md"));
            write_new_text_file(&path, prompt)?;
            paths.resolved_user_prompt_path = Some(path);
        }
    }
    let summary_path = run_case_dir.join(format!("{artifact_stem}.json"));
    write_case_summary_json(&summary_path, result, scorecard)?;
    paths.summary_json_path = Some(summary_path);
    Ok(paths)
}

pub(super) fn write_structured_run_outputs(
    runs_dir: &Path,
    label: &str,
    report: &BenchmarkRunStructuredReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_path = runs_dir.join(format!("{label}.json"));
    let csv_path = runs_dir.join(format!("{label}.csv"));
    let ndjson_path = runs_dir.join(format!("{label}.ndjson"));
    write_new_text_file(&json_path, &serde_json::to_string_pretty(report)?)?;
    write_new_text_file(&csv_path, &render_case_scores_csv(report))?;
    write_new_text_file(&ndjson_path, &render_dimension_scores_ndjson(report)?)?;
    Ok(())
}

pub(super) fn preflight_run_output_paths(
    run_case_dir: &Path,
    report_path: &Path,
    runs_dir: &Path,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let aggregate_paths = [
        report_path.to_path_buf(),
        runs_dir.join(format!("{label}.json")),
        runs_dir.join(format!("{label}.csv")),
        runs_dir.join(format!("{label}.ndjson")),
    ];
    if run_case_dir.exists() {
        return Err(format!(
            "benchmark output directory already exists: {}",
            run_case_dir.display()
        )
        .into());
    }
    if let Some(existing) = aggregate_paths.iter().find(|path| path.exists()) {
        return Err(format!("benchmark report already exists: {}", existing.display()).into());
    }
    Ok(())
}

pub(super) fn secret_scan_report(
    report: &str,
    executions: &[RunExecution],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut findings = Vec::new();
    scan_sensitive_text("aggregate report", report, &mut findings);
    for execution in executions {
        if let Some(output) = execution.result.final_output.as_deref() {
            scan_sensitive_text(
                &format!("case {} final output", execution.case.case_id),
                output,
                &mut findings,
            );
        }
        if let Some(json_body) = execution.result.research_source_diagnostics_json.as_deref() {
            scan_sensitive_text(
                &format!("case {} source diagnostics", execution.case.case_id),
                json_body,
                &mut findings,
            );
        }
        if let Some(json_body) = execution
            .result
            .research_controller_artifacts_json
            .as_deref()
        {
            scan_sensitive_text(
                &format!("case {} controller artifacts", execution.case.case_id),
                json_body,
                &mut findings,
            );
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!("secret/raw-payload scan failed: {}", findings.join(" | ")).into())
    }
}

fn scan_sensitive_text(label: &str, text: &str, findings: &mut Vec<String>) {
    let lower = text.to_ascii_lowercase();
    let patterns = [
        ("api_key", "raw provider key field"),
        ("client_secret", "raw client secret field"),
        ("x-naver-client-secret", "raw provider header"),
        ("authorization:", "raw authorization header"),
        ("bearer ", "bearer token fragment"),
        ("raw provider payload", "raw provider payload marker"),
        ("response body:", "raw response body marker"),
    ];
    for (needle, description) in patterns {
        if lower.contains(needle) {
            findings.push(format!("{label} contains {description}"));
        }
    }
}

pub(super) fn write_new_text_file(
    path: &Path,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(body.as_bytes())?;
    Ok(())
}
