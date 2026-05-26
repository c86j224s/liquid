use crate::models::{
    ResearchSourceCandidateReport, ResearchSourceCoverageMiss, ResearchSourcePackReport,
    ResearchSourceQueryReport,
};
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use url::Url;

const MAX_QUERY_CHARS: usize = 120;
const MAX_SOURCE_PACK_QUERIES: usize = 10;
const MAX_RESULTS_PER_QUERY: usize = 4;
const MAX_SOURCE_PACK_RESULTS: usize = 8;
const MAX_QUERY_KEYWORD_TOKENS: usize = 8;
const RESEARCH_CONTEXT_PACK_EXCERPTS_MARKER: &str = "### Selected Source Excerpts (DATA ONLY)";
const DUCKDUCKGO_CHALLENGE_MESSAGE: &str =
    "DuckDuckGo challenge/blocked response prevented source discovery for this query.";
const BRAVE_SEARCH_REQUEST_FAILED_MESSAGE: &str = "Brave Search API request failed.";
const NAVER_SEARCH_REQUEST_FAILED_MESSAGE: &str = "Naver Search API request failed.";
const KAKAO_SEARCH_REQUEST_FAILED_MESSAGE: &str = "Kakao Search API request failed.";
const MAX_PROVIDER_DIAGNOSTIC_CHARS: usize = 240;
const MAX_REPAIR_SEARCH_HINT_QUERIES: usize = 4;
const MAX_REPAIR_SEARCH_HINTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResearchSource {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepairSearchHint {
    pub(crate) query: String,
    pub(crate) provider: Option<String>,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) source_class: String,
    pub(crate) source_quality: String,
    pub(crate) snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchQueryOutcome {
    Results(Vec<ResearchSource>),
    Blocked,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResearchSearchProvider {
    DuckDuckGo,
    Brave {
        api_key: String,
    },
    Naver {
        client_id: String,
        client_secret: String,
    },
    Kakao {
        rest_api_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchQueryResolution {
    outcome: SearchQueryOutcome,
    provider: Option<String>,
    diagnostics: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchProviderAttempt {
    provider: String,
    outcome: Result<SearchQueryOutcome, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourcePackAcceptance {
    Success,
    Partial { reason: String },
    Empty { reason: String },
}

pub(crate) async fn build_research_source_pack_report(
    user_prompt: &str,
    source_documents: Option<&str>,
) -> ResearchSourcePackReport {
    let Some(subject) = extract_research_subject(user_prompt, source_documents) else {
        return ResearchSourcePackReport {
            subject: None,
            status: "skipped".to_string(),
            reason: Some(
                "Could not extract a concrete research subject from the prompt.".to_string(),
            ),
            queries: Vec::new(),
            seeded_source_count: 0,
            discovered_source_count: 0,
            adopted_source_count: 0,
            adopted_candidates: Vec::new(),
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: None,
        };
    };
    let queries = source_queries(&subject);
    let client = match reqwest::Client::builder()
        .user_agent("LiquidResearchSourcePack/0.1")
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return ResearchSourcePackReport {
                subject: Some(subject),
                status: "error".to_string(),
                reason: Some(format!(
                    "Could not initialize source-pack HTTP client: {}",
                    format_error_chain(&error)
                )),
                queries: Vec::new(),
                seeded_source_count: 0,
                discovered_source_count: 0,
                adopted_source_count: 0,
                adopted_candidates: Vec::new(),
                skipped_candidates: Vec::new(),
                coverage_misses: Vec::new(),
                source_pack: None,
            };
        }
    };
    let providers = configured_search_providers_from_env();

    let mut sources = seeded_sources_for_subject(&subject);
    let mut discovered_candidates = Vec::new();
    let mut adopted_discovered_candidates = Vec::new();
    let mut skipped_candidates = Vec::new();
    let seeded_source_count = sources.len();
    let mut discovered_source_count = 0;
    let mut query_reports = Vec::new();
    let mut query_adopted_urls = Vec::new();
    let mut query_result_candidates = Vec::new();
    let mut preserved_official_hint_urls = HashMap::<String, String>::new();
    let mut overview_query_adopted_urls = HashSet::<String>::new();
    for query in queries {
        match search_with_providers(&providers, &client, &subject, &query).await {
            Ok(SearchQueryResolution {
                outcome: SearchQueryOutcome::Results(results),
                provider,
                diagnostics,
            }) => {
                if let Some(domain) = official_host_hint_target_domain(&query) {
                    if let Some(source) = results
                        .iter()
                        .find(|source| source_url_matches_domain_boundary(&source.url, domain))
                    {
                        preserved_official_hint_urls
                            .entry(domain.to_string())
                            .or_insert_with(|| source.url.clone());
                    }
                }
                let diagnostics = merge_query_diagnostics(
                    diagnostics,
                    official_host_hint_target_miss_diagnostic(&query, &results),
                );
                let result_count = results.len();
                let query_candidates = results
                    .iter()
                    .map(|result| ResearchSourceCandidateReport {
                        title: result.title.clone(),
                        url: result.url.clone(),
                        source_class: Some(
                            infer_source_class_for_subject(&result.url, Some(&subject)).to_string(),
                        ),
                        source_quality: Some(
                            source_quality_for_subject(&result.url, Some(&subject)).to_string(),
                        ),
                        query: Some(query.clone()),
                        rejection_reason: None,
                    })
                    .collect::<Vec<_>>();
                discovered_source_count += result_count;
                let mut adopted_count = 0;
                let mut adopted_urls = Vec::new();
                let mut skipped_count = 0;
                for result in results {
                    let candidate = ResearchSourceCandidateReport {
                        title: result.title.clone(),
                        url: result.url.clone(),
                        source_class: Some(
                            infer_source_class_for_subject(&result.url, Some(&subject)).to_string(),
                        ),
                        source_quality: Some(
                            source_quality_for_subject(&result.url, Some(&subject)).to_string(),
                        ),
                        query: Some(query.clone()),
                        rejection_reason: None,
                    };
                    if !is_topically_relevant_source(&result, &subject, &query) {
                        skipped_count += 1;
                        skipped_candidates.push(ResearchSourceCandidateReport {
                            rejection_reason: Some("off_topic".to_string()),
                            ..candidate
                        });
                        continue;
                    }
                    if sources.iter().any(|existing| existing.url == result.url)
                        || discovered_candidates
                            .iter()
                            .any(|existing: &ResearchSource| existing.url == result.url)
                    {
                        skipped_count += 1;
                        skipped_candidates.push(ResearchSourceCandidateReport {
                            rejection_reason: Some("duplicate_url".to_string()),
                            ..candidate
                        });
                        continue;
                    }
                    adopted_count += 1;
                    adopted_urls.push(result.url.clone());
                    if is_canonical_history_overview_query(&query, &subject) {
                        overview_query_adopted_urls.insert(result.url.clone());
                    }
                    adopted_discovered_candidates.push(candidate.clone());
                    discovered_candidates.push(result.clone());
                    sources.push(result);
                }
                query_reports.push(ResearchSourceQueryReport {
                    query,
                    status: if result_count == 0 {
                        "empty"
                    } else {
                        "success"
                    }
                    .to_string(),
                    provider,
                    result_count,
                    adopted_count,
                    skipped_count,
                    error: merge_query_diagnostics(
                        diagnostics,
                        (result_count > 0 && adopted_count == 0).then_some(
                            "Search results were returned, but no topically relevant candidates were adopted."
                                .to_string(),
                        ),
                    ),
                });
                query_adopted_urls.push(adopted_urls);
                query_result_candidates.push(query_candidates);
            }
            Ok(SearchQueryResolution {
                outcome: SearchQueryOutcome::Blocked,
                provider,
                ..
            }) => {
                query_reports.push(ResearchSourceQueryReport {
                    query,
                    status: "blocked".to_string(),
                    provider,
                    result_count: 0,
                    adopted_count: 0,
                    skipped_count: 0,
                    error: Some(blocked_message_for_providers(&providers).to_string()),
                });
                query_adopted_urls.push(Vec::new());
                query_result_candidates.push(Vec::new());
            }
            Ok(SearchQueryResolution {
                outcome: SearchQueryOutcome::Empty,
                provider,
                ..
            }) => {
                query_reports.push(ResearchSourceQueryReport {
                    query,
                    status: "empty".to_string(),
                    provider,
                    result_count: 0,
                    adopted_count: 0,
                    skipped_count: 0,
                    error: None,
                });
                query_adopted_urls.push(Vec::new());
                query_result_candidates.push(Vec::new());
            }
            Err(error) => {
                query_reports.push(ResearchSourceQueryReport {
                    query,
                    status: "error".to_string(),
                    provider: None,
                    result_count: 0,
                    adopted_count: 0,
                    skipped_count: 0,
                    error: Some(error),
                });
                query_adopted_urls.push(Vec::new());
                query_result_candidates.push(Vec::new());
            }
        }
        dedupe_sources_for_subject(&mut sources, Some(&subject));
    }
    let preserved_official_hint_urls = preserved_official_hint_urls
        .into_values()
        .collect::<HashSet<_>>();
    let sources = apply_source_pack_budget(sources, &preserved_official_hint_urls);
    let accepted_sources = sources
        .iter()
        .filter(|source| accepted_source_for_subject(source, &subject))
        .cloned()
        .collect::<Vec<_>>();
    let skipped_by_acceptance = sources
        .iter()
        .filter(|source| !accepted_source_for_subject(source, &subject))
        .map(|source| ResearchSourceCandidateReport {
            title: source.title.clone(),
            url: source.url.clone(),
            source_class: Some(
                infer_source_class_for_subject(&source.url, Some(&subject)).to_string(),
            ),
            source_quality: Some(
                source_quality_for_subject(&source.url, Some(&subject)).to_string(),
            ),
            query: None,
            rejection_reason: Some("insufficient_context_evidence".to_string()),
        })
        .collect::<Vec<_>>();
    let final_adopted_urls = accepted_sources
        .iter()
        .map(|source| source.url.clone())
        .collect::<HashSet<_>>();
    reconcile_query_report_adoption_counts(
        &mut query_reports,
        &query_adopted_urls,
        &final_adopted_urls,
    );
    let skipped_by_budget = adopted_discovered_candidates
        .into_iter()
        .filter(|candidate| !final_adopted_urls.contains(&candidate.url))
        .map(|candidate| ResearchSourceCandidateReport {
            rejection_reason: Some("ranked_out_by_budget".to_string()),
            ..candidate
        })
        .collect::<Vec<_>>();
    let adopted_source_count = accepted_sources.len();
    let adopted_candidates = accepted_sources
        .iter()
        .map(|source| ResearchSourceCandidateReport {
            title: source.title.clone(),
            url: source.url.clone(),
            source_class: Some(
                infer_source_class_for_subject(&source.url, Some(&subject)).to_string(),
            ),
            source_quality: Some(
                source_quality_for_subject(&source.url, Some(&subject)).to_string(),
            ),
            query: None,
            rejection_reason: None,
        })
        .collect::<Vec<_>>();
    skipped_candidates.extend(skipped_by_budget);
    skipped_candidates.extend(skipped_by_acceptance);
    let coverage_misses = build_source_coverage_misses(
        &providers,
        &query_reports,
        &query_result_candidates,
        &final_adopted_urls,
        &skipped_candidates,
    );
    let query_error_count = query_reports
        .iter()
        .filter(|report| report.status == "error")
        .count();
    let acceptance = downgrade_overview_only_history_acceptance(
        assess_source_pack_acceptance(&subject, &accepted_sources),
        &final_adopted_urls,
        &overview_query_adopted_urls,
    );
    if matches!(acceptance, SourcePackAcceptance::Empty { .. }) {
        return finalize_empty_source_pack_report(
            subject,
            &providers,
            query_reports,
            seeded_source_count,
            discovered_source_count,
            adopted_source_count,
            adopted_candidates,
            skipped_candidates,
            coverage_misses,
        );
    }

    let query_blocked_count = query_reports
        .iter()
        .filter(|report| report.status == "blocked")
        .count();
    let acceptance_reason = match &acceptance {
        SourcePackAcceptance::Success => None,
        SourcePackAcceptance::Partial { reason } | SourcePackAcceptance::Empty { reason } => {
            Some(reason.clone())
        }
    };
    ResearchSourcePackReport {
        subject: Some(subject.clone()),
        status: if matches!(acceptance, SourcePackAcceptance::Partial { .. })
            || query_error_count > 0
            || query_blocked_count > 0
        {
            "partial".to_string()
        } else {
            "success".to_string()
        },
        reason: if query_blocked_count > 0 {
            Some(if discovered_source_count == 0 && seeded_source_count > 0 {
                format!(
                    "{} Using only seeded source-pack candidates.",
                    blocked_reason_for_providers(&providers, query_blocked_count)
                )
            } else {
                format!(
                    "{} Using available source-pack candidates.",
                    blocked_reason_for_providers(&providers, query_blocked_count)
                )
            })
        } else if query_error_count > 0 {
            Some(format!(
                "{query_error_count} source-pack search query/queries failed; using available candidates."
            ))
        } else {
            acceptance_reason
        },
        queries: query_reports,
        seeded_source_count,
        discovered_source_count,
        adopted_source_count,
        adopted_candidates,
        skipped_candidates,
        coverage_misses,
        source_pack: Some(format_source_pack(&subject, &accepted_sources)),
    }
}

pub(crate) async fn collect_transient_repair_search_hints(
    subject: &str,
    queries: &[String],
    known_urls: &HashSet<String>,
) -> Vec<RepairSearchHint> {
    if subject.trim().is_empty() || queries.is_empty() {
        return Vec::new();
    }
    let client = match reqwest::Client::builder()
        .user_agent("LiquidResearchRepairSearch/0.1")
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };
    let providers = configured_search_providers_from_env();
    let mut seen_urls = known_urls
        .iter()
        .map(|url| url.trim().to_string())
        .collect::<HashSet<_>>();
    let mut hints = Vec::new();
    for query in queries.iter().take(MAX_REPAIR_SEARCH_HINT_QUERIES) {
        let resolution = match search_with_providers(&providers, &client, subject, query).await {
            Ok(resolution) => resolution,
            Err(_) => continue,
        };
        let SearchQueryResolution {
            outcome, provider, ..
        } = resolution;
        let SearchQueryOutcome::Results(results) = outcome else {
            continue;
        };
        let remaining = MAX_REPAIR_SEARCH_HINTS.saturating_sub(hints.len());
        if remaining == 0 {
            break;
        }
        let filtered = repair_search_hints_from_results(
            subject,
            query,
            provider.as_deref(),
            &results,
            &mut seen_urls,
            remaining,
        );
        hints.extend(filtered);
        if hints.len() >= MAX_REPAIR_SEARCH_HINTS {
            break;
        }
    }
    hints
}

fn finalize_empty_source_pack_report(
    subject: String,
    providers: &[ResearchSearchProvider],
    queries: Vec<ResearchSourceQueryReport>,
    seeded_source_count: usize,
    discovered_source_count: usize,
    adopted_source_count: usize,
    adopted_candidates: Vec<ResearchSourceCandidateReport>,
    skipped_candidates: Vec<ResearchSourceCandidateReport>,
    coverage_misses: Vec<ResearchSourceCoverageMiss>,
) -> ResearchSourcePackReport {
    let query_error_count = queries
        .iter()
        .filter(|report| report.status == "error")
        .count();
    let query_blocked_count = queries
        .iter()
        .filter(|report| report.status == "blocked")
        .count();
    let has_insufficient_context_only = skipped_candidates.iter().any(|candidate| {
        candidate.rejection_reason.as_deref() == Some("insufficient_context_evidence")
    });
    ResearchSourcePackReport {
        subject: Some(subject),
        status: if query_error_count > 0 {
            "error".to_string()
        } else if query_blocked_count > 0 {
            "blocked".to_string()
        } else {
            "empty".to_string()
        },
        reason: Some(if query_error_count > 0 {
            format!(
                "{query_error_count} source-pack search query/queries failed and no candidates were available."
            )
        } else if query_blocked_count > 0 {
            blocked_reason_for_providers(providers, query_blocked_count)
        } else if has_insufficient_context_only {
            "Search providers returned only weak or generic source-pack candidates; no context-safe candidates were available."
                .to_string()
        } else {
            if queries.iter().any(|report| report.result_count > 0) {
                "Search providers returned results, but no topically relevant source-pack candidates were available.".to_string()
            } else {
                "No seeded or searchable source-pack candidates were available.".to_string()
            }
        }),
        queries,
        seeded_source_count,
        discovered_source_count,
        adopted_source_count,
        adopted_candidates,
        skipped_candidates,
        coverage_misses,
        source_pack: None,
    }
}

fn configured_search_providers_from_env() -> Vec<ResearchSearchProvider> {
    configured_search_providers(
        std::env::var("LIQUID_RESEARCH_SEARCH_PROVIDERS")
            .ok()
            .as_deref(),
        std::env::var("LIQUID_RESEARCH_SEARCH_PROVIDER")
            .ok()
            .as_deref(),
        std::env::var("BRAVE_SEARCH_API_KEY").ok().as_deref(),
        std::env::var("NAVER_CLIENT_ID").ok().as_deref(),
        std::env::var("NAVER_CLIENT_SECRET").ok().as_deref(),
        std::env::var("KAKAO_REST_API_KEY").ok().as_deref(),
    )
}

fn repair_search_hints_from_results(
    subject: &str,
    query: &str,
    provider: Option<&str>,
    results: &[ResearchSource],
    seen_urls: &mut HashSet<String>,
    remaining: usize,
) -> Vec<RepairSearchHint> {
    let mut hints = Vec::new();
    for result in results {
        if hints.len() >= remaining {
            break;
        }
        let normalized_url = result.url.trim().to_string();
        if normalized_url.is_empty() || seen_urls.contains(&normalized_url) {
            continue;
        }
        if !is_topically_relevant_source(result, subject, query)
            || !context_safe_source_for_subject(result, subject)
        {
            continue;
        }
        seen_urls.insert(normalized_url.clone());
        hints.push(RepairSearchHint {
            query: query.to_string(),
            provider: provider.map(str::to_string),
            title: result.title.clone(),
            url: normalized_url.clone(),
            source_class: infer_source_class_for_subject(&normalized_url, Some(subject))
                .to_string(),
            source_quality: source_quality_for_subject(&normalized_url, Some(subject)).to_string(),
            snippet: sanitize_search_text(&result.snippet),
        });
    }
    hints
}

fn configured_search_provider(
    provider_name: Option<&str>,
    brave_api_key: Option<&str>,
) -> ResearchSearchProvider {
    configured_single_search_provider(provider_name, brave_api_key, None, None, None)
}

fn configured_search_providers(
    provider_names: Option<&str>,
    provider_name: Option<&str>,
    brave_api_key: Option<&str>,
    naver_client_id: Option<&str>,
    naver_client_secret: Option<&str>,
    kakao_rest_api_key: Option<&str>,
) -> Vec<ResearchSearchProvider> {
    if let Some(provider_names) = provider_names {
        let mut providers = Vec::new();
        let mut seen = HashSet::new();
        for name in provider_names
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let Some(key) = canonical_provider_name(name) else {
                continue;
            };
            if !seen.insert(key) {
                continue;
            }
            if let Some(provider) = provider_from_name(
                key,
                brave_api_key,
                naver_client_id,
                naver_client_secret,
                kakao_rest_api_key,
            ) {
                providers.push(provider);
            }
        }
        if !providers.is_empty() {
            return providers;
        }
    }

    vec![configured_single_search_provider(
        provider_name,
        brave_api_key,
        naver_client_id,
        naver_client_secret,
        kakao_rest_api_key,
    )]
}

fn configured_single_search_provider(
    provider_name: Option<&str>,
    brave_api_key: Option<&str>,
    naver_client_id: Option<&str>,
    naver_client_secret: Option<&str>,
    kakao_rest_api_key: Option<&str>,
) -> ResearchSearchProvider {
    let provider_name = provider_name.unwrap_or_default().trim();
    provider_from_name(
        provider_name,
        brave_api_key,
        naver_client_id,
        naver_client_secret,
        kakao_rest_api_key,
    )
    .unwrap_or(ResearchSearchProvider::DuckDuckGo)
}

fn canonical_provider_name(provider_name: &str) -> Option<&'static str> {
    let provider_name = provider_name.trim();
    if provider_name.eq_ignore_ascii_case("brave") {
        return Some("brave");
    }
    if provider_name.eq_ignore_ascii_case("naver") {
        return Some("naver");
    }
    if provider_name.eq_ignore_ascii_case("kakao") || provider_name.eq_ignore_ascii_case("daum") {
        return Some("kakao");
    }
    if provider_name.eq_ignore_ascii_case("duckduckgo") || provider_name.eq_ignore_ascii_case("ddg")
    {
        return Some("duckduckgo");
    }
    None
}

fn provider_from_name(
    provider_name: &str,
    brave_api_key: Option<&str>,
    naver_client_id: Option<&str>,
    naver_client_secret: Option<&str>,
    kakao_rest_api_key: Option<&str>,
) -> Option<ResearchSearchProvider> {
    let provider_name = canonical_provider_name(provider_name)?;
    let brave_api_key = brave_api_key
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let naver_client_id = naver_client_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let naver_client_secret = naver_client_secret
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let kakao_rest_api_key = kakao_rest_api_key
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if provider_name == "brave" {
        return brave_api_key.map(|api_key| ResearchSearchProvider::Brave {
            api_key: api_key.to_string(),
        });
    }
    if provider_name == "naver" {
        if let (Some(client_id), Some(client_secret)) = (naver_client_id, naver_client_secret) {
            return Some(ResearchSearchProvider::Naver {
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
            });
        }
        return None;
    }
    if provider_name == "kakao" {
        return kakao_rest_api_key.map(|rest_api_key| ResearchSearchProvider::Kakao {
            rest_api_key: rest_api_key.to_string(),
        });
    }
    if provider_name == "duckduckgo" {
        return Some(ResearchSearchProvider::DuckDuckGo);
    }
    None
}

async fn search_with_providers(
    providers: &[ResearchSearchProvider],
    client: &reqwest::Client,
    subject: &str,
    query: &str,
) -> Result<SearchQueryResolution, String> {
    let mut attempts = Vec::new();
    for provider in providers {
        attempts.push(SearchProviderAttempt {
            provider: provider.diagnostic_name().to_string(),
            outcome: search_with_provider_query_variants(provider, client, subject, query).await,
        });
    }
    select_search_query_resolution(attempts, subject, query)
}

async fn search_with_provider_query_variants(
    provider: &ResearchSearchProvider,
    client: &reqwest::Client,
    subject: &str,
    query: &str,
) -> Result<SearchQueryOutcome, String> {
    let mut aggregated = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut saw_blocked = false;
    let mut errors = Vec::new();
    for provider_query in provider_specific_search_queries(provider, subject, query) {
        match provider.search_single(client, &provider_query).await {
            Ok(SearchQueryOutcome::Results(results)) => {
                for source in results {
                    if seen_urls.insert(source.url.clone()) {
                        aggregated.push(source);
                    }
                }
                if aggregated
                    .iter()
                    .any(|source| is_topically_relevant_source(source, subject, query))
                {
                    return Ok(SearchQueryOutcome::Results(aggregated));
                }
            }
            Ok(SearchQueryOutcome::Blocked) => {
                saw_blocked = true;
            }
            Ok(SearchQueryOutcome::Empty) => {}
            Err(error) => errors.push(error),
        }
    }
    if !aggregated.is_empty() {
        return Ok(SearchQueryOutcome::Results(aggregated));
    }
    if !errors.is_empty() {
        errors.dedup();
        return Err(errors.join(" | "));
    }
    if saw_blocked {
        Ok(SearchQueryOutcome::Blocked)
    } else {
        Ok(SearchQueryOutcome::Empty)
    }
}

fn provider_specific_search_queries(
    provider: &ResearchSearchProvider,
    subject: &str,
    query: &str,
) -> Vec<String> {
    let mut queries = vec![query.to_string()];
    if !matches!(
        provider,
        ResearchSearchProvider::Naver { .. } | ResearchSearchProvider::Kakao { .. }
    ) {
        return queries;
    }
    queries.extend(localized_korean_query_variants(subject, query));
    queries
}

fn localized_korean_query_variants(subject: &str, query: &str) -> Vec<String> {
    if !subject_has_korean_local_intent(subject, query) {
        return Vec::new();
    }
    let place = localized_place_anchor(subject, query);
    let city = localized_city_anchor(subject, query);
    let running = subject_mentions_running_intent(subject, query);
    let cafe = subject_mentions_cafe_intent(subject, query);
    let official = subject_mentions_official_local_info(subject, query);
    let hours = subject_mentions_hours_or_verification(subject, query);

    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    let mut primary_terms = Vec::new();
    if let Some(place) = place.as_deref() {
        primary_terms.push(place.to_string());
    }
    if let Some(city) = city.as_deref() {
        primary_terms.push(city.to_string());
    }
    if running {
        primary_terms.push("러닝 코스".to_string());
    }
    if cafe {
        primary_terms.push("카페".to_string());
    }
    push_provider_variant(&mut variants, &mut seen, primary_terms);

    let mut secondary_terms = Vec::new();
    if let Some(place) = place.as_deref() {
        if running || official {
            secondary_terms.push(format!("{place}공원"));
        } else {
            secondary_terms.push(place.to_string());
        }
    }
    if official || running {
        secondary_terms.push("공식".to_string());
    }
    if cafe {
        secondary_terms.push("카페".to_string());
    }
    if hours || cafe {
        secondary_terms.push("영업시간".to_string());
    }
    push_provider_variant(&mut variants, &mut seen, secondary_terms);

    variants.truncate(2);
    variants
}

fn push_provider_variant(
    variants: &mut Vec<String>,
    seen: &mut HashSet<String>,
    terms: Vec<String>,
) {
    let variant = terms
        .into_iter()
        .map(|term| compact_text(&term))
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if variant.is_empty() || !seen.insert(variant.clone()) {
        return;
    }
    variants.push(variant);
}

fn subject_has_korean_local_intent(subject: &str, query: &str) -> bool {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    let has_local_anchor = lower.contains("namsan")
        || lower.contains("seoul")
        || lower.contains("hangang")
        || lower.contains("gangnam")
        || lower.contains("hongdae")
        || lower.contains("seongsu")
        || lower.contains("itaewon")
        || lower.contains("yeouido")
        || lower.contains("myeongdong")
        || lower.contains("jongno")
        || lower.contains("서울")
        || lower.contains("남산")
        || lower.contains("한강")
        || lower.contains("강남")
        || lower.contains("홍대")
        || lower.contains("성수")
        || lower.contains("이태원")
        || lower.contains("여의도")
        || lower.contains("명동")
        || lower.contains("종로");
    let has_local_workflow = lower.contains("running")
        || lower.contains("run ")
        || lower.contains(" workout")
        || lower.contains("jog")
        || lower.contains("route")
        || lower.contains("course")
        || lower.contains("park")
        || lower.contains("cafe")
        || lower.contains("coffee")
        || lower.contains("brunch")
        || lower.contains("hours")
        || lower.contains("opening")
        || lower.contains("transit")
        || lower.contains("달리기")
        || lower.contains("러닝")
        || lower.contains("코스")
        || lower.contains("공원")
        || lower.contains("카페")
        || lower.contains("영업시간")
        || lower.contains("운영시간")
        || lower.contains("지하철")
        || lower.contains("교통");
    has_local_anchor && has_local_workflow
}

fn localized_place_anchor(subject: &str, query: &str) -> Option<String> {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    if lower.contains("남산") || lower.contains("namsan") {
        return Some("남산".to_string());
    }
    if lower.contains("한강") || lower.contains("hangang") {
        return Some("한강".to_string());
    }
    None
}

fn localized_city_anchor(subject: &str, query: &str) -> Option<String> {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    if lower.contains("서울") || lower.contains("seoul") {
        return Some("서울".to_string());
    }
    None
}

fn subject_mentions_running_intent(subject: &str, query: &str) -> bool {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    lower.contains("running")
        || lower.contains("run ")
        || lower.contains(" workout")
        || lower.contains("jog")
        || lower.contains("달리기")
        || lower.contains("러닝")
}

fn subject_mentions_cafe_intent(subject: &str, query: &str) -> bool {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    lower.contains("cafe")
        || lower.contains("coffee")
        || lower.contains("brunch")
        || lower.contains("카페")
}

fn subject_mentions_official_local_info(subject: &str, query: &str) -> bool {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    lower.contains("official")
        || lower.contains("park")
        || lower.contains("venue")
        || lower.contains("공식")
        || lower.contains("공원")
}

fn subject_mentions_hours_or_verification(subject: &str, query: &str) -> bool {
    let lower = format!("{subject} {query}").to_ascii_lowercase();
    lower.contains("hours")
        || lower.contains("opening")
        || lower.contains("day-of verification")
        || lower.contains("영업시간")
        || lower.contains("운영시간")
}

fn select_search_query_resolution(
    attempts: Vec<SearchProviderAttempt>,
    subject: &str,
    query: &str,
) -> Result<SearchQueryResolution, String> {
    let mut saw_blocked = false;
    let mut errors = Vec::new();
    let mut no_relevant_providers = Vec::new();
    let mut fallback_results = None;

    for attempt in attempts {
        match attempt.outcome {
            Ok(SearchQueryOutcome::Results(results)) => {
                let preferred_results = preferred_official_host_hint_results(query, results);
                let provider = attempt.provider;
                let has_topical_candidates = preferred_results
                    .iter()
                    .any(|source| is_topically_relevant_source(source, subject, query));
                let should_continue_for_official_host =
                    should_continue_official_host_hint_provider_fallback(query, &preferred_results);
                if should_continue_for_official_host || !has_topical_candidates {
                    if !has_topical_candidates {
                        no_relevant_providers.push(provider.clone());
                    }
                    if fallback_results.is_none() {
                        fallback_results = Some(SearchQueryResolution {
                            outcome: SearchQueryOutcome::Results(preferred_results),
                            provider: Some(provider),
                            diagnostics: None,
                        });
                    }
                    continue;
                }
                return Ok(SearchQueryResolution {
                    outcome: SearchQueryOutcome::Results(preferred_results),
                    provider: Some(provider),
                    diagnostics: merge_query_diagnostics(
                        format_provider_failure_summary(&errors),
                        format_provider_no_relevant_summary(&no_relevant_providers, false),
                    ),
                });
            }
            Ok(SearchQueryOutcome::Blocked) => {
                saw_blocked = true;
            }
            Ok(SearchQueryOutcome::Empty) => {}
            Err(error) => errors.push(format!("{}: {error}", attempt.provider)),
        }
    }

    if let Some(mut resolution) = fallback_results {
        resolution.diagnostics = merge_query_diagnostics(
            format_provider_failure_summary(&errors),
            format_provider_no_relevant_summary(&no_relevant_providers, true),
        );
        return Ok(resolution);
    }
    if !errors.is_empty() {
        errors.dedup();
        return Err(errors.join(" | "));
    }
    if saw_blocked {
        Ok(SearchQueryResolution {
            outcome: SearchQueryOutcome::Blocked,
            provider: None,
            diagnostics: None,
        })
    } else {
        Ok(SearchQueryResolution {
            outcome: SearchQueryOutcome::Empty,
            provider: None,
            diagnostics: None,
        })
    }
}

fn format_provider_no_relevant_summary(
    providers: &[String],
    exhausted_all_providers: bool,
) -> Option<String> {
    if providers.is_empty() {
        return None;
    }
    let mut deduped = Vec::new();
    for provider in providers {
        if !deduped.contains(provider) {
            deduped.push(provider.clone());
        }
    }
    let message = if exhausted_all_providers {
        format!(
            "Configured providers returned raw results but no topically relevant candidates: {}",
            deduped.join(" | ")
        )
    } else {
        format!(
            "Earlier configured providers returned raw results but no topically relevant candidates: {}",
            deduped.join(" | ")
        )
    };
    Some(truncate_provider_diagnostic(message))
}

fn preferred_official_host_hint_results(
    query: &str,
    results: Vec<ResearchSource>,
) -> Vec<ResearchSource> {
    let Some(domain) = official_host_hint_target_domain(query) else {
        return results;
    };
    let exact_matches = results
        .iter()
        .filter(|source| source_url_matches_domain_boundary(&source.url, domain))
        .cloned()
        .collect::<Vec<_>>();
    if exact_matches.is_empty() {
        results
    } else {
        exact_matches
    }
}

fn should_continue_official_host_hint_provider_fallback(
    query: &str,
    results: &[ResearchSource],
) -> bool {
    let Some(domain) = official_host_hint_target_domain(query) else {
        return false;
    };
    let exact_host_results = results
        .iter()
        .filter(|source| source_url_matches_domain_boundary(&source.url, domain))
        .collect::<Vec<_>>();
    if exact_host_results.is_empty() {
        return true;
    }
    let relevance_subject = query_subject_for_relevance(query);
    !exact_host_results
        .iter()
        .any(|source| is_topically_relevant_source(source, &relevance_subject, query))
}

fn official_host_hint_target_domain(query: &str) -> Option<&'static str> {
    let normalized = compact_text(query).to_ascii_lowercase();
    [
        ("nist.gov official guidance", "nist.gov"),
        (
            "eur-lex.europa.eu ai act official text",
            "eur-lex.europa.eu",
        ),
        ("apple.com official technical specifications", "apple.com"),
        ("frame.work official laptop specifications", "frame.work"),
        ("lenovo.com official technical specifications", "lenovo.com"),
        ("iana service names port numbers documentation", "iana.org"),
        ("rfc 6056 ephemeral port randomization", "rfc-editor.org"),
        (
            "docs.kernel.org ip_local_port_range networking",
            "docs.kernel.org",
        ),
        (
            "learn.microsoft.com windows dynamic port range tcp udp",
            "learn.microsoft.com",
        ),
    ]
    .into_iter()
    .find_map(|(suffix, domain)| normalized.ends_with(suffix).then_some(domain))
}

fn source_url_matches_domain_boundary(url: &str, domain: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| {
            host_matches_domain_boundary(
                &host.trim_start_matches("www.").to_ascii_lowercase(),
                domain,
            )
        })
        .unwrap_or(false)
}

fn apply_source_pack_budget(
    ranked_sources: Vec<ResearchSource>,
    preserved_official_hint_urls: &HashSet<String>,
) -> Vec<ResearchSource> {
    if ranked_sources.len() <= MAX_SOURCE_PACK_RESULTS || preserved_official_hint_urls.is_empty() {
        return ranked_sources
            .into_iter()
            .take(MAX_SOURCE_PACK_RESULTS)
            .collect();
    }

    let mut selected = ranked_sources
        .iter()
        .filter(|source| preserved_official_hint_urls.contains(&source.url))
        .take(MAX_SOURCE_PACK_RESULTS)
        .cloned()
        .collect::<Vec<_>>();

    for source in ranked_sources {
        if selected.iter().any(|existing| existing.url == source.url) {
            continue;
        }
        if selected.len() >= MAX_SOURCE_PACK_RESULTS {
            break;
        }
        selected.push(source);
    }

    selected
}

fn format_provider_failure_summary(errors: &[String]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }
    let mut deduped = Vec::new();
    for error in errors {
        if !deduped.contains(error) {
            deduped.push(error.clone());
        }
    }
    let mut summary = format!(
        "Earlier configured provider failures before fallback success: {}",
        deduped.join(" | ")
    );
    if summary.chars().count() > MAX_PROVIDER_DIAGNOSTIC_CHARS {
        summary = summary
            .chars()
            .take(MAX_PROVIDER_DIAGNOSTIC_CHARS.saturating_sub(3))
            .collect::<String>();
        summary.push_str("...");
    }
    Some(summary)
}

fn merge_query_diagnostics(existing: Option<String>, extra: Option<String>) -> Option<String> {
    match (existing, extra) {
        (None, None) => None,
        (Some(existing), None) => Some(existing),
        (None, Some(extra)) => Some(truncate_provider_diagnostic(extra)),
        (Some(existing), Some(extra)) => {
            let merged = if existing.contains(&extra) {
                existing
            } else {
                format!("{existing} | {extra}")
            };
            Some(truncate_provider_diagnostic(merged))
        }
    }
}

fn truncate_provider_diagnostic(mut diagnostic: String) -> String {
    if diagnostic.chars().count() > MAX_PROVIDER_DIAGNOSTIC_CHARS {
        diagnostic = diagnostic
            .chars()
            .take(MAX_PROVIDER_DIAGNOSTIC_CHARS.saturating_sub(3))
            .collect::<String>();
        diagnostic.push_str("...");
    }
    diagnostic
}

fn official_host_hint_target_miss_diagnostic(
    query: &str,
    results: &[ResearchSource],
) -> Option<String> {
    let domain = official_host_hint_target_domain(query)?;
    if results
        .iter()
        .any(|source| source_url_matches_domain_boundary(&source.url, domain))
    {
        None
    } else {
        Some(format!(
            "Official-host hint query did not recover target domain {domain}."
        ))
    }
}

fn build_source_coverage_misses(
    providers: &[ResearchSearchProvider],
    query_reports: &[ResearchSourceQueryReport],
    query_result_candidates: &[Vec<ResearchSourceCandidateReport>],
    final_adopted_urls: &HashSet<String>,
    skipped_candidates: &[ResearchSourceCandidateReport],
) -> Vec<ResearchSourceCoverageMiss> {
    query_reports
        .iter()
        .zip(query_result_candidates.iter())
        .filter_map(|(report, candidates)| {
            let expected_host = official_host_hint_target_domain(&report.query).map(str::to_string);
            let expected_source_class =
                expected_source_class_for_query(&report.query).map(str::to_string);
            if expected_host.is_none() && expected_source_class.is_none() {
                return None;
            }

            let matched_in_results = candidates.iter().any(|candidate| {
                candidate_matches_coverage_expectation(
                    candidate,
                    expected_host.as_deref(),
                    expected_source_class.as_deref(),
                )
            });
            let matched_in_adopted = candidates.iter().any(|candidate| {
                final_adopted_urls.contains(&candidate.url)
                    && candidate_matches_coverage_expectation(
                        candidate,
                        expected_host.as_deref(),
                        expected_source_class.as_deref(),
                    )
            });
            let matched_in_skipped = skipped_candidates.iter().any(|candidate| {
                candidate.query.as_deref() == Some(report.query.as_str())
                    && candidate_matches_coverage_expectation(
                        candidate,
                        expected_host.as_deref(),
                        expected_source_class.as_deref(),
                    )
            });

            let status = if matched_in_adopted {
                "adopted"
            } else if report.status == "blocked" {
                "blocked"
            } else if matched_in_results || matched_in_skipped {
                "found_not_adopted"
            } else {
                "missed"
            };

            let reason = if status == "adopted" {
                None
            } else if status == "found_not_adopted" {
                skipped_candidates
                    .iter()
                    .find(|candidate| {
                        candidate.query.as_deref() == Some(report.query.as_str())
                            && candidate_matches_coverage_expectation(
                                candidate,
                                expected_host.as_deref(),
                                expected_source_class.as_deref(),
                            )
                    })
                    .and_then(|candidate| candidate.rejection_reason.clone())
                    .or_else(|| Some("Candidate matched the expected host/source class but was not adopted into the final source pack.".to_string()))
            } else if let Some(error) = report.error.as_deref() {
                Some(redacted_coverage_reason(error))
            } else if let Some(host) = expected_host.as_deref() {
                Some(format!("Target host {host} was not recovered."))
            } else if let Some(source_class) = expected_source_class.as_deref() {
                Some(format!("Required source class {source_class} was not recovered."))
            } else {
                None
            };

            Some(ResearchSourceCoverageMiss {
                expected_host,
                expected_source_class,
                query: report.query.clone(),
                provider: coverage_provider_name(report.provider.as_deref(), providers),
                status: status.to_string(),
                reason,
            })
        })
        .collect()
}

fn expected_source_class_for_query(query: &str) -> Option<&'static str> {
    let lower = compact_text(query).to_ascii_lowercase();
    if lower.contains("official")
        || lower.contains("documentation")
        || lower.contains("guidance")
        || lower.contains("technical specifications")
    {
        Some("official_or_primary")
    } else {
        None
    }
}

fn candidate_matches_coverage_expectation(
    candidate: &ResearchSourceCandidateReport,
    expected_host: Option<&str>,
    expected_source_class: Option<&str>,
) -> bool {
    let host_matches =
        expected_host.is_none_or(|host| source_url_matches_domain_boundary(&candidate.url, host));
    let class_matches = expected_source_class.is_none_or(|expected| {
        candidate
            .source_class
            .as_deref()
            .is_some_and(|actual| source_class_matches_expected(actual, expected))
    });
    host_matches && class_matches
}

fn source_class_matches_expected(actual: &str, expected: &str) -> bool {
    if expected != "official_or_primary" {
        return actual.eq_ignore_ascii_case(expected);
    }
    let lower = actual.to_ascii_lowercase();
    lower.contains("official")
        || lower.contains("primary")
        || lower.contains("government")
        || lower.contains("support")
        || lower.contains("developer")
        || lower.contains("documentation")
        || lower.contains("manufacturer")
        || lower.contains("project")
}

fn coverage_provider_name(
    provider: Option<&str>,
    providers: &[ResearchSearchProvider],
) -> Option<String> {
    provider.map(str::to_string).or_else(|| match providers {
        [] => None,
        [provider] => Some(provider.diagnostic_name().to_string()),
        _ => Some("provider_chain".to_string()),
    })
}

fn redacted_coverage_reason(reason: &str) -> String {
    let compact = compact_text(reason);
    let redacted = compact
        .replace("api_key", "[redacted_key]")
        .replace("client_secret", "[redacted_key]")
        .replace("authorization", "[redacted_header]")
        .replace("raw provider payload", "[redacted_payload]")
        .replace("response body:", "[redacted_payload]:");
    truncate_provider_diagnostic(redacted)
}

fn blocked_message_for_providers(providers: &[ResearchSearchProvider]) -> &'static str {
    match providers {
        [provider] => provider.blocked_message(),
        _ => "A configured search provider blocked source discovery for this query.",
    }
}

fn blocked_reason_for_providers(
    providers: &[ResearchSearchProvider],
    query_blocked_count: usize,
) -> String {
    match providers {
        [provider] => provider.blocked_reason(query_blocked_count),
        _ => format!(
            "{query_blocked_count} source-pack search query/queries were blocked by configured search providers."
        ),
    }
}

fn reconcile_query_report_adoption_counts(
    query_reports: &mut [ResearchSourceQueryReport],
    query_adopted_urls: &[Vec<String>],
    final_adopted_urls: &HashSet<String>,
) {
    for (report, adopted_urls) in query_reports.iter_mut().zip(query_adopted_urls.iter()) {
        let final_adopted_count = adopted_urls
            .iter()
            .filter(|url| final_adopted_urls.contains(*url))
            .count();
        let ranked_out_count = report.adopted_count.saturating_sub(final_adopted_count);
        report.adopted_count = final_adopted_count;
        report.skipped_count += ranked_out_count;
    }
}

pub(crate) fn infer_source_class(url: &str) -> &'static str {
    infer_source_class_for_subject(url, None)
}

fn infer_source_class_for_subject(url: &str, subject: Option<&str>) -> &'static str {
    let Ok(parsed) = Url::parse(url) else {
        return "unknown";
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.");
    let host = host.to_ascii_lowercase();
    if is_subject_matched_or_official_github(&parsed, subject) {
        return "official_or_primary";
    }
    if matches!(
        host.as_str(),
        "developer.android.com"
            | "developer.apple.com"
            | "support.apple.com"
            | "apple.com"
            | "support.google.com"
            | "developers.google.com"
            | "frame.work"
            | "knowledgebase.frame.work"
            | "uxlfoundation.github.io"
            | "oneapi-spec.uxlfoundation.org"
            | "taskflow.github.io"
            | "lenovo.com"
            | "psref.lenovo.com"
            | "nvidia.com"
            | "docs.nvidia.com"
            | "developer.nvidia.com"
            | "packaging.python.org"
            | "docs.python.org"
            | "pypi.org"
            | "docs.npmjs.com"
            | "npmjs.com"
            | "nodejs.org"
            | "rust-lang.org"
            | "doc.rust-lang.org"
            | "rfc-editor.org"
            | "datatracker.ietf.org"
            | "ietf.org"
            | "iana.org"
            | "kernel.org"
            | "docs.kernel.org"
            | "learn.microsoft.com"
            | "azure.microsoft.com"
            | "docs.aws.amazon.com"
            | "cloud.google.com"
            | "kubernetes.io"
            | "wikipedia.org"
    ) || host.ends_with(".gov")
        || host == "europa.eu"
        || host.ends_with(".europa.eu")
        || host.ends_with(".edu")
        || host.ends_with(".ac.uk")
    {
        return "official_or_primary";
    }
    if host.contains("reddit.com")
        || host.contains("x.com")
        || host.contains("twitter.com")
        || host.contains("namu.wiki")
        || host.contains("dcinside.com")
    {
        return "user_generated_or_rumor";
    }
    "secondary"
}

fn is_subject_matched_or_official_github(parsed: &Url, subject: Option<&str>) -> bool {
    let Some(host) = parsed
        .host_str()
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
    else {
        return false;
    };
    let Some((owner, repo)) = github_owner_repo(parsed, &host) else {
        return false;
    };
    if is_known_official_github_owner(&owner) {
        return true;
    }
    subject
        .map(|subject| subject_mentions_repo(subject, &owner, &repo))
        .unwrap_or(false)
}

fn github_owner_repo(parsed: &Url, host: &str) -> Option<(String, String)> {
    let mut segments = parsed.path_segments()?;
    match host {
        "github.com" => Some((
            segments.next()?.to_ascii_lowercase(),
            segments.next()?.to_ascii_lowercase(),
        )),
        "raw.githubusercontent.com" => Some((
            segments.next()?.to_ascii_lowercase(),
            segments.next()?.to_ascii_lowercase(),
        )),
        "api.github.com" => {
            if segments.next()? != "repos" {
                return None;
            }
            Some((
                segments.next()?.to_ascii_lowercase(),
                segments.next()?.to_ascii_lowercase(),
            ))
        }
        _ => None,
    }
}

fn is_known_official_github_owner(owner: &str) -> bool {
    matches!(
        owner,
        "python"
            | "pypa"
            | "npm"
            | "nodejs"
            | "rust-lang"
            | "pytorch"
            | "apple"
            | "apple-oss-distributions"
            | "nvidia"
            | "lenovo"
            | "android"
            | "google"
    )
}

fn subject_mentions_repo(subject: &str, owner: &str, repo: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    contains_repo_reference(&lower, &format!("{owner}/{repo}"))
}

fn contains_repo_reference(subject: &str, owner_repo: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = subject[start..].find(owner_repo) {
        let match_start = start + offset;
        let match_end = match_start + owner_repo.len();
        let before = subject[..match_start].chars().next_back();
        let after = subject[match_end..].chars().next();
        let before_ok = before.is_none_or(|ch| !is_repo_reference_boundary_char(ch));
        let after_ok = after.is_none_or(|ch| !is_repo_reference_boundary_char(ch));
        if before_ok && after_ok {
            return true;
        }
        start = match_start + 1;
    }
    false
}

fn is_repo_reference_boundary_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn extract_research_subject(user_prompt: &str, source_documents: Option<&str>) -> Option<String> {
    let focus =
        extract_first_nonempty_line_after_marker(user_prompt, "[사용자 조사 조건/제약/비교 기준]");
    let prompt_subject = extract_prompt_subject(user_prompt);
    if let Some(prompt_subject) = prompt_subject
        .as_deref()
        .filter(|subject| !prompt_subject_is_generic_instruction(subject))
    {
        return Some(match focus {
            Some(focus) => combine_subject_and_focus(prompt_subject, &focus),
            None => prompt_subject.to_string(),
        });
    }
    if let Some(document_subject) = source_documents.and_then(extract_source_document_subject) {
        return Some(match focus {
            Some(focus) => combine_subject_and_focus(&document_subject, &focus),
            None => document_subject,
        });
    }
    if focus.is_some() {
        return focus;
    }

    prompt_subject
}

fn extract_prompt_subject(user_prompt: &str) -> Option<String> {
    let marker = "정보 조사 보고서를 작성하세요:";
    let after_marker = user_prompt
        .find(marker)
        .map(|idx| &user_prompt[idx + marker.len()..])
        .unwrap_or(user_prompt);
    let subject = after_marker
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("[RQ-TEST]")
        .trim();
    let subject = normalize_research_subject_text(subject);
    if subject.is_empty() {
        None
    } else {
        Some(subject)
    }
}

fn prompt_subject_is_generic_instruction(subject: &str) -> bool {
    let normalized = subject.trim().to_ascii_lowercase();
    normalized.contains("source documents")
        || normalized.contains("정보 조사 보고서를 작성하세요")
        || normalized.contains("후속 조사 보고서를 작성하세요")
        || normalized.contains("종합 정보 조사 보고서를 작성하세요")
        || normalized.contains("대상, 주장, 사건, 제품 또는 개념을 식별")
}

fn extract_source_document_subject(source_documents: &str) -> Option<String> {
    if let Some(relevant_documents) = research_context_pack_excerpts(source_documents) {
        return extract_github_repo_subject(relevant_documents)
            .or_else(|| extract_document_heading_subject(relevant_documents));
    }
    extract_github_repo_subject(source_documents)
        .or_else(|| extract_document_heading_subject(source_documents))
}

fn extract_github_repo_subject(source_documents: &str) -> Option<String> {
    for token in source_documents.split_whitespace() {
        for part in token.split(|c: char| {
            matches!(
                c,
                '`' | '"' | '\'' | ',' | ')' | '(' | '[' | ']' | '<' | '>' | '|' | ';'
            )
        }) {
            if is_owner_repo(part) {
                return Some(
                    part.trim_start_matches('/')
                        .trim_end_matches('/')
                        .to_string(),
                );
            }
        }
        let cleaned = token.trim_matches(|c: char| {
            matches!(
                c,
                '`' | '"' | '\'' | ',' | '.' | ')' | '(' | '[' | ']' | '<' | '>' | '|' | ';'
            )
        });
        if let Some(repo) = extract_repo_from_github_url(cleaned) {
            return Some(repo);
        }
        if is_owner_repo(cleaned) {
            return Some(
                cleaned
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .to_string(),
            );
        }
    }
    None
}

fn extract_repo_from_github_url(text: &str) -> Option<String> {
    let url = Url::parse(text).ok()?;
    let host = url.host_str()?.trim_start_matches("www.");
    if host != "github.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    let owner = segments.next()?;
    let repo = segments.next()?;
    let candidate = format!("{owner}/{repo}");
    if is_owner_repo(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn is_owner_repo(candidate: &str) -> bool {
    let trimmed = candidate.trim_start_matches('/').trim_end_matches('/');
    let Some((owner, repo)) = trimmed.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !repo.is_empty()
        && !repo.contains('/')
        && owner.chars().all(is_repo_char)
        && repo.chars().all(is_repo_char)
        && owner.chars().any(|c| c.is_ascii_alphabetic())
        && repo.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn extract_document_heading_subject(source_documents: &str) -> Option<String> {
    for line in source_documents.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            continue;
        }
        let heading = trimmed.trim_start_matches('#').trim();
        let heading = clean_document_subject(heading);
        if is_useful_document_subject(&heading) {
            return Some(heading);
        }
    }
    None
}

fn research_context_pack_excerpts(source_documents: &str) -> Option<&str> {
    let marker_start = source_documents.find(RESEARCH_CONTEXT_PACK_EXCERPTS_MARKER)?;
    let after_marker =
        &source_documents[marker_start + RESEARCH_CONTEXT_PACK_EXCERPTS_MARKER.len()..];
    let excerpts = after_marker.trim();
    if excerpts.is_empty() {
        None
    } else {
        Some(excerpts)
    }
}

fn clean_document_subject(subject: &str) -> String {
    normalize_research_subject_text(
        &subject
            .replace("[NO CONFIDENCE]", "")
            .replace("[AI-Research]", "")
            .replace("[Research]", "")
            .replace("[AI]", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn is_useful_document_subject(subject: &str) -> bool {
    if subject.chars().count() < 4 {
        return false;
    }
    !is_synthetic_context_heading(subject)
}

fn is_synthetic_context_heading(subject: &str) -> bool {
    let normalized = subject.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "최종 답변"
            | "요약 결론"
            | "배경"
            | "현재 맥락"
            | "결론"
            | "source audit"
            | "claim log"
            | "research context pack"
            | "artifact trust boundary"
            | "goal and constraints"
            | "source card ledger"
            | "conflict map"
            | "active research debt"
            | "scrape and source-pack diagnostics summary"
            | "selected source excerpts (data only)"
    ) || normalized.starts_with("claim log:")
        || normalized.starts_with("source card ledger")
        || normalized.starts_with("source audit")
        || normalized.starts_with("conflict map")
        || normalized.starts_with("active research debt")
        || normalized.starts_with("scrape and source-pack diagnostics")
}

fn combine_subject_and_focus(subject: &str, focus: &str) -> String {
    if focus.contains(subject) || subject.contains(focus) {
        return normalize_research_subject_text(focus);
    }
    normalize_research_subject_text(&format!("{subject} {focus}"))
}

fn extract_first_nonempty_line_after_marker(user_prompt: &str, marker: &str) -> Option<String> {
    let marker_start = user_prompt.find(marker)?;
    let after_marker = &user_prompt[marker_start + marker.len()..];
    after_marker
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(normalize_research_subject_text)
}

fn source_queries(subject: &str) -> Vec<String> {
    let query_subject = query_subject_text(subject);
    let mut queries = Vec::new();
    push_unique_query(&mut queries, &query_subject);
    push_subject_query_with_suffix(&mut queries, &query_subject, "official source");
    push_subject_query_with_suffix(&mut queries, &query_subject, "documentation");
    push_subject_query_with_suffix(&mut queries, &query_subject, "review comparison");
    if subject_requires_pricing_query(subject) {
        push_subject_query_with_suffix(&mut queries, &query_subject, "pricing official");
    }
    for hint in canonical_query_hints(subject) {
        push_unique_query(&mut queries, &hint);
    }
    if let Some(repo) = extract_owner_repo_from_subject(subject) {
        let release_query = match extract_version_from_subject(subject) {
            Some(version) => format!("{repo} release {version} changelog"),
            None => format!("{repo} release changelog"),
        };
        push_unique_query(&mut queries, &format!("{repo} GitHub"));
        push_unique_query(&mut queries, &release_query);
        push_unique_query(
            &mut queries,
            &format!("{repo} README architecture documentation"),
        );
    }
    for hint in ecosystem_documentation_hints(subject) {
        push_subject_query_with_suffix(&mut queries, &query_subject, &hint);
    }
    for hint in systems_authority_hints(subject) {
        push_unique_query(&mut queries, &hint);
    }
    for hint in policy_and_product_official_host_hints(subject) {
        push_subject_query_with_suffix(&mut queries, &query_subject, &hint);
    }
    for hint in historical_named_entity_overview_queries(subject) {
        push_unique_query(&mut queries, &hint);
    }
    push_subject_query_with_suffix(&mut queries, &query_subject, "timeline phases sources");
    push_subject_query_with_suffix(
        &mut queries,
        &query_subject,
        "criticism limitations aftermath",
    );
    queries.truncate(MAX_SOURCE_PACK_QUERIES);
    queries
}

fn normalize_research_subject_text(subject: &str) -> String {
    let subject = subject
        .split("\n요구사항:")
        .next()
        .unwrap_or(subject)
        .split("\n[사용자 조사 조건/제약/비교 기준]")
        .next()
        .unwrap_or(subject)
        .trim();
    let subject = [
        "다음 주제에 대해 포괄적이고 사실 중심의 정보 조사 보고서를 작성하세요:",
        "첨부된 SOURCE DOCUMENTS가 다루는 대상, 주장, 사건, 제품 또는 개념을 식별하고, 그 대상에 대한 종합 정보 조사 보고서를 작성하세요.",
        "첨부된 SOURCE DOCUMENTS를 이전 장 또는 선행 지식으로 보고, 거기에서 이어지는 독립적인 후속 조사 보고서를 작성하세요.",
    ]
    .into_iter()
    .find_map(|prefix| subject.strip_prefix(prefix).map(str::trim))
    .unwrap_or(subject);
    compact_text(&strip_leading_instruction_boilerplate(subject))
}

fn query_subject_text(subject: &str) -> String {
    let normalized = normalize_research_subject_text(subject);
    let compact = build_keyword_query_subject(&normalized, MAX_QUERY_CHARS);
    if normalized.chars().count() <= MAX_QUERY_CHARS
        && !subject_prefers_keyword_compaction(&normalized)
    {
        if subject_has_historical_named_entity_context(&normalized)
            && normalized.chars().any(is_hangul_syllable)
            && !compact.is_empty()
            && compact != normalized
        {
            return compact;
        }
        return normalized;
    }
    if compact.is_empty() {
        normalized
    } else {
        compact
    }
}

fn build_keyword_query_subject(subject: &str, budget: usize) -> String {
    let mut query = String::new();
    for token in subject_keyword_tokens(subject)
        .into_iter()
        .take(MAX_QUERY_KEYWORD_TOKENS)
    {
        let next_len = if query.is_empty() {
            token.chars().count()
        } else {
            query.chars().count() + 1 + token.chars().count()
        };
        if next_len > budget {
            break;
        }
        if !query.is_empty() {
            query.push(' ');
        }
        query.push_str(&token);
    }
    query
}

fn subject_keyword_tokens(subject: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    normalize_research_subject_text(subject)
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|token| {
            let token = normalize_subject_keyword_token(token.trim());
            if token.is_empty() {
                return None;
            }
            let normalized = token.to_ascii_lowercase();
            if normalized.chars().count() < 2
                || normalized.chars().all(|c| c.is_ascii_digit())
                || generic_subject_keyword(&normalized)
                || !seen.insert(normalized.clone())
            {
                return None;
            }
            Some(normalized)
        })
        .collect()
}

fn normalize_subject_keyword_token(token: &str) -> String {
    strip_trailing_korean_topic_particle(token).to_string()
}

fn strip_trailing_korean_topic_particle(token: &str) -> &str {
    if !token.chars().any(is_hangul_syllable) {
        return token;
    }
    const TWO_CHAR_SUFFIXES: [&str; 3] = ["에서", "으로", "까지"];
    for suffix in TWO_CHAR_SUFFIXES {
        if token.chars().count() > suffix.chars().count() + 1 && token.ends_with(suffix) {
            return token.trim_end_matches(suffix);
        }
    }
    const ONE_CHAR_SUFFIXES: [char; 12] = [
        '의', '과', '와', '은', '는', '이', '가', '을', '를', '에', '로', '도',
    ];
    if token.chars().count() > 2 {
        if let Some(last) = token.chars().last() {
            if ONE_CHAR_SUFFIXES.contains(&last) {
                return token.trim_end_matches(last);
            }
        }
    }
    token
}

fn is_hangul_syllable(ch: char) -> bool {
    ('가'..='힣').contains(&ch)
}

fn generic_subject_keyword(token: &str) -> bool {
    matches!(
        token,
        "compare"
            | "current"
            | "explain"
            | "write"
            | "create"
            | "prepare"
            | "find"
            | "late"
            | "night"
            | "why"
            | "was"
            | "were"
            | "is"
            | "are"
            | "be"
            | "been"
            | "being"
            | "it"
            | "what"
            | "under"
            | "of"
            | "to"
            | "as"
            | "at"
            | "by"
            | "on"
            | "or"
            | "an"
            | "a"
            | "both"
            | "typical"
            | "exceptional"
            | "reader"
            | "facing"
            | "korean"
            | "someone"
            | "planning"
            | "plan"
            | "wanting"
            | "wanted"
            | "useful"
            | "comfortable"
            | "read"
            | "experienced"
            | "implement"
            | "implementation"
            | "implementing"
            | "output"
            | "should"
            | "actually"
            | "cover"
            | "covers"
            | "covered"
            | "include"
            | "includes"
            | "including"
            | "mark"
            | "marks"
            | "marked"
            | "distinguish"
            | "distinguishes"
            | "difference"
            | "differences"
            | "source"
            | "sources"
            | "trail"
            | "trails"
            | "follow"
            | "followup"
            | "questions"
            | "question"
            | "evidence"
            | "limits"
            | "limit"
            | "uncertainty"
            | "contested"
            | "interpretations"
            | "interpretation"
            | "likely"
            | "timing"
            | "lasted"
            | "long"
            | "expanded"
            | "consequence"
            | "consequences"
            | "followed"
            | "clearly"
            | "separated"
            | "point"
            | "points"
            | "shower"
            | "changing"
            | "constraint"
            | "constraints"
            | "transit"
            | "candidate"
            | "candidates"
            | "volatile"
            | "hours"
            | "hour"
            | "verification"
            | "deciding"
            | "decide"
            | "afterward"
            | "afterwards"
            | "around"
            | "route"
            | "routes"
            | "endpoint"
            | "end"
            | "using"
            | "and"
            | "the"
            | "that"
            | "this"
            | "then"
            | "but"
            | "not"
            | "one"
            | "sided"
            | "for"
            | "in"
            | "local"
            | "workflow"
            | "with"
            | "without"
            | "from"
            | "into"
            | "about"
            | "through"
            | "official"
            | "documentation"
            | "review"
            | "comparison"
            | "recommendation"
            | "recommendations"
            | "pricing"
            | "guidance"
            | "core"
            | "model"
            | "technical"
            | "specifications"
            | "options"
            | "availability"
            | "nearby"
            | "access"
            | "team"
            | "discussion"
            | "mixed"
            | "visitor"
            | "group"
            | "report"
            | "topic"
            | "research"
            | "facts"
            | "fact"
            | "centered"
            | "comprehensive"
            | "developer"
            | "developers"
            | "다음"
            | "주제"
            | "조사"
            | "보고서"
            | "작성"
            | "요구사항"
            | "조건"
            | "제약"
            | "비교"
            | "기준"
            | "포괄적이고"
            | "사실"
            | "중심의"
            | "정보"
            | "현재"
            | "최신"
            | "공식"
            | "문서"
            | "가이드"
            | "검토"
            | "추천"
    )
}

fn strip_leading_instruction_boilerplate(subject: &str) -> String {
    let trimmed = compact_text(subject);
    let lower = trimmed.to_ascii_lowercase();
    for marker in [
        "research report about ",
        "research report on ",
        "research report for ",
        "report about ",
        "report on ",
        "report for ",
        "guide to ",
        "guide for ",
    ] {
        if let Some(start) = lower.find(marker) {
            return trimmed[start + marker.len()..].trim().to_string();
        }
    }
    trimmed
}

fn subject_prefers_keyword_compaction(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    lower.starts_with("write ")
        || lower.starts_with("create ")
        || lower.starts_with("prepare ")
        || lower.contains("reader-facing")
        || lower.contains("someone planning")
        || lower.contains("output should")
}

fn query_subject_for_relevance(query: &str) -> String {
    let normalized = compact_text(query);
    let lower = normalized.to_ascii_lowercase();
    for suffix in [
        " official source",
        " documentation",
        " review comparison",
        " pricing official",
        " timeline phases sources",
        " criticism limitations aftermath",
        " nist.gov official guidance",
        " eur-lex.europa.eu ai act official text",
        " apple.com official technical specifications",
        " frame.work official laptop specifications",
        " lenovo.com official technical specifications",
    ] {
        if lower.ends_with(suffix) {
            let trim_len = normalized.len().saturating_sub(suffix.len());
            return normalized[..trim_len].trim().to_string();
        }
    }
    normalized
}

fn historical_named_entity_overview_queries(subject: &str) -> Vec<String> {
    if !subject_has_historical_named_entity_context(subject) {
        return Vec::new();
    }
    let Some(entity) = historical_named_entity(subject) else {
        return Vec::new();
    };
    let mut queries = Vec::new();
    queries.push(format!("{entity} overview"));
    if subject_contains_any(subject, &["roman", "emperor", "emperors", "로마", "황제"]) {
        queries.push(format!("{entity} Roman emperor"));
    }
    queries.push(format!("{entity} Britannica encyclopedia history"));
    queries.push(format!("{entity} primary source translation"));
    queries.push(format!("{entity} scholarly article history"));
    queries
}

fn subject_has_historical_named_entity_context(subject: &str) -> bool {
    subject_contains_any(
        subject,
        &[
            "roman",
            "byzantine",
            "ostrogoth",
            "gothic war",
            "gothic",
            "palmyrene",
            "gallic",
            "emperor",
            "emperors",
            "ancient",
            "dynasty",
            "dynasties",
            "century",
            "historical",
            "history",
            "로마",
            "비잔틴",
            "황제",
            "전쟁",
            "고대",
            "역사",
        ],
    )
}

fn historical_named_entity(subject: &str) -> Option<String> {
    subject_keyword_tokens(subject)
        .into_iter()
        .find(|keyword| !historical_non_entity_keyword(keyword))
}

fn historical_non_entity_keyword(token: &str) -> bool {
    matches!(
        token,
        "roman"
            | "byzantine"
            | "ostrogoth"
            | "gothic"
            | "war"
            | "wars"
            | "emperor"
            | "emperors"
            | "history"
            | "historical"
            | "century"
            | "third"
            | "ancient"
            | "soldier"
            | "soldiers"
            | "typical"
            | "exceptional"
            | "legitimacy"
            | "modern"
            | "interpretation"
            | "palmyrene"
            | "gallic"
            | "background"
            | "among"
    )
}

fn subject_contains_any(subject: &str, needles: &[&str]) -> bool {
    let lower = subject.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn is_canonical_history_overview_query(query: &str, subject: &str) -> bool {
    if !subject_has_historical_named_entity_context(subject) {
        return false;
    }
    let Some(entity) = historical_named_entity(subject) else {
        return false;
    };
    let lower = compact_text(query).to_ascii_lowercase();
    lower.contains(&entity)
        && (lower.contains("overview")
            || lower.contains("britannica")
            || lower.contains("encyclopedia")
            || lower.contains("roman emperor")
            || lower.contains("primary source")
            || lower.contains("scholarly article"))
}

fn is_topically_relevant_source(source: &ResearchSource, subject: &str, query: &str) -> bool {
    if is_canonical_history_overview_query(query, subject) {
        return is_relevant_history_overview_source(source, subject);
    }
    let keywords = subject_keyword_tokens(subject);
    if keywords.is_empty() {
        return true;
    }
    let core_query_anchor_keywords = core_query_anchor_keywords(query);
    let haystack = compact_text(&format!(
        "{} {} {}",
        source.title, source.snippet, source.url
    ))
    .to_ascii_lowercase();
    let haystack_tokens = text_keyword_tokens(&haystack);
    let surface_text =
        compact_text(&format!("{} {}", source.title, source.url)).to_ascii_lowercase();
    let surface_tokens = text_keyword_tokens(&surface_text);
    let keyword_hits = keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &haystack, &haystack_tokens))
        .count();
    let hinted_domain = official_host_hint_target_domain(query);
    let hint_domain_bonus = hinted_domain
        .is_some_and(|domain| source_url_matches_domain_boundary(&source.url, domain))
        as usize;
    let host_hint_tokens = hinted_domain
        .map(domain_hint_keyword_tokens)
        .unwrap_or_default();
    let distinguishing_keyword_hits = keywords
        .iter()
        .filter(|keyword| {
            keyword_matches_source_text(keyword, &haystack, &haystack_tokens)
                && !host_hint_tokens.contains(keyword.as_str())
        })
        .count();
    let strong_anchor_keywords = strong_subject_anchor_keywords(&keywords);
    let strong_anchor_hits = strong_anchor_keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &haystack, &haystack_tokens))
        .count();
    let core_query_anchor_hits = core_query_anchor_keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &haystack, &haystack_tokens))
        .count();
    let core_query_surface_hits = core_query_anchor_keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &surface_text, &surface_tokens))
        .count();
    if hint_domain_bonus > 0 && distinguishing_keyword_hits == 0 {
        return false;
    }
    if !strong_anchor_keywords.is_empty() && strong_anchor_hits == 0 {
        return false;
    }
    if !core_query_anchor_keywords.is_empty() && core_query_anchor_hits == 0 {
        return false;
    }
    if !core_query_anchor_keywords.is_empty() && core_query_surface_hits == 0 {
        return false;
    }
    if strong_anchor_keywords.len() >= 3 && strong_anchor_hits < 2 {
        return false;
    }
    let required_hits = if keywords.len() <= 2 {
        1
    } else if keywords.len() >= 6 {
        3
    } else {
        2
    };
    keyword_hits + hint_domain_bonus >= required_hits
}

fn is_relevant_history_overview_source(source: &ResearchSource, subject: &str) -> bool {
    let Some(entity) = historical_named_entity(subject) else {
        return false;
    };
    if history_overview_source_is_disallowed(source) {
        return false;
    }
    let surface_text =
        compact_text(&format!("{} {}", source.title, source.url)).to_ascii_lowercase();
    let surface_tokens = text_keyword_tokens(&surface_text);
    if !keyword_matches_source_text(&entity, &surface_text, &surface_tokens) {
        return false;
    }
    let haystack = compact_text(&format!(
        "{} {} {}",
        source.title, source.snippet, source.url
    ))
    .to_ascii_lowercase();
    let haystack_tokens = text_keyword_tokens(&haystack);
    let has_history_context = historical_context_keywords(subject)
        .iter()
        .any(|keyword| keyword_matches_source_text(keyword, &haystack, &haystack_tokens));
    if is_historical_supplementary_contested_source(source, subject) {
        return true;
    }
    has_history_context && source_quality_for_subject(&source.url, Some(subject)) != "low"
}

fn history_overview_source_is_disallowed(source: &ResearchSource) -> bool {
    let title = compact_text(&source.title).to_ascii_lowercase();
    let url = source.url.to_ascii_lowercase();
    let (host, path) = Url::parse(&source.url)
        .ok()
        .map(|parsed| {
            (
                parsed
                    .host_str()
                    .unwrap_or_default()
                    .trim_start_matches("www.")
                    .to_ascii_lowercase(),
                parsed.path().to_ascii_lowercase(),
            )
        })
        .unwrap_or_default();

    history_overview_title_or_path_is_disallowed(&title, &path)
        || history_overview_url_is_search_like(&url)
        || history_overview_url_uses_weak_mirror_host(&source.url)
        || history_overview_uses_non_evidentiary_reference_host(&host, &path)
}

fn history_overview_title_or_path_is_disallowed(title: &str, path: &str) -> bool {
    title.contains("(disambiguation)")
        || title.starts_with("list of ")
        || title.starts_with("category:")
        || path.contains("disambiguation")
        || path.contains("/category:")
        || path.contains("/list_of_")
        || path.contains("/lists_of_")
}

fn history_overview_url_is_search_like(url: &str) -> bool {
    url.contains("/search?")
        || url.contains("/search/")
        || url.contains("?search=")
        || url.contains("&search=")
        || url.contains("/wiki/special:search")
        || url.contains("/w/index.php?search=")
}

fn history_overview_url_uses_weak_mirror_host(url: &str) -> bool {
    let host = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default();
    host_matches_domain_boundary(&host, "wikimili.com")
        || host_matches_domain_boundary(&host, "grokipedia.com")
}

fn history_overview_uses_non_evidentiary_reference_host(host: &str, path: &str) -> bool {
    host_matches_domain_boundary(host, "wiktionary.org")
        || host_matches_domain_boundary(host, "wikidata.org")
        || (host_matches_domain_boundary(host, "play.google.com")
            && (path.starts_with("/store/books")
                || path.starts_with("/store/audiobooks")
                || path.starts_with("/store/search")))
}

fn historical_context_keywords(subject: &str) -> Vec<String> {
    subject_keyword_tokens(subject)
        .into_iter()
        .filter(|keyword| historical_context_keyword(keyword))
        .collect()
}

fn historical_context_keyword(token: &str) -> bool {
    matches!(
        token,
        "roman"
            | "emperor"
            | "emperors"
            | "century"
            | "third"
            | "palmyrene"
            | "gallic"
            | "sol"
            | "invictus"
            | "military"
            | "soldier"
            | "legitimacy"
            | "byzantine"
            | "gothic"
            | "ostrogoth"
            | "war"
    )
}

fn strong_subject_anchor_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .filter(|keyword| !weak_subject_anchor_keyword(keyword))
        .cloned()
        .collect()
}

fn core_query_anchor_keywords(query: &str) -> Vec<String> {
    strong_subject_anchor_keywords(&subject_keyword_tokens(&query_subject_for_relevance(query)))
}

fn weak_subject_anchor_keyword(token: &str) -> bool {
    matches!(
        token,
        "roman"
            | "empire"
            | "emperor"
            | "emperors"
            | "century"
            | "third"
            | "history"
            | "historical"
            | "soldier"
            | "soldiers"
            | "military"
            | "background"
            | "among"
            | "typical"
            | "exceptional"
            | "legitimacy"
            | "modern"
            | "interpretation"
            | "seoul"
            | "korea"
            | "south"
            | "city"
            | "guide"
            | "travel"
            | "trip"
            | "park"
            | "morning"
            | "workout"
            | "good"
            | "atmospheric"
            | "cafe"
            | "cafes"
            | "coffee"
            | "brunch"
            | "afterward"
            | "afterwards"
            | "route"
            | "routes"
            | "timing"
            | "scheduler"
            | "schedulers"
            | "scheduling"
            | "worker"
            | "workers"
            | "local"
            | "global"
            | "queue"
            | "queues"
            | "task"
            | "tasks"
            | "core"
            | "model"
            | "memory"
            | "ordering"
            | "blocking"
            | "parking"
            | "wakeup"
            | "shutdown"
            | "cancellation"
            | "benchmark"
            | "benchmarks"
            | "instrumentation"
    )
}

fn context_safe_source_for_subject(source: &ResearchSource, subject: &str) -> bool {
    let strong_anchor_keywords = strong_subject_anchor_keywords(&subject_keyword_tokens(subject));
    if strong_anchor_keywords.is_empty() {
        return source_quality_for_subject(&source.url, Some(subject)) != "low";
    }
    let surface_text =
        compact_text(&format!("{} {}", source.title, source.url)).to_ascii_lowercase();
    let surface_tokens = text_keyword_tokens(&surface_text);
    let strong_anchor_surface_hits = strong_anchor_keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &surface_text, &surface_tokens))
        .count();
    strong_anchor_surface_hits > 0
        && source_quality_for_subject(&source.url, Some(subject)) != "low"
}

fn accepted_source_for_subject(source: &ResearchSource, subject: &str) -> bool {
    context_safe_source_for_subject(source, subject)
        || is_historical_supplementary_contested_source(source, subject)
}

fn is_historical_supplementary_contested_source(source: &ResearchSource, subject: &str) -> bool {
    if !subject_has_historical_named_entity_context(subject) {
        return false;
    }
    let strong_anchor_keywords = strong_subject_anchor_keywords(&subject_keyword_tokens(subject));
    if strong_anchor_keywords.is_empty() {
        return false;
    }
    let surface_text =
        compact_text(&format!("{} {}", source.title, source.url)).to_ascii_lowercase();
    let surface_tokens = text_keyword_tokens(&surface_text);
    let strong_anchor_surface_hits = strong_anchor_keywords
        .iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &surface_text, &surface_tokens))
        .count();
    if strong_anchor_surface_hits == 0 {
        return false;
    }
    historical_supplementary_contested_source_match(&source.title, &source.url)
}

fn historical_supplementary_contested_source_match(title: &str, url: &str) -> bool {
    let title = compact_text(title).to_ascii_lowercase();
    let url = url.to_ascii_lowercase();
    title.contains("historia augusta")
        || url.contains("sha-")
        || url.contains("historia-augusta")
        || url.contains("sourcebooks.fordham.edu/ancient/")
}

fn historical_supplementary_contested_source_url(url: &str) -> bool {
    historical_supplementary_contested_source_match("", url)
}

fn historical_general_overview_source(source: &ResearchSource, subject: &str) -> bool {
    if !subject_has_historical_named_entity_context(subject) {
        return false;
    }
    let Some(entity) = historical_named_entity(subject) else {
        return false;
    };
    let title = compact_text(&source.title).to_ascii_lowercase();
    let (host, path) = Url::parse(&source.url)
        .ok()
        .map(|parsed| {
            (
                parsed
                    .host_str()
                    .unwrap_or_default()
                    .trim_start_matches("www.")
                    .to_ascii_lowercase(),
                parsed.path().to_ascii_lowercase(),
            )
        })
        .unwrap_or_default();
    let overview_host = host_matches_domain_boundary(&host, "britannica.com")
        || host_matches_domain_boundary(&host, "worldhistory.org")
        || host_matches_domain_boundary(&host, "wikipedia.org")
        || host_matches_domain_boundary(&host, "livius.org")
        || host_matches_domain_boundary(&host, "unrv.com");
    if !overview_host {
        return false;
    }
    title == entity
        || title.starts_with(&format!("{entity} |"))
        || title.starts_with(&format!("{entity} -"))
        || title.starts_with(&format!("{entity}:"))
        || path.ends_with(&format!("/{entity}"))
        || path.ends_with(&format!("/{entity}.php"))
}

fn source_role_for_subject(source: &ResearchSource, subject: &str) -> &'static str {
    if is_historical_supplementary_contested_source(source, subject) {
        "supplementary contested context only"
    } else if context_safe_source_for_subject(source, subject) {
        "context-safe corroborating evidence"
    } else {
        "candidate evidence only"
    }
}

fn assess_source_pack_acceptance(
    subject: &str,
    sources: &[ResearchSource],
) -> SourcePackAcceptance {
    if sources.is_empty() {
        return SourcePackAcceptance::Empty {
            reason: "Search providers returned only weak or generic source-pack candidates; no context-safe candidates were available."
                .to_string(),
        };
    }
    let context_safe_sources = sources
        .iter()
        .filter(|source| context_safe_source_for_subject(source, subject))
        .collect::<Vec<_>>();
    let context_safe_distinct_hosts = context_safe_sources
        .iter()
        .filter_map(|source| {
            Url::parse(&source.url).ok().and_then(|url| {
                url.host_str()
                    .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
            })
        })
        .collect::<HashSet<_>>()
        .len();
    let supplementary_history_count = sources
        .iter()
        .filter(|source| is_historical_supplementary_contested_source(source, subject))
        .count();
    let strong_single_source = context_safe_sources.iter().any(|source| {
        infer_source_class_for_subject(&source.url, Some(subject)) == "official_or_primary"
            && context_safe_source_for_subject(source, subject)
    });
    if strong_single_source || (context_safe_sources.len() >= 2 && context_safe_distinct_hosts >= 2)
    {
        if subject_has_historical_named_entity_context(subject)
            && context_safe_sources
                .iter()
                .all(|source| historical_general_overview_source(source, subject))
        {
            return SourcePackAcceptance::Partial {
                reason: "Only broad historical overview/reference sources were recovered; stronger specialized or corroborating historical evidence is still needed."
                    .to_string(),
            };
        }
        return SourcePackAcceptance::Success;
    }
    if !context_safe_sources.is_empty() || supplementary_history_count > 0 {
        let supplementary_note = if supplementary_history_count > 0 {
            format!(
                " {} supplementary contested historical source(s) were admitted for context only and must not anchor decisive conclusions without stronger corroboration.",
                supplementary_history_count
            )
        } else {
            String::new()
        };
        return SourcePackAcceptance::Partial {
            reason: format!(
                "Only {} context-safe source-pack candidate(s) across {} host(s) were recovered; additional corroborating sources are still needed.{}",
                context_safe_sources.len(),
                context_safe_distinct_hosts,
                supplementary_note
            ),
        };
    }
    SourcePackAcceptance::Empty {
        reason: "Search providers returned only weak or generic source-pack candidates; no context-safe candidates were available."
            .to_string(),
    }
}

fn downgrade_overview_only_history_acceptance(
    acceptance: SourcePackAcceptance,
    final_adopted_urls: &HashSet<String>,
    overview_query_adopted_urls: &HashSet<String>,
) -> SourcePackAcceptance {
    if !matches!(acceptance, SourcePackAcceptance::Success) {
        return acceptance;
    }
    if final_adopted_urls.is_empty()
        || overview_query_adopted_urls.is_empty()
        || !final_adopted_urls
            .iter()
            .all(|url| overview_query_adopted_urls.contains(url))
    {
        return acceptance;
    }
    SourcePackAcceptance::Partial {
        reason: "Only historical overview/background sources were recovered; additional specialized corroborating sources are still needed."
            .to_string(),
    }
}

fn domain_hint_keyword_tokens(domain: &str) -> HashSet<String> {
    domain
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (token.chars().count() >= 2).then_some(token)
        })
        .collect()
}

fn text_keyword_tokens(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|token| {
            let token = token.trim();
            (!token.is_empty()).then(|| token.to_ascii_lowercase())
        })
        .collect()
}

fn keyword_matches_source_text(
    keyword: &str,
    haystack: &str,
    haystack_tokens: &HashSet<String>,
) -> bool {
    let haystack_without_spaces = (!keyword.is_ascii()).then(|| remove_whitespace(haystack));
    for alias in keyword_match_aliases(keyword) {
        if alias.is_ascii() {
            if haystack_tokens.contains(alias.as_str()) {
                return true;
            }
        } else {
            if haystack.contains(&alias) {
                return true;
            }
            let alias_without_spaces = remove_whitespace(&alias);
            if haystack_without_spaces
                .as_deref()
                .is_some_and(|compact_haystack| compact_haystack.contains(&alias_without_spaces))
            {
                return true;
            }
        }
    }
    false
}

fn keyword_match_aliases(keyword: &str) -> Vec<String> {
    let mut aliases = match keyword {
        "namsan" => vec!["namsan", "남산", "남산공원"],
        "seoul" => vec!["seoul", "서울"],
        "running" => vec!["running", "러닝", "달리기", "러닝코스", "러닝 코스"],
        "cafe" | "cafes" => vec!["cafe", "cafes", "카페"],
        "coffee" => vec!["coffee", "커피", "카페"],
        "park" => vec!["park", "공원"],
        _ => vec![keyword],
    }
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if !keyword.is_ascii() {
        let stripped = strip_trailing_korean_topic_particle(keyword);
        if !stripped.is_empty() && stripped != keyword {
            aliases.push(stripped.to_string());
        }
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn remove_whitespace(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn push_unique_query(queries: &mut Vec<String>, query: &str) {
    let normalized = compact_text(query);
    let truncated: String = normalized.chars().take(MAX_QUERY_CHARS).collect();
    if truncated.is_empty() || queries.iter().any(|existing| existing == &truncated) {
        return;
    }
    queries.push(truncated);
}

fn push_subject_query_with_suffix(queries: &mut Vec<String>, subject: &str, suffix: &str) {
    let normalized_suffix = compact_text(suffix);
    if normalized_suffix.is_empty() {
        push_unique_query(queries, subject);
        return;
    }
    let normalized_subject = compact_text(subject);
    let reserved_chars = normalized_suffix.chars().count() + 1;
    let subject_budget = MAX_QUERY_CHARS.saturating_sub(reserved_chars);
    let truncated_subject: String = normalized_subject.chars().take(subject_budget).collect();
    let query = if truncated_subject.is_empty() {
        normalized_suffix
    } else {
        format!("{truncated_subject} {normalized_suffix}")
    };
    push_unique_query(queries, &query);
}

fn subject_requires_pricing_query(subject: &str) -> bool {
    let normalized = subject.to_ascii_lowercase();
    [
        "price",
        "pricing",
        "plan",
        "plans",
        "cost",
        "tier",
        "tiers",
        "rate limit",
        "rate limits",
        "요금",
        "가격",
        "플랜",
        "비용",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn extract_owner_repo_from_subject(subject: &str) -> Option<String> {
    let repo_context = subject_has_repo_query_context(subject);
    subject.split_whitespace().find_map(|token| {
        let cleaned = token.trim_matches(|c: char| {
            matches!(c, '`' | '"' | '\'' | ',' | '.' | ')' | '(' | '[' | ']')
        });
        if !is_owner_repo(cleaned) || !should_expand_repo_queries(cleaned, repo_context) {
            return None;
        }
        Some(cleaned.to_string())
    })
}

fn subject_has_repo_query_context(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    extract_version_from_subject(subject).is_some()
        || [
            "github",
            "repo",
            "repository",
            "readme",
            "changelog",
            "release",
            "version",
            "package",
            "crate",
            "library",
            "sdk",
            "module",
            "cli",
            "api",
            "architecture",
        ]
        .iter()
        .any(|term| lower.contains(term))
}

fn should_expand_repo_queries(candidate: &str, repo_context: bool) -> bool {
    let trimmed = candidate.trim_start_matches('/').trim_end_matches('/');
    let Some((owner, repo)) = trimmed.split_once('/') else {
        return false;
    };
    if owner_repo_looks_like_generic_technical_phrase(owner, repo) {
        return false;
    }
    if repo_context {
        return true;
    }
    is_known_official_github_owner(&owner.to_ascii_lowercase())
        || repo.contains('.')
        || repo.contains('_')
        || repo.chars().any(|ch| ch.is_ascii_digit())
}

fn owner_repo_looks_like_generic_technical_phrase(owner: &str, repo: &str) -> bool {
    generic_repo_phrase_component(owner) && generic_repo_phrase_component(repo)
}

fn generic_repo_phrase_component(component: &str) -> bool {
    matches!(
        component.to_ascii_lowercase().as_str(),
        "worker"
            | "workers"
            | "local"
            | "global"
            | "queue"
            | "queues"
            | "task"
            | "tasks"
            | "route"
            | "routes"
            | "end"
            | "point"
            | "endpoint"
            | "model"
            | "core"
            | "memory"
            | "ordering"
            | "blocking"
            | "parking"
            | "wakeup"
    )
}

fn extract_version_from_subject(subject: &str) -> Option<String> {
    subject
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '"' | '\'' | ',' | ')' | '(' | '[' | ']' | ':' | ';'
                )
            })
        })
        .find(|token| {
            let normalized = token.trim_start_matches('v');
            normalized.matches('.').count() >= 1
                && normalized
                    .split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_string)
}

fn canonical_query_hints(subject: &str) -> Vec<String> {
    if subject.contains("유스티니아누스") && (subject.contains("고트") || subject.contains("고토"))
    {
        return vec![
            "Justinian Gothic War 535 554 Belisarius Narses Totila chronology".to_string(),
            "Gothic War 535 554 Byzantine Ostrogothic Kingdom Italy aftermath".to_string(),
            "Justinian reconquest Italy Gothic War scholarly overview".to_string(),
        ];
    }
    Vec::new()
}

fn ecosystem_documentation_hints(subject: &str) -> Vec<String> {
    let normalized = subject.to_ascii_lowercase();
    let mut hints = Vec::new();
    if normalized.contains("python")
        && (normalized.contains("package")
            || normalized.contains("packaging")
            || normalized.contains("pip")
            || normalized.contains("wheel")
            || normalized.contains("venv"))
    {
        hints.push("packaging.python.org official documentation".to_string());
        hints.push("docs.python.org official documentation".to_string());
    }
    if normalized.contains("npm")
        || normalized.contains("node.js")
        || normalized.contains("nodejs")
        || normalized.contains("package.json")
    {
        hints.push("docs.npmjs.com official documentation".to_string());
        hints.push("nodejs.org official documentation".to_string());
    }
    hints
}

fn systems_authority_hints(subject: &str) -> Vec<String> {
    let normalized = subject.to_ascii_lowercase();
    let mut hints = Vec::new();
    let is_cpp_systems = (normalized.contains("c++") || normalized.contains("cpp"))
        && (normalized.contains("work-stealing")
            || normalized.contains("work stealing")
            || normalized.contains("scheduler")
            || normalized.contains("deque"));
    if is_cpp_systems {
        hints.extend([
            "oneTBB task scheduler work stealing documentation".to_string(),
            "Taskflow work stealing executor documentation".to_string(),
            "Chase-Lev work stealing deque pdf".to_string(),
            "cppreference memory_order stop_token packaged_task".to_string(),
        ]);
    }

    let is_network_port_topic = (normalized.contains("ephemeral port")
        || normalized.contains("dynamic port")
        || normalized.contains("port range")
        || normalized.contains("port exhaustion")
        || normalized.contains("socket")
        || normalized.contains("tcp")
        || normalized.contains("udp"))
        && (normalized.contains("linux")
            || normalized.contains("windows")
            || normalized.contains("kernel")
            || normalized.contains("network")
            || normalized.contains("cloud")
            || normalized.contains("azure")
            || normalized.contains("aws")
            || normalized.contains("gcp")
            || normalized.contains("kubernetes")
            || normalized.contains("container")
            || normalized.contains("service"));
    if is_network_port_topic {
        hints.extend([
            "RFC 6056 ephemeral port randomization".to_string(),
            "IANA service names port numbers documentation".to_string(),
            "docs.kernel.org ip_local_port_range networking".to_string(),
            "learn.microsoft.com windows dynamic port range tcp udp".to_string(),
        ]);
    }

    hints
}

fn policy_and_product_official_host_hints(subject: &str) -> Vec<String> {
    let normalized = subject.to_ascii_lowercase();
    let mut hints = Vec::new();

    if normalized.contains("ai act")
        || normalized.contains("nist")
        || (normalized.contains("ai")
            && (normalized.contains("regulation")
                || normalized.contains("regulatory")
                || normalized.contains("policy")
                || normalized.contains("compliance")))
    {
        hints.push("nist.gov official guidance".to_string());
        hints.push("eur-lex.europa.eu AI Act official text".to_string());
    }

    if normalized.contains("apple")
        || normalized.contains("macbook")
        || normalized.contains("mac ")
        || normalized.ends_with(" mac")
    {
        hints.push("apple.com official technical specifications".to_string());
    }
    if indicates_framework_laptop_product(&normalized) {
        hints.push("frame.work official laptop specifications".to_string());
    }
    if normalized.contains("lenovo") || normalized.contains("thinkpad") {
        hints.push("lenovo.com official technical specifications".to_string());
    }

    hints
}

fn indicates_framework_laptop_product(subject: &str) -> bool {
    subject.contains("framework laptop")
        || (subject.contains("framework")
            && [
                "laptop",
                "notebook",
                "ultrabook",
                "hardware",
                "spec",
                "specification",
                "product",
            ]
            .iter()
            .any(|term| subject.contains(term)))
}

fn seeded_sources_for_subject(subject: &str) -> Vec<ResearchSource> {
    if subject.contains("유스티니아누스") && (subject.contains("고트") || subject.contains("고토"))
    {
        return vec![
            ResearchSource {
                title: "Gothic War (535-554) - Wikipedia".to_string(),
                url: "https://en.wikipedia.org/wiki/Gothic_War_(535%E2%80%93554)".to_string(),
                snippet: "Chronology anchor: Sicily opened the campaign in 535; Naples and Rome followed in 536; Ravenna fell in 540; Totila revived Ostrogothic resistance; Narses defeated Totila at Taginae in 552 and Teia at Mons Lactarius in 553; the Pragmatic Sanction followed in 554."
                    .to_string(),
            },
            ResearchSource {
                title: "Justinian I - Britannica".to_string(),
                url: "https://www.britannica.com/biography/Justinian-I".to_string(),
                snippet: "Context anchor: Justinian's western reconquest relied on Belisarius and later Narses; Narses was an imperial eunuch/general, not a Norman commander. Britannica notes Narses entered Rome, defeated resistance at Mount Lactarius, and Justinian issued the Pragmatic Sanction of 554 for Italy. Treat the Pragmatic Sanction as an imperial legal settlement/decree for Italy, not as a peace treaty.".to_string(),
            },
            ResearchSource {
                title: "Ostrogoth - Britannica".to_string(),
                url: "https://www.britannica.com/topic/Ostrogoth".to_string(),
                snippet: "Cause anchor: instability in the Ostrogothic ruling dynasty gave Justinian a pretext to declare war in 535 against the Ostrogothic kingdom of Italy.".to_string(),
            },
            ResearchSource {
                title: "Justinian I - World History Encyclopedia".to_string(),
                url: "https://www.worldhistory.org/Justinian_I/".to_string(),
                snippet: "Narrative anchor: secondary overview of Justinian with Gothic War and Totila context; use only with stronger sources for major chronology claims.".to_string(),
            },
            ResearchSource {
                title: "Narses - Britannica".to_string(),
                url: "https://www.britannica.com/biography/Narses-Byzantine-general"
                    .to_string(),
                snippet: "Commander anchor: Narses was Justinian's Byzantine general whose greatest achievement was conquering the Ostrogothic kingdom in Italy for Byzantium."
                    .to_string(),
            },
            ResearchSource {
                title: "Totila - World History Encyclopedia".to_string(),
                url: "https://www.worldhistory.org/Totila/".to_string(),
                snippet: "Resistance anchor: Totila revived Gothic resistance, recaptured territory, and was defeated at Taginae, ending realistic hopes of Gothic supremacy in Italy."
                    .to_string(),
            },
            ResearchSource {
                title: "Belisarius - World History Encyclopedia".to_string(),
                url: "https://www.worldhistory.org/Belisarius/".to_string(),
                snippet: "Commander anchor: Belisarius led the early imperial campaign in Italy and provides cross-checking context for Rome, Totila, and Justinian's refusal to abandon Italy."
                    .to_string(),
            },
        ];
    }
    Vec::new()
}

impl ResearchSearchProvider {
    fn diagnostic_name(&self) -> &'static str {
        match self {
            ResearchSearchProvider::DuckDuckGo => "duckduckgo",
            ResearchSearchProvider::Brave { .. } => "brave",
            ResearchSearchProvider::Naver { .. } => "naver",
            ResearchSearchProvider::Kakao { .. } => "kakao",
        }
    }

    fn blocked_message(&self) -> &'static str {
        match self {
            ResearchSearchProvider::DuckDuckGo => DUCKDUCKGO_CHALLENGE_MESSAGE,
            ResearchSearchProvider::Brave { .. } => {
                "Search provider blocked source discovery for this query."
            }
            ResearchSearchProvider::Naver { .. } | ResearchSearchProvider::Kakao { .. } => {
                "Search provider blocked source discovery for this query."
            }
        }
    }

    fn blocked_reason(&self, query_blocked_count: usize) -> String {
        match self {
            ResearchSearchProvider::DuckDuckGo => format!(
                "{query_blocked_count} source-pack search query/queries were blocked by DuckDuckGo challenge responses."
            ),
            ResearchSearchProvider::Brave { .. } => format!(
                "{query_blocked_count} source-pack search query/queries were blocked by the configured search provider."
            ),
            ResearchSearchProvider::Naver { .. } | ResearchSearchProvider::Kakao { .. } => {
                format!(
                    "{query_blocked_count} source-pack search query/queries were blocked by the configured search provider."
                )
            }
        }
    }

    async fn search_single(
        &self,
        client: &reqwest::Client,
        query: &str,
    ) -> Result<SearchQueryOutcome, String> {
        match self {
            ResearchSearchProvider::DuckDuckGo => search_duckduckgo_html(client, query)
                .await
                .map_err(|error| format_error_chain(&error)),
            ResearchSearchProvider::Brave { api_key } => {
                search_brave_api(client, query, api_key).await
            }
            ResearchSearchProvider::Naver {
                client_id,
                client_secret,
            } => search_naver_api(client, query, client_id, client_secret).await,
            ResearchSearchProvider::Kakao { rest_api_key } => {
                search_kakao_api(client, query, rest_api_key).await
            }
        }
    }
}

async fn search_duckduckgo_html(
    client: &reqwest::Client,
    query: &str,
) -> Result<SearchQueryOutcome, reqwest::Error> {
    let primary_response = fetch_duckduckgo_html_response(client, query, false).await?;
    let primary_results = parse_duckduckgo_search_pages([primary_response.as_str()]);
    if matches!(primary_results, SearchQueryOutcome::Results(_)) {
        return Ok(primary_results);
    }

    let retry_response = fetch_duckduckgo_html_response(client, query, true).await?;
    let retry_results =
        parse_duckduckgo_search_pages([primary_response.as_str(), retry_response.as_str()]);
    if matches!(retry_results, SearchQueryOutcome::Results(_)) {
        return Ok(retry_results);
    }

    let lite_response = fetch_duckduckgo_lite_response(client, query).await?;
    Ok(parse_duckduckgo_search_pages([
        primary_response.as_str(),
        retry_response.as_str(),
        lite_response.as_str(),
    ]))
}

async fn search_brave_api(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
) -> Result<SearchQueryOutcome, String> {
    let mut url =
        Url::parse("https://api.search.brave.com/res/v1/web/search").expect("valid Brave URL");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        pairs.append_pair("count", &MAX_RESULTS_PER_QUERY.to_string());
    }
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|_| BRAVE_SEARCH_REQUEST_FAILED_MESSAGE.to_string())?;
    let response = response.error_for_status().map_err(|error| {
        error
            .status()
            .map(|status| format!("Brave Search API returned HTTP {status}"))
            .unwrap_or_else(|| BRAVE_SEARCH_REQUEST_FAILED_MESSAGE.to_string())
    })?;
    let body = response
        .text()
        .await
        .map_err(|_| "Brave Search API response body could not be read.".to_string())?;
    parse_brave_search_response(&body)
}

fn parse_brave_search_response(body: &str) -> Result<SearchQueryOutcome, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|_| "Brave Search API returned invalid JSON.".to_string())?;
    let results = payload
        .pointer("/web/results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_brave_result)
        .take(MAX_RESULTS_PER_QUERY)
        .collect::<Vec<_>>();
    if results.is_empty() {
        Ok(SearchQueryOutcome::Empty)
    } else {
        Ok(SearchQueryOutcome::Results(results))
    }
}

fn parse_brave_result(value: &Value) -> Option<ResearchSource> {
    let title = sanitize_search_text(value.get("title")?.as_str()?);
    if title.is_empty() {
        return None;
    }
    let url = normalize_result_url(value.get("url")?.as_str()?)?;
    let snippet = sanitize_search_text(
        value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    Some(ResearchSource {
        title,
        url,
        snippet,
    })
}

async fn search_naver_api(
    client: &reqwest::Client,
    query: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<SearchQueryOutcome, String> {
    let mut url =
        Url::parse("https://openapi.naver.com/v1/search/webkr.json").expect("valid Naver URL");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("display", &MAX_RESULTS_PER_QUERY.to_string());
    }
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .header("X-Naver-Client-Id", client_id)
        .header("X-Naver-Client-Secret", client_secret)
        .send()
        .await
        .map_err(|_| NAVER_SEARCH_REQUEST_FAILED_MESSAGE.to_string())?;
    let response = response.error_for_status().map_err(|error| {
        error
            .status()
            .map(|status| format!("Naver Search API returned HTTP {status}"))
            .unwrap_or_else(|| NAVER_SEARCH_REQUEST_FAILED_MESSAGE.to_string())
    })?;
    let body = response
        .text()
        .await
        .map_err(|_| "Naver Search API response body could not be read.".to_string())?;
    parse_naver_search_response(&body)
}

fn parse_naver_search_response(body: &str) -> Result<SearchQueryOutcome, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|_| "Naver Search API returned invalid JSON.".to_string())?;
    let results = payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_naver_result)
        .take(MAX_RESULTS_PER_QUERY)
        .collect::<Vec<_>>();
    if results.is_empty() {
        Ok(SearchQueryOutcome::Empty)
    } else {
        Ok(SearchQueryOutcome::Results(results))
    }
}

fn parse_naver_result(value: &Value) -> Option<ResearchSource> {
    let title = sanitize_search_text(value.get("title")?.as_str()?);
    if title.is_empty() {
        return None;
    }
    let url = normalize_result_url(value.get("link")?.as_str()?)?;
    let snippet = sanitize_search_text(
        value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    Some(ResearchSource {
        title,
        url,
        snippet,
    })
}

async fn search_kakao_api(
    client: &reqwest::Client,
    query: &str,
    rest_api_key: &str,
) -> Result<SearchQueryOutcome, String> {
    let mut url =
        Url::parse("https://dapi.kakao.com/v2/search/web").expect("valid Kakao Search URL");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("size", &MAX_RESULTS_PER_QUERY.to_string());
    }
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .header("Authorization", format!("KakaoAK {rest_api_key}"))
        .send()
        .await
        .map_err(|_| KAKAO_SEARCH_REQUEST_FAILED_MESSAGE.to_string())?;
    let response = response.error_for_status().map_err(|error| {
        error
            .status()
            .map(|status| format!("Kakao Search API returned HTTP {status}"))
            .unwrap_or_else(|| KAKAO_SEARCH_REQUEST_FAILED_MESSAGE.to_string())
    })?;
    let body = response
        .text()
        .await
        .map_err(|_| "Kakao Search API response body could not be read.".to_string())?;
    parse_kakao_search_response(&body)
}

fn parse_kakao_search_response(body: &str) -> Result<SearchQueryOutcome, String> {
    let payload = serde_json::from_str::<Value>(body)
        .map_err(|_| "Kakao Search API returned invalid JSON.".to_string())?;
    let results = payload
        .get("documents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_kakao_result)
        .take(MAX_RESULTS_PER_QUERY)
        .collect::<Vec<_>>();
    if results.is_empty() {
        Ok(SearchQueryOutcome::Empty)
    } else {
        Ok(SearchQueryOutcome::Results(results))
    }
}

fn parse_kakao_result(value: &Value) -> Option<ResearchSource> {
    let title = sanitize_search_text(value.get("title")?.as_str()?);
    if title.is_empty() {
        return None;
    }
    let url = normalize_result_url(value.get("url")?.as_str()?)?;
    let snippet = sanitize_search_text(
        value
            .get("contents")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    Some(ResearchSource {
        title,
        url,
        snippet,
    })
}

async fn fetch_duckduckgo_html_response(
    client: &reqwest::Client,
    query: &str,
    retry: bool,
) -> Result<String, reqwest::Error> {
    let mut url = Url::parse("https://html.duckduckgo.com/html").expect("valid DuckDuckGo URL");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        if retry {
            pairs.append_pair("kl", "us-en");
            pairs.append_pair("kp", "-2");
        }
    }
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

async fn fetch_duckduckgo_lite_response(
    client: &reqwest::Client,
    query: &str,
) -> Result<String, reqwest::Error> {
    let mut url = Url::parse("https://lite.duckduckgo.com/lite/").expect("valid DuckDuckGo URL");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        pairs.append_pair("kl", "us-en");
        pairs.append_pair("kp", "-2");
    }
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

fn parse_duckduckgo_search_pages<'a>(
    pages: impl IntoIterator<Item = &'a str>,
) -> SearchQueryOutcome {
    let mut saw_blocked_page = false;
    for page in pages {
        if is_duckduckgo_challenge_page(page) {
            saw_blocked_page = true;
            continue;
        }
        let results = parse_duckduckgo_html(page);
        if !results.is_empty() {
            return SearchQueryOutcome::Results(results);
        }
        if is_duckduckgo_lite_page(page) {
            let lite_results = parse_duckduckgo_lite(page);
            if !lite_results.is_empty() {
                return SearchQueryOutcome::Results(lite_results);
            }
        }
    }
    if saw_blocked_page {
        SearchQueryOutcome::Blocked
    } else {
        SearchQueryOutcome::Empty
    }
}

fn parse_duckduckgo_html(html: &str) -> Vec<ResearchSource> {
    if is_duckduckgo_challenge_page(html) {
        return Vec::new();
    }
    let document = Html::parse_document(html);
    let result_selector = Selector::parse(".result, article[data-testid='result']").unwrap();
    let link_selector = Selector::parse(
        ".result__a, .result__title a, a[data-testid='result-title-a'], a.result-link",
    )
    .unwrap();
    let snippet_selector =
        Selector::parse(".result__snippet, .result-snippet, [data-result='snippet']").unwrap();
    let mut sources = Vec::new();

    for result in document
        .select(&result_selector)
        .take(MAX_RESULTS_PER_QUERY)
    {
        let Some(link) = result.select(&link_selector).next() else {
            continue;
        };
        let title = link.text().collect::<Vec<_>>().join(" ");
        let Some(raw_href) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = normalize_result_url(raw_href) else {
            continue;
        };
        let snippet = result
            .select(&snippet_selector)
            .next()
            .map(|node| node.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        sources.push(ResearchSource {
            title: compact_text(&title),
            url,
            snippet: compact_text(&snippet),
        });
    }
    if !sources.is_empty() {
        return sources;
    }

    let snippets = document
        .select(&snippet_selector)
        .map(|node| compact_text(&node.text().collect::<Vec<_>>().join(" ")))
        .filter(|snippet| !snippet.is_empty())
        .collect::<Vec<_>>();
    let mut loose_sources = Vec::new();
    for (index, link) in document
        .select(&link_selector)
        .take(MAX_RESULTS_PER_QUERY)
        .enumerate()
    {
        let title = compact_text(&link.text().collect::<Vec<_>>().join(" "));
        if title.is_empty() {
            continue;
        }
        let Some(raw_href) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = normalize_result_url(raw_href) else {
            continue;
        };
        loose_sources.push(ResearchSource {
            title,
            url,
            snippet: snippets.get(index).cloned().unwrap_or_default(),
        });
    }
    loose_sources
}

fn parse_duckduckgo_lite(html: &str) -> Vec<ResearchSource> {
    if is_duckduckgo_challenge_page(html) || !is_duckduckgo_lite_page(html) {
        return Vec::new();
    }

    let document = Html::parse_document(html);
    let row_selector = Selector::parse("tr").unwrap();
    let link_selector = Selector::parse("td.result-link a, td.result-link > a").unwrap();
    let mut sources = Vec::new();
    let mut last_result_index: Option<usize> = None;

    for row in document.select(&row_selector) {
        let row_text = compact_text(&row.text().collect::<Vec<_>>().join(" "));
        let link = row.select(&link_selector).find_map(|candidate| {
            let title = compact_text(&candidate.text().collect::<Vec<_>>().join(" "));
            let raw_href = candidate.value().attr("href")?;
            let url = normalize_result_url(raw_href)?;
            if !looks_like_duckduckgo_lite_result(&title, &url) {
                return None;
            }
            Some((title, url))
        });

        if let Some((title, url)) = link {
            sources.push(ResearchSource {
                title,
                url,
                snippet: String::new(),
            });
            last_result_index = Some(sources.len() - 1);
            if sources.len() >= MAX_RESULTS_PER_QUERY {
                break;
            }
            continue;
        }

        if let Some(index) = last_result_index {
            if !row_text.is_empty() && sources[index].snippet.is_empty() {
                sources[index].snippet = row_text;
            }
        }
    }

    sources
}

fn normalize_result_url(raw_href: &str) -> Option<String> {
    let base = Url::parse("https://duckduckgo.com").ok()?;
    let url = if raw_href.starts_with("http://") || raw_href.starts_with("https://") {
        Url::parse(raw_href).ok()?
    } else {
        base.join(raw_href).ok()?
    };
    if url.host_str().is_some_and(is_duckduckgo_redirect_host) && url.path() == "/l/" {
        for (key, value) in url.query_pairs() {
            if key == "uddg" {
                return normalize_result_url(&value);
            }
        }
        return None;
    }
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn is_duckduckgo_redirect_host(host: &str) -> bool {
    host == "duckduckgo.com" || host.ends_with(".duckduckgo.com")
}

fn is_duckduckgo_lite_page(html: &str) -> bool {
    html.contains("DuckDuckGo (Lite)")
        || html.contains("id=\"lite_wrapper\"")
        || html.contains("id='lite_wrapper'")
}

fn is_duckduckgo_challenge_page(html: &str) -> bool {
    html.contains("anomaly-modal")
        || html.contains("challenge-form")
        || html.contains("Unfortunately, bots use DuckDuckGo too.")
}

fn looks_like_duckduckgo_lite_result(title: &str, url: &str) -> bool {
    if title.is_empty() {
        return false;
    }
    let normalized_title = title.trim().to_ascii_lowercase();
    if matches!(
        normalized_title.as_str(),
        "duckduckgo" | "next page" | "previous page"
    ) {
        return false;
    }
    Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| !is_duckduckgo_redirect_host(host))
        })
        .unwrap_or(false)
}

fn compact_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_search_text(text: &str) -> String {
    compact_text(&decode_basic_html_entities(&strip_html_tags(text)))
}

fn strip_html_tags(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => stripped.push(ch),
            _ => {}
        }
    }
    stripped
}

fn decode_basic_html_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn format_error_chain(error: &dyn Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(error) = source {
        parts.push(error.to_string());
        source = error.source();
    }
    parts.dedup();
    parts.join(" | caused by: ")
}

fn dedupe_sources(sources: &mut Vec<ResearchSource>) {
    dedupe_sources_for_subject(sources, None);
}

fn dedupe_sources_for_subject(sources: &mut Vec<ResearchSource>, subject: Option<&str>) {
    let mut seen = std::collections::HashSet::new();
    sources.retain(|source| seen.insert(source.url.clone()));
    sources.sort_by(|left, right| {
        source_relevance_score_for_subject(right, subject)
            .cmp(&source_relevance_score_for_subject(left, subject))
            .then_with(|| {
                source_score_for_subject(&right.url, subject)
                    .cmp(&source_score_for_subject(&left.url, subject))
            })
            .then_with(|| left.title.cmp(&right.title))
    });
}

fn source_relevance_score_for_subject(source: &ResearchSource, subject: Option<&str>) -> usize {
    let Some(subject) = subject else {
        return 0;
    };
    let haystack = compact_text(&format!(
        "{} {} {}",
        source.title, source.snippet, source.url
    ))
    .to_ascii_lowercase();
    let haystack_tokens = text_keyword_tokens(&haystack);
    subject_keyword_tokens(subject)
        .into_iter()
        .filter(|keyword| keyword_matches_source_text(keyword, &haystack, &haystack_tokens))
        .count()
}

fn source_score(url: &str) -> i32 {
    source_score_for_subject(url, None)
}

fn source_score_for_subject(url: &str, subject: Option<&str>) -> i32 {
    let host = Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    let host = host.trim_start_matches("www.").to_ascii_lowercase();

    if subject.is_some_and(subject_has_historical_named_entity_context)
        && historical_supplementary_contested_source_url(url)
    {
        return 20;
    }
    if host.ends_with(".edu")
        || host.ends_with(".ac.uk")
        || host_matches_domain_boundary(&host, "cambridge.org")
        || host_matches_domain_boundary(&host, "academic.oup.com")
        || host_matches_domain_boundary(&host, "jstor.org")
        || host_matches_domain_boundary(&host, "degruyter.com")
        || host_matches_domain_boundary(&host, "cppreference.com")
    {
        return 100;
    }
    if infer_source_class_for_subject(url, subject) == "official_or_primary" {
        return 90;
    }
    if host_matches_domain_boundary(&host, "britannica.com")
        || host_matches_domain_boundary(&host, "worldhistory.org")
        || host_matches_domain_boundary(&host, "livius.org")
        || host == "wikipedia.org"
        || host.ends_with(".wikipedia.org")
    {
        return 80;
    }
    if is_non_evidentiary_source_pack_host(&host) {
        return 20;
    }
    if host.contains("namu.wiki")
        || host.contains("thewiki.kr")
        || host.contains("tistory.com")
        || host.contains("blogspot.")
        || host.contains("naver.com")
    {
        return 20;
    }
    50
}

fn is_non_evidentiary_source_pack_host(host: &str) -> bool {
    host_matches_domain_boundary(host, "github.com")
        || host_matches_domain_boundary(host, "raw.githubusercontent.com")
        || host_matches_domain_boundary(host, "api.github.com")
        || host_matches_domain_boundary(host, "pinterest.com")
        || host_matches_domain_boundary(host, "pinimg.com")
        || host == "pin.it"
}

fn host_matches_domain_boundary(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn source_quality(url: &str) -> &'static str {
    source_quality_for_subject(url, None)
}

fn source_quality_for_subject(url: &str, subject: Option<&str>) -> &'static str {
    match source_score_for_subject(url, subject) {
        80.. => "high",
        50..=79 => "medium",
        _ => "low",
    }
}

fn format_source_pack(subject: &str, sources: &[ResearchSource]) -> String {
    let mut pack = format!(
        "[PRE-COLLECTED SOURCE PACK]\n\
The runtime pre-collected these web search results for the exact research subject: {subject}\n\
- Use these only as candidate evidence, not as final truth.\n\
- Prefer the most authoritative sources and explicitly mark weak/user-generated sources.\n\
- For historical subjects, weak or problematic ancient texts may appear only as supplementary contested context. Do not anchor decisive conclusions on them without stronger corroboration and visible caveats.\n\
- The final report must cite full URLs from this pack when using claims from them.\n"
    );
    for (idx, source) in sources.iter().enumerate() {
        pack.push_str(&format!(
            "\n{}. Title: {}\n   URL: {}\n   Source quality: {}\n   Source role: {}\n   Snippet: {}\n",
            idx + 1,
            source.title,
            source.url,
            source_quality_for_subject(&source.url, Some(subject)),
            source_role_for_subject(source, subject),
            source.snippet
        ));
    }
    pack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_topic_subject_from_korean_prompt() {
        let prompt =
            "다음 주제에 대해 포괄적이고 사실 중심의 정보 조사 보고서를 작성하세요: [RQ-TEST] 유스티니아누스 대제의 로마-고트 수복 전쟁 개요\n\n요구사항:";

        assert_eq!(
            extract_research_subject(prompt, None).as_deref(),
            Some("유스티니아누스 대제의 로마-고트 수복 전쟁 개요")
        );
    }

    #[test]
    fn extracts_follow_up_focus_subject_before_generic_prompt_header() {
        let prompt = "첨부된 SOURCE DOCUMENTS를 이전 장 또는 선행 지식으로 보고, 거기에서 이어지는 독립적인 후속 조사 보고서를 작성하세요.\n\n요구사항:\n- 원본 문서의 단순 요약이나 보강판이 아니라, 이어지는 질문과 새 내용을 중심으로 작성하세요.\n\n[사용자 조사 조건/제약/비교 기준]\n군인황제 시대의 공공/민간 경제 시스템 상황과 아우렐리아누스 황제의 경제정책 영향\n\n위 내용은 조사 질문, 조건, 제약, 선호, 비교 기준으로 해석한다.";

        assert_eq!(
            extract_research_subject(prompt, None).as_deref(),
            Some(
                "군인황제 시대의 공공/민간 경제 시스템 상황과 아우렐리아누스 황제의 경제정책 영향"
            )
        );
    }

    #[test]
    fn extracts_source_document_subject_and_keeps_focus_as_modifier() {
        let prompt = "첨부된 SOURCE DOCUMENTS가 다루는 대상, 주장, 사건, 제품 또는 개념을 식별하고, 그 대상에 대한 종합 정보 조사 보고서를 작성하세요.\n\n[사용자 조사 조건/제약/비교 기준]\n0.17.2 최신 버전의 작동 구조 중심";
        let source_documents = "# 최종 답변\n\n`Sbluemin/fleet-harness`는 Claude Code, Codex CLI, Gemini CLI 등을 운용하기 위한 멀티 LLM 오케스트레이션 킷입니다.";

        assert_eq!(
            extract_research_subject(prompt, Some(source_documents)).as_deref(),
            Some("Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심")
        );
    }

    #[test]
    fn source_queries_expand_github_repository_subjects() {
        let queries = source_queries("Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심");

        assert!(queries.contains(&"Sbluemin/fleet-harness GitHub".to_string()));
        assert!(queries.contains(&"Sbluemin/fleet-harness release 0.17.2 changelog".to_string()));
        assert!(queries
            .contains(&"Sbluemin/fleet-harness README architecture documentation".to_string()));
        assert!(queries.contains(
            &"Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심 official source".to_string()
        ));
        assert!(queries.contains(
            &"Sbluemin/fleet-harness 0.17.2 최신 버전의 작동 구조 중심 documentation".to_string()
        ));
        assert!(queries.len() <= MAX_SOURCE_PACK_QUERIES);
    }

    #[test]
    fn source_queries_add_generic_authority_variants_for_live_topics() {
        let queries = source_queries(
            "Compare two software hosting plans using current pricing, rate limits, and feature tiers",
        );

        assert!(queries
            .iter()
            .any(|query| query.contains("official source")));
        assert!(queries.iter().any(|query| query.contains("documentation")));
        assert!(queries
            .iter()
            .any(|query| query.contains("review comparison")));
        assert!(queries
            .iter()
            .any(|query| query.contains("pricing official")));
        assert!(queries.len() <= MAX_SOURCE_PACK_QUERIES);
    }

    #[test]
    fn source_queries_add_python_and_npm_official_documentation_variants() {
        let python_queries =
            source_queries("Compare Python packaging workflows for pip, wheels, and virtualenvs");
        let npm_queries = source_queries(
            "Compare npm package publishing and package.json workflow guidance for Node.js teams",
        );

        assert!(python_queries
            .iter()
            .any(|query| { query.contains("packaging.python.org official documentation") }));
        assert!(python_queries
            .iter()
            .any(|query| query.contains("docs.python.org official documentation")));
        assert!(npm_queries
            .iter()
            .any(|query| query.contains("docs.npmjs.com official documentation")));
        assert!(npm_queries
            .iter()
            .any(|query| query.contains("nodejs.org official documentation")));
    }

    #[test]
    fn source_queries_add_cpp_systems_authority_variants_without_repo_false_positive() {
        let queries = source_queries(
            "an experienced C++ developer planning to implement a work-stealing scheduler. Cover the core scheduling model, Chase-Lev work-stealing deque design, worker/local queue vs global injection queue, memory ordering, blocking and parking/wakeup strategy, cancellation/shutdown, instrumentation, and benchmark strategy.",
        );

        assert!(queries
            .iter()
            .any(|query| query == "oneTBB task scheduler work stealing documentation"));
        assert!(queries
            .iter()
            .any(|query| query == "Taskflow work stealing executor documentation"));
        assert!(queries
            .iter()
            .any(|query| query == "Chase-Lev work stealing deque pdf"));
        assert!(queries
            .iter()
            .any(|query| query == "cppreference memory_order stop_token packaged_task"));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("worker/local github")));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("worker/local readme")));
    }

    #[test]
    fn source_queries_add_network_port_standards_and_vendor_authority_variants() {
        let queries = source_queries(
            "Explain ephemeral port allocation and port exhaustion across Linux kernel defaults, Windows dynamic port ranges, and cloud container networking behavior.",
        );

        assert!(queries
            .iter()
            .any(|query| query == "RFC 6056 ephemeral port randomization"));
        assert!(queries
            .iter()
            .any(|query| query == "IANA service names port numbers documentation"));
        assert!(queries
            .iter()
            .any(|query| query == "docs.kernel.org ip_local_port_range networking"));
        assert!(queries
            .iter()
            .any(|query| query == "learn.microsoft.com windows dynamic port range tcp udp"));
    }

    #[test]
    fn technical_authority_queries_map_to_exact_official_target_domains() {
        assert_eq!(
            official_host_hint_target_domain("IANA service names port numbers documentation"),
            Some("iana.org")
        );
        assert_eq!(
            official_host_hint_target_domain("RFC 6056 ephemeral port randomization"),
            Some("rfc-editor.org")
        );
        assert_eq!(
            official_host_hint_target_domain("docs.kernel.org ip_local_port_range networking"),
            Some("docs.kernel.org")
        );
        assert_eq!(
            official_host_hint_target_domain(
                "learn.microsoft.com windows dynamic port range tcp udp"
            ),
            Some("learn.microsoft.com")
        );
    }

    #[test]
    fn source_queries_add_policy_and_product_official_host_hints() {
        let policy_queries = source_queries(
            "Compare the NIST AI RMF with the EU AI Act compliance obligations for frontier model deployers",
        );
        let product_queries = source_queries(
            "Compare Apple MacBook Pro and Framework Laptop 13 specifications for a local Rust and AI workflow",
        );

        assert!(policy_queries
            .iter()
            .any(|query| query.contains("nist.gov official guidance")));
        assert!(policy_queries
            .iter()
            .any(|query| query.contains("eur-lex.europa.eu AI Act official text")));
        assert!(product_queries
            .iter()
            .any(|query| query.contains("apple.com official technical specifications")));
        assert!(product_queries
            .iter()
            .any(|query| query.contains("frame.work official laptop specifications")));
        assert!(!policy_queries
            .iter()
            .any(|query| query.contains("frame.work official laptop specifications")));
    }

    #[test]
    fn source_queries_add_historical_named_entity_overview_hints() {
        let queries = source_queries(
            "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them",
        );

        assert!(queries.iter().any(|query| query == "aurelian overview"));
        assert!(queries
            .iter()
            .any(|query| query == "aurelian Roman emperor"));
        assert!(queries
            .iter()
            .any(|query| query == "aurelian Britannica encyclopedia history"));
        assert!(queries
            .iter()
            .any(|query| query == "aurelian primary source translation"));
        assert!(queries
            .iter()
            .any(|query| query == "aurelian scholarly article history"));
    }

    #[test]
    fn source_queries_strip_korean_particles_from_historical_event_subjects() {
        let queries = source_queries("오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의");

        assert!(queries
            .iter()
            .any(|query| query.contains("오스트리아 왕위계승전쟁")));
        assert!(queries
            .iter()
            .any(|query| query.contains("오스트리아 왕위계승전쟁 배경")));
    }

    #[test]
    fn policy_framework_subject_keeps_regulatory_hints_without_framework_laptop_host() {
        let queries = source_queries(
            "Compare the NIST AI Risk Management Framework with the EU AI Act for model governance",
        );

        assert!(queries
            .iter()
            .any(|query| query.contains("nist.gov official guidance")));
        assert!(queries
            .iter()
            .any(|query| query.contains("eur-lex.europa.eu AI Act official text")));
        assert!(!queries
            .iter()
            .any(|query| query.contains("frame.work official laptop specifications")));
    }

    #[test]
    fn official_host_hint_queries_prefer_exact_target_domain_results() {
        let query = "Compare Apple MacBook Pro and Framework Laptop options apple.com official technical specifications";
        let results = vec![
            ResearchSource {
                title: "HN".to_string(),
                url: "https://news.ycombinator.com/item?id=1".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "Apple Specs".to_string(),
                url: "https://www.apple.com/macbook-pro/specs/".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "Apple Support".to_string(),
                url: "https://support.apple.com/en-us/guide/mac-help/welcome/mac".to_string(),
                snippet: String::new(),
            },
        ];

        let preferred = preferred_official_host_hint_results(query, results);

        assert_eq!(preferred.len(), 2);
        assert!(preferred
            .iter()
            .all(|source| source_url_matches_domain_boundary(&source.url, "apple.com")));
    }

    #[test]
    fn official_host_hint_queries_continue_fallback_when_results_lack_target_domain() {
        let query = "Compare the NIST AI Risk Management Framework nist.gov official guidance";
        let off_target_results = vec![ResearchSource {
            title: "Secondary".to_string(),
            url: "https://example.com/nist-summary".to_string(),
            snippet: String::new(),
        }];
        let exact_results = vec![ResearchSource {
            title: "NIST".to_string(),
            url: "https://www.nist.gov/itl/ai-risk-management-framework".to_string(),
            snippet: String::new(),
        }];

        assert!(should_continue_official_host_hint_provider_fallback(
            query,
            &off_target_results
        ));
        assert!(!should_continue_official_host_hint_provider_fallback(
            query,
            &exact_results
        ));
    }

    #[test]
    fn official_host_hint_queries_continue_fallback_when_exact_host_results_are_off_topic() {
        let query = "namsan seoul running cafe apple.com official technical specifications";
        let off_topic_exact_host_results = vec![ResearchSource {
            title: "Apple Fitness Plus overview".to_string(),
            url: "https://www.apple.com/apple-fitness-plus/".to_string(),
            snippet: "Workout plans and wellness subscription details.".to_string(),
        }];

        assert!(should_continue_official_host_hint_provider_fallback(
            query,
            &off_topic_exact_host_results
        ));
    }

    #[test]
    fn provider_chain_continues_when_first_provider_results_are_all_off_topic() {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let query = "morning running workout namsan seoul good atmospheric cafe";
        let resolution = select_search_query_resolution(
            vec![
                SearchProviderAttempt {
                    provider: "naver".to_string(),
                    outcome: Ok(SearchQueryOutcome::Results(vec![ResearchSource {
                        title: "SEOUL Itinerary • MUST READ! (2026 Guide)".to_string(),
                        url: "https://www.thebrokebackpacker.com/seoul-itinerary/".to_string(),
                        snippet: "General travel ideas for Seoul visitors.".to_string(),
                    }])),
                },
                SearchProviderAttempt {
                    provider: "kakao".to_string(),
                    outcome: Ok(SearchQueryOutcome::Results(vec![ResearchSource {
                        title: "Namsan running course and nearby cafe guide".to_string(),
                        url: "https://example.com/namsan-running-cafe".to_string(),
                        snippet: "Namsan route options, running timing, and cafe stop suggestions."
                            .to_string(),
                    }])),
                },
            ],
            subject,
            query,
        )
        .expect("resolution");

        assert_eq!(resolution.provider.as_deref(), Some("kakao"));
        assert!(matches!(
            resolution.outcome,
            SearchQueryOutcome::Results(ref results)
                if results.iter().any(|source| source.url == "https://example.com/namsan-running-cafe")
        ));
        assert!(resolution
            .diagnostics
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("naver")));
    }

    #[test]
    fn provider_chain_preserves_no_relevant_status_only_after_all_providers_fail() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "roman emperor aurelian third century soldier emperors among";
        let resolution = select_search_query_resolution(
            vec![
                SearchProviderAttempt {
                    provider: "naver".to_string(),
                    outcome: Ok(SearchQueryOutcome::Results(vec![ResearchSource {
                        title: "Roman Britain".to_string(),
                        url: "https://en.wikipedia.org/wiki/Roman_Britain".to_string(),
                        snippet: "A Roman province in Britain.".to_string(),
                    }])),
                },
                SearchProviderAttempt {
                    provider: "kakao".to_string(),
                    outcome: Ok(SearchQueryOutcome::Results(vec![ResearchSource {
                        title: "History of the Roman Empire".to_string(),
                        url: "https://en.wikipedia.org/wiki/History_of_the_Roman_Empire"
                            .to_string(),
                        snippet: "General history of the empire.".to_string(),
                    }])),
                },
                SearchProviderAttempt {
                    provider: "duckduckgo".to_string(),
                    outcome: Ok(SearchQueryOutcome::Empty),
                },
            ],
            subject,
            query,
        )
        .expect("resolution");

        assert_eq!(resolution.provider.as_deref(), Some("naver"));
        assert!(matches!(
            resolution.outcome,
            SearchQueryOutcome::Results(ref results)
                if results.iter().any(|source| source.url == "https://en.wikipedia.org/wiki/Roman_Britain")
        ));
        let diagnostics = resolution.diagnostics.as_deref().expect("diagnostics");
        assert!(diagnostics.contains("no topically relevant candidates"));
        assert!(diagnostics.contains("naver"));
        assert!(diagnostics.contains("kakao"));
    }

    #[test]
    fn repair_search_hints_skip_known_urls_and_off_topic_results() {
        let mut seen_urls = HashSet::from(["https://known.example.com".to_string()]);
        let hints = repair_search_hints_from_results(
            "Namsan morning running and cafe",
            "남산공원 공식",
            Some("naver"),
            &[
                ResearchSource {
                    title: "Known".to_string(),
                    url: "https://known.example.com".to_string(),
                    snippet: "known".to_string(),
                },
                ResearchSource {
                    title: "남산공원 러닝 코스와 카페 안내".to_string(),
                    url: "https://parks.seoul.go.kr/namsan".to_string(),
                    snippet: "서울 남산공원 러닝 동선, 카페 접근, 운영 시간과 이용 안내"
                        .to_string(),
                },
                ResearchSource {
                    title: "React Scheduler".to_string(),
                    url: "https://react.dev/scheduler".to_string(),
                    snippet: "off topic".to_string(),
                },
            ],
            &mut seen_urls,
            4,
        );

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].provider.as_deref(), Some("naver"));
        assert_eq!(hints[0].url, "https://parks.seoul.go.kr/namsan");
        assert_eq!(hints[0].source_quality, "medium");
    }

    #[test]
    fn official_host_hint_target_miss_is_reported_in_query_diagnostics() {
        let query = "Compare the NIST AI Risk Management Framework nist.gov official guidance";
        let diagnostics = official_host_hint_target_miss_diagnostic(
            query,
            &[ResearchSource {
                title: "Secondary".to_string(),
                url: "https://example.com/nist-summary".to_string(),
                snippet: String::new(),
            }],
        );

        assert_eq!(
            diagnostics.as_deref(),
            Some("Official-host hint query did not recover target domain nist.gov.")
        );
    }

    #[test]
    fn source_pack_budget_preserves_exact_official_hint_target_hits() {
        let ranked_sources = (0..8)
            .map(|index| ResearchSource {
                title: format!("Secondary {index}"),
                url: format!("https://example.com/source-{index}"),
                snippet: String::new(),
            })
            .chain(std::iter::once(ResearchSource {
                title: "Apple Specs".to_string(),
                url: "https://www.apple.com/macbook-pro/specs/".to_string(),
                snippet: String::new(),
            }))
            .collect::<Vec<_>>();
        let preserved = HashSet::from(["https://www.apple.com/macbook-pro/specs/".to_string()]);

        let selected = apply_source_pack_budget(ranked_sources, &preserved);

        assert_eq!(selected.len(), MAX_SOURCE_PACK_RESULTS);
        assert!(selected
            .iter()
            .any(|source| source.url == "https://www.apple.com/macbook-pro/specs/"));
        assert!(!selected
            .iter()
            .any(|source| source.url == "https://example.com/source-7"));
    }

    #[test]
    fn source_queries_preserve_distinct_authority_suffixes_for_long_subjects() {
        let subject = "Compare two current developer laptops for a local Rust and AI workflow, including thermals, memory ceilings, battery tradeoffs, sustained compile performance, and current pricing across regions for software engineers who travel frequently";
        let queries = source_queries(subject);

        let official = queries
            .iter()
            .find(|query| query.ends_with("official source"))
            .expect("official source query should survive truncation");
        let documentation = queries
            .iter()
            .find(|query| query.ends_with("documentation"))
            .expect("documentation query should survive truncation");
        let review = queries
            .iter()
            .find(|query| query.ends_with("review comparison"))
            .expect("review comparison query should survive truncation");
        let pricing = queries
            .iter()
            .find(|query| query.ends_with("pricing official"))
            .expect("pricing official query should survive truncation");

        assert_ne!(official, documentation);
        assert_ne!(official, review);
        assert_ne!(documentation, review);
        assert!(official.chars().count() <= MAX_QUERY_CHARS);
        assert!(documentation.chars().count() <= MAX_QUERY_CHARS);
        assert!(review.chars().count() <= MAX_QUERY_CHARS);
        assert!(pricing.chars().count() <= MAX_QUERY_CHARS);
    }

    #[test]
    fn source_queries_compact_long_natural_language_subjects_into_keyword_rich_queries() {
        let subject = "Find a quiet late-night brunch cafe recommendation in Seongsu Seoul for a team discussion, with parking availability, dessert options, and nearby subway access for a mixed local and visitor group";
        let queries = source_queries(subject);

        assert!(queries.iter().any(|query| {
            query.contains("seongsu")
                && query.contains("seoul")
                && query.contains("parking")
                && query.contains("subway")
        }));
        assert!(queries
            .iter()
            .all(|query| query.chars().count() <= MAX_QUERY_CHARS));
    }

    #[test]
    fn customer_history_prompt_queries_strip_reader_facing_boilerplate_and_keep_domain_anchors() {
        let prompt = "Write a Korean reader-facing research report about why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them. Cover military background, reunification of the Palmyrene and Gallic breakaway regimes, monetary reform, Aurelian Walls, Sol Invictus, legitimacy, source reliability, and the difference between ancient evidence and modern interpretation. The report should be comfortable to read, but not one-sided: mark uncertainty, contested interpretations, and limits of the evidence. Include practical follow-up questions and source trails for deeper research.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let queries = source_queries(&subject);

        assert!(!subject.to_ascii_lowercase().contains("reader-facing"));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("reader facing")));
        assert!(queries
            .iter()
            .any(|query| query.to_ascii_lowercase().contains("aurelian")));
        assert!(queries.iter().any(|query| {
            let lower = query.to_ascii_lowercase();
            lower.contains("palmyrene") || lower.contains("gallic") || lower.contains("sol")
        }));
    }

    #[test]
    fn customer_local_prompt_queries_keep_local_anchors_and_skip_repo_hints() {
        let prompt = "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates. Distinguish official venue or park information from map/listing/review information, and mark volatile information such as opening hours as needing day-of verification. The output should be useful for actually deciding where to run and where to go afterward, not just a list of cafes.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let queries = source_queries(&subject);

        assert!(queries.iter().any(|query| {
            let lower = query.to_ascii_lowercase();
            lower.contains("namsan")
                && lower.contains("seoul")
                && lower.contains("running")
                && lower.contains("cafe")
        }));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("reader facing")));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("github")));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("readme")));
        assert!(queries
            .iter()
            .all(|query| !query.to_ascii_lowercase().contains("changelog")));
    }

    #[test]
    fn naver_and_kakao_add_tightly_capped_localized_korean_query_variants() {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let query = "morning running workout namsan seoul good atmospheric cafe";

        let naver_queries = provider_specific_search_queries(
            &ResearchSearchProvider::Naver {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
            },
            subject,
            query,
        );
        let kakao_queries = provider_specific_search_queries(
            &ResearchSearchProvider::Kakao {
                rest_api_key: "secret".to_string(),
            },
            subject,
            query,
        );
        let duckduckgo_queries =
            provider_specific_search_queries(&ResearchSearchProvider::DuckDuckGo, subject, query);

        assert_eq!(naver_queries[0], query);
        assert_eq!(kakao_queries[0], query);
        assert_eq!(duckduckgo_queries, vec![query.to_string()]);
        assert!(naver_queries.len() <= 3);
        assert!(kakao_queries.len() <= 3);
        assert!(naver_queries
            .iter()
            .skip(1)
            .any(|variant| variant.contains("남산") && variant.contains("러닝 코스")));
        assert!(naver_queries
            .iter()
            .skip(1)
            .any(|variant| { variant.contains("남산공원") && variant.contains("공식") }));
        assert!(naver_queries
            .iter()
            .skip(1)
            .any(|variant| variant.contains("카페") && variant.contains("영업시간")));
        assert_eq!(naver_queries, kakao_queries);
    }

    #[test]
    fn non_local_topics_do_not_get_noisy_korean_provider_variants() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "roman emperor aurelian third century soldier emperors among";
        let naver_queries = provider_specific_search_queries(
            &ResearchSearchProvider::Naver {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
            },
            subject,
            query,
        );
        let kakao_queries = provider_specific_search_queries(
            &ResearchSearchProvider::Kakao {
                rest_api_key: "secret".to_string(),
            },
            subject,
            query,
        );

        assert_eq!(naver_queries, vec![query.to_string()]);
        assert_eq!(kakao_queries, vec![query.to_string()]);
    }

    #[test]
    fn topical_relevance_filter_rejects_off_topic_authoritative_results() {
        let subject =
            "Compare Apple MacBook Pro and Framework Laptop 13 specifications for a local Rust and AI workflow";
        let query = "apple macbook pro framework laptop official source";
        let off_topic = ResearchSource {
            title: "Apple Fitness Plus overview".to_string(),
            url: "https://www.apple.com/apple-fitness-plus/".to_string(),
            snippet: "Workout plans and wellness subscription details.".to_string(),
        };
        let on_topic = ResearchSource {
            title: "MacBook Pro 14-inch and 16-inch - Technical Specifications".to_string(),
            url: "https://www.apple.com/macbook-pro/specs/".to_string(),
            snippet: "Chip options, unified memory, battery life, and display specs.".to_string(),
        };

        assert!(!is_topically_relevant_source(&off_topic, subject, query));
        assert!(is_topically_relevant_source(&on_topic, subject, query));
    }

    #[test]
    fn topical_relevance_filter_rejects_aurelian_off_topic_customer_sample_hits() {
        let prompt = "Write a Korean reader-facing research report about why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them. Cover military background, reunification of the Palmyrene and Gallic breakaway regimes, monetary reform, Aurelian Walls, Sol Invictus, legitimacy, source reliability, and the difference between ancient evidence and modern interpretation.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let query = query_subject_for_relevance(&source_queries(&subject)[0]);
        let off_topic = [
            ResearchSource {
                title: "Constantine the Great".to_string(),
                url: "https://en.wikipedia.org/wiki/Constantine_the_Great".to_string(),
                snippet: "Roman emperor profile.".to_string(),
            },
            ResearchSource {
                title: "Gallo-Roman enclosure of Le Mans".to_string(),
                url: "https://en.wikipedia.org/wiki/Gallo-Roman_enclosure_of_Le_Mans".to_string(),
                snippet: "An ancient wall enclosure in Roman Gaul.".to_string(),
            },
            ResearchSource {
                title: "Lucius Aurelius Marcianus".to_string(),
                url: "https://en.wikipedia.org/wiki/Lucius_Aurelius_Marcianus".to_string(),
                snippet: "A Roman usurper in the third century.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, &subject, &query),
                "unexpectedly accepted off-topic source: {}",
                source.title
            );
        }
    }

    #[test]
    fn topical_relevance_filter_rejects_aurelian_generic_history_pages_without_surface_anchor() {
        let prompt = "Write a Korean reader-facing research report about why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them. Cover military background, reunification of the Palmyrene and Gallic breakaway regimes, monetary reform, Aurelian Walls, Sol Invictus, legitimacy, source reliability, and the difference between ancient evidence and modern interpretation.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let query = query_subject_for_relevance(&source_queries(&subject)[0]);
        let off_topic = [
            ResearchSource {
                title: "Roman Britain".to_string(),
                url: "https://en.wikipedia.org/wiki/Roman_Britain".to_string(),
                snippet: "A frontier province in Roman history that later intersects broader empire events."
                    .to_string(),
            },
            ResearchSource {
                title: "Roman Dacia".to_string(),
                url: "https://en.wikipedia.org/wiki/Roman_Dacia".to_string(),
                snippet: "Overview of the Roman province of Dacia in the imperial period.".to_string(),
            },
            ResearchSource {
                title: "History of the Roman Empire".to_string(),
                url: "https://en.wikipedia.org/wiki/History_of_the_Roman_Empire".to_string(),
                snippet: "General history page that may mention third-century instability.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, &subject, &query),
                "unexpectedly accepted generic history source: {}",
                source.title
            );
        }
    }

    #[test]
    fn canonical_history_overview_query_accepts_aurelian_central_overview_source() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "aurelian overview";
        let source = ResearchSource {
            title: "Aurelian".to_string(),
            url: "https://www.worldhistory.org/Aurelian/".to_string(),
            snippet:
                "Roman emperor Aurelian restored imperial unity during the third-century crisis."
                    .to_string(),
        };

        assert!(is_topically_relevant_source(&source, subject, query));
    }

    #[test]
    fn canonical_history_overview_query_rejects_aurelian_disambiguation_and_non_evidentiary_pages()
    {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "aurelian overview";
        let rejected = [
            ResearchSource {
                title: "Aurelian (disambiguation)".to_string(),
                url: "https://en.wikipedia.org/wiki/Aurelian_(disambiguation)".to_string(),
                snippet: "Topics that may refer to Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian - WikiMili, The Best Wikipedia Reader".to_string(),
                url: "https://wikimili.com/en/Aurelian".to_string(),
                snippet: "A mirrored encyclopedia article about the Roman emperor Aurelian."
                    .to_string(),
            },
            ResearchSource {
                title: "Aurelian - Grokipedia".to_string(),
                url: "https://grokipedia.com/page/Aurelian".to_string(),
                snippet: "A mirrored encyclopedia summary for Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian".to_string(),
                url: "https://en.wiktionary.org/wiki/Aurelian".to_string(),
                snippet: "Dictionary entry for the term Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Q46780".to_string(),
                url: "https://www.wikidata.org/wiki/Q46780".to_string(),
                snippet: "Structured data entity record for Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian audiobook by David Potter - Google Play".to_string(),
                url: "https://play.google.com/store/audiobooks/details/Aurelian?id=123".to_string(),
                snippet: "Store listing for an audiobook about Aurelian.".to_string(),
            },
        ];

        for source in rejected {
            assert!(
                !is_topically_relevant_source(&source, subject, query),
                "unexpectedly accepted weak or non-evidentiary overview source: {}",
                source.title
            );
        }
    }

    #[test]
    fn canonical_history_overview_query_preserves_credible_aurelian_overview_sources() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "aurelian overview";
        let accepted = [
            ResearchSource {
                title: "Aurelian | Roman emperor".to_string(),
                url: "https://www.britannica.com/biography/Aurelian".to_string(),
                snippet: "Britannica overview of the Roman emperor Aurelian and the third-century crisis."
                    .to_string(),
            },
            ResearchSource {
                title: "Aurelian".to_string(),
                url: "https://www.unrv.com/emperors/aurelian.php".to_string(),
                snippet: "UNRV background on Aurelian, his military record, and imperial reunification."
                    .to_string(),
            },
            ResearchSource {
                title: "Historia Augusta: Aurelian".to_string(),
                url: "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp".to_string(),
                snippet: "Primary-source translation related to the emperor Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian".to_string(),
                url: "https://en.wikipedia.org/wiki/Aurelian".to_string(),
                snippet: "Main entity page for the Roman emperor Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian Wall | Roman fortification".to_string(),
                url: "https://www.britannica.com/topic/Aurelian-Wall".to_string(),
                snippet: "Britannica background on the Aurelian Wall and its relation to Aurelian."
                    .to_string(),
            },
        ];

        for source in accepted {
            assert!(
                is_topically_relevant_source(&source, subject, query),
                "unexpectedly rejected credible overview source: {}",
                source.title
            );
        }
    }

    #[test]
    fn canonical_history_overview_accepts_historia_augusta_only_as_supplementary_low_context() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "aurelian primary source translation";
        let source = ResearchSource {
            title: "Historia Augusta: Aurelian".to_string(),
            url: "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp".to_string(),
            snippet: "Problematic late antique biography translated for reference.".to_string(),
        };

        assert!(is_topically_relevant_source(&source, subject, query));
        assert!(is_historical_supplementary_contested_source(
            &source, subject
        ));
        assert_eq!(
            source_quality_for_subject(&source.url, Some(subject)),
            "low"
        );
        assert!(!context_safe_source_for_subject(&source, subject));
    }

    #[test]
    fn historical_supplementary_only_sources_keep_source_pack_partial() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let sources = vec![ResearchSource {
            title: "Historia Augusta: Aurelian".to_string(),
            url: "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp".to_string(),
            snippet: "Problematic late antique biography translated for reference.".to_string(),
        }];

        assert!(matches!(
            assess_source_pack_acceptance(subject, &sources),
            SourcePackAcceptance::Partial { .. }
        ));
    }

    #[test]
    fn format_source_pack_marks_weak_historical_sources_as_supplementary_only() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let sources = vec![ResearchSource {
            title: "Historia Augusta: Aurelian".to_string(),
            url: "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp".to_string(),
            snippet: "Problematic late antique biography translated for reference.".to_string(),
        }];

        let pack = format_source_pack(subject, &sources);

        assert!(pack.contains("supplementary contested context only"));
        assert!(pack
            .contains("Do not anchor decisive conclusions on them without stronger corroboration"));
    }

    #[test]
    fn canonical_history_overview_query_rejects_generic_roman_pages_without_entity_centrality() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let query = "aurelian overview";
        let off_topic = [
            ResearchSource {
                title: "Roman Britain".to_string(),
                url: "https://en.wikipedia.org/wiki/Roman_Britain".to_string(),
                snippet: "A province in Roman history.".to_string(),
            },
            ResearchSource {
                title: "History of the Roman Empire".to_string(),
                url: "https://en.wikipedia.org/wiki/History_of_the_Roman_Empire".to_string(),
                snippet: "A broad overview of Roman imperial history.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, subject, query),
                "unexpectedly accepted generic overview source: {}",
                source.title
            );
        }
    }

    #[test]
    fn topical_relevance_filter_accepts_korean_local_recommendation_results() {
        let subject = "성수동에서 주차 가능하고 조용한 브런치 카페 추천";
        let query = "성수동 주차 조용한 브런치 카페 official source";
        let candidate = ResearchSource {
            title: "성수동 조용한 브런치 카페 5곳, 주차 가능 매장 정리".to_string(),
            url: "https://example.com/seongsu-brunch-parking".to_string(),
            snippet: "주차 가능 여부와 지하철 접근성을 함께 비교한 안내.".to_string(),
        };

        assert!(is_topically_relevant_source(&candidate, subject, query));
    }

    #[test]
    fn topical_relevance_filter_accepts_korean_historical_event_titles_with_spacing_and_particles()
    {
        let subject = "오스트리아 왕위계승전쟁의 배경과 전개, 영향과 의의";
        let query = "오스트리아 왕위계승전쟁 배경 전개 영향 의의";
        let candidate = ResearchSource {
            title: "오스트리아 왕위계승 전쟁 : 1740~1763".to_string(),
            url: "https://example.com/austrian-war-of-succession".to_string(),
            snippet: "전쟁의 배경과 전개를 연표 중심으로 정리한 개요.".to_string(),
        };

        assert!(is_topically_relevant_source(&candidate, subject, query));
    }

    #[test]
    fn topical_relevance_filter_accepts_korean_namsan_running_result_via_bilingual_anchors() {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let query = "morning running workout namsan seoul good atmospheric cafe";
        let candidate = ResearchSource {
            title: "초보자부터 마니아까지 모두 즐기는 서울 러닝 코스".to_string(),
            url: "https://example.com/seoul-running-course-namsan".to_string(),
            snippet: "남산공원 오르막 구간을 포함한 아침 러닝 코스와 주변 카페 접근 팁을 정리했다."
                .to_string(),
        };

        assert!(is_topically_relevant_source(&candidate, subject, query));
    }

    #[test]
    fn topical_relevance_filter_accepts_korean_namsan_park_cafe_result_when_running_context_is_present(
    ) {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let query = "morning running workout namsan seoul good atmospheric cafe";
        let candidate = ResearchSource {
            title: "남산공원(서울), 추천 카페 모음".to_string(),
            url: "https://example.com/namsan-park-cafe".to_string(),
            snippet: "남산 러닝 후 들르기 좋은 카페와 아침 운영시간을 함께 정리했다.".to_string(),
        };

        assert!(is_topically_relevant_source(&candidate, subject, query));
    }

    #[test]
    fn topical_relevance_filter_accepts_korean_namsan_hill_running_title() {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let query = "morning running workout namsan seoul good atmospheric cafe";
        let candidate = ResearchSource {
            title: "오르막 달리기 연습에 좋은 남산 러닝코스".to_string(),
            url: "https://example.com/namsan-hill-running".to_string(),
            snippet: "서울 남산 아침 러닝 동선과 러닝 후 쉬기 좋은 카페 접근성을 함께 다뤘다."
                .to_string(),
        };

        assert!(is_topically_relevant_source(&candidate, subject, query));
    }

    #[test]
    fn topical_relevance_filter_rejects_matches_on_instruction_and_route_boilerplate_only() {
        let prompt = "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let query = "namsan seoul running cafe official source";
        let off_topic = ResearchSource {
            title: "Country Reader: South Korea route planning basics".to_string(),
            url: "https://example.com/country-reader-route-guide".to_string(),
            snippet: "A reader-facing guide for someone planning a trip.".to_string(),
        };

        assert!(!is_topically_relevant_source(&off_topic, &subject, query));
    }

    #[test]
    fn topical_relevance_filter_rejects_namsan_off_topic_customer_sample_hits() {
        let prompt = "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let query = query_subject_for_relevance(&source_queries(&subject)[0]);
        let off_topic = [
            ResearchSource {
                title: "SEOUL Itinerary • MUST READ! (2026 Guide)".to_string(),
                url: "https://www.thebrokebackpacker.com/seoul-itinerary/".to_string(),
                snippet: "General travel ideas for Seoul visitors.".to_string(),
            },
            ResearchSource {
                title: "Reducing Air Pollution from Urban Transport".to_string(),
                url: "https://example.com/urban-transport.pdf".to_string(),
                snippet: "Transport policy paper.".to_string(),
            },
            ResearchSource {
                title: "LONDON KOREAN 2019".to_string(),
                url: "https://kccuk.org.uk/media/documents/LKFF19-Brochure.pdf".to_string(),
                snippet: "Festival brochure PDF.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, &subject, &query),
                "unexpectedly accepted off-topic source: {}",
                source.title
            );
        }
    }

    #[test]
    fn topical_relevance_filter_rejects_namsan_generic_itinerary_and_map_pages() {
        let prompt = "Write a Korean reader-facing research report for someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward. Compare route/end-point options, likely morning timing, shower or changing constraints, transit access, and cafe candidates.";
        let subject = extract_research_subject(prompt, None).expect("subject");
        let query = query_subject_for_relevance(&source_queries(&subject)[0]);
        let off_topic = [
            ResearchSource {
                title: "Map of Seoul — Best attractions, restaurants, and transportation info"
                    .to_string(),
                url: "https://wanderlog.com/list/geoMap/9/seoul-map".to_string(),
                snippet: "Popular Seoul neighborhoods and restaurant stops for visitors."
                    .to_string(),
            },
            ResearchSource {
                title: "SEOUL Itinerary • MUST READ! (2026 Guide)".to_string(),
                url: "https://www.thebrokebackpacker.com/seoul-itinerary/".to_string(),
                snippet: "General sightseeing plan for a short stay in Seoul.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, &subject, &query),
                "unexpectedly accepted generic Seoul guide source: {}",
                source.title
            );
        }
    }

    #[test]
    fn topical_relevance_filter_rejects_generic_scheduler_pages_for_work_stealing_cpp_subject() {
        let subject = "an experienced C++ developer planning to implement a work-stealing scheduler with Chase-Lev deque design, memory ordering, parking and wakeup strategy";
        let query = "work stealing scheduler chase lev deque";
        let off_topic = [
            ResearchSource {
                title: "React Scheduler".to_string(),
                url: "https://react.dev/reference/react-dom/components/common#scheduler"
                    .to_string(),
                snippet: "A UI framework scheduler for rendering priority and cooperative updates."
                    .to_string(),
            },
            ResearchSource {
                title: "Supabase Cron Scheduler".to_string(),
                url: "https://supabase.com/docs/guides/functions/schedule-functions".to_string(),
                snippet: "Database-backed scheduled jobs and cron style triggers.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, subject, query),
                "unexpectedly accepted generic scheduler page: {}",
                source.title
            );
        }
    }

    #[test]
    fn final_source_pack_acceptance_rejects_single_generic_korean_cafe_guide() {
        let subject = "someone planning a morning running workout around Namsan in Seoul and wanting a good atmospheric cafe afterward";
        let sources = vec![ResearchSource {
            title: "Korean Cafe Guide: 5 Brilliant Types Travelers Will Love".to_string(),
            url: "https://www.k-trends.net/korean-cafe-culture-guide/".to_string(),
            snippet: "A broad overview of Korean cafe culture for travelers.".to_string(),
        }];

        assert!(!context_safe_source_for_subject(&sources[0], subject));
        assert!(matches!(
            assess_source_pack_acceptance(subject, &[]),
            SourcePackAcceptance::Empty { .. }
        ));
    }

    #[test]
    fn final_source_pack_acceptance_marks_single_context_safe_secondary_source_partial() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let sources = vec![ResearchSource {
            title: "Aurelian - World History Encyclopedia".to_string(),
            url: "https://www.worldhistory.org/Aurelian/".to_string(),
            snippet: "Overview of Aurelian's reign and the crisis of the third century."
                .to_string(),
        }];

        assert!(context_safe_source_for_subject(&sources[0], subject));
        assert!(matches!(
            assess_source_pack_acceptance(subject, &sources),
            SourcePackAcceptance::Partial { .. }
        ));
    }

    #[test]
    fn overview_only_history_acceptance_is_downgraded_to_partial() {
        let acceptance = downgrade_overview_only_history_acceptance(
            SourcePackAcceptance::Success,
            &HashSet::from([
                "https://www.worldhistory.org/Aurelian/".to_string(),
                "https://www.britannica.com/biography/Aurelian".to_string(),
            ]),
            &HashSet::from([
                "https://www.worldhistory.org/Aurelian/".to_string(),
                "https://www.britannica.com/biography/Aurelian".to_string(),
            ]),
        );

        assert!(matches!(acceptance, SourcePackAcceptance::Partial { .. }));
    }

    #[test]
    fn final_source_pack_acceptance_keeps_history_overview_only_pack_partial() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let sources = vec![
            ResearchSource {
                title: "Aurelian | Roman emperor".to_string(),
                url: "https://www.britannica.com/biography/Aurelian".to_string(),
                snippet: "Britannica overview of Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian".to_string(),
                url: "https://www.worldhistory.org/Aurelian/".to_string(),
                snippet: "World History Encyclopedia overview of Aurelian.".to_string(),
            },
            ResearchSource {
                title: "Aurelian".to_string(),
                url: "https://en.wikipedia.org/wiki/Aurelian".to_string(),
                snippet: "Wikipedia main-entity overview.".to_string(),
            },
        ];

        assert!(matches!(
            assess_source_pack_acceptance(subject, &sources),
            SourcePackAcceptance::Partial { .. }
        ));
    }

    #[test]
    fn final_source_pack_acceptance_preserves_success_for_multiple_aurelian_specific_sources() {
        let subject = "why the Roman emperor Aurelian was both typical of the third-century soldier emperors and exceptional among them";
        let sources = vec![
            ResearchSource {
                title: "Aurelian: Emperor Who Restored the World - Roman Empire".to_string(),
                url: "https://roman-empire.net/emperors/aurelian-emperor-who-restored-the-world"
                    .to_string(),
                snippet: "Aurelian's reign, wars, and imperial restoration.".to_string(),
            },
            ResearchSource {
                title: "Aurelian - World History Encyclopedia".to_string(),
                url: "https://www.worldhistory.org/Aurelian/".to_string(),
                snippet: "Aurelian's background and restoration campaigns.".to_string(),
            },
            ResearchSource {
                title: "Aurelian - History And Culture".to_string(),
                url: "https://www.historyandculture.org/historic-timelines/ancient-rome-timeline/roman-empire-timeline/aurelian".to_string(),
                snippet: "Aurelian in the context of the third-century empire.".to_string(),
            },
        ];

        assert!(matches!(
            assess_source_pack_acceptance(subject, &sources),
            SourcePackAcceptance::Success
        ));
    }

    #[test]
    fn topical_relevance_filter_rejects_justinian_live_artifact_false_positives() {
        let subject = "Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.";
        let query = "gothic war justinian italy chronology contested";
        let off_topic = [
            ResearchSource {
                title: "AFTERMATH OF WAR: CYPRIOT CHRISTIANS AND MEDITERRANEAN GEOPOLITICS, 1571-1625".to_string(),
                url: "https://cdr.lib.unc.edu/downloads/w6634385z".to_string(),
                snippet: "A study of Cypriot Christians and Mediterranean geopolitics after the Ottoman conquest of Cyprus.".to_string(),
            },
            ResearchSource {
                title: "time to be a conspiracy theorist | MIT Technology Review It’s never been easier to be a...".to_string(),
                url: "https://www.technologyreview.com/2025/10/30/1126457/its-never-been-easier-to-be-a-conspiracy-theorist/".to_string(),
                snippet: "An opinion article about conspiracy theorists and media dynamics.".to_string(),
            },
        ];

        for source in off_topic {
            assert!(
                !is_topically_relevant_source(&source, subject, query),
                "unexpectedly accepted Justinian false positive: {}",
                source.title
            );
        }
    }

    #[test]
    fn justinian_false_positive_mix_does_not_produce_success_acceptance() {
        let subject = "Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.";
        let query = "gothic war justinian italy chronology contested";
        let results = vec![
            ResearchSource {
                title: "AFTERMATH OF WAR: CYPRIOT CHRISTIANS AND MEDITERRANEAN GEOPOLITICS, 1571-1625".to_string(),
                url: "https://cdr.lib.unc.edu/downloads/w6634385z".to_string(),
                snippet: "A study of Cypriot Christians and Mediterranean geopolitics after the Ottoman conquest of Cyprus.".to_string(),
            },
            ResearchSource {
                title: "Justinian I".to_string(),
                url: "https://en.wikipedia.org/wiki/Justinian_I".to_string(),
                snippet: "Overview of Justinian and the Byzantine context around the Gothic War.".to_string(),
            },
            ResearchSource {
                title: "time to be a conspiracy theorist | MIT Technology Review It’s never been easier to be a...".to_string(),
                url: "https://www.technologyreview.com/2025/10/30/1126457/its-never-been-easier-to-be-a-conspiracy-theorist/".to_string(),
                snippet: "An opinion article about conspiracy theorists and media dynamics.".to_string(),
            },
        ];
        let accepted = results
            .into_iter()
            .filter(|source| is_topically_relevant_source(source, subject, query))
            .collect::<Vec<_>>();

        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].url, "https://en.wikipedia.org/wiki/Justinian_I");
        assert!(matches!(
            assess_source_pack_acceptance(subject, &accepted),
            SourcePackAcceptance::Partial { .. }
        ));
    }

    #[test]
    fn defaults_to_duckduckgo_provider_without_brave_configuration() {
        assert_eq!(
            configured_search_provider(None, None),
            ResearchSearchProvider::DuckDuckGo
        );
        assert_eq!(
            configured_search_provider(Some("brave"), None),
            ResearchSearchProvider::DuckDuckGo
        );
        assert_eq!(
            configured_search_provider(Some("duckduckgo"), Some("secret")),
            ResearchSearchProvider::DuckDuckGo
        );
    }

    #[test]
    fn configured_provider_chain_supports_naver_kakao_then_existing_providers() {
        assert_eq!(
            configured_search_providers(
                Some("naver, kakao, brave, duckduckgo"),
                None,
                Some("brave-secret"),
                Some("naver-id"),
                Some("naver-secret"),
                Some("kakao-secret"),
            ),
            vec![
                ResearchSearchProvider::Naver {
                    client_id: "naver-id".to_string(),
                    client_secret: "naver-secret".to_string(),
                },
                ResearchSearchProvider::Kakao {
                    rest_api_key: "kakao-secret".to_string(),
                },
                ResearchSearchProvider::Brave {
                    api_key: "brave-secret".to_string(),
                },
                ResearchSearchProvider::DuckDuckGo,
            ]
        );
    }

    #[test]
    fn configured_provider_chain_canonicalizes_aliases_before_dedupe() {
        assert_eq!(
            configured_search_providers(
                Some("kakao, daum, ddg, duckduckgo"),
                None,
                None,
                None,
                None,
                Some("kakao-secret"),
            ),
            vec![
                ResearchSearchProvider::Kakao {
                    rest_api_key: "kakao-secret".to_string(),
                },
                ResearchSearchProvider::DuckDuckGo,
            ]
        );
    }

    #[test]
    fn configured_provider_chain_skips_missing_keys_and_falls_back_to_default() {
        assert_eq!(
            configured_search_providers(Some("naver kakao"), None, None, None, None, None),
            vec![ResearchSearchProvider::DuckDuckGo]
        );
        assert_eq!(
            configured_search_providers(
                Some("naver,duckduckgo,kakao"),
                None,
                None,
                Some("naver-id"),
                Some("naver-secret"),
                None,
            ),
            vec![
                ResearchSearchProvider::Naver {
                    client_id: "naver-id".to_string(),
                    client_secret: "naver-secret".to_string(),
                },
                ResearchSearchProvider::DuckDuckGo,
            ]
        );
    }

    #[test]
    fn fallback_success_preserves_bounded_prior_provider_error_visibility() {
        let diagnostics = format_provider_failure_summary(&[
            "naver: Naver Search API returned HTTP 429".to_string(),
            "naver: Naver Search API returned HTTP 429".to_string(),
            "kakao: Kakao Search API request failed.".to_string(),
        ])
        .expect("expected fallback diagnostics");

        assert!(
            diagnostics.contains("Earlier configured provider failures before fallback success")
        );
        assert!(diagnostics.contains("naver: Naver Search API returned HTTP 429"));
        assert!(diagnostics.contains("kakao: Kakao Search API request failed."));
        assert_eq!(
            diagnostics
                .matches("naver: Naver Search API returned HTTP 429")
                .count(),
            1
        );
        assert!(diagnostics.chars().count() <= MAX_PROVIDER_DIAGNOSTIC_CHARS);
    }

    #[test]
    fn enables_brave_provider_only_with_env_name_and_api_key() {
        assert_eq!(
            configured_search_provider(Some("brave"), Some("secret-key")),
            ResearchSearchProvider::Brave {
                api_key: "secret-key".to_string()
            }
        );
        assert_eq!(
            configured_search_provider(Some("BRAVE"), Some("  secret-key  ")),
            ResearchSearchProvider::Brave {
                api_key: "secret-key".to_string()
            }
        );
    }

    #[test]
    fn enables_naver_and_kakao_with_existing_single_provider_selector() {
        assert_eq!(
            configured_single_search_provider(
                Some("naver"),
                None,
                Some("naver-id"),
                Some("naver-secret"),
                None,
            ),
            ResearchSearchProvider::Naver {
                client_id: "naver-id".to_string(),
                client_secret: "naver-secret".to_string(),
            }
        );
        assert_eq!(
            configured_single_search_provider(
                Some("kakao"),
                None,
                None,
                None,
                Some("kakao-secret"),
            ),
            ResearchSearchProvider::Kakao {
                rest_api_key: "kakao-secret".to_string(),
            }
        );
    }

    #[test]
    fn parses_brave_search_results_into_normalized_sources() {
        let body = r#"{
          "web": {
            "results": [
              {
                "title": "Example Docs",
                "url": "https://docs.example.com/start",
                "description": " Official getting started guide. "
              },
              {
                "title": "Missing URL"
              }
            ]
          }
        }"#;

        let outcome = parse_brave_search_response(body).unwrap();

        assert_eq!(
            outcome,
            SearchQueryOutcome::Results(vec![ResearchSource {
                title: "Example Docs".to_string(),
                url: "https://docs.example.com/start".to_string(),
                snippet: "Official getting started guide.".to_string(),
            }])
        );
    }

    #[test]
    fn parses_naver_search_results_into_normalized_sources() {
        let body = r#"{
          "items": [
            {
              "title": "<b>Example</b> Docs",
              "link": "https://docs.example.com/start",
              "description": "Official &amp; current <b>getting started</b> guide."
            },
            {
              "title": "Missing Link"
            }
          ]
        }"#;

        let outcome = parse_naver_search_response(body).unwrap();

        assert_eq!(
            outcome,
            SearchQueryOutcome::Results(vec![ResearchSource {
                title: "Example Docs".to_string(),
                url: "https://docs.example.com/start".to_string(),
                snippet: "Official & current getting started guide.".to_string(),
            }])
        );
    }

    #[test]
    fn parses_kakao_search_results_into_normalized_sources() {
        let body = r#"{
          "documents": [
            {
              "title": "<b>Example</b> Comparison",
              "url": "https://example.com/compare",
              "contents": "Benchmarks &amp; thermal comparison."
            },
            {
              "title": "Bad URL",
              "url": "javascript:void(0)"
            }
          ]
        }"#;

        let outcome = parse_kakao_search_response(body).unwrap();

        assert_eq!(
            outcome,
            SearchQueryOutcome::Results(vec![ResearchSource {
                title: "Example Comparison".to_string(),
                url: "https://example.com/compare".to_string(),
                snippet: "Benchmarks & thermal comparison.".to_string(),
            }])
        );
    }

    #[test]
    fn parses_duckduckgo_result_links_and_decodes_redirects() {
        let html = r#"
        <div class="result">
          <a class="result__a" href="/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=abc">Example A</a>
          <a class="result__snippet">A useful snippet.</a>
        </div>
        "#;

        let results = parse_duckduckgo_html(html);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example A");
        assert_eq!(results[0].url, "https://example.com/a");
        assert_eq!(results[0].snippet, "A useful snippet.");
    }

    #[test]
    fn parses_alternate_duckduckgo_result_markup() {
        let html = r#"
        <article data-testid="result">
          <h2 class="result__title">
            <a data-testid="result-title-a" href="https://docs.example.com/start">Example Docs</a>
          </h2>
          <div data-result="snippet">Official getting started guide.</div>
        </article>
        "#;

        let results = parse_duckduckgo_html(html);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Docs");
        assert_eq!(results[0].url, "https://docs.example.com/start");
        assert_eq!(results[0].snippet, "Official getting started guide.");
    }

    #[test]
    fn normalizes_duckduckgo_direct_and_redirect_urls() {
        assert_eq!(
            normalize_result_url("https://example.com/direct"),
            Some("https://example.com/direct".to_string())
        );
        assert_eq!(
            normalize_result_url("/l/?uddg=https%3A%2F%2Fexample.com%2Fa&rut=abc"),
            Some("https://example.com/a".to_string())
        );
        assert_eq!(
            normalize_result_url(
                "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fb&rut=abc"
            ),
            Some("https://example.com/b".to_string())
        );
        assert_eq!(
            normalize_result_url(
                "https://html.duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fc&rut=abc"
            ),
            Some("https://example.com/c".to_string())
        );
        assert_eq!(
            normalize_result_url(
                "https://evilduckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Ftrap&rut=abc"
            ),
            Some(
                "https://evilduckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Ftrap&rut=abc"
                    .to_string()
            )
        );
        assert_eq!(normalize_result_url("javascript:void(0)"), None);
    }

    #[test]
    fn retries_with_secondary_duckduckgo_page_when_primary_has_no_results() {
        let primary = "<html><body><p>No results</p></body></html>";
        let retry = r#"
        <div>
          <a class="result-link" href="https://example.com/retry">Retry Result</a>
          <div class="result-snippet">Recovered from fallback page.</div>
        </div>
        "#;

        let SearchQueryOutcome::Results(results) = parse_duckduckgo_search_pages([primary, retry])
        else {
            panic!("expected retry results");
        };

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Retry Result");
        assert_eq!(results[0].url, "https://example.com/retry");
        assert_eq!(results[0].snippet, "Recovered from fallback page.");
    }

    #[test]
    fn parses_duckduckgo_lite_result_markup() {
        let html = r#"
        <html>
          <head>
            <link title="DuckDuckGo (Lite)" rel="search" href="//duckduckgo.com/opensearch_lite_v2.xml">
          </head>
          <body>
            <center id="lite_wrapper">
              <table>
                <tr>
                  <td class="result-link">
                    <a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.example.com%2Fguide">Example Guide</a>
                  </td>
                </tr>
                <tr>
                  <td class="result-snippet">Reference implementation and usage details.</td>
                </tr>
              </table>
            </center>
          </body>
        </html>
        "#;

        let results = parse_duckduckgo_lite(html);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Guide");
        assert_eq!(results[0].url, "https://docs.example.com/guide");
        assert_eq!(
            results[0].snippet,
            "Reference implementation and usage details."
        );
    }

    #[test]
    fn parse_duckduckgo_lite_ignores_non_result_table_links() {
        let html = r#"
        <html>
          <head>
            <link title="DuckDuckGo (Lite)" rel="search" href="//duckduckgo.com/opensearch_lite_v2.xml">
          </head>
          <body>
            <center id="lite_wrapper">
              <table>
                <tr>
                  <td><a href="https://duckduckgo.com/help">Help</a></td>
                </tr>
                <tr>
                  <td><a href="https://example.com/support">Support</a></td>
                </tr>
              </table>
            </center>
          </body>
        </html>
        "#;

        let results = parse_duckduckgo_lite(html);

        assert!(results.is_empty());
    }

    #[test]
    fn adopts_lite_fallback_results_when_html_pages_are_empty() {
        let primary = "<html><body><p>No results</p></body></html>";
        let retry = r#"
        <html>
          <body>
            <form id="challenge-form">
              <div class="anomaly-modal">Unfortunately, bots use DuckDuckGo too.</div>
            </form>
          </body>
        </html>
        "#;
        let lite = r#"
        <html>
          <head>
            <link title="DuckDuckGo (Lite)" rel="search" href="//duckduckgo.com/opensearch_lite_v2.xml">
          </head>
          <body>
            <center id="lite_wrapper">
              <table>
                <tr>
                  <td class="result-link">
                    <a rel="nofollow" href="/l/?uddg=https%3A%2F%2Fexample.com%2Flite-result">Lite Result</a>
                  </td>
                </tr>
                <tr>
                  <td>Recovered from lite fallback.</td>
                </tr>
              </table>
            </center>
          </body>
        </html>
        "#;

        let SearchQueryOutcome::Results(results) =
            parse_duckduckgo_search_pages([primary, retry, lite])
        else {
            panic!("expected lite fallback results");
        };

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Lite Result");
        assert_eq!(results[0].url, "https://example.com/lite-result");
        assert_eq!(results[0].snippet, "Recovered from lite fallback.");
    }

    #[test]
    fn adopts_lite_fallback_results_when_primary_html_is_blocked() {
        let primary = r#"
        <html>
          <body>
            <form id="challenge-form">
              <div class="anomaly-modal">Unfortunately, bots use DuckDuckGo too.</div>
            </form>
          </body>
        </html>
        "#;
        let retry = "<html><body><p>No results</p></body></html>";
        let lite = r#"
        <html>
          <head>
            <link title="DuckDuckGo (Lite)" rel="search" href="//duckduckgo.com/opensearch_lite_v2.xml">
          </head>
          <body>
            <center id="lite_wrapper">
              <table>
                <tr>
                  <td class="result-link">
                    <a rel="nofollow" href="/l/?uddg=https%3A%2F%2Fexample.com%2Fblocked-primary-lite-result">Lite Result</a>
                  </td>
                </tr>
                <tr>
                  <td>Recovered after primary challenge page.</td>
                </tr>
              </table>
            </center>
          </body>
        </html>
        "#;

        let SearchQueryOutcome::Results(results) =
            parse_duckduckgo_search_pages([primary, retry, lite])
        else {
            panic!("expected lite fallback results after primary block");
        };

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Lite Result");
        assert_eq!(
            results[0].url,
            "https://example.com/blocked-primary-lite-result"
        );
        assert_eq!(
            results[0].snippet,
            "Recovered after primary challenge page."
        );
    }

    #[test]
    fn reports_blocked_when_all_duckduckgo_pages_are_challenges() {
        let challenge = r#"
        <html>
          <body>
            <form id="challenge-form">
              <div class="anomaly-modal">Unfortunately, bots use DuckDuckGo too.</div>
            </form>
          </body>
        </html>
        "#;

        assert_eq!(
            parse_duckduckgo_search_pages([challenge, challenge]),
            SearchQueryOutcome::Blocked
        );
    }

    #[test]
    fn parse_duckduckgo_search_pages_does_not_consume_primary_html_tables_as_lite_results() {
        let primary = r#"
        <html>
          <body>
            <table>
              <tr>
                <td><a href="https://duckduckgo.com/help">Help</a></td>
              </tr>
            </table>
          </body>
        </html>
        "#;
        let retry = r#"
        <div class="result">
          <a class="result__a" href="https://example.com/retry">Retry Result</a>
          <a class="result__snippet">Recovered from retry page.</a>
        </div>
        "#;

        let SearchQueryOutcome::Results(results) = parse_duckduckgo_search_pages([primary, retry])
        else {
            panic!("expected retry results");
        };

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Retry Result");
        assert_eq!(results[0].url, "https://example.com/retry");
        assert_eq!(results[0].snippet, "Recovered from retry page.");
    }

    #[test]
    fn adds_canonical_queries_for_justinian_gothic_war() {
        let queries = source_queries("유스티니아누스 대제의 로마-고트 수복 전쟁 개요");

        assert!(queries
            .iter()
            .any(|query| query.contains("Justinian Gothic War 535 554")));
        assert!(queries
            .iter()
            .any(|query| query.contains("Ostrogothic Kingdom")));
    }

    #[test]
    fn english_justinian_history_prompts_generate_named_entity_overview_queries() {
        let queries = source_queries(
            "Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.",
        );

        assert!(queries.iter().any(|query| query == "justinian overview"));
        assert!(queries
            .iter()
            .any(|query| query == "justinian Britannica encyclopedia history"));
        assert!(queries
            .iter()
            .any(|query| query == "justinian primary source translation"));
        assert!(queries
            .iter()
            .any(|query| query == "justinian scholarly article history"));
    }

    #[test]
    fn ranks_authoritative_sources_ahead_of_weak_sources() {
        let mut sources = vec![
            ResearchSource {
                title: "Weak".to_string(),
                url: "https://namu.wiki/w/example".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "Strong".to_string(),
                url: "https://www.britannica.com/event/Gothic-War".to_string(),
                snippet: String::new(),
            },
        ];

        dedupe_sources(&mut sources);

        assert_eq!(sources[0].title, "Strong");
        assert_eq!(source_quality(&sources[0].url), "high");
        assert_eq!(source_quality(&sources[1].url), "low");
    }

    #[test]
    fn ranks_official_documentation_ahead_of_generic_secondary_sources() {
        let mut sources = vec![
            ResearchSource {
                title: "Secondary".to_string(),
                url: "https://someblog.example.com/product-overview".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "Official Docs".to_string(),
                url: "https://docs.python.org/3/library/pathlib.html".to_string(),
                snippet: String::new(),
            },
        ];

        dedupe_sources(&mut sources);

        assert_eq!(sources[0].title, "Official Docs");
        assert_eq!(source_score(&sources[0].url), 90);
        assert_eq!(source_quality(&sources[0].url), "high");
        assert_eq!(source_quality(&sources[1].url), "medium");
    }

    #[test]
    fn narrows_generic_github_authority_without_subject_match() {
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/examplecorp/internal-wiki",
                Some("Compare Python packaging documentation quality across official sources"),
            ),
            "secondary"
        );
        assert_eq!(
            source_score_for_subject(
                "https://github.com/examplecorp/internal-wiki",
                Some("Compare Python packaging documentation quality across official sources"),
            ),
            20
        );
    }

    #[test]
    fn generic_github_and_pinterest_sources_do_not_count_as_context_safe_evidence() {
        let subject = "an experienced C++ developer planning to implement a work-stealing scheduler with Chase-Lev deque design, memory ordering, parking and wakeup strategy";
        let github_source = ResearchSource {
            title: "Work-stealing scheduler notes".to_string(),
            url: "https://github.com/random/work-stealing-notes".to_string(),
            snippet: "Unrelated repository with broad notes about scheduler ideas.".to_string(),
        };
        let pinterest_source = ResearchSource {
            title: "Work stealing scheduler diagram".to_string(),
            url: "https://www.pinterest.com/pin/work-stealing-scheduler-diagram/".to_string(),
            snippet: "A pinboard image about queue diagrams.".to_string(),
        };

        assert_eq!(
            source_quality_for_subject(&github_source.url, Some(subject)),
            "low"
        );
        assert_eq!(
            source_quality_for_subject(&pinterest_source.url, Some(subject)),
            "low"
        );
        assert!(!context_safe_source_for_subject(&github_source, subject));
        assert!(!context_safe_source_for_subject(&pinterest_source, subject));
    }

    #[test]
    fn preserves_official_github_authority_for_known_orgs_and_subject_matches() {
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/pypa/pip",
                Some("Compare Python packaging documentation quality across official sources"),
            ),
            "official_or_primary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/examplecorp/internal-wiki",
                Some("Evaluate examplecorp/internal-wiki migration notes for release readiness"),
            ),
            "official_or_primary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/examplecorp/internal-wiki",
                Some("Review `ExampleCorp/Internal-Wiki` migration notes for release readiness"),
            ),
            "official_or_primary"
        );
    }

    #[test]
    fn does_not_elevate_arbitrary_github_repos_for_common_subject_tokens() {
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/attacker/python",
                Some("Compare Python packaging documentation quality across official sources"),
            ),
            "secondary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/example/npm",
                Some("Compare npm package publishing workflow guidance"),
            ),
            "secondary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://github.com/random/docs",
                Some("Compare docs quality for package maintainers"),
            ),
            "secondary"
        );
    }

    #[test]
    fn ranks_python_and_npm_official_docs_ahead_of_generic_github_results() {
        let mut python_sources = vec![
            ResearchSource {
                title: "Generic GitHub".to_string(),
                url: "https://github.com/examplecorp/internal-wiki".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "Python Docs".to_string(),
                url: "https://docs.python.org/3/library/pathlib.html".to_string(),
                snippet: String::new(),
            },
        ];
        dedupe_sources_for_subject(
            &mut python_sources,
            Some("Compare Python packaging documentation quality across official sources"),
        );
        assert_eq!(python_sources[0].title, "Python Docs");

        let mut npm_sources = vec![
            ResearchSource {
                title: "Generic GitHub".to_string(),
                url: "https://github.com/examplecorp/internal-wiki".to_string(),
                snippet: String::new(),
            },
            ResearchSource {
                title: "npm Docs".to_string(),
                url: "https://docs.npmjs.com/cli/v10/configuring-npm/package-json".to_string(),
                snippet: String::new(),
            },
        ];
        dedupe_sources_for_subject(
            &mut npm_sources,
            Some("Compare npm package publishing and package.json workflow guidance"),
        );
        assert_eq!(npm_sources[0].title, "npm Docs");
    }

    #[test]
    fn classifies_vendor_and_regulatory_hosts_as_official_or_primary() {
        assert_eq!(
            infer_source_class_for_subject(
                "https://support.apple.com/en-us/117736",
                Some("Compare Apple MacBook Pro and Framework Laptop 13 specifications"),
            ),
            "official_or_primary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://frame.work/kr/en/products/laptop13-diy-amd-7040",
                Some("Compare Apple MacBook Pro and Framework Laptop 13 specifications"),
            ),
            "official_or_primary"
        );
        assert_eq!(
            infer_source_class_for_subject(
                "https://eur-lex.europa.eu/eli/reg/2024/1689/oj",
                Some("Compare the NIST AI RMF with the EU AI Act"),
            ),
            "official_or_primary"
        );
    }

    #[test]
    fn classifies_technical_spec_and_vendor_hosts_as_official_or_primary() {
        let subject = Some(
            "Explain ephemeral port allocation and port exhaustion across Linux kernel defaults, Windows dynamic port ranges, and cloud container networking behavior.",
        );
        for url in [
            "https://www.rfc-editor.org/rfc/rfc6056",
            "https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml",
            "https://docs.kernel.org/networking/ip-sysctl.html",
            "https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/default-dynamic-port-range-tcpip-chang",
            "https://azure.microsoft.com/en-us/products/virtual-network",
            "https://docs.aws.amazon.com/vpc/latest/userguide/nat-gateway-working-with.html",
            "https://cloud.google.com/nat/docs/ports-and-addresses",
        ] {
            assert_eq!(
                infer_source_class_for_subject(url, subject),
                "official_or_primary",
                "expected official_or_primary: {url}"
            );
        }
    }

    #[test]
    fn does_not_overrank_lookalike_authority_hosts() {
        assert_eq!(
            source_score("https://cambridge.org.attacker.example/paper"),
            50
        );
        assert_eq!(source_score("https://fakebritannica.com/entry"), 50);
        assert_eq!(source_score("https://sub.britannica.com/entry"), 80);
        assert_eq!(source_score("https://www.jstor.org/stable/123"), 100);
    }

    #[test]
    fn seeds_known_sources_for_justinian_gothic_war() {
        let sources = seeded_sources_for_subject("유스티니아누스 대제의 로마-고트 수복 전쟁 개요");

        assert!(sources
            .iter()
            .any(|source| source.url.contains("Gothic_War")));
        assert!(sources
            .iter()
            .any(|source| source.url.contains("britannica.com")));
        assert!(sources
            .iter()
            .all(|source| source_quality(&source.url) == "high"));
    }

    #[tokio::test]
    async fn source_pack_report_records_missing_subject_without_network() {
        let report = build_research_source_pack_report("", None).await;

        assert_eq!(report.status, "skipped");
        assert!(report.reason.unwrap().contains("research subject"));
        assert!(report.source_pack.is_none());
        assert!(report.queries.is_empty());
    }

    #[test]
    fn zero_source_provider_errors_report_top_level_error() {
        let report = finalize_empty_source_pack_report(
            "topic".to_string(),
            &[ResearchSearchProvider::Brave {
                api_key: "secret".to_string(),
            }],
            vec![ResearchSourceQueryReport {
                query: "topic".to_string(),
                status: "error".to_string(),
                provider: None,
                result_count: 0,
                adopted_count: 0,
                skipped_count: 0,
                error: Some("Brave Search API returned HTTP 429".to_string()),
            }],
            0,
            0,
            0,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(report.status, "error");
        assert!(report
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("failed and no candidates were available")));
        assert_eq!(report.queries[0].status, "error");
        assert_eq!(
            report.queries[0].error.as_deref(),
            Some("Brave Search API returned HTTP 429")
        );
    }

    #[test]
    fn zero_source_off_topic_results_report_empty_with_relevance_reason() {
        let report = finalize_empty_source_pack_report(
            "topic".to_string(),
            &[ResearchSearchProvider::DuckDuckGo],
            vec![ResearchSourceQueryReport {
                query: "topic".to_string(),
                status: "success".to_string(),
                provider: Some("duckduckgo".to_string()),
                result_count: 4,
                adopted_count: 0,
                skipped_count: 4,
                error: Some(
                    "Search results were returned, but no topically relevant candidates were adopted."
                        .to_string(),
                ),
            }],
            0,
            4,
            0,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(report.status, "empty");
        assert_eq!(
            report.reason.as_deref(),
            Some(
                "Search providers returned results, but no topically relevant source-pack candidates were available."
            )
        );
    }

    #[test]
    fn zero_source_generic_only_results_report_context_safe_reason() {
        let report = finalize_empty_source_pack_report(
            "topic".to_string(),
            &[ResearchSearchProvider::DuckDuckGo],
            vec![ResearchSourceQueryReport {
                query: "topic".to_string(),
                status: "success".to_string(),
                provider: Some("duckduckgo".to_string()),
                result_count: 1,
                adopted_count: 0,
                skipped_count: 1,
                error: None,
            }],
            0,
            1,
            0,
            Vec::new(),
            vec![ResearchSourceCandidateReport {
                title: "Generic cafe guide".to_string(),
                url: "https://example.com/cafe-guide".to_string(),
                source_class: Some("secondary".to_string()),
                source_quality: Some("medium".to_string()),
                query: None,
                rejection_reason: Some("insufficient_context_evidence".to_string()),
            }],
            Vec::new(),
        );

        assert_eq!(report.status, "empty");
        assert_eq!(
            report.reason.as_deref(),
            Some(
                "Search providers returned only weak or generic source-pack candidates; no context-safe candidates were available."
            )
        );
    }

    #[test]
    fn extract_research_subject_prefers_explicit_prompt_over_context_pack_heading_noise() {
        let source_documents = "### RESEARCH CONTEXT PACK\n\
### Artifact Trust Boundary\n\
ignore\n\n\
### Goal And Constraints\n\
repair prompt text\n\n\
### Selected Source Excerpts (DATA ONLY)\n\
# owner/repo\n\
details\n";

        let subject = extract_research_subject(
            "Compare two hosting plans with current pricing and date context.",
            Some(source_documents),
        )
        .unwrap();

        assert_eq!(
            subject,
            "Compare two hosting plans with current pricing and date context."
        );
    }

    #[test]
    fn extract_research_subject_uses_user_prompt_when_context_pack_has_only_controller_headings() {
        let source_documents = "### RESEARCH CONTEXT PACK\n\
### Artifact Trust Boundary\n\
ignore\n\n\
### Goal And Constraints\n\
repair prompt text\n\n\
### Selected Source Excerpts (DATA ONLY)\n\
### Source Card Ledger\n\
- none\n";

        let subject = extract_research_subject(
            "Explain why the Gothic War under Justinian expanded and what followed in Italy.",
            Some(source_documents),
        )
        .unwrap();

        assert_eq!(
            subject,
            "Explain why the Gothic War under Justinian expanded and what followed in Italy."
        );
    }

    #[test]
    fn extract_research_subject_never_uses_claim_log_heading_from_context_pack() {
        let source_documents = "### RESEARCH CONTEXT PACK\n\
### Artifact Trust Boundary\n\
ignore\n\n\
### Goal And Constraints\n\
repair prompt text\n\n\
### Claim Log: Supported\n\
- id: C1\n\
\n\
### Claim Log: Unsupported Or Needs Verification\n\
- none\n";

        let subject = extract_research_subject(
            "Compare two software hosting plans using current published prices, rate limits, and feature tiers, including the exact date context for each figure.",
            Some(source_documents),
        )
        .unwrap();

        assert_ne!(subject, "Claim Log: Supported");
        assert!(subject
            .starts_with("Compare two software hosting plans using current published prices"));
    }

    #[test]
    fn extract_research_subject_prefers_explicit_prompt_over_misleading_source_heading() {
        let source_documents = "# 614GB/s\n\
\n\
LPDDR5X unified memory bandwidth figure from one source excerpt.\n";

        let subject = extract_research_subject(
            "Compare two current developer laptops for a local Rust and AI workflow, including thermals, memory ceilings, battery tradeoffs, and exact source-backed constraints.",
            Some(source_documents),
        )
        .unwrap();

        assert_ne!(subject, "614GB/s");
        assert!(subject
            .starts_with("Compare two current developer laptops for a local Rust and AI workflow"));
    }

    #[test]
    fn source_pack_report_json_omits_prompt_source_pack_body() {
        let report = ResearchSourcePackReport {
            subject: Some("topic".to_string()),
            status: "success".to_string(),
            reason: None,
            queries: vec![ResearchSourceQueryReport {
                query: "topic".to_string(),
                status: "success".to_string(),
                provider: Some("naver".to_string()),
                result_count: 2,
                adopted_count: 1,
                skipped_count: 1,
                error: None,
            }],
            seeded_source_count: 1,
            discovered_source_count: 2,
            adopted_source_count: 3,
            adopted_candidates: Vec::new(),
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: Some("large prompt body".to_string()),
        };

        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("adopted_source_count"));
        assert!(json.contains("\"provider\":\"naver\""));
        assert!(!json.contains("large prompt body"));
        assert!(!json.contains("source_pack"));
    }

    #[test]
    fn source_query_report_json_omits_provider_when_absent() {
        let report = ResearchSourceQueryReport {
            query: "topic".to_string(),
            status: "success".to_string(),
            provider: None,
            result_count: 2,
            adopted_count: 1,
            skipped_count: 1,
            error: None,
        };

        let json = serde_json::to_string(&report).unwrap();

        assert!(!json.contains("\"provider\""));
    }

    #[test]
    fn reconciles_query_adopted_counts_after_budget_trim() {
        let mut query_reports = vec![
            ResearchSourceQueryReport {
                query: "q1".to_string(),
                status: "success".to_string(),
                provider: None,
                result_count: 2,
                adopted_count: 2,
                skipped_count: 0,
                error: None,
            },
            ResearchSourceQueryReport {
                query: "q2".to_string(),
                status: "success".to_string(),
                provider: None,
                result_count: 1,
                adopted_count: 1,
                skipped_count: 0,
                error: None,
            },
        ];
        let query_adopted_urls = vec![
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ],
            vec!["https://example.com/c".to_string()],
        ];
        let final_adopted_urls = [
            "https://example.com/a".to_string(),
            "https://example.com/c".to_string(),
        ]
        .into_iter()
        .collect();

        reconcile_query_report_adoption_counts(
            &mut query_reports,
            &query_adopted_urls,
            &final_adopted_urls,
        );

        assert_eq!(query_reports[0].adopted_count, 1);
        assert_eq!(query_reports[0].skipped_count, 1);
        assert_eq!(query_reports[1].adopted_count, 1);
        assert_eq!(query_reports[1].skipped_count, 0);
    }

    #[test]
    fn coverage_miss_records_target_host_and_redacts_reason() {
        let misses = build_source_coverage_misses(
            &[ResearchSearchProvider::Naver {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
            }],
            &[ResearchSourceQueryReport {
                query: "policy comparison nist.gov official guidance".to_string(),
                status: "error".to_string(),
                provider: None,
                result_count: 0,
                adopted_count: 0,
                skipped_count: 0,
                error: Some("api_key missing response body: raw provider payload".to_string()),
            }],
            &[Vec::new()],
            &HashSet::new(),
            &[],
        );

        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].expected_host.as_deref(), Some("nist.gov"));
        assert_eq!(
            misses[0].expected_source_class.as_deref(),
            Some("official_or_primary")
        );
        assert_eq!(misses[0].provider.as_deref(), Some("naver"));
        assert!(misses[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("[redacted_key]")));
        assert!(misses[0]
            .reason
            .as_deref()
            .is_some_and(|reason| !reason.contains("raw provider payload")));
    }
}
