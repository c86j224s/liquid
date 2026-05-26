use crate::models::{
    GitHubComment, GitHubIssue, GitHubUser, ScrapeDiagnostics, ScrapeRawCaptureDiagnostics,
    ScrapeTaskInput,
};
use chrono::Utc;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub(crate) fn strip_html(html: &str) -> String {
    html2md::parse_html(html)
}

pub(crate) fn sanitize_input(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .trim()
        .to_string()
}

// --- Smart Scraping Logic ---

trait ScrapingStrategy {
    fn extract(&self, html: &str, url: &url::Url) -> Option<(String, String)>;
}

const MIN_EXTRACTED_MARKDOWN_CHARS: usize = 120;
const SCRAPE_TIMEOUT_SECS: u64 = 30;
const MAX_SCRAPE_BODY_BYTES: usize = 5 * 1024 * 1024;

pub(crate) struct ReadabilityStrategy;
impl ScrapingStrategy for ReadabilityStrategy {
    fn extract(&self, html: &str, url: &url::Url) -> Option<(String, String)> {
        let mut cursor = std::io::Cursor::new(html);
        if let Ok(scraped) = readability::extractor::extract(&mut cursor, url) {
            Some((scraped.title, scraped.content))
        } else {
            None
        }
    }
}

pub(crate) struct SubstackStrategy;
impl ScrapingStrategy for SubstackStrategy {
    fn extract(&self, html: &str, _url: &url::Url) -> Option<(String, String)> {
        // Substack 특화: <article> 태그 내의 내용을 우선적으로 찾음
        let fragment = scraper::Html::parse_fragment(html);
        let article_sel = scraper::Selector::parse("article.newsletter-post").ok()?;
        let title_sel = scraper::Selector::parse("h1.post-title").ok()?;

        let article = fragment.select(&article_sel).next()?;
        let title = fragment
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_else(|| "Untitled Substack".to_string());

        // 본문 내에서 광고/구독 버튼 등 노이즈 제거 (간이 예시)
        let content_html = article.html();
        Some((title, content_html))
    }
}

pub(crate) struct DiscourseStrategy;
impl DiscourseStrategy {
    pub(crate) fn topic_json_url(url: &url::Url) -> Option<String> {
        let segments = url.path_segments()?.collect::<Vec<_>>();
        if segments.first()? != &"t" {
            return None;
        }
        let topic_id = segments
            .iter()
            .rev()
            .find(|segment| segment.chars().all(|ch| ch.is_ascii_digit()))?;
        let mut json_url = url.clone();
        json_url.set_path(&format!("/t/{}.json", topic_id));
        json_url.set_query(None);
        json_url.set_fragment(None);
        Some(json_url.to_string())
    }

    pub(crate) async fn fetch(url: &url::Url, max_bytes: usize) -> Option<(String, String)> {
        let topic_api = Self::topic_json_url(url)?;
        let topic_url = url::Url::parse(&topic_api).ok()?;
        let res = guarded_get(&topic_url).await.ok()?;
        let body = read_limited_response_text(res, max_bytes).await.ok()?;
        let data = serde_json::from_str::<serde_json::Value>(&body).ok()?;
        Self::format_markdown(&data)
    }

    pub(crate) fn format_markdown(data: &serde_json::Value) -> Option<(String, String)> {
        let title = sanitize_input(data["title"].as_str()?);
        if title.is_empty() {
            return None;
        }
        let posts = data["post_stream"]["posts"].as_array()?;
        let mut sections = Vec::new();
        for post in posts {
            let cooked = post["cooked"].as_str().unwrap_or_default();
            let markdown = html2md::parse_html(cooked).trim().to_string();
            if markdown_content_is_sufficient(&title, &markdown) {
                let username =
                    sanitize_markdown_metadata(post["username"].as_str().unwrap_or("unknown"));
                let post_number = post["post_number"].as_i64().unwrap_or(0);
                sections.push(format!(
                    "## Post {} by {}\n\n{}",
                    post_number, username, markdown
                ));
            }
        }
        if sections.is_empty() {
            return None;
        }
        Some((title, sections.join("\n\n---\n\n")))
    }
}

pub(crate) fn sanitize_markdown_metadata(value: &str) -> String {
    escape_markdown_text(&sanitize_input(value))
        .trim()
        .to_string()
}

pub(crate) fn parse_github_issue_url(url: &url::Url) -> Option<(String, String, u64)> {
    if url.domain()? != "github.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    let owner = segments.next()?.to_string();
    let repo = segments.next()?.to_string();
    if segments.next()? != "issues" {
        return None;
    }
    let number = segments.next()?.parse::<u64>().ok()?;
    Some((owner, repo, number))
}

pub(crate) async fn fetch_github_issue_content(url: &url::Url) -> Option<(String, String)> {
    let (owner, repo, number) = parse_github_issue_url(url)?;
    let issue_api = format!(
        "https://api.github.com/repos/{}/{}/issues/{}",
        owner, repo, number
    );
    let issue_url = url::Url::parse(&issue_api).ok()?;

    let issue = guarded_get(&issue_url)
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<GitHubIssue>()
        .await
        .ok()?;

    let comments = fetch_github_issue_comments(&issue_url).await?;

    let title = format!("{}/{}#{} {}", owner, repo, number, issue.title);
    let mut markdown = String::new();
    markdown.push_str(&format!("## GitHub Issue: {}\n\n", issue.title));
    markdown.push_str(&format!("- **Repository**: `{}/{}`\n", owner, repo));
    markdown.push_str(&format!("- **Issue**: #{}\n", number));
    markdown.push_str(&format!("- **State**: {}\n", issue.state));
    markdown.push_str(&format!(
        "- **Author**: {}\n",
        github_login(issue.user.as_ref())
    ));
    markdown.push_str(&format!("- **Created**: {}\n", issue.created_at));
    markdown.push_str(&format!("- **Updated**: {}\n", issue.updated_at));
    if !issue.labels.is_empty() {
        let labels = issue
            .labels
            .iter()
            .map(|label| format!("`{}`", label.name))
            .collect::<Vec<_>>()
            .join(", ");
        markdown.push_str(&format!("- **Labels**: {}\n", labels));
    }
    markdown.push_str("\n### Issue Body\n\n");
    markdown.push_str(issue.body.as_deref().unwrap_or("_No body provided._"));
    markdown.push_str("\n\n");

    if comments.is_empty() {
        markdown.push_str("### Comments\n\n_No comments._\n");
    } else {
        markdown.push_str(&format!("### Comments ({} total)\n\n", comments.len()));
        for (idx, comment) in comments.iter().enumerate() {
            markdown.push_str(&format!(
                "#### Comment {} by {} at {}\n\n",
                idx + 1,
                github_login(comment.user.as_ref()),
                comment.created_at
            ));
            if comment.updated_at != comment.created_at {
                markdown.push_str(&format!("_Updated: {}_\n\n", comment.updated_at));
            }
            markdown.push_str(comment.body.as_deref().unwrap_or("_No comment body._"));
            markdown.push_str("\n\n---\n\n");
        }
    }

    Some((title, markdown))
}

pub(crate) async fn fetch_github_issue_comments(
    issue_api: &url::Url,
) -> Option<Vec<GitHubComment>> {
    const PER_PAGE: usize = 100;
    const MAX_PAGES: usize = 100;
    let mut comments = Vec::new();

    for page in 1..=MAX_PAGES {
        let mut comments_url = issue_api.clone();
        let mut path = comments_url.path().trim_end_matches('/').to_string();
        path.push_str("/comments");
        comments_url.set_path(&path);
        comments_url.set_query(Some(&format!("per_page={}&page={}", PER_PAGE, page)));
        let mut page_comments = guarded_get(&comments_url)
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json::<Vec<GitHubComment>>()
            .await
            .ok()?;
        let page_len = page_comments.len();
        comments.append(&mut page_comments);
        if page_len < PER_PAGE {
            return Some(comments);
        }
    }

    Some(comments)
}

pub(crate) fn github_login(user: Option<&GitHubUser>) -> &str {
    user.map(|u| u.login.as_str()).unwrap_or("unknown")
}

pub(crate) fn get_smart_content(html: &str, url_str: &str) -> (String, String) {
    let (title, content, _) = get_smart_content_with_strategy(html, url_str);
    (title, content)
}

pub(crate) fn get_smart_content_with_strategy(
    html: &str,
    url_str: &str,
) -> (String, String, String) {
    let url = url::Url::parse(url_str).unwrap();
    let normalized_html = preserve_code_line_spans(html);
    let html = normalized_html.as_str();
    let strategy_name = if url.domain().map_or(false, |d| d.contains("substack.com")) {
        "substack"
    } else {
        "readability"
    };

    // 1. 도메인별 특화 전략 선택
    let strategy: Box<dyn ScrapingStrategy> =
        if url.domain().map_or(false, |d| d.contains("substack.com")) {
            Box::new(SubstackStrategy)
        } else {
            Box::new(ReadabilityStrategy)
        };

    if let Some((title, content)) = strategy.extract(html, &url) {
        let title = resolve_scraped_title(html, &title);
        let use_fallback = !url.domain().map_or(false, |d| d.contains("substack.com"))
            && (!extracted_content_is_sufficient(&title, &content)
                || extracted_content_looks_noisy(&content));
        if use_fallback {
            if let Some((fallback_title, fallback_content)) = fallback_page_content(html, &title) {
                if extracted_content_is_sufficient(&fallback_title, &fallback_content)
                    && !extracted_content_looks_noisy(&fallback_content)
                {
                    return (
                        fallback_title,
                        fallback_content,
                        "fallback_page".to_string(),
                    );
                }
            }
        }
        if extracted_content_is_sufficient(&title, &content) {
            return (title, content, strategy_name.to_string());
        }
        return (title, content, strategy_name.to_string());
    }

    if let Some((fallback_title, fallback_content)) = fallback_page_content(html, "") {
        if extracted_content_is_sufficient(&fallback_title, &fallback_content) {
            return (
                fallback_title,
                fallback_content,
                "fallback_page".to_string(),
            );
        }
    }

    (
        "Error".to_string(),
        "Failed to extract content".to_string(),
        "failed".to_string(),
    )
}

pub(crate) fn extract_smart_content(
    html: &str,
    url_str: &str,
) -> Result<(String, String, String), String> {
    let (title, content, strategy) = get_smart_content_with_strategy(html, url_str);
    if title == "Error" || !extracted_content_is_sufficient(&title, &content) {
        return Err("Extracted content too short".to_string());
    }
    Ok((title, content, strategy))
}

pub(crate) fn extracted_content_is_sufficient(title: &str, content_html: &str) -> bool {
    let markdown = html2md::parse_html(content_html);
    markdown_content_is_sufficient(title, &markdown)
}

pub(crate) fn extracted_content_looks_noisy(content_html: &str) -> bool {
    let markdown = html2md::parse_html(content_html);
    let haystack = format!("{}\n{}", content_html, markdown).to_lowercase();
    [
        "#d-splash",
        "googletagmanager",
        "data:image/svg+xml",
        "&lt;div",
        "&lt;script",
        "&lt;iframe",
        "splash-dot",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

pub(crate) fn markdown_content_is_sufficient(title: &str, markdown: &str) -> bool {
    let text = markdown_plain_text(markdown);
    if text.chars().count() < MIN_EXTRACTED_MARKDOWN_CHARS {
        return false;
    }
    let normalized_title = normalize_text_for_quality(title);
    if normalized_title.is_empty() {
        return true;
    }
    normalize_text_for_quality(&text) != normalized_title
}

pub(crate) fn markdown_plain_text(markdown: &str) -> String {
    markdown
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("![")
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn normalize_text_for_quality(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalize_scraped_title(title: &str) -> Option<String> {
    let mut normalized = sanitize_input(title);
    if normalized.is_empty() {
        return None;
    }

    loop {
        let trimmed = normalized.trim_end();
        let stripped = trimmed
            .strip_suffix('|')
            .or_else(|| trimmed.strip_suffix('-'))
            .or_else(|| trimmed.strip_suffix('—'))
            .or_else(|| trimmed.strip_suffix('–'))
            .or_else(|| trimmed.strip_suffix('·'))
            .or_else(|| trimmed.strip_suffix('•'))
            .or_else(|| trimmed.strip_suffix('»'));
        let Some(next) = stripped else {
            break;
        };
        normalized = next.trim_end().to_string();
        if normalized.is_empty() {
            return None;
        }
    }

    Some(normalized)
}

pub(crate) fn resolve_scraped_title(html: &str, extracted_title: &str) -> String {
    if let Some(title) = normalize_scraped_title(extracted_title) {
        return title;
    }

    let document = scraper::Html::parse_document(html);
    extract_document_title(&document).unwrap_or_default()
}

pub(crate) fn fallback_page_content(html: &str, preferred_title: &str) -> Option<(String, String)> {
    let document = scraper::Html::parse_document(html);
    let title = if preferred_title.trim().is_empty() {
        extract_document_title(&document).unwrap_or_else(|| "Untitled".to_string())
    } else {
        preferred_title.trim().to_string()
    };

    let selectors = [
        "article",
        "main",
        "[role=\"main\"]",
        ".topic-body",
        ".cooked",
        ".post",
        ".post-stream",
        "#main-outlet",
        "body",
    ];
    let mut largest_candidate = None;
    for selector in selectors {
        let Ok(parsed) = scraper::Selector::parse(selector) else {
            continue;
        };
        let best_for_selector = document
            .select(&parsed)
            .map(|element| element.html())
            .max_by_key(|candidate| html2md::parse_html(candidate).chars().count());
        if let Some(content) = best_for_selector {
            if largest_candidate.as_ref().is_none_or(|candidate: &String| {
                html2md::parse_html(&content).chars().count()
                    > html2md::parse_html(candidate).chars().count()
            }) {
                largest_candidate = Some(content.clone());
            }
            if extracted_content_is_sufficient(&title, &content)
                && !extracted_content_looks_noisy(&content)
            {
                return Some((title, content));
            }
        }
    }
    largest_candidate
        .filter(|content| !extracted_content_looks_noisy(content))
        .map(|content| (title, content))
}

pub(crate) fn extract_document_title(document: &scraper::Html) -> Option<String> {
    let selectors = ["meta[property=\"og:title\"]", "h1", "title"];
    for selector in selectors {
        let parsed = scraper::Selector::parse(selector).ok()?;
        if let Some(element) = document.select(&parsed).next() {
            if selector.starts_with("meta") {
                if let Some(content) = element.value().attr("content") {
                    if let Some(title) = normalize_scraped_title(content) {
                        return Some(title);
                    }
                }
            }
            let title = element.text().collect::<String>();
            if let Some(title) = normalize_scraped_title(&title) {
                return Some(title);
            }
        }
    }
    None
}

pub(crate) async fn scrape_url_to_markdown(
    url_str: &str,
    references: &[String],
) -> Result<ScrapeResult, ScrapeDiagnostics> {
    let normalized_url = normalize_public_url(url_str).ok_or_else(|| {
        scrape_failure_diagnostics(
            url_str,
            "",
            references,
            "Invalid URL",
            None,
            None,
            None,
            None,
        )
    })?;
    let url = url::Url::parse(&normalized_url).map_err(|_| {
        scrape_failure_diagnostics(
            url_str,
            &normalized_url,
            references,
            "Invalid URL",
            None,
            None,
            None,
            None,
        )
    })?;
    if let Some((title, content)) = fetch_github_issue_content(&url).await {
        let header = format_scrape_header(&title, &normalized_url, references);
        return Ok(ScrapeResult {
            title: title.clone(),
            markdown: format!("{}{}", header, content),
            diagnostics: scrape_success_diagnostics(
                url_str,
                &normalized_url,
                Some(&normalized_url),
                references,
                "github_issue",
                Some(title),
                None,
                content.chars().count(),
                content.chars().count(),
            ),
        });
    }
    if let Some((title, content)) = DiscourseStrategy::fetch(&url, MAX_SCRAPE_BODY_BYTES).await {
        let header = format_scrape_header(&title, &normalized_url, references);
        return Ok(ScrapeResult {
            title: title.clone(),
            markdown: format!("{}{}", header, content),
            diagnostics: scrape_success_diagnostics(
                url_str,
                &normalized_url,
                Some(&normalized_url),
                references,
                "discourse",
                Some(title),
                None,
                content.chars().count(),
                content.chars().count(),
            ),
        });
    }

    let res = guarded_get(&url).await.map_err(|error| {
        scrape_failure_diagnostics(
            url_str,
            &normalized_url,
            references,
            &error,
            None,
            None,
            None,
            None,
        )
    })?;
    let final_url = res.url().to_string();
    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    if !res.status().is_success() {
        return Err(scrape_failure_diagnostics(
            url_str,
            &normalized_url,
            references,
            "Failed to fetch URL",
            Some(&final_url),
            content_type.as_deref(),
            None,
            Some(res.status().as_u16()),
        ));
    }
    let body = read_limited_response_text(res, MAX_SCRAPE_BODY_BYTES)
        .await
        .map_err(|error| {
            scrape_failure_diagnostics(
                url_str,
                &normalized_url,
                references,
                &error,
                Some(&final_url),
                content_type.as_deref(),
                None,
                None,
            )
        })?;
    let raw_body_bytes = body.len();
    let raw_body_chars = body.chars().count();
    let (title, content_html, extraction_strategy) = extract_smart_content(&body, &normalized_url)
        .map_err(|error| {
            scrape_failure_diagnostics(
                url_str,
                &normalized_url,
                references,
                &error,
                Some(&final_url),
                content_type.as_deref(),
                Some((raw_body_bytes, raw_body_chars)),
                None,
            )
        })?;
    let content = html2md::parse_html(&content_html);
    if !markdown_content_is_sufficient(&title, &content) {
        return Err(scrape_failure_diagnostics(
            url_str,
            &normalized_url,
            references,
            "Extracted content too short",
            Some(&final_url),
            content_type.as_deref(),
            Some((raw_body_bytes, raw_body_chars)),
            None,
        ));
    }
    let header = format_scrape_header(&title, &normalized_url, references);
    Ok(ScrapeResult {
        title: title.clone(),
        markdown: format!("{}{}", header, content),
        diagnostics: ScrapeDiagnostics {
            original_url: sanitize_input(url_str),
            normalized_url,
            final_url: Some(final_url),
            status_class: "success".to_string(),
            failure_reason: None,
            http_status_code: None,
            extraction_strategy: Some(extraction_strategy),
            title: Some(title),
            content_type,
            raw_body_bytes: Some(raw_body_bytes),
            raw_body_chars: Some(raw_body_chars),
            extracted_html_chars: content_html.chars().count(),
            markdown_chars: content.chars().count(),
            sufficiency_result: "sufficient".to_string(),
            insufficiency_reason: None,
            reference_links: references.to_vec(),
            accessed_at: Utc::now().to_rfc3339(),
            raw_capture: ScrapeRawCaptureDiagnostics {
                mode: "omitted".to_string(),
                path: None,
                hash: None,
                omitted_reason: Some("raw snapshots disabled by default".to_string()),
            },
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScrapeResult {
    pub(crate) title: String,
    pub(crate) markdown: String,
    pub(crate) diagnostics: ScrapeDiagnostics,
}

fn scrape_success_diagnostics(
    original_url: &str,
    normalized_url: &str,
    final_url: Option<&str>,
    references: &[String],
    extraction_strategy: &str,
    title: Option<String>,
    content_type: Option<String>,
    extracted_html_chars: usize,
    markdown_chars: usize,
) -> ScrapeDiagnostics {
    ScrapeDiagnostics {
        original_url: sanitize_input(original_url),
        normalized_url: normalized_url.to_string(),
        final_url: final_url.map(ToOwned::to_owned),
        status_class: "success".to_string(),
        failure_reason: None,
        http_status_code: None,
        extraction_strategy: Some(extraction_strategy.to_string()),
        title,
        content_type,
        raw_body_bytes: None,
        raw_body_chars: None,
        extracted_html_chars,
        markdown_chars,
        sufficiency_result: "sufficient".to_string(),
        insufficiency_reason: None,
        reference_links: references.to_vec(),
        accessed_at: Utc::now().to_rfc3339(),
        raw_capture: ScrapeRawCaptureDiagnostics {
            mode: "omitted".to_string(),
            path: None,
            hash: None,
            omitted_reason: Some("raw snapshots disabled by default".to_string()),
        },
    }
}

fn scrape_failure_diagnostics(
    original_url: &str,
    normalized_url: &str,
    references: &[String],
    error: &str,
    final_url: Option<&str>,
    content_type: Option<&str>,
    raw_body_sizes: Option<(usize, usize)>,
    http_status_code: Option<u16>,
) -> ScrapeDiagnostics {
    let lower_error = error.to_ascii_lowercase();
    let status_class = if lower_error.contains("blocked private or local") {
        "blocked"
    } else if lower_error.contains("invalid url") {
        "invalid"
    } else if lower_error.contains("extracted content too short") {
        "insufficient_extraction"
    } else if lower_error.contains("redirect") || lower_error.contains("too many redirects") {
        "redirect"
    } else if lower_error.contains("failed to resolve")
        || lower_error.contains("failed to fetch url")
    {
        "fetch_failed"
    } else {
        "error"
    };
    let insufficiency_reason = if status_class == "insufficient_extraction" {
        Some("markdown_below_minimum_threshold".to_string())
    } else {
        None
    };
    ScrapeDiagnostics {
        original_url: sanitize_input(original_url),
        normalized_url: normalized_url.to_string(),
        final_url: final_url.map(ToOwned::to_owned),
        status_class: status_class.to_string(),
        failure_reason: Some(error.to_string()),
        http_status_code,
        extraction_strategy: None,
        title: None,
        content_type: content_type.map(ToOwned::to_owned),
        raw_body_bytes: raw_body_sizes.map(|sizes| sizes.0),
        raw_body_chars: raw_body_sizes.map(|sizes| sizes.1),
        extracted_html_chars: 0,
        markdown_chars: 0,
        sufficiency_result: "insufficient".to_string(),
        insufficiency_reason,
        reference_links: references.to_vec(),
        accessed_at: Utc::now().to_rfc3339(),
        raw_capture: ScrapeRawCaptureDiagnostics {
            mode: "omitted".to_string(),
            path: None,
            hash: None,
            omitted_reason: Some(if status_class == "blocked" {
                "private_or_local_response_not_stored".to_string()
            } else {
                "raw snapshots disabled by default".to_string()
            }),
        },
    }
}

pub(crate) fn format_scrape_failure_for_task(diagnostics: &ScrapeDiagnostics) -> String {
    let detail = diagnostics
        .failure_reason
        .as_deref()
        .unwrap_or("Scrape failed");
    match diagnostics.status_class.as_str() {
        "fetch_failed" => format!("Scrape fetch failed: {detail}"),
        "redirect" => format!("Scrape redirect failed: {detail}"),
        "insufficient_extraction" => {
            if let Some(reason) = diagnostics.insufficiency_reason.as_deref() {
                format!("Scrape extraction insufficient: {reason} ({detail})")
            } else {
                format!("Scrape extraction insufficient: {detail}")
            }
        }
        "blocked" => format!("Scrape blocked: {detail}"),
        "invalid" => format!("Scrape invalid: {detail}"),
        _ => format!("Scrape error: {detail}"),
    }
}

pub(crate) fn is_geeknews_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !url
        .host_str()
        .map(|host| host.eq_ignore_ascii_case("news.hada.io"))
        .unwrap_or(false)
    {
        return false;
    }
    url.path() == "/topic"
        && url
            .query_pairs()
            .any(|(key, value)| key == "id" && !value.is_empty())
}

pub(crate) fn extract_geeknews_original_url(html: &str, geeknews_url: &str) -> Option<String> {
    let base_url = url::Url::parse(geeknews_url).ok()?;
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse(".topictitle a[href]").ok()?;
    for element in document.select(&selector) {
        let href = element.value().attr("href")?;
        let candidate = base_url.join(href).ok()?;
        if candidate
            .host_str()
            .map(|host| host.eq_ignore_ascii_case("news.hada.io"))
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(normalized) = normalize_public_url(candidate.as_str()) {
            return Some(normalized);
        }
    }
    None
}

pub(crate) fn append_reference_once(references: &mut Vec<String>, reference: String) {
    if !references.iter().any(|item| item == &reference) {
        references.push(reference);
    }
}

pub(crate) fn parse_scrape_task_input(
    user_prompt: Option<&str>,
    fallback_url: &str,
) -> ScrapeTaskInput {
    if let Some(prompt) = user_prompt {
        if let Ok(input) = serde_json::from_str::<ScrapeTaskInput>(prompt) {
            return ScrapeTaskInput {
                url: input.url,
                references: normalize_reference_links(input.references),
                mode: input.mode,
            };
        }
        return ScrapeTaskInput {
            url: prompt.to_string(),
            references: Vec::new(),
            mode: None,
        };
    }

    ScrapeTaskInput {
        url: fallback_url.to_string(),
        references: Vec::new(),
        mode: None,
    }
}

pub(crate) fn normalize_reference_links(references: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for reference in references {
        let reference = sanitize_input(&reference);
        if let Some(reference) = normalize_reference_url(&reference) {
            append_reference_once(&mut normalized, reference);
        }
    }
    normalized
}

pub(crate) fn normalize_public_url(raw_url: &str) -> Option<String> {
    let trimmed = sanitize_input(raw_url);
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(&trimmed).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    Some(url.to_string())
}

pub(crate) async fn guarded_get(url: &url::Url) -> Result<reqwest::Response, String> {
    const MAX_SCRAPE_REDIRECTS: usize = 10;
    let mut current_url = url.clone();
    for _ in 0..=MAX_SCRAPE_REDIRECTS {
        let addrs = ensure_public_fetch_url(&current_url).await?;
        let client = pinned_scrape_client(&current_url, &addrs)?;
        let response = client
            .get(current_url.clone())
            .header(reqwest::header::USER_AGENT, "liquid")
            .send()
            .await
            .map_err(|_| "Failed to fetch URL".to_string())?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
            return Err("Redirect missing Location".to_string());
        };
        let location = location
            .to_str()
            .map_err(|_| "Invalid redirect URL".to_string())?;
        current_url = current_url
            .join(location)
            .map_err(|_| "Invalid redirect URL".to_string())?;
    }
    Err("Too many redirects".to_string())
}

pub(crate) async fn ensure_public_fetch_url(url: &url::Url) -> Result<Vec<SocketAddr>, String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Invalid URL".to_string());
    }
    let host = url.host_str().ok_or_else(|| "Invalid URL".to_string())?;
    ensure_public_host_literal(host)?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "Failed to resolve scrape target".to_string())?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err("Failed to resolve scrape target".to_string());
    }
    for addr in &addrs {
        if is_blocked_ip(addr.ip()) {
            return Err("Blocked private or local scrape target".to_string());
        }
    }
    Ok(addrs)
}

pub(crate) fn pinned_scrape_client(
    url: &url::Url,
    addrs: &[SocketAddr],
) -> Result<reqwest::Client, String> {
    let host = url.host_str().ok_or_else(|| "Invalid URL".to_string())?;
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(SCRAPE_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|_| "Failed to create scrape client".to_string())
}

pub(crate) fn ensure_public_host_literal(host: &str) -> Result<(), String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return Err("Blocked private or local scrape target".to_string());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err("Blocked private or local scrape target".to_string());
        }
    }
    if let Some(ip) = parse_ipv4_style_host(&host) {
        if is_blocked_ip(IpAddr::V4(ip)) {
            return Err("Blocked private or local scrape target".to_string());
        }
    }
    Ok(())
}

pub(crate) fn parse_ipv4_style_host(host: &str) -> Option<Ipv4Addr> {
    if host.is_empty()
        || !host.as_bytes()[0].is_ascii_digit()
        || !host
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == 'x' || ch == 'X' || ch == '.')
    {
        return None;
    }
    if !host.contains('.') {
        let value = parse_ipv4_number(host)?;
        return Some(Ipv4Addr::from(value));
    }
    let parts = host
        .split('.')
        .map(parse_ipv4_number)
        .collect::<Option<Vec<_>>>()?;
    if parts.len() != 4 || parts.iter().any(|part| *part > 255) {
        return None;
    }
    Some(Ipv4Addr::new(
        parts[0] as u8,
        parts[1] as u8,
        parts[2] as u8,
        parts[3] as u8,
    ))
}

pub(crate) fn parse_ipv4_number(value: &str) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    let (digits, radix) = if let Some(rest) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (rest, 16)
    } else if value.len() > 1 && value.starts_with('0') {
        (&value[1..], 8)
    } else {
        (value, 10)
    };
    u32::from_str_radix(digits, radix).ok()
}

pub(crate) fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_blocked_ipv4(ip),
        IpAddr::V6(ip) => is_blocked_ipv6(ip),
    }
}

pub(crate) fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip == Ipv4Addr::new(255, 255, 255, 255)
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
}

pub(crate) fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(mapped);
    }
    let segments = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
}

pub(crate) fn normalize_reference_url(reference: &str) -> Option<String> {
    normalize_public_url(reference)
}

pub(crate) fn format_scrape_header(title: &str, url_str: &str, references: &[String]) -> String {
    let safe_title = sanitize_markdown_metadata(title);
    let safe_source = escape_html_text(url_str);
    let mut header = format!(
        "# {}\n\n> **출처**: [{}]({})\n\n",
        safe_title, safe_source, safe_source
    );
    if !references.is_empty() {
        header.push_str("> **참고자료**:\n");
        for reference in references {
            header.push_str(&format!("> - <{}>\n", escape_html_text(reference)));
        }
        header.push('\n');
    }
    header.push_str("---\n\n");
    header
}

pub(crate) async fn read_limited_response_text(
    mut res: reqwest::Response,
    max_bytes: usize,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|_| "Failed to read body".to_string())?
    {
        if bytes.len() + chunk.len() > max_bytes {
            return Err("Response body too large".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "Response body is not valid UTF-8".to_string())
}

pub(crate) fn preserve_code_line_spans(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut cursor = 0;

    while let Some(pre_rel) = find_ascii_case_insensitive(&html[cursor..], "<pre") {
        let pre_start = cursor + pre_rel;
        let Some(pre_end_rel) = find_ascii_case_insensitive(&html[pre_start..], "</pre>") else {
            break;
        };
        let pre_end = pre_start + pre_end_rel + "</pre>".len();
        output.push_str(&html[cursor..pre_start]);
        output.push_str(&normalize_line_span_pre(&html[pre_start..pre_end]));
        cursor = pre_end;
    }

    output.push_str(&html[cursor..]);
    output
}

pub(crate) fn normalize_line_span_pre(pre_html: &str) -> String {
    let fragment = scraper::Html::parse_fragment(pre_html);
    let lines = select_pre_code_lines(&fragment);
    if lines.is_empty() {
        return pre_html.to_string();
    }

    format!(
        "<pre><code>{}</code></pre>",
        escape_html_text(&lines.join("\n"))
    )
}

pub(crate) fn select_pre_code_lines(fragment: &scraper::Html) -> Vec<String> {
    if let Ok(line_selector) = scraper::Selector::parse("span.line") {
        let lines = fragment
            .select(&line_selector)
            .map(|line| line.text().collect::<String>())
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            return lines;
        }
    }

    let Ok(pandoc_selector) = scraper::Selector::parse("pre code > span[id]") else {
        return Vec::new();
    };
    let lines = fragment
        .select(&pandoc_selector)
        .filter(|line| {
            line.value()
                .attr("id")
                .map(is_pandoc_line_span_id)
                .unwrap_or(false)
        })
        .map(|line| line.text().collect::<String>())
        .collect::<Vec<_>>();
    if lines.len() >= 2 {
        lines
    } else {
        Vec::new()
    }
}

pub(crate) fn is_pandoc_line_span_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("cb") else {
        return false;
    };
    let Some((block, line)) = rest.split_once('-') else {
        return false;
    };
    !block.is_empty()
        && !line.is_empty()
        && block.chars().all(|c| c.is_ascii_digit())
        && line.chars().all(|c| c.is_ascii_digit())
}

pub(crate) fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

pub(crate) fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn escape_markdown_text(text: &str) -> String {
    escape_html_text(text)
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('#', "\\#")
        .replace('*', "\\*")
        .replace('_', "\\_")
}

pub(crate) fn normalize_multiline_linked_images(markdown: &str) -> String {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    let mut i = 0;
    let mut fence_marker: Option<String> = None;

    while i < lines.len() {
        if let Some(marker) = fence_marker.as_deref() {
            output.push(lines[i].to_string());
            if closes_markdown_fence(lines[i], marker) {
                fence_marker = None;
            }
            i += 1;
            continue;
        }

        if let Some(marker) = opens_markdown_fence(lines[i]) {
            fence_marker = Some(marker);
            output.push(lines[i].to_string());
            i += 1;
            continue;
        }

        if let Some((normalized_image, next_index)) = parse_multiline_linked_image(&lines, i) {
            output.push(normalized_image);
            i = next_index;
            continue;
        }

        output.push(lines[i].to_string());
        i += 1;
    }

    let mut normalized = output.join("\n");
    if markdown.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

pub(crate) fn parse_multiline_linked_image(
    lines: &[&str],
    start: usize,
) -> Option<(String, usize)> {
    let (indent, opening) = trim_markdown_content_indent(lines.get(start)?)?;
    if opening != "[" {
        return None;
    }

    let mut image_line_index = start + 1;
    while image_line_index < lines.len() && lines[image_line_index].trim().is_empty() {
        image_line_index += 1;
    }

    let mut link_line_index = image_line_index + 1;
    while link_line_index < lines.len() && lines[link_line_index].trim().is_empty() {
        link_line_index += 1;
    }

    let (_, image_line) = trim_markdown_content_indent(lines.get(image_line_index)?)?;
    let (_, link_line) = trim_markdown_content_indent(lines.get(link_line_index)?)?;
    let (href, trailing_text) = parse_markdown_link_target_with_trailing_text(link_line)?;
    if !image_line.starts_with("<img ") || !image_line.ends_with('>') {
        return None;
    }

    Some((
        format!(
            "{}<a href=\"{}\">{}</a>{}",
            " ".repeat(indent),
            escape_html_attribute(&href),
            image_line,
            trailing_text
        ),
        link_line_index + 1,
    ))
}

pub(crate) fn trim_markdown_content_indent(line: &str) -> Option<(usize, &str)> {
    if line.starts_with('\t') {
        return None;
    }
    let indent = line.chars().take_while(|c| *c == ' ').count();
    if indent > 3 {
        return None;
    }
    Some((indent, &line[indent..]))
}

pub(crate) fn parse_markdown_link_target_with_trailing_text(line: &str) -> Option<(String, &str)> {
    let rest = line.strip_prefix("](")?;
    let close_index = find_markdown_link_target_close(rest)?;
    let href = normalize_public_url(&rest[..close_index])?;
    Some((href, &rest[close_index + 1..]))
}

pub(crate) fn find_markdown_link_target_close(target_and_trailing: &str) -> Option<usize> {
    let mut nested_parens = 0usize;
    let mut previous_escape = false;

    for (index, ch) in target_and_trailing.char_indices() {
        if previous_escape {
            previous_escape = false;
            continue;
        }

        match ch {
            '\\' => previous_escape = true,
            '(' => nested_parens += 1,
            ')' if nested_parens == 0 => return Some(index),
            ')' => nested_parens -= 1,
            _ => {}
        }
    }

    None
}

pub(crate) fn opens_markdown_fence(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return None;
    }

    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let count = trimmed.chars().take_while(|c| *c == marker).count();
    if count < 3 {
        return None;
    }

    Some(marker.to_string().repeat(count))
}

pub(crate) fn closes_markdown_fence(line: &str, marker: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return false;
    }

    let marker_char = marker.chars().next().unwrap_or('`');
    let marker_len = trimmed.chars().take_while(|c| *c == marker_char).count();
    marker_len >= marker.len() && trimmed[marker_len..].trim().is_empty()
}

pub(crate) fn escape_html_attribute(text: &str) -> String {
    escape_html_text(text).replace('"', "&quot;")
}

use crate::models::{ScrapRequest, TaskUpdateEvent};
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

pub(crate) async fn scrap_url(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ScrapRequest>,
) -> impl IntoResponse {
    let Some(mut url_str) = normalize_public_url(&payload.url) else {
        return (StatusCode::BAD_REQUEST, "Invalid URL").into_response();
    };
    let mode = payload.mode.unwrap_or_else(|| "general".to_string());
    if !matches!(mode.as_str(), "general" | "geeknews") {
        return (StatusCode::BAD_REQUEST, "Invalid scrape mode").into_response();
    }

    let translate = payload.translate.unwrap_or(false);
    let model = payload.model.unwrap_or_default();
    if translate && model.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "Model is required for scrape+translate",
        )
            .into_response();
    }

    let mut references = normalize_reference_links(payload.references.unwrap_or_default());
    if mode == "geeknews" {
        if !is_geeknews_url(&url_str) {
            return (StatusCode::BAD_REQUEST, "GeekNews URL is required").into_response();
        }
        let geeknews_url = url_str.clone();
        let geeknews_url_parsed = match url::Url::parse(&geeknews_url) {
            Ok(url) => url,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid URL").into_response(),
        };
        let geeknews_response = match guarded_get(&geeknews_url_parsed).await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => response,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "Failed to load GeekNews URL").into_response()
                }
            },
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "Failed to load GeekNews URL").into_response()
            }
        };
        let geeknews_page =
            match read_limited_response_text(geeknews_response, MAX_SCRAPE_BODY_BYTES).await {
                Ok(body) => body,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "Failed to load GeekNews URL").into_response()
                }
            };
        let Some(original_url) = extract_geeknews_original_url(&geeknews_page, &geeknews_url)
        else {
            return (StatusCode::BAD_REQUEST, "Original URL not found").into_response();
        };
        append_reference_once(&mut references, geeknews_url);
        url_str = original_url;
    }

    let scrape_input = ScrapeTaskInput {
        url: url_str.clone(),
        references,
        mode: Some(mode),
    };
    let scrape_input_json = match serde_json::to_string(&scrape_input) {
        Ok(json) => json,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let file_prefix = if translate { "[Scrape+KO]" } else { "[Scrape]" };
    let result = match sqlx::query("INSERT INTO tasks (original_name, status, model, user_prompt, file_prefix, file_type) VALUES (?, 'queued', ?, ?, ?, 'md')")
        .bind(&url_str)
        .bind(&model)
        .bind(&scrape_input_json)
        .bind(file_prefix)
        .execute(&state.db).await {
            Ok(r) => r,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let task_id = result.last_insert_rowid();
    let _ = state.tx.send(TaskUpdateEvent {
        id: task_id,
        status: "queued".to_string(),
        original_name: url_str,
        quality_current_iteration: None,
        quality_max_iterations: None,
        quality_status: None,
        research_controller_stage: None,
        research_controller_iteration: None,
        research_controller_max_iterations: None,
    });
    state.queue_notify.notify_waiters();

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "task_id": task_id })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::setup_db;
    use crate::test_support::{temp_test_dir, test_state};
    use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
    use std::fs as std_fs;

    #[test]
    fn test_sanitize_input() {
        let input = "  Hello\nWorld\t  ";
        assert_eq!(sanitize_input(input), "Hello\nWorld");
    }

    #[test]
    fn test_strip_html() {
        let html = "<h1>Title</h1><p>Paragraph</p>";
        let md = strip_html(html);
        assert!(md.to_lowercase().contains("title"));
        assert!(md.contains("Paragraph"));
    }

    #[test]
    fn test_substack_scraping_strategy() {
        let html = r#"
            <html>
                <body>
                    <article class="newsletter-post">
                        <h1 class="post-title">Real Content Title</h1>
                        <div class="body">This is the actual article content.</div>
                    </article>
                    <div class="footer">Ignored content</div>
                </body>
            </html>
        "#;
        let url = "https://example.substack.com/p/test";
        let (title, content) = get_smart_content(html, url);

        assert_eq!(title, "Real Content Title");
        assert!(content.contains("actual article content"));
        assert!(!content.contains("Ignored content"));
    }

    #[test]
    fn test_discourse_strategy_topic_json_url() {
        let url =
            url::Url::parse("https://discuss.example.com/t/topic-slug/9760?foo=bar#reply").unwrap();

        assert_eq!(
            DiscourseStrategy::topic_json_url(&url).as_deref(),
            Some("https://discuss.example.com/t/9760.json")
        );
    }

    #[test]
    fn test_discourse_strategy_formats_posts_from_json() {
        let data = serde_json::json!({
            "title": "Gemma local model discussion",
            "post_stream": {
                "posts": [
                    {
                        "post_number": 1,
                        "username": "alice",
                        "cooked": "<p>Gemma local model discussion includes practical setup notes, benchmark context, tool calling caveats, and debugging lessons for Codex CLI workflows.</p>"
                    },
                    {
                        "post_number": 2,
                        "username": "bob",
                        "cooked": "<p>Short reply.</p>"
                    }
                ]
            }
        });

        let (title, markdown) = DiscourseStrategy::format_markdown(&data).unwrap();

        assert_eq!(title, "Gemma local model discussion");
        assert!(markdown.contains("## Post 1 by alice"));
        assert!(markdown.contains("practical setup notes"));
        assert!(!markdown.contains("Short reply"));
    }

    #[test]
    fn test_discourse_strategy_keeps_plain_title_and_escapes_only_markdown_body_metadata() {
        let data = serde_json::json!({
            "title": "<script>alert(1)</script> [bad](javascript:alert(1))",
            "post_stream": {
                "posts": [
                    {
                        "post_number": 1,
                        "username": "mallory <img src=x onerror=alert(1)> [x](y)",
                        "cooked": "<p>This post contains enough ordinary article content to pass extraction quality checks while metadata remains hostile and must be escaped.</p>"
                    }
                ]
            }
        });

        let (title, markdown) = DiscourseStrategy::format_markdown(&data).unwrap();
        let header = format_scrape_header(&title, "https://discuss.example.com/t/topic/1", &[]);

        assert_eq!(
            title,
            "<script>alert(1)</script> [bad](javascript:alert(1))"
        );
        assert!(header.contains(
            "&lt;script&gt;alert\\(1\\)&lt;/script&gt; \\[bad\\]\\(javascript:alert\\(1\\)\\)"
        ));
        assert!(!markdown.contains("<img"));
        assert!(!markdown.contains("[x](y)"));
        assert!(markdown.contains("\\[x\\]\\(y\\)"));
    }

    #[test]
    fn test_discourse_title_is_only_escaped_once_in_markdown_header() {
        let data = serde_json::json!({
            "title": "AT&T",
            "post_stream": {
                "posts": [
                    {
                        "post_number": 1,
                        "username": "alice",
                        "cooked": "<p>This post contains enough plain text content to pass discourse extraction quality checks without any extra formatting tricks.</p>"
                    }
                ]
            }
        });

        let (title, _markdown) = DiscourseStrategy::format_markdown(&data).unwrap();
        let header = format_scrape_header(&title, "https://discuss.example.com/t/topic/1", &[]);

        assert_eq!(title, "AT&T");
        assert!(header.starts_with("# AT&amp;T\n"));
        assert!(!header.contains("AT&amp;amp;T"));
    }

    #[test]
    fn test_preserve_code_line_spans() {
        let html = r#"<pre><code><span class="line"><span style="color:#6A737D">// concat.go</span></span><span class="line"><span style="color:#D73A49">package</span><span> concat</span></span><span class="line"><span style="color:#D73A49">import</span><span> "strings"</span></span></code></pre>"#;
        let normalized = preserve_code_line_spans(html);
        let md = html2md::parse_html(&normalized);

        assert!(md.contains("// concat.go\npackage concat\nimport"));
        assert!(!md.contains("// concat.gopackage concatimport"));
    }

    #[test]
    fn test_preserve_pandoc_source_code_line_spans() {
        let html = r#"<pre class="sourceCode"><code><span id="cb1-1"><span class="kw">def</span> f():</span><span id="cb1-2">    return 1</span><span id="cb1-3">print(f())</span></code></pre>"#;
        let normalized = preserve_code_line_spans(html);
        let md = html2md::parse_html(&normalized);

        assert!(md.contains("def f():\n    return 1\nprint(f())"));
        assert!(!md.contains("def f():    return 1print"));
    }

    #[test]
    fn test_single_pandoc_like_span_does_not_normalize_pre() {
        let html = r#"<pre><code><span id="cb1-1">single line only</span></code></pre>"#;
        let normalized = preserve_code_line_spans(html);

        assert_eq!(normalized, html);
    }

    #[test]
    fn test_smart_content_preserves_code_lines_before_extraction() {
        let html = r#"
            <html>
                <body>
                    <article class="newsletter-post">
                        <h1 class="post-title">Code Article</h1>
                        <pre><code><span class="line"><span style="color:#6A737D">// concat.go</span></span><span class="line"><span style="color:#D73A49">package</span><span> concat</span></span><span class="line"><span style="color:#D73A49">import</span><span> "strings"</span></span></code></pre>
                    </article>
                </body>
            </html>
        "#;
        let (_title, content_html) = get_smart_content(html, "https://example.substack.com/p/code");
        let md = html2md::parse_html(&content_html);

        assert!(md.contains("// concat.go\npackage concat\nimport"));
        assert!(!md.contains("// concat.gopackage concatimport"));
    }

    #[test]
    fn test_smart_content_falls_back_when_readability_is_too_small() {
        let repeated = "This fallback paragraph contains substantial technical discussion about model serving, command line workflows, prompts, configuration, and practical debugging details. ".repeat(3);
        let html = format!(
            r#"
            <html>
                <head><title>Tiny Readability Page</title></head>
                <body>
                    <h1>Tiny Readability Page</h1>
                    <div class="topic-body"><div class="cooked"><p>{}</p></div></div>
                </body>
            </html>
        "#,
            repeated
        );

        let (title, content_html, strategy) =
            extract_smart_content(&html, "https://discuss.example.com/t/topic/1").unwrap();
        let markdown = html2md::parse_html(&content_html);

        assert_eq!(title, "Tiny Readability Page");
        assert!(matches!(strategy.as_str(), "fallback_page" | "readability"));
        assert!(markdown.contains("substantial technical discussion"));
        assert!(markdown_content_is_sufficient(&title, &markdown));
    }

    #[test]
    fn test_resolve_scraped_title_falls_back_to_document_title_when_extracted_title_is_blank() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="So you've installed `fzf`. Now what?">
                    <title>Ignored Browser Title</title>
                </head>
                <body>
                    <h1>So you've installed `fzf`. Now what?</h1>
                    <article><p>Enough content to represent a real article body.</p></article>
                </body>
            </html>
        "#;

        assert_eq!(
            resolve_scraped_title(html, "   "),
            "So you've installed `fzf`. Now what?"
        );
    }

    #[test]
    fn test_normalize_scraped_title_removes_empty_trailing_separator_noise() {
        assert_eq!(
            normalize_scraped_title("Useful title | ").as_deref(),
            Some("Useful title")
        );
        assert_eq!(
            normalize_scraped_title("Useful title — ").as_deref(),
            Some("Useful title")
        );
        assert_eq!(
            normalize_scraped_title("Still: useful?").as_deref(),
            Some("Still: useful?")
        );
    }

    #[test]
    fn test_fallback_prefers_specific_content_over_noisy_body() {
        let noisy_script = "var loader = 'splash screen and tracking markup'; ".repeat(20);
        let article = "This topic body contains the useful article text about local model serving, command line setup, benchmark results, and practical debugging notes. ".repeat(2);
        let html = format!(
            r#"
            <html>
                <head><title>Useful Article</title></head>
                <body>
                    <script>{}</script>
                    <div class="topic-body"><div class="cooked"><p>{}</p></div></div>
                    <footer>unrelated footer text</footer>
                </body>
            </html>
        "#,
            noisy_script, article
        );

        let (_title, content_html) =
            fallback_page_content(&html, "Useful Article").expect("fallback content");
        let markdown = html2md::parse_html(&content_html);

        assert!(markdown.contains("useful article text"));
        assert!(!markdown.contains("splash screen and tracking markup"));
    }

    #[test]
    fn test_noisy_extraction_is_detected_and_fallback_skips_it() {
        let noisy_content = format!(
            r#"
            <div id="d-splash">loader</div>
            <script>{}</script>
            <div class="topic-body"><div class="cooked"><p>{}</p></div></div>
        "#,
            "const svg = 'data:image/svg+xml'; ".repeat(20),
            "The useful discussion explains local model setup, tool calling behavior, benchmark tradeoffs, and debugging lessons. ".repeat(3)
        );
        let clean_content = "The clean discussion explains local model setup, tool calling behavior, benchmark tradeoffs, and debugging lessons without splash markup. ".repeat(3);
        let html = format!(
            r#"
            <html>
                <head><title>Noisy Readability Page</title></head>
                <body>
                    <article>{}</article>
                    <div class="topic-body"><div class="cooked"><p>{}</p></div></div>
                </body>
            </html>
        "#,
            noisy_content, clean_content
        );

        assert!(extracted_content_looks_noisy(&noisy_content));
        let (_title, content_html) =
            fallback_page_content(&html, "Noisy Readability Page").unwrap();
        let markdown = html2md::parse_html(&content_html);

        assert!(markdown.contains("clean discussion explains"));
        assert!(!markdown.contains("data:image/svg+xml"));
        assert!(!markdown.contains("d-splash"));
    }

    #[test]
    fn test_smart_content_rejects_title_only_extraction() {
        let html = r#"
            <html>
                <head><title>Only A Title</title></head>
                <body><h1>Only A Title</h1></body>
            </html>
        "#;

        let err = extract_smart_content(html, "https://example.com/title-only").unwrap_err();

        assert_eq!(err, "Extracted content too short");
    }

    #[test]
    fn test_scrape_header_places_references_below_source() {
        let references = normalize_reference_links(vec![
            "https://example.com/ref-a".to_string(),
            "   ".to_string(),
            "javascript:alert(1)".to_string(),
            "not-a-url".to_string(),
            "https://example.com/path\n<script>alert(1)</script>".to_string(),
            "https://example.com/ref-b".to_string(),
        ]);
        let header = format_scrape_header("Article", "https://example.com/article", &references);

        let source_pos = header.find("> **출처**").unwrap();
        let refs_pos = header.find("> **참고자료**").unwrap();
        let divider_pos = header.find("---").unwrap();

        assert!(source_pos < refs_pos);
        assert!(refs_pos < divider_pos);
        assert!(header.contains("> - <https://example.com/ref-a>"));
        assert!(header.contains("> - <https://example.com/ref-b>"));
        assert!(!header.contains("javascript:"));
        assert!(!header.contains("not-a-url"));
        assert!(!header.contains("<script>"));
        assert!(
            normalize_public_url("https://example.com/path\n<script>alert(1)</script>").is_none()
        );
        assert_eq!(
            normalize_public_url(" https://example.com/path?q=1 "),
            Some("https://example.com/path?q=1".to_string())
        );
    }

    #[test]
    fn test_format_scrape_header_escapes_hostile_title_text() {
        let header = format_scrape_header(
            r#"<script>alert(1)</script> [bad](javascript:alert(1))"#,
            "https://example.com/article",
            &[],
        );

        assert!(header.starts_with(
            "# &lt;script&gt;alert\\(1\\)&lt;/script&gt; \\[bad\\]\\(javascript:alert\\(1\\)\\)"
        ));
        assert!(!header.contains("# <script>alert(1)</script> [bad](javascript:alert(1))"));
    }

    #[test]
    fn test_fallback_title_stays_plain_for_metadata_but_safe_in_markdown_header() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="<script>alert(1)</script> [bad](javascript:alert(1))">
                </head>
                <body>
                    <h1><script>alert(1)</script> [bad](javascript:alert(1))</h1>
                    <article><p>Real article body with enough text to exceed the minimum extraction threshold for this regression test.</p></article>
                </body>
            </html>
        "#;

        let title = resolve_scraped_title(html, "");
        let header = format_scrape_header(&title, "https://example.com/article", &[]);

        assert_eq!(
            title,
            "<script>alert(1)</script> [bad](javascript:alert(1))"
        );
        assert!(header.contains(
            "&lt;script&gt;alert\\(1\\)&lt;/script&gt; \\[bad\\]\\(javascript:alert\\(1\\)\\)"
        ));
        assert!(!header.contains("# <script>alert(1)</script> [bad](javascript:alert(1))"));
    }

    #[test]
    fn test_ssrf_host_literal_guard_blocks_local_private_and_metadata_targets() {
        let blocked = [
            "localhost",
            "internal.localhost",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.100.100.200",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "2130706433",
            "0x7f000001",
            "0177.0.0.1",
        ];

        for host in blocked {
            assert!(
                ensure_public_host_literal(host).is_err(),
                "host should be blocked: {host}"
            );
        }
    }

    #[test]
    fn test_ssrf_host_literal_guard_allows_public_hosts() {
        let allowed = [
            "example.com",
            "github.com",
            "93.184.216.34",
            "2606:2800:220:1:248:1893:25c8:1946",
        ];

        for host in allowed {
            assert!(
                ensure_public_host_literal(host).is_ok(),
                "host should be allowed: {host}"
            );
        }
    }

    #[tokio::test]
    async fn test_ssrf_url_guard_blocks_literal_private_url_before_fetch() {
        let url = url::Url::parse("http://169.254.169.254/latest/meta-data/").unwrap();

        assert!(ensure_public_fetch_url(&url).await.is_err());
    }

    #[tokio::test]
    async fn test_scrape_url_to_markdown_returns_invalid_diagnostics() {
        let diagnostics = scrape_url_to_markdown("notaurl", &[]).await.unwrap_err();

        assert_eq!(diagnostics.status_class, "invalid");
        assert_eq!(diagnostics.failure_reason.as_deref(), Some("Invalid URL"));
    }

    #[tokio::test]
    async fn test_scrape_url_to_markdown_blocks_local_target_with_diagnostics() {
        let diagnostics = scrape_url_to_markdown("http://127.0.0.1/private", &[])
            .await
            .unwrap_err();

        assert_eq!(diagnostics.status_class, "blocked");
        assert!(diagnostics
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("Blocked private or local"));
        assert_eq!(
            diagnostics.raw_capture.omitted_reason.as_deref(),
            Some("private_or_local_response_not_stored")
        );
    }

    #[test]
    fn test_scrape_failure_diagnostics_classify_fetch_redirect_and_extraction_failures() {
        let fetch = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Failed to fetch URL",
            None,
            None,
            None,
            None,
        );
        let redirect = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Invalid redirect URL",
            None,
            None,
            None,
            None,
        );
        let missing_location = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Redirect missing Location",
            None,
            None,
            None,
            None,
        );
        let short = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Extracted content too short",
            Some("https://example.com/final"),
            Some("text/html"),
            Some((1024, 900)),
            None,
        );

        assert_eq!(fetch.status_class, "fetch_failed");
        assert_eq!(fetch.failure_reason.as_deref(), Some("Failed to fetch URL"));
        assert_eq!(fetch.http_status_code, None);
        assert_eq!(redirect.status_class, "redirect");
        assert_eq!(
            redirect.failure_reason.as_deref(),
            Some("Invalid redirect URL")
        );
        assert_eq!(missing_location.status_class, "redirect");
        assert_eq!(
            missing_location.failure_reason.as_deref(),
            Some("Redirect missing Location")
        );
        assert_eq!(short.status_class, "insufficient_extraction");
        assert_eq!(
            short.insufficiency_reason.as_deref(),
            Some("markdown_below_minimum_threshold")
        );
        assert_eq!(
            short.final_url.as_deref(),
            Some("https://example.com/final")
        );
        assert_eq!(short.raw_body_bytes, Some(1024));
        assert_eq!(short.raw_body_chars, Some(900));
    }

    #[test]
    fn test_scrape_failure_diagnostics_preserve_http_status_code() {
        let fetch = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Failed to fetch URL",
            Some("https://example.com/final"),
            Some("text/html"),
            None,
            Some(403),
        );

        assert_eq!(fetch.status_class, "fetch_failed");
        assert_eq!(fetch.failure_reason.as_deref(), Some("Failed to fetch URL"));
        assert_eq!(fetch.http_status_code, Some(403));
        assert_eq!(
            fetch.final_url.as_deref(),
            Some("https://example.com/final")
        );
    }

    #[test]
    fn test_format_scrape_failure_for_task_returns_actionable_messages() {
        let redirect = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Too many redirects",
            None,
            None,
            None,
            None,
        );
        let short = scrape_failure_diagnostics(
            "https://example.com",
            "https://example.com",
            &[],
            "Extracted content too short",
            None,
            None,
            None,
            None,
        );

        assert_eq!(
            format_scrape_failure_for_task(&redirect),
            "Scrape redirect failed: Too many redirects"
        );
        assert_eq!(
            format_scrape_failure_for_task(&short),
            "Scrape extraction insufficient: markdown_below_minimum_threshold (Extracted content too short)"
        );
    }

    #[tokio::test]
    async fn test_ssrf_url_guard_fails_closed_when_dns_lookup_fails() {
        let url = url::Url::parse("https://nonexistent.invalid/").unwrap();

        assert_eq!(
            ensure_public_fetch_url(&url).await.unwrap_err(),
            "Failed to resolve scrape target"
        );
    }

    #[tokio::test]
    async fn test_ssrf_resolver_returns_vetted_socket_addrs_for_public_literal() {
        let url = url::Url::parse("https://93.184.216.34/path").unwrap();
        let addrs = ensure_public_fetch_url(&url).await.unwrap();

        assert!(!addrs.is_empty());
        assert!(addrs.iter().all(|addr| addr.port() == 443));
        assert!(addrs.iter().all(|addr| !is_blocked_ip(addr.ip())));
    }

    #[test]
    fn test_pinned_scrape_client_accepts_vetted_addresses() {
        let url = url::Url::parse("https://example.com/path").unwrap();
        let addrs = [SocketAddr::from((Ipv4Addr::new(93, 184, 216, 34), 443))];

        assert!(pinned_scrape_client(&url, &addrs).is_ok());
    }

    #[test]
    fn test_parse_scrape_task_input_supports_legacy_and_references() {
        let input = ScrapeTaskInput {
            url: "https://example.com/article".to_string(),
            references: vec!["https://example.com/ref".to_string()],
            mode: Some("geeknews".to_string()),
        };
        let json = serde_json::to_string(&input).unwrap();
        let parsed = parse_scrape_task_input(Some(&json), "fallback");
        let legacy = parse_scrape_task_input(Some("https://legacy.example/article"), "fallback");

        assert_eq!(parsed.url, "https://example.com/article");
        assert_eq!(parsed.references, vec!["https://example.com/ref"]);
        assert_eq!(parsed.mode.as_deref(), Some("geeknews"));
        assert_eq!(legacy.url, "https://legacy.example/article");
        assert!(legacy.references.is_empty());
        assert!(legacy.mode.is_none());
    }

    #[test]
    fn test_parse_scrape_task_input_supports_legacy_json_without_mode() {
        let json =
            r#"{"url":"https://example.com/article","references":["https://example.com/ref"]}"#;
        let parsed = parse_scrape_task_input(Some(json), "fallback");

        assert_eq!(parsed.url, "https://example.com/article");
        assert_eq!(parsed.references, vec!["https://example.com/ref"]);
        assert!(parsed.mode.is_none());
    }

    #[test]
    fn test_geeknews_original_url_extraction_and_reference_append() {
        let geeknews_url = "https://news.hada.io/topic?id=12345";
        let html = r#"
            <html>
              <body>
                <a href="https://social.example/profile">Footer social</a>
                <div class="topictitle">
                    <a href="https://example.com/original?x=1">Original article</a>
                </div>
              </body>
            </html>
        "#;
        let original = extract_geeknews_original_url(html, geeknews_url);
        let mut references = normalize_reference_links(vec![
            "https://example.com/ref".to_string(),
            geeknews_url.to_string(),
        ]);
        append_reference_once(&mut references, normalize_public_url(geeknews_url).unwrap());

        assert_eq!(
            original,
            Some("https://example.com/original?x=1".to_string())
        );
        assert_eq!(
            references,
            vec![
                "https://example.com/ref".to_string(),
                "https://news.hada.io/topic?id=12345".to_string()
            ]
        );
    }

    #[test]
    fn test_geeknews_original_url_ignores_external_links_outside_topic_title() {
        let geeknews_url = "https://news.hada.io/topic?id=12345";
        let html = r#"
            <html>
              <body>
                <div class="topicbody">토픽 본문</div>
                <footer>
                    <a href="https://social.example/profile">Social profile</a>
                </footer>
              </body>
            </html>
        "#;

        assert_eq!(extract_geeknews_original_url(html, geeknews_url), None);
    }

    #[test]
    fn test_geeknews_original_url_ignores_internal_topic_title_links() {
        let geeknews_url = "https://news.hada.io/topic?id=12345";
        let html = r#"
            <html>
              <body>
                <div class="topictitle">
                    <a href="/topic?id=67890">Internal topic</a>
                    <a href="/show">Show HN style link</a>
                </div>
                <a href="https://example.com/footer">Footer link</a>
              </body>
            </html>
        "#;

        assert_eq!(extract_geeknews_original_url(html, geeknews_url), None);
    }

    #[test]
    fn test_geeknews_url_requires_topic_detail_shape() {
        assert!(is_geeknews_url("https://news.hada.io/topic?id=1"));
        assert!(is_geeknews_url(
            "https://news.hada.io/topic?id=12345&utm_source=x"
        ));
        assert!(!is_geeknews_url("https://news.hada.io/"));
        assert!(!is_geeknews_url("https://news.hada.io/new"));
        assert!(!is_geeknews_url("https://news.hada.io/past"));
        assert!(!is_geeknews_url("https://news.hada.io/topic"));
        assert!(!is_geeknews_url("https://news.hada.io/topic?foo=1"));
        assert!(!is_geeknews_url("https://news.hada.io/topic?id="));
        assert!(!is_geeknews_url("https://example.com/topic?id=1"));
    }

    #[tokio::test]
    async fn test_scrap_url_persists_general_mode_payload() {
        let dir = temp_test_dir("scrap-general-mode");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        let response = scrap_url(
            State(state),
            Json(ScrapRequest {
                url: "https://example.com/article".to_string(),
                translate: None,
                model: None,
                references: Some(vec!["https://example.com/ref".to_string()]),
                mode: Some("general".to_string()),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let prompt = sqlx::query_scalar::<_, String>(
            "SELECT user_prompt FROM tasks WHERE original_name = ?",
        )
        .bind("https://example.com/article")
        .fetch_one(&db)
        .await
        .unwrap();
        let input = parse_scrape_task_input(Some(&prompt), "fallback");
        assert_eq!(input.url, "https://example.com/article");
        assert_eq!(input.references, vec!["https://example.com/ref"]);
        assert_eq!(input.mode.as_deref(), Some("general"));

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_normalize_multiline_linked_images() {
        let markdown = r#"Intro

[

<img src="https://example.com/image.png" alt="Alt">

](https://example.com/target?x=1&y=2)

Outro
"#;
        let normalized = normalize_multiline_linked_images(markdown);

        assert!(normalized.contains(
            r#"<a href="https://example.com/target?x=1&amp;y=2"><img src="https://example.com/image.png" alt="Alt"></a>"#
        ));
        assert!(!normalized.contains("\n[\n\n<img"));
        assert!(normalized.ends_with('\n'));
    }

    #[test]
    fn test_normalize_multiline_linked_images_preserves_trailing_caption() {
        let markdown = r#"[

<img src="https://example.com/image.png" alt="Alt">

](https://example.com/target)caption text
"#;
        let normalized = normalize_multiline_linked_images(markdown);

        assert!(normalized.contains(
            r#"<a href="https://example.com/target"><img src="https://example.com/image.png" alt="Alt"></a>caption text"#
        ));
        assert!(!normalized.contains("](https://example.com/target)caption text"));
    }

    #[test]
    fn test_normalize_multiline_linked_images_allows_parentheses_in_target() {
        let markdown = r#"[

<img src="https://example.com/image.png" alt="Alt">

](https://example.com/foo_(bar))caption text
"#;
        let normalized = normalize_multiline_linked_images(markdown);

        assert!(normalized.contains(
            r#"<a href="https://example.com/foo_(bar)"><img src="https://example.com/image.png" alt="Alt"></a>caption text"#
        ));
        assert!(!normalized.contains(")caption text</p>"));
    }

    #[test]
    fn test_normalize_multiline_linked_images_allows_two_space_indented_content() {
        let markdown = "  [\n\n  <img src=\"https://example.com/image.png\" alt=\"Alt\">\n\n  ](https://example.com/target)caption";
        let normalized = normalize_multiline_linked_images(markdown);

        assert!(normalized.contains(
            "  <a href=\"https://example.com/target\"><img src=\"https://example.com/image.png\" alt=\"Alt\"></a>caption"
        ));
        assert!(!normalized.contains("  ["));
    }

    #[test]
    fn test_normalize_multiline_linked_images_rejects_unsafe_targets() {
        let markdown = "[\n\n<img src=\"https://example.com/image.png\" alt=\"Alt\">\n\n](javascript:alert(1))";
        let normalized = normalize_multiline_linked_images(markdown);

        assert!(!normalized.contains("<a href="));
        assert!(normalized.contains("](javascript:alert(1))"));
    }

    #[test]
    fn test_normalize_multiline_linked_images_ignores_indented_code() {
        let markdown =
            "    [\n\n    <img src=\"x\" onerror=\"alert(1)\">\n\n    ](https://example.com)";
        let normalized = normalize_multiline_linked_images(markdown);

        assert_eq!(normalized, markdown);
        assert!(!normalized.contains("<a href="));
    }

    #[test]
    fn test_normalize_multiline_linked_images_ignores_fenced_code() {
        let markdown = r#"```md
[

<img src="https://example.com/image.png" alt="Alt">

](https://example.com/target)
```
"#;
        let normalized = normalize_multiline_linked_images(markdown);

        assert_eq!(normalized, markdown);
        assert!(!normalized.contains("<a href="));
    }

    #[test]
    fn test_normalize_multiline_linked_images_ignores_fence_marker_text() {
        let markdown = r#"```md
```not-a-close
[

<img src="https://example.com/image.png" alt="Alt">

](https://example.com/target)
```
"#;
        let normalized = normalize_multiline_linked_images(markdown);

        assert_eq!(normalized, markdown);
        assert!(!normalized.contains("<a href="));
    }
}
