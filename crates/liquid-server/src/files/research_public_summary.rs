use liquid_protocol::{
    ResearchContextPackingDiagnostics, ResearchControllerArtifacts,
    ResearchControllerArtifactsSummary, ResearchScrapeDiagnosticsSummary,
    ResearchSourceDiagnosticsEnvelope, ResearchSourceDiagnosticsSummary,
};
use std::net::{IpAddr, Ipv4Addr};
use url::Url;

pub(super) fn summarize_research_controller_artifacts_for_public_api(
    artifacts_json: Option<&str>,
) -> Option<ResearchControllerArtifactsSummary> {
    let artifacts = serde_json::from_str::<ResearchControllerArtifacts>(artifacts_json?).ok()?;
    Some(ResearchControllerArtifactsSummary {
        version: artifacts.version,
        event_count: artifacts.events.len(),
        source_card_count: artifacts.source_cards.len(),
        claim_count: artifacts.claim_log.len(),
        conflict_count: artifacts.conflict_map.len(),
        open_debt_count: artifacts
            .research_debt
            .iter()
            .filter(|debt| debt.status != "closed")
            .count(),
        warning_count: artifacts.warnings.len(),
        quality_gate_status: artifacts
            .quality_gate
            .as_ref()
            .and_then(|gate| public_quality_gate_status(gate.status.as_str())),
        quality_gate_failure_count: artifacts
            .quality_gate
            .as_ref()
            .map(|gate| gate.failure_messages.len())
            .unwrap_or(0),
    })
}

pub(super) fn summarize_research_source_diagnostics_for_public_api(
    diagnostics_json: Option<&str>,
) -> Option<ResearchSourceDiagnosticsSummary> {
    let diagnostics =
        serde_json::from_str::<ResearchSourceDiagnosticsEnvelope>(diagnostics_json?).ok()?;
    Some(ResearchSourceDiagnosticsSummary {
        version: diagnostics.version,
        subject: diagnostics
            .subject
            .as_deref()
            .map(redact_public_diagnostic_text),
        source_pack_status: diagnostics
            .source_pack
            .as_ref()
            .map(|report| public_source_pack_status(report.status.as_str())),
        source_pack_query_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.queries.len())
            .unwrap_or(0),
        source_pack_adopted_source_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.adopted_source_count)
            .unwrap_or(0),
        source_pack_skipped_candidate_count: diagnostics
            .source_pack
            .as_ref()
            .map(|report| report.skipped_candidates.len())
            .unwrap_or(0),
        scrape_count: diagnostics.scrapes.len(),
        scrape_failure_count: diagnostics
            .scrapes
            .iter()
            .filter(|scrape| scrape.failure_reason.is_some())
            .count(),
        scrapes: diagnostics
            .scrapes
            .iter()
            .map(|scrape| {
                let failure_reason = scrape
                    .failure_reason
                    .as_deref()
                    .map(redact_public_diagnostic_text);
                let insufficiency_reason = scrape
                    .insufficiency_reason
                    .as_deref()
                    .map(redact_public_diagnostic_text);
                let status_class = public_scrape_status_class(scrape.status_class.as_str());
                let sufficiency_result =
                    public_sufficiency_result(scrape.sufficiency_result.as_str());
                ResearchScrapeDiagnosticsSummary {
                    status_class: status_class.clone(),
                    user_message: public_scrape_failure_message(
                        status_class.as_str(),
                        failure_reason.as_deref(),
                        scrape.http_status_code,
                        insufficiency_reason.as_deref(),
                    ),
                    failure_reason,
                    http_status_code: scrape.http_status_code,
                    sufficiency_result,
                    insufficiency_reason,
                    original_url_host: redact_url_host(&scrape.original_url),
                    final_url_host: scrape.final_url.as_deref().and_then(redact_url_host),
                    reference_link_count: scrape.reference_links.len(),
                }
            })
            .collect(),
        context_packing: diagnostics
            .context_packing
            .map(public_context_packing_diagnostics),
    })
}

fn public_quality_gate_status(status: &str) -> Option<String> {
    match status.trim().to_ascii_lowercase().as_str() {
        "passed" | "failed" | "untrusted" => Some(status.trim().to_ascii_lowercase()),
        _ => None,
    }
}

fn public_source_pack_status(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "success" | "error" | "blocked" | "empty" | "partial" | "skipped" | "none" => {
            status.trim().to_ascii_lowercase()
        }
        _ => "unknown".to_string(),
    }
}

fn public_scrape_status_class(status_class: &str) -> String {
    match status_class.trim().to_ascii_lowercase().as_str() {
        "ok"
        | "success"
        | "blocked"
        | "http_error"
        | "network_error"
        | "blocked_private_url"
        | "fetch_failed"
        | "redirect"
        | "invalid_url"
        | "invalid"
        | "insufficient_content"
        | "insufficient_extraction"
        | "error"
        | "failed" => status_class.trim().to_ascii_lowercase(),
        _ => "unknown".to_string(),
    }
}

fn public_sufficiency_result(result: &str) -> String {
    match result.trim().to_ascii_lowercase().as_str() {
        "sufficient" | "insufficient" | "unknown" => result.trim().to_ascii_lowercase(),
        _ => "unknown".to_string(),
    }
}

fn public_context_packing_diagnostics(
    mut diagnostics: ResearchContextPackingDiagnostics,
) -> ResearchContextPackingDiagnostics {
    diagnostics.strategy = match diagnostics.strategy.trim() {
        "artifact_ledgers" | "raw_source_fallback" => diagnostics.strategy.trim().to_string(),
        _ => "unknown".to_string(),
    };
    diagnostics.notes.clear();
    diagnostics
}

fn public_scrape_failure_message(
    status_class: &str,
    failure_reason: Option<&str>,
    http_status_code: Option<u16>,
    insufficiency_reason: Option<&str>,
) -> Option<String> {
    let detail = failure_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let guidance = match status_class.trim().to_ascii_lowercase().as_str() {
        "blocked" | "blocked_private_url" => {
            "스크랩이 차단되었습니다. 대상 사이트가 자동 수집을 막았거나 공개 인터넷에서 접근할 수 없는 주소여서 서버가 수집을 중단했습니다. 브라우저에서 직접 열리는 공개 문서인지 확인하고, 로그인이나 사내망이 필요한 페이지라면 접근 가능한 공개 링크로 다시 시도해 주세요."
        }
        "fetch_failed" | "network_error" | "http_error" => {
            "페이지를 가져오지 못했습니다. DNS 오류, 일시적인 네트워크 문제, 원격 서버 응답 실패가 원인일 수 있습니다. 잠시 후 다시 시도하거나 브라우저에서 같은 URL이 실제로 열리는지 확인해 주세요."
        }
        "redirect" => {
            "리다이렉트 처리에 실패했습니다. 중간 이동이 너무 많거나 최종 목적지 URL이 잘못되었을 수 있습니다. 단축 링크 대신 최종 문서 URL로 다시 시도해 주세요."
        }
        "insufficient_extraction" => {
            if insufficiency_reason == Some("markdown_below_minimum_threshold") {
                "페이지는 열렸지만 본문을 충분히 추출하지 못했습니다. 자바스크립트 의존 페이지, 접근 제한 페이지, 짧은 안내문일 수 있습니다. 본문이 직접 보이는 문서 링크나 PDF/원문 링크로 다시 시도해 주세요."
            } else {
                "페이지는 열렸지만 본문을 충분히 추출하지 못했습니다. 본문이 직접 보이는 공개 문서 링크로 다시 시도해 주세요."
            }
        }
        "invalid" | "invalid_url" => {
            "스크랩할 URL 형식이 올바르지 않습니다. http:// 또는 https:// 로 시작하는 공개 URL인지 확인해 주세요."
        }
        _ => {
            "스크랩 처리 중 예기치 않은 오류가 발생했습니다. 입력 URL을 다시 확인하고, 같은 문제가 반복되면 기술 세부를 함께 확인해 주세요."
        }
    };

    let mut lines = vec![guidance.to_string(), String::new()];
    if let Some(status_code) = http_status_code {
        lines.push(format!("HTTP 상태: {status_code}"));
    }
    lines.push(format!("진단 분류: {status_class}"));
    lines.push(format!("기술 세부: {detail}"));
    Some(lines.join("\n"))
}

fn redact_url_host(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?;
    if public_url_host(&url) {
        Some(host.to_string())
    } else {
        Some("[redacted-private-host]".to_string())
    }
}

pub fn redact_public_diagnostic_text(value: &str) -> String {
    let local_paths_redacted = redact_local_paths(value);
    let sensitive_values_redacted = redact_sensitive_key_values(&local_paths_redacted);
    let hashes_redacted = redact_hash_values(&sensitive_values_redacted);
    let redacted = [
        ("raw provider payload", "[redacted-payload]"),
        ("provider payload", "[redacted-payload]"),
        ("response body:", "[redacted-payload]:"),
        ("resolved system prompt", "[redacted-prompt]"),
        ("resolved user prompt", "[redacted-prompt]"),
        ("resolved prompt", "[redacted-prompt]"),
        (
            "controller artifacts json",
            "[redacted-controller-artifacts]",
        ),
        ("controller artifact", "[redacted-controller-artifacts]"),
        ("source diagnostics json", "[redacted-source-diagnostics]"),
        ("raw-capture", "[redacted-raw-capture]"),
        ("raw capture", "[redacted-raw-capture]"),
        ("raw_capture", "[redacted-raw-capture]"),
    ]
    .into_iter()
    .fold(hashes_redacted, |text, (needle, replacement)| {
        replace_ascii_case_insensitive(&text, needle, replacement)
    });
    redact_private_host_tokens(&redact_urls_in_text(&redacted))
}

fn redact_local_paths(value: &str) -> String {
    redact_prefixed_values(
        value,
        &["/tmp/", "/var/folders/", "/users/"],
        "[redacted-local-path]",
        is_sensitive_value_delimiter,
    )
}

fn redact_hash_values(value: &str) -> String {
    redact_prefixed_values(
        value,
        &["sha256-", "sha256:"],
        "[redacted-hash]",
        is_sensitive_value_delimiter,
    )
}

fn redact_sensitive_key_values(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let lower = value.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some((start, key, replacement)) = next_sensitive_key(&lower, cursor) {
        output.push_str(&value[cursor..start]);
        output.push_str(replacement);
        cursor = consume_sensitive_assignment(value, key, start + key.len());
    }
    output.push_str(&value[cursor..]);
    output
}

fn next_sensitive_key<'a>(
    lower: &'a str,
    cursor: usize,
) -> Option<(usize, &'static str, &'static str)> {
    [
        ("api_key", "[redacted-key]"),
        ("client_secret", "[redacted-key]"),
        ("authorization", "[redacted-header]"),
        ("raw_provider_payload", "[redacted-payload]"),
        ("provider_payload", "[redacted-payload]"),
        ("resolved_system_prompt", "[redacted-prompt]"),
        ("resolved_user_prompt", "[redacted-prompt]"),
        ("resolved_prompt", "[redacted-prompt]"),
        (
            "research_controller_artifacts_json",
            "[redacted-controller-artifacts]",
        ),
        (
            "controller_artifacts_json",
            "[redacted-controller-artifacts]",
        ),
        (
            "research_source_diagnostics_json",
            "[redacted-source-diagnostics]",
        ),
        ("source_diagnostics_json", "[redacted-source-diagnostics]"),
    ]
    .into_iter()
    .filter_map(|(key, replacement)| {
        lower[cursor..]
            .find(key)
            .map(|offset| (cursor + offset, key, replacement))
    })
    .min_by_key(|(start, _, _)| *start)
}

fn consume_sensitive_assignment(value: &str, key: &str, key_end: usize) -> usize {
    let mut cursor = key_end;
    let bytes = value.as_bytes();
    if cursor < value.len() && matches!(bytes[cursor], b'"' | b'\'') {
        let after_quote = cursor + 1;
        let mut lookahead = after_quote;
        while lookahead < value.len() && bytes[lookahead].is_ascii_whitespace() {
            lookahead += 1;
        }
        if lookahead < value.len() && matches!(bytes[lookahead], b'=' | b':') {
            cursor = after_quote;
        }
    }
    while cursor < value.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if cursor < value.len() && matches!(bytes[cursor], b'=' | b':') {
        cursor += 1;
        while cursor < value.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < value.len() && matches!(bytes[cursor], b'"' | b'\'') {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < value.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            if cursor < value.len() {
                cursor += 1;
            }
            cursor
        } else {
            if is_bulk_sensitive_key(key) {
                consume_bulk_sensitive_value(value, cursor)
            } else if key == "authorization" {
                consume_until(value, cursor, is_sensitive_assignment_delimiter)
            } else {
                consume_until(value, cursor, is_unquoted_sensitive_key_value_delimiter)
            }
        }
    } else {
        cursor
    }
}

fn is_bulk_sensitive_key(key: &str) -> bool {
    matches!(
        key,
        "raw_provider_payload"
            | "provider_payload"
            | "resolved_system_prompt"
            | "resolved_user_prompt"
            | "resolved_prompt"
            | "research_controller_artifacts_json"
            | "controller_artifacts_json"
            | "research_source_diagnostics_json"
            | "source_diagnostics_json"
    )
}

fn consume_bulk_sensitive_value(value: &str, cursor: usize) -> usize {
    let bytes = value.as_bytes();
    if cursor < value.len() && matches!(bytes[cursor], b'{' | b'[') {
        consume_balanced_json_like(value, cursor).unwrap_or(value.len())
    } else {
        consume_until(value, cursor, |ch| ch == '\n')
    }
}

fn consume_balanced_json_like(value: &str, cursor: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let opener = *bytes.get(cursor)?;
    let closer = match opener {
        b'{' => b'}',
        b'[' => b']',
        _ => return None,
    };
    let mut depth = 0usize;
    let mut idx = cursor;
    let mut in_string = false;
    let mut escaped = false;
    while idx < value.len() {
        let byte = bytes[idx];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == opener {
            depth += 1;
        } else if byte == closer {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx + 1);
            }
        }
        idx += 1;
    }
    None
}

fn redact_prefixed_values(
    value: &str,
    prefixes: &[&str],
    replacement: &str,
    is_delimiter: fn(char) -> bool,
) -> String {
    let mut output = String::with_capacity(value.len());
    let lower = value.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some((start, prefix)) = next_prefixed_value(&lower, cursor, prefixes) {
        output.push_str(&value[cursor..start]);
        output.push_str(replacement);
        cursor = consume_until(value, start + prefix.len(), is_delimiter);
    }
    output.push_str(&value[cursor..]);
    output
}

fn next_prefixed_value<'a>(
    lower: &'a str,
    cursor: usize,
    prefixes: &[&'a str],
) -> Option<(usize, &'a str)> {
    prefixes
        .iter()
        .filter_map(|prefix| {
            lower[cursor..]
                .find(prefix)
                .map(|offset| (cursor + offset, *prefix))
        })
        .min_by_key(|(start, _)| *start)
}

fn consume_until(value: &str, start: usize, is_delimiter: fn(char) -> bool) -> usize {
    value[start..]
        .char_indices()
        .find_map(|(offset, ch)| is_delimiter(ch).then_some(start + offset))
        .unwrap_or(value.len())
}

fn is_sensitive_value_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
        )
}

fn is_sensitive_assignment_delimiter(ch: char) -> bool {
    ch == '\n'
        || matches!(
            ch,
            '"' | '\'' | '<' | '>' | '[' | ']' | '{' | '}' | ',' | ';'
        )
}

fn is_unquoted_sensitive_key_value_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ',' | ';')
}

fn replace_ascii_case_insensitive(value: &str, needle: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    let lower = value.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    while let Some(relative_start) = lower[cursor..].find(&needle) {
        let start = cursor + relative_start;
        let end = start + needle.len();
        output.push_str(&value[cursor..start]);
        output.push_str(replacement);
        cursor = end;
    }
    output.push_str(&value[cursor..]);
    output
}

fn redact_urls_in_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative_start) = find_next_http_scheme(&value[cursor..]) {
        let start = cursor + relative_start;
        output.push_str(&value[cursor..start]);
        let raw_end = value[start..]
            .char_indices()
            .find_map(|(offset, ch)| {
                if ch.is_whitespace()
                    || matches!(
                        ch,
                        '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
                    )
                {
                    Some(start + offset)
                } else {
                    None
                }
            })
            .unwrap_or(value.len());
        let raw_candidate = &value[start..raw_end];
        let candidate = raw_candidate.trim_end_matches(|ch: char| ".:!?".contains(ch));
        let suffix = &raw_candidate[candidate.len()..];
        if private_or_metadata_url(candidate) {
            output.push_str("<redacted-private-url>");
        } else if let Some(host) = redact_url_host(candidate) {
            output.push_str(&format!("<redacted-url:{host}>"));
        } else {
            output.push_str("<redacted-url>");
        }
        output.push_str(suffix);
        cursor = raw_end;
    }
    output.push_str(&value[cursor..]);
    output
}

fn redact_private_host_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if token_contains_private_or_metadata_host(token) {
                "[redacted-private-url]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn private_or_metadata_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .is_some_and(|url| !public_url_host(&url))
}

fn public_url_host(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(host)) => public_domain_host(host),
        Some(url::Host::Ipv4(ip)) => public_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => public_ip(IpAddr::V6(ip)),
        None => false,
    }
}

fn public_domain_host(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    !lower.is_empty()
        && lower != "localhost"
        && lower != "metadata.google.internal"
        && !lower.ends_with(".localhost")
        && !lower.ends_with(".local")
        && !lower.ends_with(".internal")
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_loopback()
                && !ip.is_private()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && ip != std::net::Ipv4Addr::new(255, 255, 255, 255)
                && octets[0] != 0
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(mapped));
            }
            !ip.is_loopback()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
        }
    }
}

fn token_contains_private_or_metadata_host(token: &str) -> bool {
    if keyed_private_or_metadata_host_fragment(token) {
        return true;
    }
    token
        .split(|ch: char| {
            matches!(
                ch,
                '=' | '"' | '\'' | '{' | '}' | '(' | ')' | ',' | ';' | '<' | '>'
            )
        })
        .any(private_or_metadata_host_fragment)
}

fn keyed_private_or_metadata_host_fragment(token: &str) -> bool {
    ['=', ':'].into_iter().any(|delimiter| {
        let Some((key, value)) = token.split_once(delimiter) else {
            return false;
        };
        if key.contains(':') || value.is_empty() || value.starts_with(':') {
            return false;
        }
        host_context_key(key) && private_or_metadata_host_fragment_with_context(value, true)
    })
}

fn host_context_key(key: &str) -> bool {
    let lower = key
        .trim_matches(|ch: char| {
            matches!(
                ch,
                ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\'' | '<' | '>'
            )
        })
        .to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "host"
            | "source"
            | "url"
            | "uri"
            | "endpoint"
            | "target"
            | "address"
            | "addr"
            | "ip"
            | "origin"
    ) || lower.ends_with("_host")
        || lower.ends_with("_url")
        || lower.ends_with("_uri")
        || lower.ends_with("_ip")
}

fn private_or_metadata_host_fragment(fragment: &str) -> bool {
    private_or_metadata_host_fragment_with_context(fragment, false)
}

fn private_or_metadata_host_fragment_with_context(
    fragment: &str,
    allow_bare_ipv4_style: bool,
) -> bool {
    let candidate = fragment.trim_matches(|ch: char| {
        matches!(
            ch,
            ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\'' | '<' | '>'
        )
    });
    let lower = candidate.to_ascii_lowercase();
    if lower == "localhost" || lower == "metadata.google.internal" {
        return true;
    }
    if private_or_metadata_ip_fragment(candidate, allow_bare_ipv4_style) {
        return true;
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return private_or_metadata_url(candidate);
    }
    if !(candidate.contains('.') || candidate.contains(':')) {
        return false;
    }
    Url::parse(&format!("http://{candidate}"))
        .ok()
        .is_some_and(|url| !public_url_host(&url))
}

fn private_or_metadata_ip_fragment(candidate: &str, allow_bare_ipv4_style: bool) -> bool {
    private_host_candidates(candidate).into_iter().any(|host| {
        let parsed = host.parse::<IpAddr>().ok().or_else(|| {
            if allow_bare_ipv4_style || host.contains('.') {
                parse_ipv4_style_host(&host).map(IpAddr::V4)
            } else {
                None
            }
        });
        parsed.is_some_and(|ip| !public_ip(ip))
    })
}

fn parse_ipv4_style_host(host: &str) -> Option<Ipv4Addr> {
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

fn parse_ipv4_number(value: &str) -> Option<u32> {
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

fn private_host_candidates(candidate: &str) -> Vec<String> {
    let trimmed = candidate.trim_matches(|ch: char| {
        matches!(
            ch,
            ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\'' | '<' | '>'
        )
    });
    if trimmed.is_empty() {
        return Vec::new();
    }
    let primary = if let Some(rest) = trimmed.strip_prefix('[') {
        rest.find(']').map(|end| rest[..end].to_string())
    } else {
        Some(
            trimmed
                .split(['/', '?', '#'])
                .next()
                .unwrap_or(trimmed)
                .trim_matches(|ch| matches!(ch, '[' | ']'))
                .trim_end_matches('.')
                .to_string(),
        )
    };
    let mut candidates = Vec::new();
    if let Some(host) = primary.filter(|host| !host.is_empty()) {
        candidates.push(host);
    }
    for fragment in trimmed.split(|ch: char| !(ch.is_ascii_hexdigit() || ch == '.' || ch == ':')) {
        let fragment = fragment.trim_end_matches('.');
        if !fragment.is_empty() && (fragment.contains(':') || fragment.contains('.')) {
            candidates.push(fragment.to_string());
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn find_next_http_scheme(value: &str) -> Option<usize> {
    let lower = value.to_ascii_lowercase();
    match (lower.find("http://"), lower.find("https://")) {
        (Some(http), Some(https)) => Some(http.min(https)),
        (Some(http), None) => Some(http),
        (None, Some(https)) => Some(https),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_research_summary_omits_raw_diagnostics_and_redacts_urls() {
        let summary = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{
                "version":1,
                "subject":"Read https://example.com/report?token=secret now",
                "source_pack":{"subject":"Topic","status":"failed at URL=HTTPS://internal.example/path","reason":null,"queries":[],"seeded_source_count":0,"discovered_source_count":0,"adopted_source_count":0,"adopted_candidates":[],"skipped_candidates":[]},
                "scrapes":[{
                    "original_url":"https://example.com/report?token=secret",
                    "normalized_url":"https://example.com/report?token=secret",
                    "final_url":"https://api.example.com/final#frag",
                    "status_class":"Resolved prompt: https://example.com/?token=status",
                    "failure_reason":"provider returned RAW PROVIDER PAYLOAD with Resolved prompt and raw_capture path /tmp/liquid/raw/provider-payload.json hash sha256-secret from url=https://example.com/report?token=secret",
                    "http_status_code":500,
                    "extraction_strategy":null,
                    "title":null,
                    "content_type":null,
                    "raw_body_bytes":1048576,
                    "raw_body_chars":999999,
                    "extracted_html_chars":10,
                    "markdown_chars":10,
                    "sufficiency_result":"raw_capture path /tmp/liquid/raw/x",
                    "insufficiency_reason":"debug log at source:(https://example.com/debug?secret=1)",
                    "reference_links":["https://docs.example.com/a?x=1", "https://docs.example.com/b?x=2"],
                    "accessed_at":"2026-05-13T00:00:00Z",
                    "raw_capture":{"mode":"stored","path":"/tmp/liquid/raw/provider-payload.json","hash":"sha256-secret","omitted_reason":null}
                }],
                "context_packing":{"strategy":"artifact_ledgers","included_source_card_count":1,"omitted_source_card_count":0,"included_excerpt_chars":10,"omitted_raw_chars":999999,"total_raw_chars":1000009,"active_debt_count":0,"unresolved_conflict_count":0,"notes":["raw path /tmp/liquid/raw/provider-payload.json token=secret"]}
            }"#,
        ))
        .expect("public diagnostics summary");

        assert_eq!(
            summary.subject.as_deref(),
            Some("Read <redacted-url:example.com> now")
        );
        assert_eq!(summary.source_pack_status.as_deref(), Some("unknown"));
        assert_eq!(
            summary.scrapes[0].original_url_host.as_deref(),
            Some("example.com")
        );
        assert_eq!(
            summary.scrapes[0].final_url_host.as_deref(),
            Some("api.example.com")
        );
        assert_eq!(summary.scrapes[0].status_class, "unknown");
        assert_eq!(summary.scrapes[0].sufficiency_result, "unknown");
        assert_eq!(summary.scrapes[0].reference_link_count, 2);
        assert_eq!(
            summary.scrapes[0].failure_reason.as_deref(),
            Some("provider returned [redacted-payload] with [redacted-prompt] and [redacted-raw-capture] path [redacted-local-path] hash [redacted-hash] from url=<redacted-url:example.com>")
        );
        assert_eq!(
            summary.scrapes[0].insufficiency_reason.as_deref(),
            Some("debug log at source:(<redacted-url:example.com>)")
        );

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("/tmp/liquid/raw"));
        assert!(!serialized.contains("sha256-secret"));
        assert!(!serialized.contains("provider-payload.json"));
        assert!(!serialized.contains("token=secret"));
        assert!(!serialized.contains("debug?secret"));
        assert!(!serialized.contains("raw provider payload"));
        assert!(!serialized.contains("Resolved prompt"));
        assert!(!serialized.contains("raw_capture"));
        assert!(!serialized.contains("sha256-secret"));
        assert!(serialized.contains("[redacted-payload]"));
        assert!(serialized.contains("omitted_raw_chars"));
        assert!(!serialized.contains("bounded counts only"));
        assert!(!serialized.contains("raw path"));
        assert!(!serialized.contains("failed at URL"));
        assert!(!serialized.contains("internal.example"));

        let raw_fallback = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{
                "version":1,
                "context_packing":{"strategy":"raw_source_fallback","included_source_card_count":0,"omitted_source_card_count":0,"included_excerpt_chars":0,"omitted_raw_chars":0,"total_raw_chars":0,"active_debt_count":0,"unresolved_conflict_count":0,"notes":["Resolved prompt: secret"]}
            }"#,
        ))
        .expect("raw fallback context packing summary");
        let context_packing = raw_fallback
            .context_packing
            .expect("raw fallback context packing");
        assert_eq!(context_packing.strategy, "raw_source_fallback");
        assert!(context_packing.notes.is_empty());

        let unknown_strategy = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{
                "version":1,
                "context_packing":{"strategy":"Resolved prompt https://example.com/?token=secret","included_source_card_count":0,"omitted_source_card_count":0,"included_excerpt_chars":0,"omitted_raw_chars":0,"total_raw_chars":0,"active_debt_count":0,"unresolved_conflict_count":0,"notes":[]}
            }"#,
        ))
        .expect("unknown context packing summary");
        assert_eq!(
            unknown_strategy.context_packing.unwrap().strategy,
            "unknown"
        );

        let producer_status = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{
                "version":1,
                "scrapes":[
                    {"original_url":"https://example.com/a","normalized_url":"https://example.com/a","final_url":null,"status_class":"success","failure_reason":null,"http_status_code":null,"extraction_strategy":null,"title":null,"content_type":null,"raw_body_bytes":null,"raw_body_chars":null,"extracted_html_chars":10,"markdown_chars":10,"sufficiency_result":"sufficient","insufficiency_reason":null,"reference_links":[],"accessed_at":"2026-05-13T00:00:00Z","raw_capture":{"mode":"omitted","path":null,"hash":null,"omitted_reason":"disabled"}},
                    {"original_url":"https://example.com/b","normalized_url":"https://example.com/b","final_url":null,"status_class":"fetch_failed","failure_reason":"Failed to fetch URL","http_status_code":null,"extraction_strategy":null,"title":null,"content_type":null,"raw_body_bytes":null,"raw_body_chars":null,"extracted_html_chars":0,"markdown_chars":0,"sufficiency_result":"insufficient","insufficiency_reason":null,"reference_links":[],"accessed_at":"2026-05-13T00:00:00Z","raw_capture":{"mode":"omitted","path":null,"hash":null,"omitted_reason":"disabled"}}
                ]
            }"#,
        ))
        .expect("producer status summary");
        assert_eq!(producer_status.scrapes[0].status_class, "success");
        assert_eq!(producer_status.scrapes[1].status_class, "fetch_failed");
        assert!(producer_status.scrapes[1].user_message.is_some());
    }

    #[test]
    fn public_research_summary_allowlists_source_pack_status() {
        let allowed = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{"version":1,"source_pack":{"subject":"Topic","status":"SKIPPED","reason":null,"queries":[],"seeded_source_count":0,"discovered_source_count":0,"adopted_source_count":0,"adopted_candidates":[],"skipped_candidates":[]}}"#,
        ))
        .expect("allowed source pack status summary");
        assert_eq!(allowed.source_pack_status.as_deref(), Some("skipped"));

        let unknown = summarize_research_source_diagnostics_for_public_api(Some(
            "{\"version\":1,\"source_pack\":{\"subject\":\"Topic\",\"status\":\"### User Request: leak this\",\"reason\":null,\"queries\":[],\"seeded_source_count\":0,\"discovered_source_count\":0,\"adopted_source_count\":0,\"adopted_candidates\":[],\"skipped_candidates\":[]}}",
        ))
        .expect("unknown source pack status summary");
        assert_eq!(unknown.source_pack_status.as_deref(), Some("unknown"));
    }

    #[test]
    fn public_diagnostic_redaction_consumes_secret_values() {
        let summary = summarize_research_source_diagnostics_for_public_api(Some(
            r#"{
                "version":1,
                "scrapes":[{
                    "original_url":"https://example.com/report",
                    "normalized_url":"https://example.com/report",
                    "final_url":null,
                    "status_class":"fetch_failed",
                    "failure_reason":"path=/tmp/liquid/raw/x \"path\":\"/Users/dev/raw/y\" hash=sha256-secret api_key=sk-secret client_secret=\"top-secret\" Authorization: Bearer abc.def \"api_key\":\"sk-json\" \"client_secret\":\"json-secret\" \"Authorization\":\"Bearer json.token\" provider_payload=\"raw body\" resolved_prompt=\"hidden prompt\" research_controller_artifacts_json={\"a\":\"secret\",\"b\":\"leak\"} source_diagnostics_json=\"raw diagnostics\"",
                    "http_status_code":500,
                    "extraction_strategy":null,
                    "title":null,
                    "content_type":null,
                    "raw_body_bytes":null,
                    "raw_body_chars":null,
                    "extracted_html_chars":0,
                    "markdown_chars":0,
                    "sufficiency_result":"insufficient",
                    "insufficiency_reason":null,
                    "reference_links":[],
                    "accessed_at":"2026-05-13T00:00:00Z",
                    "raw_capture":{"mode":"stored","path":"/tmp/liquid/raw/provider-payload.json","hash":"sha256-secret","omitted_reason":null}
                }]
            }"#,
        ))
        .expect("redacted diagnostics summary");

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("/tmp/liquid/raw"));
        assert!(!serialized.contains("/Users/dev/raw"));
        assert!(!serialized.contains("sha256-secret"));
        assert!(!serialized.contains("sk-secret"));
        assert!(!serialized.contains("top-secret"));
        assert!(!serialized.contains("Bearer abc.def"));
        assert!(!serialized.contains("sk-json"));
        assert!(!serialized.contains("json-secret"));
        assert!(!serialized.contains("Bearer json.token"));
        assert!(!serialized.contains("raw body"));
        assert!(!serialized.contains("hidden prompt"));
        assert!(!serialized.contains("\"b\":\"leak\""));
        assert!(!serialized.contains("research_controller_artifacts_json"));
        assert!(!serialized.contains("source_diagnostics_json"));
        assert!(serialized.contains("[redacted-local-path]"));
        assert!(serialized.contains("[redacted-hash]"));
        assert!(serialized.contains("[redacted-key]"));
        assert!(serialized.contains("[redacted-header]"));
        assert!(serialized.contains("[redacted-payload]"));
        assert!(serialized.contains("[redacted-prompt]"));
        assert!(serialized.contains("[redacted-controller-artifacts]"));
        assert!(serialized.contains("[redacted-source-diagnostics]"));
    }

    #[test]
    fn public_diagnostic_redaction_hides_private_and_metadata_hosts() {
        let redacted = redact_public_diagnostic_text(
            "url=http://169.254.169.254/latest source:metadata.google.internal json={\"source\":\"[::1]\"} mapped=http://[::ffff:127.0.0.1]/x source=2130706433 host=0x7f000001 octal=0177.0.0.1 status=500 attempt=3 public=https://example.com/path",
        );

        assert!(!redacted.contains("169.254.169.254"));
        assert!(!redacted.contains("metadata.google.internal"));
        assert!(!redacted.contains("::1"));
        assert!(!redacted.contains("127.0.0.1"));
        assert!(!redacted.contains("2130706433"));
        assert!(!redacted.contains("0x7f000001"));
        assert!(!redacted.contains("0177.0.0.1"));
        assert!(redacted.contains("status=500"));
        assert!(redacted.contains("attempt=3"));
        assert!(redacted.contains("[redacted-private-url]"));
        assert!(redacted.contains("<redacted-private-url>"));
        assert!(redacted.contains("<redacted-url:example.com>"));
    }

    #[test]
    fn public_controller_artifact_summary_is_counts_only() {
        let summary = summarize_research_controller_artifacts_for_public_api(Some(
            r#"{
                "version":1,
                "events":[{"stage":"draft","iteration":1,"max_iterations":3,"status":"running","message":"Resolved prompt: secret"}],
                "source_cards":[{"id":"S1","url":"https://example.com/private?token=secret","title":"Secret source","source_class":"official_or_primary","extracted_facts":["secret fact"],"confidence":"high"}],
                "claim_log":[{"id":"C1","claim":"secret claim","support_source_card_ids":["S1"]}],
                "conflict_map":[],
                "research_debt":[{"id":"D1","severity":"medium","missing_evidence":"secret debt","status":"open"}],
                "warnings":["secret warning"],
                "quality_gate":{"status":"UNTRUSTED","failure_messages":["secret failure"],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":1}
            }"#,
        ))
        .expect("public controller summary");

        assert_eq!(summary.source_card_count, 1);
        assert_eq!(summary.claim_count, 1);
        assert_eq!(summary.open_debt_count, 1);
        assert_eq!(summary.warning_count, 1);
        assert_eq!(summary.quality_gate_status.as_deref(), Some("untrusted"));
        assert_eq!(summary.quality_gate_failure_count, 1);
        let malicious_status = summarize_research_controller_artifacts_for_public_api(Some(
            r#"{"version":1,"quality_gate":{"status":"https://example.com/?token=secret Resolved prompt","failure_messages":[],"unsupported_claim_count":0,"unresolved_conflict_count":0,"open_debt_count":0}}"#,
        ))
        .expect("malicious quality status summary");
        assert_eq!(malicious_status.quality_gate_status, None);

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("secret claim"));
        assert!(!serialized.contains("private?token"));
        assert!(!serialized.contains("Resolved prompt"));
    }
}
