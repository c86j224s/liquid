use std::collections::HashSet;

use liquid_protocol::{
    ResearchClaimLogEntry, ResearchControllerArtifacts, ResearchSourceCard,
    ResearchSourceDiagnosticsEnvelope, ResearchSourcePackReport,
};
use liquid_research_artifacts::{
    PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING, PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT,
    PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION,
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF,
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING,
};
use liquid_research_core::{
    extract_supported_visible_claim_log_entries, normalize_absolute_public_evidence_url,
    source_card_is_local_pi_provenance_scaffold,
};
use url::Url;

use crate::push_unique_warning;

pub const MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR: &str =
    "missing machine-readable research artifact JSON block";

pub fn has_local_pi_source_pack_source_card_scaffold(
    artifacts: &ResearchControllerArtifacts,
) -> bool {
    !artifacts.source_cards.is_empty()
        && artifacts
            .warnings
            .iter()
            .any(|warning| warning == PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING)
        && artifacts
            .source_cards
            .iter()
            .all(source_card_is_local_pi_provenance_scaffold)
}

pub fn scaffold_support_url_is_public_full_url(url: &str) -> bool {
    normalize_absolute_public_evidence_url(url.trim()).is_some()
}

pub fn source_card_id_is_public_support(
    artifacts: &ResearchControllerArtifacts,
    source_card_id: &str,
) -> bool {
    let trimmed = source_card_id.trim();
    !trimmed.is_empty()
        && artifacts.source_cards.iter().any(|card| {
            card.id.trim() == trimmed && scaffold_support_url_is_public_full_url(&card.url)
        })
}

pub fn has_supported_claim_log_for_scaffold(artifacts: &ResearchControllerArtifacts) -> bool {
    artifacts.claim_log.iter().any(|claim| {
        claim
            .support_source_card_ids
            .iter()
            .any(|id| source_card_id_is_public_support(artifacts, id))
            || claim
                .support_urls
                .iter()
                .any(|url| scaffold_support_url_is_public_full_url(url))
    })
}

pub fn scaffold_claim_has_direct_public_url_support(claim: &ResearchClaimLogEntry) -> bool {
    claim
        .support_urls
        .iter()
        .any(|url| scaffold_support_url_is_public_full_url(url))
}

pub fn scaffold_trust_block_failure(
    artifacts: &ResearchControllerArtifacts,
    _research_intensity: Option<&str>,
    quality_depth: Option<&str>,
) -> Option<String> {
    if has_local_pi_source_pack_source_card_scaffold(artifacts) {
        if !has_supported_claim_log_for_scaffold(artifacts) {
            return Some(
                "Research artifact gate failed: local pi provenance scaffold requires at least one supported Claim Log row before trust can pass".to_string(),
            );
        }
        if quality_depth == Some("strict") {
            let all_rows_have_public_url_support = !artifacts.claim_log.is_empty()
                && artifacts
                    .claim_log
                    .iter()
                    .all(scaffold_claim_has_direct_public_url_support);
            if !all_rows_have_public_url_support {
                return Some(
                    "Research artifact gate failed: strict local pi provenance scaffold requires every Claim Log row to include at least one public support URL before trust can pass".to_string(),
                );
            }
        }
    }
    None
}

pub fn local_pi_source_pack_scaffold_cards_for_iteration(
    current: &ResearchControllerArtifacts,
    parsed: Option<&ResearchControllerArtifacts>,
    parse_error: Option<&str>,
    diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
    is_local_pi: bool,
) -> Option<Vec<ResearchSourceCard>> {
    if !is_local_pi || !current.source_cards.is_empty() || !current.claim_log.is_empty() {
        return None;
    }
    let parse_condition_matches = match (parsed, parse_error) {
        (Some(artifacts), None) => {
            artifacts.source_cards.is_empty() && artifacts.claim_log.is_empty()
        }
        (None, Some(error)) => error == MISSING_RESEARCH_ARTIFACT_BLOCK_ERROR,
        _ => false,
    };
    if !parse_condition_matches {
        return None;
    }
    let source_pack = diagnostics?.source_pack.as_ref()?;
    if source_pack.status != "success" || source_pack.adopted_candidates.is_empty() {
        return None;
    }
    let cards = build_local_pi_source_pack_provenance_source_cards(source_pack);
    (!cards.is_empty()).then_some(cards)
}

pub fn preserve_local_pi_scaffolded_source_cards_for_iteration(
    current: &ResearchControllerArtifacts,
    parsed: &mut ResearchControllerArtifacts,
    is_local_pi: bool,
) {
    if !is_local_pi
        || !parsed.source_cards.is_empty()
        || !has_local_pi_source_pack_source_card_scaffold(current)
    {
        return;
    }
    parsed.source_cards = current.source_cards.clone();
    push_unique_warning(
        &mut parsed.warnings,
        PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING.to_string(),
    );
}

pub fn local_pi_repaired_claim_log_for_iteration(
    current: &ResearchControllerArtifacts,
    parsed_claim_log_is_empty: bool,
    scaffold_authorized: bool,
    normalized_output: &str,
    source_cards: &[ResearchSourceCard],
    is_local_pi: bool,
) -> Option<Vec<ResearchClaimLogEntry>> {
    if !is_local_pi
        || !current.claim_log.is_empty()
        || source_cards.is_empty()
        || !parsed_claim_log_is_empty
        || !scaffold_authorized
    {
        return None;
    }
    let repaired = extract_supported_visible_claim_log_entries(normalized_output, source_cards);
    (!repaired.is_empty()).then_some(repaired)
}

pub fn local_pi_scaffold_repair_warning() -> &'static str {
    PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING
}

pub fn local_pi_scaffold_source_card_warning() -> &'static str {
    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING
}

pub fn trusted_public_source_card_ids(artifacts: &ResearchControllerArtifacts) -> HashSet<String> {
    artifacts
        .source_cards
        .iter()
        .filter(|card| scaffold_support_url_is_public_full_url(&card.url))
        .map(|card| card.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

fn build_local_pi_source_pack_provenance_source_cards(
    report: &ResearchSourcePackReport,
) -> Vec<ResearchSourceCard> {
    let mut seen_urls = HashSet::new();
    report
        .adopted_candidates
        .iter()
        .filter_map(|candidate| {
            let url = normalize_source_pack_scaffold_url(&candidate.url)?;
            if !seen_urls.insert(url.clone()) {
                return None;
            }
            let parsed = Url::parse(&url).ok()?;
            let title = sanitize_scaffold_source_card_title(
                &candidate.title,
                parsed.host_str().unwrap_or(&url),
            );
            let source_class =
                infer_source_class_for_subject(&url, report.subject.as_deref()).to_string();
            let confidence =
                Some(source_quality_for_subject(&url, report.subject.as_deref()).to_string());
            Some(ResearchSourceCard {
                id: format!("SP{}", seen_urls.len()),
                url,
                title,
                source_class,
                accessed_at: None,
                extracted_facts: vec![PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT.to_string()],
                limitation: Some(PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION.to_string()),
                diagnostics_ref: Some(
                    PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF.to_string(),
                ),
                confidence,
            })
        })
        .collect()
}

fn normalize_source_pack_scaffold_url(raw_url: &str) -> Option<String> {
    normalize_absolute_public_evidence_url(raw_url.trim())
}

fn sanitize_scaffold_source_card_title(title: &str, fallback_host: &str) -> String {
    let sanitized = sanitize_search_text(title);
    let bounded = if sanitized.chars().count() > 240 {
        sanitized.chars().take(240).collect::<String>()
    } else {
        sanitized
    };
    let bounded = bounded.trim();
    if bounded.is_empty() {
        sanitize_search_text(fallback_host)
    } else {
        bounded.to_string()
    }
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
            | "docs.ollama.com"
            | "lmstudio.ai"
            | "docs.vllm.ai"
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

fn source_quality_for_subject(url: &str, subject: Option<&str>) -> &'static str {
    match source_score_for_subject(url, subject) {
        80.. => "high",
        50..=79 => "medium",
        _ => "low",
    }
}

fn source_score_for_subject(url: &str, subject: Option<&str>) -> u8 {
    let Ok(parsed) = Url::parse(url) else {
        return 0;
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
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

fn subject_contains_any(subject: &str, needles: &[&str]) -> bool {
    let lower = subject.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn historical_supplementary_contested_source_url(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.contains("sha-")
        || url.contains("historia-augusta")
        || url.contains("sourcebooks.fordham.edu/ancient/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquid_protocol::ResearchSourceCandidateReport;

    #[test]
    fn normalize_public_evidence_url_rejects_private_local_and_metadata_hosts() {
        for raw_url in [
            "http://127.0.0.1/private",
            "http://169.254.169.254/latest/meta-data/",
            "http://localhost/private",
            "https://metadata.google.internal/computeMetadata/v1",
            "https://instance-data.ec2.internal/latest/meta-data/",
            "https://service.internal/secret",
            "https://printer.local/status",
            "https://nas.localdomain/admin",
            "https://router.home.arpa/",
        ] {
            assert_eq!(
                liquid_research_core::normalize_public_evidence_url(raw_url),
                None,
                "unsafe evidence URL should be rejected: {raw_url}"
            );
        }
        assert_eq!(
            liquid_research_core::normalize_public_evidence_url("https://docs.vllm.ai/en/latest/")
                .as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
    }

    #[test]
    fn normalize_absolute_public_evidence_url_rejects_duckduckgo_relative_uddg() {
        assert_eq!(
            liquid_research_core::normalize_absolute_public_evidence_url(
                "https://duckduckgo.com/l/?uddg=%2Frelative%2Ftarget"
            ),
            None
        );
        assert_eq!(
            liquid_research_core::normalize_absolute_public_evidence_url(
                "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
            )
            .as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
    }

    #[test]
    fn scaffold_support_url_is_public_full_url_rejects_duckduckgo_relative_uddg() {
        assert!(!scaffold_support_url_is_public_full_url(
            "https://duckduckgo.com/l/?uddg=%2Frelative%2Ftarget"
        ));
        assert!(scaffold_support_url_is_public_full_url(
            "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
        ));
    }

    #[test]
    fn scaffold_support_url_is_public_full_url_accepts_uppercase_http_schemes() {
        assert!(scaffold_support_url_is_public_full_url(
            "HTTPS://docs.vllm.ai/en/latest/"
        ));
        assert!(scaffold_support_url_is_public_full_url(
            "https://duckduckgo.com/l/?uddg=HTTPS%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
        ));
    }

    #[test]
    fn local_pi_scaffold_cards_reject_metadata_candidates() {
        let report = ResearchSourcePackReport {
            subject: Some("Example subject".to_string()),
            status: "success".to_string(),
            reason: None,
            queries: Vec::new(),
            seeded_source_count: 0,
            discovered_source_count: 2,
            adopted_source_count: 2,
            adopted_candidates: vec![
                ResearchSourceCandidateReport {
                    url: "https://metadata.google.internal/hidden".to_string(),
                    title: "Hidden metadata".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
                ResearchSourceCandidateReport {
                    url: "https://docs.vllm.ai/en/latest/".to_string(),
                    title: "vLLM docs".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
            ],
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: None,
        };

        let cards = build_local_pi_source_pack_provenance_source_cards(&report);

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].url, "https://docs.vllm.ai/en/latest/");
        assert_eq!(cards[0].id, "SP1");
    }

    #[test]
    fn local_pi_scaffold_cards_reject_duckduckgo_relative_uddg_candidates() {
        let report = ResearchSourcePackReport {
            subject: Some("Example subject".to_string()),
            status: "success".to_string(),
            reason: None,
            queries: Vec::new(),
            seeded_source_count: 0,
            discovered_source_count: 2,
            adopted_source_count: 2,
            adopted_candidates: vec![
                ResearchSourceCandidateReport {
                    url: "https://duckduckgo.com/l/?uddg=%2Frelative%2Ftarget".to_string(),
                    title: "Relative redirect".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
                ResearchSourceCandidateReport {
                    url:
                        "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
                            .to_string(),
                    title: "Absolute redirect".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
            ],
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: None,
        };

        let cards = build_local_pi_source_pack_provenance_source_cards(&report);

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].url, "https://docs.vllm.ai/en/latest/");
    }

    #[test]
    fn local_pi_scaffold_cards_accept_uppercase_scheme_candidates() {
        let report = ResearchSourcePackReport {
            subject: Some("Example subject".to_string()),
            status: "success".to_string(),
            reason: None,
            queries: Vec::new(),
            seeded_source_count: 0,
            discovered_source_count: 2,
            adopted_source_count: 2,
            adopted_candidates: vec![
                ResearchSourceCandidateReport {
                    url: "HTTPS://docs.vllm.ai/en/latest/".to_string(),
                    title: "Uppercase direct".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
                ResearchSourceCandidateReport {
                    url:
                        "https://duckduckgo.com/l/?uddg=HTTPS%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
                            .to_string(),
                    title: "Uppercase redirect".to_string(),
                    source_class: Some("official_or_primary".to_string()),
                    source_quality: Some("high".to_string()),
                    query: Some("example".to_string()),
                    rejection_reason: None,
                },
            ],
            skipped_candidates: Vec::new(),
            coverage_misses: Vec::new(),
            source_pack: None,
        };

        let cards = build_local_pi_source_pack_provenance_source_cards(&report);

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].url, "https://docs.vllm.ai/en/latest/");
    }

    #[test]
    fn source_quality_scoring_keeps_scholarly_and_reference_hosts_high() {
        for url in [
            "https://www.cambridge.org/core/books/example",
            "https://academic.oup.com/example/article/1/2/3",
            "https://www.jstor.org/stable/123456",
            "https://www.degruyter.com/document/doi/10.1234/example/html",
            "https://en.cppreference.com/w/cpp/container/vector",
        ] {
            assert_eq!(
                source_quality_for_subject(url, Some("Roman emperor Aurelian historical overview")),
                "high",
                "expected high confidence for {url}"
            );
        }
    }

    #[test]
    fn source_quality_scoring_downgrades_contested_historical_supplementary_urls() {
        assert_eq!(
            source_quality_for_subject(
                "https://sourcebooks.fordham.edu/ancient/sha-aurelian.asp",
                Some("Roman emperor Aurelian historical overview"),
            ),
            "low"
        );
        assert_eq!(
            source_quality_for_subject(
                "https://www.jstor.org/stable/123456",
                Some("Roman emperor Aurelian historical overview"),
            ),
            "high"
        );
    }
}
