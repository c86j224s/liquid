use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn ensure_public_host_literal(host: &str) -> Result<(), String> {
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

pub fn parse_ipv4_style_host(host: &str) -> Option<Ipv4Addr> {
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

pub fn parse_ipv4_number(value: &str) -> Option<u32> {
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

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_blocked_ipv4(ip),
        IpAddr::V6(ip) => is_blocked_ipv6(ip),
    }
}

pub fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
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

pub fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
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

pub fn strip_html(html: &str) -> String {
    html2md::parse_html(html)
}

pub fn sanitize_input(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn sanitize_markdown_metadata(value: &str) -> String {
    escape_markdown_text(&sanitize_input(value))
        .trim()
        .to_string()
}

pub fn normalize_reference_url(reference: &str) -> Option<String> {
    normalize_http_url(reference)
}

pub fn normalize_http_url(raw_url: &str) -> Option<String> {
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

trait ScrapingStrategy {
    fn extract(&self, html: &str, url: &url::Url) -> Option<(String, String)>;
}

const MIN_EXTRACTED_MARKDOWN_CHARS: usize = 120;

pub struct ReadabilityStrategy;

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

pub struct SubstackStrategy;

impl ScrapingStrategy for SubstackStrategy {
    fn extract(&self, html: &str, _url: &url::Url) -> Option<(String, String)> {
        let fragment = scraper::Html::parse_fragment(html);
        let article_sel = scraper::Selector::parse("article.newsletter-post").ok()?;
        let title_sel = scraper::Selector::parse("h1.post-title").ok()?;

        let article = fragment.select(&article_sel).next()?;
        let title = fragment
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_else(|| "Untitled Substack".to_string());

        let content_html = article.html();
        Some((title, content_html))
    }
}

pub fn get_smart_content(html: &str, url_str: &str) -> (String, String) {
    let (title, content, _) = get_smart_content_with_strategy(html, url_str);
    (title, content)
}

pub fn get_smart_content_with_strategy(html: &str, url_str: &str) -> (String, String, String) {
    let url = url::Url::parse(url_str).unwrap();
    let normalized_html = preserve_code_line_spans(html);
    let html = normalized_html.as_str();
    let strategy_name = if url.domain().is_some_and(|d| d.contains("substack.com")) {
        "substack"
    } else {
        "readability"
    };

    let strategy: Box<dyn ScrapingStrategy> =
        if url.domain().is_some_and(|d| d.contains("substack.com")) {
            Box::new(SubstackStrategy)
        } else {
            Box::new(ReadabilityStrategy)
        };

    if let Some((title, content)) = strategy.extract(html, &url) {
        let title = resolve_scraped_title(html, &title);
        let use_fallback = !url.domain().is_some_and(|d| d.contains("substack.com"))
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

pub fn extract_smart_content(
    html: &str,
    url_str: &str,
) -> Result<(String, String, String), String> {
    let (title, content, strategy) = get_smart_content_with_strategy(html, url_str);
    if title == "Error" || !extracted_content_is_sufficient(&title, &content) {
        return Err("Extracted content too short".to_string());
    }
    Ok((title, content, strategy))
}

pub fn extracted_content_is_sufficient(title: &str, content_html: &str) -> bool {
    let markdown = html2md::parse_html(content_html);
    markdown_content_is_sufficient(title, &markdown)
}

pub fn extracted_content_looks_noisy(content_html: &str) -> bool {
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

pub fn markdown_content_is_sufficient(title: &str, markdown: &str) -> bool {
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

pub fn markdown_plain_text(markdown: &str) -> String {
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

pub fn normalize_text_for_quality(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize_scraped_title(title: &str) -> Option<String> {
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

pub fn resolve_scraped_title(html: &str, extracted_title: &str) -> String {
    if let Some(title) = normalize_scraped_title(extracted_title) {
        return title;
    }

    let document = scraper::Html::parse_document(html);
    extract_document_title(&document).unwrap_or_default()
}

pub fn fallback_page_content(html: &str, preferred_title: &str) -> Option<(String, String)> {
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

pub fn extract_document_title(document: &scraper::Html) -> Option<String> {
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

pub fn preserve_code_line_spans(html: &str) -> String {
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

pub fn normalize_line_span_pre(pre_html: &str) -> String {
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

pub fn select_pre_code_lines(fragment: &scraper::Html) -> Vec<String> {
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

pub fn is_pandoc_line_span_id(id: &str) -> bool {
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

pub fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

pub fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn escape_markdown_text(text: &str) -> String {
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

pub fn normalize_multiline_linked_images(markdown: &str) -> String {
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

pub fn parse_multiline_linked_image(lines: &[&str], start: usize) -> Option<(String, usize)> {
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

pub fn trim_markdown_content_indent(line: &str) -> Option<(usize, &str)> {
    if line.starts_with('\t') {
        return None;
    }
    let indent = line.chars().take_while(|c| *c == ' ').count();
    if indent > 3 {
        return None;
    }
    Some((indent, &line[indent..]))
}

pub fn parse_markdown_link_target_with_trailing_text(line: &str) -> Option<(String, &str)> {
    let rest = line.strip_prefix("](")?;
    let close_index = find_markdown_link_target_close(rest)?;
    let href = normalize_http_url(&rest[..close_index])?;
    Some((href, &rest[close_index + 1..]))
}

pub fn find_markdown_link_target_close(target_and_trailing: &str) -> Option<usize> {
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

pub fn opens_markdown_fence(line: &str) -> Option<String> {
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

pub fn closes_markdown_fence(line: &str, marker: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return false;
    }

    let marker_char = marker.chars().next().unwrap_or('`');
    let marker_len = trimmed.chars().take_while(|c| *c == marker_char).count();
    marker_len >= marker.len() && trimmed[marker_len..].trim().is_empty()
}

pub fn escape_html_attribute(text: &str) -> String {
    escape_html_text(text).replace('"', "&quot;")
}
