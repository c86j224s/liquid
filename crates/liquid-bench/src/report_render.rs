use super::*;

pub(super) fn render_case_section(
    case: &BenchmarkCase,
    result: &ResearchBenchmarkCaseResult,
    scorecard: &BenchmarkCaseScorecard,
    artifact_paths: &CaseArtifactPaths,
    replay_before: Option<&ReplayBeforeState>,
) -> String {
    format!(
        "\n## {title}\n\n- Category: {category}\n- Case file: {filename}\n- Prompt: {prompt}\n- Task status: {status}\n- Quality status: {quality_status}\n- Quality last failure: {quality_last_failure}\n- Replay before: {replay_before}\n- Structured overall score: {overall_score:.2}/5.00\n- Critical failure flags: {critical_failure_count}\n- Structured visibility: task={task_visibility}, quality={quality_visibility}, source_pack={source_pack_visibility}\n- Measurement kind: {measurement_kind}\n- Measurement caveat: {measurement_caveat}\n- Structured summary JSON: {summary_json}\n- Final output: {final_output}\n- Source diagnostics JSON: {diagnostics_json}\n- Controller artifacts JSON: {controller_json}\n- Resolved system prompt: {system_prompt}\n- Resolved user prompt: {user_prompt}\n\n### Rubric Dimensions\n{dimensions}\n\n### Critical Failure Flags\n{critical_flags}\n\n### Must-Pass Evidence Checks\n{checks}\n\n### Expected Failure Modes\n{failures}\n\n### Artifact And Diagnostic Availability\n- source diagnostics envelope: {source_diag_state}\n- controller artifacts envelope: {controller_state}\n- structured context packing diagnostics: {context_pack_state}\n- fixture-only pipeline evidence label: {fixture_label}\n- narrative state in controller artifacts: {narrative_state_present}\n- narrative metrics: timeline={narrative_timeline_event_count} sections={narrative_section_count} evidence_layers={narrative_evidence_layer_count} tensions={narrative_interpretive_tension_count} impacts={narrative_impact_count} reader_questions={narrative_reader_question_count} open_gaps={narrative_open_gap_count}\n- reader-quality metrics: present={reader_quality_present} argument_nodes={reader_argument_node_count} argument_edges={reader_argument_edge_count} narrative_plan={reader_narrative_plan_present} section_briefs={reader_section_brief_count} critique_present={reader_critique_present} critique_metrics={reader_critique_metric_count} critique_failed_metrics={reader_critique_failed_metric_count}\n- context-pack narrative diagnostics: present={context_pack_narrative_state_present} open_gaps={context_pack_narrative_open_gap_count}\n- context-pack reader-quality diagnostics: present={context_pack_reader_quality_present} section_briefs={context_pack_reader_section_brief_count} critique_metrics={context_pack_reader_critique_metric_count}\n",
        title = case.title,
        category = case.category,
        filename = case.filename,
        prompt = sanitize_public_artifact_text(&case.prompt),
        status = result.status,
        quality_status = result.quality_status.as_deref().unwrap_or("none"),
        quality_last_failure = result
            .quality_last_failure
            .as_deref()
            .map(sanitize_public_artifact_text)
            .unwrap_or_else(|| "none".to_string()),
        replay_before = replay_before
            .map(|before| format!(
                "status={} quality={} critical_flags={} last_failure={}",
                before.status,
                before.quality_status,
                before.critical_failure_count,
                sanitize_public_artifact_text(&before.quality_last_failure)
            ))
            .unwrap_or_else(|| "not applicable".to_string()),
        overall_score = scorecard.overall_score,
        critical_failure_count = scorecard
            .critical_flags
            .iter()
            .filter(|flag| flag.triggered)
            .count(),
        task_visibility = scorecard.visibility.task_status,
        quality_visibility = scorecard.visibility.quality_status,
        source_pack_visibility = scorecard.visibility.source_pack_status,
        measurement_kind = scorecard.measurement_kind,
        measurement_caveat = scorecard.evidence_caveat,
        summary_json = artifact_paths
            .summary_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        final_output = artifact_paths
            .final_output_path
            .as_ref()
            .map(|path| path.display().to_string())
                .unwrap_or_else(|| result
                    .error_message
                    .as_deref()
                    .map(sanitize_public_artifact_text)
                    .unwrap_or_else(|| "unavailable".to_string())),
        diagnostics_json = artifact_paths
            .diagnostics_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        controller_json = artifact_paths
            .controller_json_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        system_prompt = artifact_paths
            .resolved_system_prompt_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        user_prompt = artifact_paths
            .resolved_user_prompt_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        checks = case
            .must_pass_checks
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        failures = case
            .expected_failure_modes
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dimensions = scorecard
            .dimensions
            .iter()
            .map(|dimension| {
                format!(
                    "- {}: {}/5 [{}; {}]",
                    dimension.label, dimension.score, dimension.score_source, dimension.rationale
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        critical_flags = scorecard
            .critical_flags
            .iter()
            .map(|flag| {
                format!(
                    "- {}: {} ({})",
                    flag.label,
                    if flag.triggered { "triggered" } else { "clear" },
                    flag.rationale
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        source_diag_state = if result.research_source_diagnostics_json.is_some() {
            "present"
        } else {
            "missing"
        },
        controller_state = if result.research_controller_artifacts_json.is_some() {
            "present"
        } else {
            "missing"
        },
        context_pack_state = result
            .research_source_diagnostics_json
            .as_deref()
            .is_some_and(|json| json.contains("\"context_packing\""))
            .then_some("present")
            .unwrap_or("missing"),
        fixture_label = if scorecard.visibility.fixture_only_pipeline_evidence {
            PIPELINE_EVIDENCE_CAVEAT
        } else {
            "not applicable"
        },
        narrative_state_present = if scorecard.metrics.narrative_state_present {
            "present"
        } else {
            "missing"
        },
        narrative_timeline_event_count = scorecard.metrics.narrative_timeline_event_count,
        narrative_section_count = scorecard.metrics.narrative_section_count,
        narrative_evidence_layer_count = scorecard.metrics.narrative_evidence_layer_count,
        narrative_interpretive_tension_count =
            scorecard.metrics.narrative_interpretive_tension_count,
        narrative_impact_count = scorecard.metrics.narrative_impact_count,
        narrative_reader_question_count = scorecard.metrics.narrative_reader_question_count,
        narrative_open_gap_count = scorecard.metrics.narrative_open_gap_count,
        reader_quality_present = scorecard.metrics.reader_quality_present,
        reader_argument_node_count = scorecard.metrics.reader_argument_node_count,
        reader_argument_edge_count = scorecard.metrics.reader_argument_edge_count,
        reader_narrative_plan_present = scorecard.metrics.reader_narrative_plan_present,
        reader_section_brief_count = scorecard.metrics.reader_section_brief_count,
        reader_critique_present = scorecard.metrics.reader_critique_present,
        reader_critique_metric_count = scorecard.metrics.reader_critique_metric_count,
        reader_critique_failed_metric_count = scorecard.metrics.reader_critique_failed_metric_count,
        context_pack_narrative_state_present =
            scorecard.metrics.context_pack_narrative_state_present,
        context_pack_narrative_open_gap_count =
            scorecard.metrics.context_pack_narrative_open_gap_count,
        context_pack_reader_quality_present = scorecard.metrics.context_pack_reader_quality_present,
        context_pack_reader_section_brief_count =
            scorecard.metrics.context_pack_reader_section_brief_count,
        context_pack_reader_critique_metric_count =
            scorecard.metrics.context_pack_reader_critique_metric_count,
    )
}

pub(super) fn build_structured_run_report(
    args: &Args,
    mode: ResearchBenchmarkMode,
    timestamp: &str,
    engine_name: &str,
    model_name: &str,
    run_case_dir: &Path,
    executions: &[RunExecution],
) -> BenchmarkRunStructuredReport {
    let measurement = measurement_metadata(mode);
    let status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.result.status.clone()),
    );
    let quality_status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.scorecard.visibility.quality_status.clone()),
    );
    let source_pack_status_counts = count_by_key(
        executions
            .iter()
            .map(|execution| execution.scorecard.visibility.source_pack_status.clone()),
    );
    let overall_scores = executions
        .iter()
        .map(|execution| execution.scorecard.overall_score)
        .collect::<Vec<_>>();
    let critical_failure_case_ids = executions
        .iter()
        .filter(|execution| execution.scorecard.any_critical_failure)
        .map(|execution| execution.case.case_id.clone())
        .collect::<Vec<_>>();
    let quality_failed_case_ids = executions
        .iter()
        .filter(|execution| execution.scorecard.visibility.quality_status != "passed")
        .map(|execution| execution.case.case_id.clone())
        .collect::<Vec<_>>();
    BenchmarkRunStructuredReport {
        label: args.label.clone(),
        generated_at: timestamp.to_string(),
        mode: benchmark_mode_label(mode).to_string(),
        measurement_kind: measurement.measurement_kind.to_string(),
        evidence_caveat: measurement.evidence_caveat.to_string(),
        engine_name: public_metadata_label(engine_name),
        model_name: public_metadata_label(model_name),
        configured_search_providers: configured_search_providers(),
        research_intensity: args.research_intensity.clone(),
        quality_depth: args.quality_depth.clone(),
        max_iterations: args.max_iterations,
        cases_dir: args.cases_dir.display().to_string(),
        run_artifact_dir: run_case_dir.display().to_string(),
        summary: BenchmarkRunSummary {
            case_count: executions.len(),
            completed_case_count: executions
                .iter()
                .filter(|execution| execution.result.status == "completed")
                .count(),
            failed_case_count: executions
                .iter()
                .filter(|execution| execution.result.status != "completed")
                .count(),
            quality_passed_case_count: executions
                .iter()
                .filter(|execution| execution.scorecard.visibility.quality_status == "passed")
                .count(),
            critical_failure_case_count: critical_failure_case_ids.len(),
            overall_average_score: average_f64(&overall_scores),
            overall_min_score: min_f64(&overall_scores),
            overall_max_score: max_f64(&overall_scores),
            status_counts,
            quality_status_counts,
            source_pack_status_counts,
            critical_failure_case_ids,
            quality_failed_case_ids,
        },
        dimension_aggregates: build_dimension_aggregates(executions),
        cases: executions
            .iter()
            .map(|execution| {
                let mut scorecard = execution.scorecard.clone();
                scorecard.replay_before = execution.replay_before.clone().map(|mut before| {
                    before.quality_last_failure =
                        sanitize_public_artifact_text(&before.quality_last_failure);
                    before
                });
                scorecard
            })
            .collect(),
    }
}

pub(super) fn build_dimension_aggregates(
    executions: &[RunExecution],
) -> Vec<BenchmarkDimensionAggregate> {
    RUBRIC_DIMENSIONS
        .iter()
        .map(|spec| {
            let scores = executions
                .iter()
                .filter_map(|execution| {
                    execution
                        .scorecard
                        .dimensions
                        .iter()
                        .find(|dimension| dimension.key == spec.key)
                        .map(|dimension| dimension.score)
                })
                .collect::<Vec<_>>();
            BenchmarkDimensionAggregate {
                key: spec.key.to_string(),
                label: spec.label.to_string(),
                average_score: average_u8(&scores),
                min_score: scores.iter().copied().min().unwrap_or(0),
                max_score: scores.iter().copied().max().unwrap_or(0),
                case_count: scores.len(),
            }
        })
        .collect()
}

pub(super) fn render_markdown_aggregate_summary(summary: &BenchmarkRunSummary) -> String {
    [
        format!("- Cases: {}", summary.case_count),
        format!("- Completed: {}", summary.completed_case_count),
        format!("- Quality passed: {}", summary.quality_passed_case_count),
        format!(
            "- Critical failure cases: {} ({})",
            summary.critical_failure_case_count,
            if summary.critical_failure_case_ids.is_empty() {
                "none".to_string()
            } else {
                summary.critical_failure_case_ids.join(", ")
            }
        ),
        format!(
            "- Overall score range: {:.2} to {:.2} (avg {:.2})",
            summary.overall_min_score, summary.overall_max_score, summary.overall_average_score
        ),
        format!(
            "- Source pack statuses: {}",
            render_count_map(&summary.source_pack_status_counts)
        ),
        format!(
            "- Quality statuses: {}",
            render_count_map(&summary.quality_status_counts)
        ),
    ]
    .join("\n")
}

pub(super) fn render_markdown_dimension_summary(
    aggregates: &[BenchmarkDimensionAggregate],
) -> String {
    let mut lines = vec![
        "| Dimension | Avg | Min | Max | Cases |".to_string(),
        "| --- | --- | --- | --- | --- |".to_string(),
    ];
    lines.extend(aggregates.iter().map(|aggregate| {
        format!(
            "| {} | {:.2} | {} | {} | {} |",
            aggregate.label,
            aggregate.average_score,
            aggregate.min_score,
            aggregate.max_score,
            aggregate.case_count
        )
    }));
    lines.join("\n")
}

pub(super) fn render_case_scores_csv(report: &BenchmarkRunStructuredReport) -> String {
    let mut header = vec![
        "label",
        "case_id",
        "title",
        "category",
        "mode",
        "overall_score",
        "any_critical_failure",
        "task_status",
        "quality_status",
        "source_pack_status",
        "advisory_model_self_score",
        "historical_overlay_trigger_count",
        "historical_lens_coverage_count",
        "replay_before_status",
        "replay_before_quality_status",
        "replay_before_critical_failure_count",
        "replay_before_quality_last_failure",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    header.extend(RUBRIC_DIMENSIONS.iter().map(|spec| spec.key.to_string()));
    header.extend(
        [
            "reader_quality_present",
            "reader_argument_node_count",
            "reader_argument_edge_count",
            "reader_narrative_plan_present",
            "reader_section_brief_count",
            "reader_critique_present",
            "reader_critique_metric_count",
            "reader_critique_failed_metric_count",
            "context_pack_reader_quality_present",
            "context_pack_reader_section_brief_count",
            "context_pack_reader_critique_metric_count",
        ]
        .into_iter()
        .map(String::from),
    );
    let mut lines = vec![header.join(",")];
    for case in &report.cases {
        let mut row = vec![
            csv_escape(&report.label),
            csv_escape(&case.case_id),
            csv_escape(&case.title),
            csv_escape(&case.category),
            csv_escape(&case.mode),
            format!("{:.2}", case.overall_score),
            case.any_critical_failure.to_string(),
            csv_escape(&case.visibility.task_status),
            csv_escape(&case.visibility.quality_status),
            csv_escape(&case.visibility.source_pack_status),
            case.advisory_model_self_score
                .map(|value| format!("{value:.2}"))
                .unwrap_or_default(),
            case.historical_overlay_trigger_count.to_string(),
            case.metrics.historical_lens_coverage_count.to_string(),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.status.as_str())
                    .unwrap_or(""),
            ),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.quality_status.as_str())
                    .unwrap_or(""),
            ),
            case.replay_before
                .as_ref()
                .map(|before| before.critical_failure_count.to_string())
                .unwrap_or_default(),
            csv_escape(
                case.replay_before
                    .as_ref()
                    .map(|before| before.quality_last_failure.as_str())
                    .unwrap_or(""),
            ),
        ];
        row.extend(RUBRIC_DIMENSIONS.iter().map(|spec| {
            case.dimensions
                .iter()
                .find(|dimension| dimension.key == spec.key)
                .map(|dimension| dimension.score.to_string())
                .unwrap_or_default()
        }));
        row.extend([
            case.metrics.reader_quality_present.to_string(),
            case.metrics.reader_argument_node_count.to_string(),
            case.metrics.reader_argument_edge_count.to_string(),
            case.metrics.reader_narrative_plan_present.to_string(),
            case.metrics.reader_section_brief_count.to_string(),
            case.metrics.reader_critique_present.to_string(),
            case.metrics.reader_critique_metric_count.to_string(),
            case.metrics.reader_critique_failed_metric_count.to_string(),
            case.metrics.context_pack_reader_quality_present.to_string(),
            case.metrics
                .context_pack_reader_section_brief_count
                .to_string(),
            case.metrics
                .context_pack_reader_critique_metric_count
                .to_string(),
        ]);
        lines.push(row.join(","));
    }
    lines.join("\n")
}

pub(super) fn render_dimension_scores_ndjson(
    report: &BenchmarkRunStructuredReport,
) -> Result<String, serde_json::Error> {
    let mut lines = Vec::new();
    for case in &report.cases {
        for dimension in &case.dimensions {
            lines.push(serde_json::to_string(&json!({
                "label": report.label,
                "case_id": case.case_id,
                "title": case.title,
                "category": case.category,
                "mode": case.mode,
                "measurement_kind": case.measurement_kind,
                "dimension_key": dimension.key,
                "dimension_label": dimension.label,
                "score": dimension.score,
                "max_score": dimension.max_score,
                "assessor": dimension.assessor,
                "score_source": dimension.score_source,
                "visibility": dimension.visibility,
                "overall_score": case.overall_score,
                "any_critical_failure": case.any_critical_failure,
                "historical_overlay_trigger_count": case.historical_overlay_trigger_count,
                "historical_lens_coverage_count": case.metrics.historical_lens_coverage_count,
                "quality_status": case.visibility.quality_status,
                "source_pack_status": case.visibility.source_pack_status,
                "fixture_only_pipeline_evidence": case.visibility.fixture_only_pipeline_evidence,
                "replay_before": case.replay_before,
            }))?);
        }
    }
    Ok(lines.join("\n"))
}

pub(super) fn benchmark_mode_label(mode: ResearchBenchmarkMode) -> &'static str {
    match mode {
        ResearchBenchmarkMode::Fixture => "fixture",
        ResearchBenchmarkMode::Live => "live",
        ResearchBenchmarkMode::Replay => "replay",
    }
}

pub(super) fn measurement_metadata(mode: ResearchBenchmarkMode) -> MeasurementMetadata {
    match mode {
        ResearchBenchmarkMode::Fixture => MeasurementMetadata {
            measurement_kind: PIPELINE_EVIDENCE_MEASUREMENT,
            evidence_caveat: PIPELINE_EVIDENCE_CAVEAT,
            report_note_label: "Fixture note",
        },
        ResearchBenchmarkMode::Live => MeasurementMetadata {
            measurement_kind: LIVE_QUALITY_MEASUREMENT,
            evidence_caveat: LIVE_QUALITY_CAVEAT,
            report_note_label: "Live note",
        },
        ResearchBenchmarkMode::Replay => MeasurementMetadata {
            measurement_kind: REPLAY_QUALITY_MEASUREMENT,
            evidence_caveat: REPLAY_QUALITY_CAVEAT,
            report_note_label: "Replay note",
        },
    }
}

pub(super) fn count_by_key<I>(values: I) -> BTreeMap<String, usize>
where
    I: Iterator<Item = String>,
{
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

pub(super) fn render_count_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn average_u8(scores: &[u8]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().map(|score| *score as f64).sum::<f64>() / scores.len() as f64
    }
}

pub(super) fn average_f64(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

pub(super) fn min_f64(scores: &[f64]) -> f64 {
    scores.iter().copied().reduce(f64::min).unwrap_or(0.0)
}

pub(super) fn max_f64(scores: &[f64]) -> f64 {
    scores.iter().copied().reduce(f64::max).unwrap_or(0.0)
}

pub(super) fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
