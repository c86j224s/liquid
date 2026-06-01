#[allow(unused_imports)]
pub(super) use liquid_research_classic::{
    attention_like_concept_subject, candidate_queries_from_failure, conflict_debt_repair_guidance,
    contains_ascii_repair_token_with_boundaries, final_answer_depth_repair_guidance,
    historical_development_repair_guidance, historical_event_card_prompt_wording,
    historical_event_card_repair_guidance, historical_event_card_repair_should_trigger,
    historical_narrative_artifact_repair_guidance,
    historical_narrative_artifact_repair_should_trigger, normalize_absolute_public_support_url,
    policy_or_regulatory_like_repair_subject, render_repair_claim_context_block,
    technology_concept_like_repair_subject, technology_implementation_like_repair_subject,
    technology_like_repair_subject, technology_repair_guidance, technology_repair_marker_present,
    ClassicRepairSearchHint,
};

use super::*;

pub(super) fn build_quality_repair_prompt(
    original_user_prompt: &str,
    failure_message: &str,
    next_iteration: i64,
    max_iterations: i64,
    artifacts: Option<&ResearchControllerArtifacts>,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    repair_search_hints: &[RepairSearchHint],
) -> String {
    let classic_hints = adapt_repair_search_hints(repair_search_hints);
    liquid_research_classic::build_quality_repair_prompt(
        original_user_prompt,
        failure_message,
        next_iteration,
        max_iterations,
        artifacts,
        diagnostics,
        &classic_hints,
    )
}

pub(super) fn render_repair_search_hints_block(hints: &[RepairSearchHint]) -> String {
    let classic_hints = adapt_repair_search_hints(hints);
    liquid_research_classic::render_repair_search_hints_block(&classic_hints)
}

fn adapt_repair_search_hints(hints: &[RepairSearchHint]) -> Vec<ClassicRepairSearchHint> {
    hints
        .iter()
        .map(|hint| ClassicRepairSearchHint {
            query: hint.query.clone(),
            provider: hint.provider.clone(),
            title: hint.title.clone(),
            url: hint.url.clone(),
            source_class: hint.source_class.clone(),
            source_quality: hint.source_quality.clone(),
            snippet: hint.snippet.clone(),
        })
        .collect()
}

pub(super) async fn claim_next_ai_task(state: &AppState, lane: QueueLane) -> Option<TaskInfo> {
    loop {
        let task = match sqlx::query_as::<_, TaskInfo>(claimable_task_sql(lane))
            .fetch_optional(&state.db)
            .await
        {
            Ok(Some(task)) => task,
            Ok(None) | Err(_) => return None,
        };

        let model_input = task.model.clone().unwrap_or_default();
        let file_prefix = task.file_prefix.as_deref().unwrap_or_default();
        if model_input.is_empty() && file_prefix != "[Scrape]" {
            let updated = sqlx::query("UPDATE tasks SET status = 'failed', error_message = ? WHERE id = ? AND status = 'queued'")
                .bind("Missing model for queued task")
                .bind(task.id)
                .execute(&state.db).await
                .map(|r| r.rows_affected())
                .unwrap_or(0);
            if updated > 0 {
                let _ = state.tx.send(TaskUpdateEvent {
                    id: task.id,
                    status: "failed".to_string(),
                    original_name: task.original_name.clone(),
                    quality_current_iteration: task.quality_current_iteration,
                    quality_max_iterations: task.quality_max_iterations,
                    quality_status: task.quality_status.clone(),
                    research_controller_stage: task.research_controller_stage.clone(),
                    research_controller_iteration: task.research_controller_iteration,
                    research_controller_max_iterations: task.research_controller_max_iterations,
                });
            }
            continue;
        }

        let target_status = target_status_for_prefix(file_prefix);
        let updated = sqlx::query("UPDATE tasks SET status = ? WHERE id = ? AND status = 'queued'")
            .bind(target_status)
            .bind(task.id)
            .execute(&state.db)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0);
        if updated == 0 {
            continue;
        }

        let _ = state.tx.send(TaskUpdateEvent {
            id: task.id,
            status: target_status.to_string(),
            original_name: task.original_name.clone(),
            quality_current_iteration: task.quality_current_iteration,
            quality_max_iterations: task.quality_max_iterations,
            quality_status: task.quality_status.clone(),
            research_controller_stage: task.research_controller_stage.clone(),
            research_controller_iteration: task.research_controller_iteration,
            research_controller_max_iterations: task.research_controller_max_iterations,
        });
        return Some(task);
    }
}

pub(super) fn claimable_task_sql(lane: QueueLane) -> &'static str {
    match lane {
        QueueLane::Local => {
            "SELECT * FROM tasks \
             WHERE deleted_at IS NULL AND status = 'queued' AND (\
                engine_kind IN ('pi_ollama', 'ollama_legacy') \
                OR COALESCE(model, '') LIKE 'pi:%' \
                OR (COALESCE(model, '') != '' AND COALESCE(model, '') NOT LIKE 'cli:%')\
             ) \
             ORDER BY created_at ASC, id ASC LIMIT 1"
        }
        QueueLane::Cloud => {
            "SELECT * FROM tasks \
             WHERE deleted_at IS NULL AND status = 'queued' AND NOT (\
                COALESCE(engine_kind, '') IN ('pi_ollama', 'ollama_legacy') \
                OR COALESCE(model, '') LIKE 'pi:%' \
                OR (COALESCE(model, '') != '' AND COALESCE(model, '') NOT LIKE 'cli:%')\
             ) \
             ORDER BY created_at ASC, id ASC LIMIT 1"
        }
    }
}

pub(super) fn target_status_for_prefix(file_prefix: &str) -> &'static str {
    lifecycle_target_status(file_prefix)
}

pub(super) fn normalized_quality_max_iterations(
    file_prefix: &str,
    requested: Option<i64>,
    research_intensity: Option<&str>,
) -> i64 {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return 1;
    }
    requested
        .unwrap_or_else(|| {
            if research_intensity == Some("high") {
                2
            } else {
                1
            }
        })
        .clamp(1, 15)
}

pub(super) fn research_controller_max_iterations(
    file_prefix: &str,
    quality_max_iterations: i64,
) -> Option<i64> {
    if matches!(file_prefix, "[Research]" | "[AI-Research]") {
        Some(quality_max_iterations)
    } else {
        None
    }
}

pub(super) fn normalized_quality_depth(
    file_prefix: &str,
    requested: Option<&str>,
    research_intensity: Option<&str>,
) -> String {
    if !matches!(file_prefix, "[Research]" | "[AI-Research]") {
        return "off".to_string();
    }
    match requested {
        Some("light") | Some("standard") | Some("strict") => requested.unwrap().to_string(),
        _ if research_intensity == Some("high") => "strict".to_string(),
        _ => "standard".to_string(),
    }
}

pub(super) fn collect_repair_search_queries(
    artifacts: Option<&ResearchControllerArtifacts>,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> Vec<String> {
    let prior_queries = diagnostics
        .and_then(|diagnostics| diagnostics.source_pack.as_ref())
        .map(|report| {
            report
                .queries
                .iter()
                .map(|query| normalize_repair_search_query_key(&query.query))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let mut queries = Vec::new();
    let mut seen = HashSet::new();
    let mut debt_seen = prior_queries.clone();
    let mut coverage_seen = HashSet::new();
    if let Some(artifacts) = artifacts {
        for debt in artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
        {
            for query in &debt.candidate_queries {
                push_repair_search_query(&mut queries, &mut debt_seen, &mut seen, query);
                if queries.len() >= MAX_REPAIR_SEARCH_QUERIES {
                    return queries;
                }
            }
        }
    }
    if let Some(diagnostics) = diagnostics.and_then(|diagnostics| diagnostics.source_pack.as_ref())
    {
        for miss in &diagnostics.coverage_misses {
            let preferred_query = derive_coverage_miss_repair_query(
                diagnostics.subject.as_deref(),
                miss,
                &prior_queries,
                &seen,
            )
            .unwrap_or_else(|| miss.query.clone());
            push_repair_search_query(
                &mut queries,
                &mut coverage_seen,
                &mut seen,
                &preferred_query,
            );
            if queries.len() >= MAX_REPAIR_SEARCH_QUERIES {
                break;
            }
        }
    }
    queries
}

pub(super) fn push_repair_search_query(
    queries: &mut Vec<String>,
    dedupe_scope: &mut HashSet<String>,
    emitted: &mut HashSet<String>,
    raw: &str,
) {
    let Some(query) = sanitize_repair_search_query(raw) else {
        return;
    };
    let key = normalize_repair_search_query_key(&query);
    if !dedupe_scope.insert(key.clone()) {
        return;
    }
    if emitted.insert(key) {
        queries.push(query);
    }
}

pub(super) fn derive_coverage_miss_repair_query(
    subject: Option<&str>,
    miss: &ResearchSourceCoverageMiss,
    prior_queries: &HashSet<String>,
    emitted: &HashSet<String>,
) -> Option<String> {
    let subject = sanitize_repair_search_query(subject?)?;
    let mut parts = vec![subject];
    if let Some(host) = miss.expected_host.as_deref() {
        let normalized_host = host.trim().trim_start_matches("www.").trim();
        if !normalized_host.is_empty() {
            parts.push(normalized_host.to_string());
        }
    }
    if let Some(source_class) = miss.expected_source_class.as_deref() {
        let source_class_hint = source_class.replace('_', " ").trim().to_string();
        if !source_class_hint.is_empty() {
            parts.push(source_class_hint);
        }
    }
    let derived = sanitize_repair_search_query(&parts.join(" "))?;
    let derived_key = normalize_repair_search_query_key(&derived);
    if emitted.contains(&derived_key) {
        return None;
    }
    if !prior_queries.contains(&derived_key)
        && derived_key != normalize_repair_search_query_key(&miss.query)
    {
        return Some(derived);
    }
    if !prior_queries.contains(&normalize_repair_search_query_key(&miss.query)) {
        return Some(miss.query.clone());
    }
    Some(derived)
}

pub(super) fn sanitize_repair_search_query(raw: &str) -> Option<String> {
    let normalized = raw
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }
    let compacted = compact_repair_topic_query(trimmed).unwrap_or_else(|| trimmed.to_string());
    let final_query = compacted
        .split_whitespace()
        .take(16)
        .collect::<Vec<_>>()
        .join(" ");
    let truncated = if final_query.chars().count() > 120 {
        final_query.chars().take(120).collect::<String>()
    } else {
        final_query
    };
    let sanitized = truncated
        .trim()
        .trim_matches(|ch: char| ch == ':' || ch == '-' || ch == ',' || ch == '.')
        .to_string();
    (!sanitized.is_empty()).then_some(sanitized)
}

pub(super) fn normalize_repair_search_query_key(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(super) fn collect_repair_search_known_urls(
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> HashSet<String> {
    let mut known = HashSet::new();
    if let Some(source_pack) = diagnostics.and_then(|diagnostics| diagnostics.source_pack.as_ref())
    {
        for candidate in &source_pack.adopted_candidates {
            let url = candidate.url.trim();
            if !url.is_empty() {
                known.insert(url.to_string());
            }
        }
    }
    if let Some(diagnostics) = diagnostics {
        for scrape in &diagnostics.scrapes {
            for url in [
                Some(scrape.original_url.as_str()),
                Some(scrape.normalized_url.as_str()),
                scrape.final_url.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                let trimmed = url.trim();
                if !trimmed.is_empty() {
                    known.insert(trimmed.to_string());
                }
            }
        }
    }
    known
}

pub(super) fn refresh_pending_repair_hint_urls(
    pending: &mut HashSet<String>,
    repair_search_hints: &[RepairSearchHint],
    independently_acquired_urls: &HashSet<String>,
) {
    pending.retain(|url| !independently_acquired_urls.contains(url));
    for url in repair_search_hints
        .iter()
        .map(|hint| hint.url.trim())
        .filter(|url| !url.is_empty())
    {
        if !independently_acquired_urls.contains(url) {
            pending.insert(url.to_string());
        }
    }
}

pub(super) fn repair_search_subject(
    original_user_prompt: &str,
    artifacts: Option<&ResearchControllerArtifacts>,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
) -> Option<String> {
    diagnostics
        .and_then(|diagnostics| diagnostics.subject.as_deref())
        .or_else(|| {
            diagnostics
                .and_then(|diagnostics| diagnostics.source_pack.as_ref())
                .and_then(|pack| pack.subject.as_deref())
        })
        .and_then(sanitize_repair_search_query)
        .or_else(|| {
            artifacts
                .and_then(|artifacts| artifacts.research_debt.first())
                .and_then(|debt| debt.candidate_queries.first())
                .and_then(|query| sanitize_repair_search_query(query))
        })
        .or_else(|| compact_repair_topic_query(original_user_prompt))
}

pub(super) fn compact_repair_topic_query(topic: &str) -> Option<String> {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = trimmed
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = normalized.to_ascii_lowercase();
    let boilerplate_markers = [
        "write a korean reader-facing research report",
        "write a reader-facing research report",
        "reader-facing research report",
        "reader-facing",
        "research report",
        "the output should be useful",
        "someone planning",
    ];
    let stripped = boilerplate_markers
        .iter()
        .fold(normalized.clone(), |current, marker| {
            if current.to_ascii_lowercase().contains(marker) {
                current.replace(marker, " ").replace(
                    &marker
                        .chars()
                        .zip(marker.chars())
                        .map(|(_, ch)| ch.to_ascii_uppercase())
                        .collect::<String>(),
                    " ",
                )
            } else {
                current
            }
        });

    let prefers_keyword_compaction = lower.contains("reader-facing")
        || lower.contains("research report")
        || lower.contains("the output should be useful")
        || stripped.chars().count() > 120;
    if !prefers_keyword_compaction {
        return Some(stripped);
    }

    let stopwords = [
        "a",
        "about",
        "actual",
        "actually",
        "afterward",
        "and",
        "around",
        "be",
        "but",
        "candidates",
        "compare",
        "comfortable",
        "cover",
        "deciding",
        "difference",
        "distinguish",
        "facing",
        "for",
        "from",
        "good",
        "include",
        "just",
        "korean",
        "list",
        "mark",
        "not",
        "of",
        "one",
        "output",
        "planning",
        "practical",
        "reader",
        "reader-facing",
        "read",
        "report",
        "research",
        "route",
        "someone",
        "source",
        "the",
        "their",
        "them",
        "there",
        "they",
        "to",
        "useful",
        "where",
        "with",
        "workout",
        "write",
    ];
    let mut keywords = Vec::new();
    let mut seen = HashSet::new();
    let compact = stripped
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || !ch.is_ascii() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();
    for token in compact.split_whitespace() {
        let lower_token = token.to_ascii_lowercase();
        let is_stopword = stopwords.contains(&lower_token.as_str());
        let is_short_ascii = token.is_ascii() && lower_token.len() <= 2;
        if is_stopword || is_short_ascii {
            continue;
        }
        let normalized_token =
            token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch.is_ascii());
        if normalized_token.is_empty() {
            continue;
        }
        let owned = normalized_token.to_string();
        if seen.insert(owned.to_ascii_lowercase()) {
            keywords.push(owned);
        }
        if keywords.len() >= 10 {
            break;
        }
    }

    if keywords.is_empty() {
        Some(
            stripped
                .chars()
                .take(120)
                .collect::<String>()
                .trim()
                .to_string(),
        )
        .filter(|value| !value.is_empty())
    } else {
        Some(keywords.join(" "))
    }
}
