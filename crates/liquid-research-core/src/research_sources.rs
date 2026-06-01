use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

pub fn infer_source_class(url: &str) -> &'static str {
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

pub fn normalize_result_url(raw_href: &str) -> Option<String> {
    let base = Url::parse("https://duckduckgo.com").ok()?;
    let trimmed = raw_href.trim();
    let url = match Url::parse(trimmed) {
        Ok(url) => {
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            url
        }
        Err(url::ParseError::RelativeUrlWithoutBase) => base.join(trimmed).ok()?,
        Err(_) => return None,
    };
    if url.host_str().is_some_and(is_duckduckgo_redirect_host) && url.path() == "/l/" {
        for (key, value) in url.query_pairs() {
            if key == "uddg" {
                return normalize_result_url(&value);
            }
        }
        return None;
    }
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    match url.host()? {
        url::Host::Domain(host) => ensure_public_host_literal(host).ok()?,
        url::Host::Ipv4(ip) => {
            if is_blocked_ip(IpAddr::V4(ip)) {
                return None;
            }
        }
        url::Host::Ipv6(ip) => {
            if is_blocked_ip(IpAddr::V6(ip)) {
                return None;
            }
        }
    }
    Some(url.to_string())
}

pub fn normalize_public_evidence_url(raw_href: &str) -> Option<String> {
    normalize_result_url(raw_href)
}

pub fn normalize_absolute_public_evidence_url(raw_href: &str) -> Option<String> {
    let trimmed = raw_href.trim();
    let url = parse_absolute_http_url(trimmed)?;
    normalize_absolute_result_url(url.as_str())
}

fn normalize_absolute_result_url(raw_href: &str) -> Option<String> {
    let url = Url::parse(raw_href).ok()?;
    if url.host_str().is_some_and(is_duckduckgo_redirect_host) && url.path() == "/l/" {
        for (key, value) in url.query_pairs() {
            if key == "uddg" {
                let decoded = value.trim();
                let decoded_url = parse_absolute_http_url(decoded)?;
                return normalize_absolute_result_url(decoded_url.as_str());
            }
        }
        return None;
    }
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    match url.host()? {
        url::Host::Domain(host) => ensure_public_host_literal(host).ok()?,
        url::Host::Ipv4(ip) => {
            if is_blocked_ip(IpAddr::V4(ip)) {
                return None;
            }
        }
        url::Host::Ipv6(ip) => {
            if is_blocked_ip(IpAddr::V6(ip)) {
                return None;
            }
        }
    }
    Some(url.to_string())
}

fn parse_absolute_http_url(raw_href: &str) -> Option<Url> {
    let url = Url::parse(raw_href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

fn is_duckduckgo_redirect_host(host: &str) -> bool {
    host == "duckduckgo.com" || host.ends_with(".duckduckgo.com")
}

fn ensure_public_host_literal(host: &str) -> Result<(), String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host == "metadata"
        || host == "internal"
        || host == "metadata.google.internal"
        || host == "instance-data.ec2.internal"
        || host.ends_with(".internal")
        || host.ends_with(".local")
        || host.ends_with(".localdomain")
        || host.ends_with(".home.arpa")
    {
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

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_blocked_ipv4(ip),
        IpAddr::V6(ip) => is_blocked_ipv6(ip),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
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

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_result_url_rejects_private_local_and_metadata_targets() {
        for raw_url in [
            "http://127.0.0.1/private",
            "http://localhost/private",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.5/internal",
            "http://[::1]/private",
            "http://[fd00::1]/private",
            "https://service.localhost/private",
            "https://metadata.google.internal/computeMetadata/v1",
            "https://instance-data.ec2.internal/latest/meta-data/",
            "https://service.internal/secret",
            "https://printer.local/status",
            "https://nas.localdomain/admin",
            "https://router.home.arpa/",
        ] {
            assert_eq!(
                normalize_result_url(raw_url),
                None,
                "unsafe result URL should be rejected: {raw_url}"
            );
        }
        assert_eq!(
            normalize_result_url("https://docs.vllm.ai/en/latest/").as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
    }

    #[test]
    fn normalize_absolute_public_evidence_url_rejects_duckduckgo_relative_uddg() {
        assert_eq!(
            normalize_absolute_public_evidence_url(
                "https://duckduckgo.com/l/?uddg=%2Frelative%2Ftarget"
            ),
            None
        );
        assert_eq!(
            normalize_absolute_public_evidence_url(
                "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
            )
            .as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
    }

    #[test]
    fn normalize_absolute_public_evidence_url_accepts_uppercase_http_schemes() {
        assert_eq!(
            normalize_absolute_public_evidence_url("HTTPS://docs.vllm.ai/en/latest/").as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
        assert_eq!(
            normalize_absolute_public_evidence_url(
                "https://duckduckgo.com/l/?uddg=HTTPS%3A%2F%2Fdocs.vllm.ai%2Fen%2Flatest%2F"
            )
            .as_deref(),
            Some("https://docs.vllm.ai/en/latest/")
        );
    }
}
