use std::path::{Path as StdPath, PathBuf};
use tokio::fs;

pub fn pi_runtime_dir(data_dir: &StdPath) -> PathBuf {
    data_dir.join("pi-runtime")
}

pub fn pi_home_dir(data_dir: &StdPath) -> PathBuf {
    pi_runtime_dir(data_dir).join("home")
}

pub fn pi_agent_dir(data_dir: &StdPath) -> PathBuf {
    pi_home_dir(data_dir).join(".pi").join("agent")
}

pub fn pi_session_dir(data_dir: &StdPath, task_id: i64) -> PathBuf {
    pi_runtime_dir(data_dir)
        .join("sessions")
        .join(format!("task-{task_id}"))
}

pub const PI_WEB_SEARCH_EXTENSION: &str = r#"import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

type SearchResult = { title: string; url: string; content: string };

const USER_AGENT = "Mozilla/5.0 (compatible; LiquidPiWeb/1.0)";
const MAX_FETCH_BYTES = 1048576;

function parseIPv4Number(value: string): number | null {
  if (!value) return null;
  let radix = 10;
  let digits = value;
  if (/^0x/i.test(value)) {
    radix = 16;
    digits = value.slice(2);
  } else if (value.length > 1 && value.startsWith("0")) {
    radix = 8;
    digits = value.slice(1);
  }
  if (!digits || !/^[0-9a-f]+$/i.test(digits)) return null;
  const parsed = Number.parseInt(digits, radix);
  if (!Number.isInteger(parsed) || parsed < 0) return null;
  return parsed;
}

function parseIPv4StyleHost(hostname: string): number[] | null {
  const normalized = hostname.toLowerCase().replace(/\.$/, "");
  if (!/^[0-9]/.test(normalized) || !/^[0-9a-fx.]+$/i.test(normalized)) return null;
  if (!normalized.includes(".")) {
    const value = parseIPv4Number(normalized);
    if (value === null || value > 0xffffffff) return null;
    return [(value >>> 24) & 255, (value >>> 16) & 255, (value >>> 8) & 255, value & 255];
  }
  const parts = normalized.split(".").map(parseIPv4Number);
  if (parts.length !== 4 || parts.some((part) => part === null || part > 255)) return null;
  return parts as number[];
}

function isBlockedIPv4(octets: number[]): boolean {
  const [a, b] = octets;
  return (
    a === 0 ||
    a === 10 ||
    a === 127 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 198 && (b === 18 || b === 19)) ||
    a >= 224 ||
    (a === 255 && b === 255 && octets[2] === 255 && octets[3] === 255)
  );
}

function parseIPv6EmbeddedIPv4(normalized: string): number[] | null {
  if (normalized.includes(".")) {
    const mapped = normalized.match(/^(?:0*:)*(?:ffff:)?([0-9a-fx.]+)$/i);
    if (!mapped) return null;
    return parseIPv4StyleHost(mapped[1]);
  }
  const lower = normalized.toLowerCase();
  const ffffIndex = lower.lastIndexOf("ffff:");
  if (ffffIndex < 0) return null;
  const suffix = lower.slice(ffffIndex + "ffff:".length);
  const groups = suffix.split(":").filter((group) => group.length > 0);
  if (groups.length !== 2) return null;
  const hi = Number.parseInt(groups[0], 16);
  const lo = Number.parseInt(groups[1], 16);
  if (![hi, lo].every((value) => Number.isInteger(value) && value >= 0 && value <= 0xffff)) return null;
  return [(hi >>> 8) & 255, hi & 255, (lo >>> 8) & 255, lo & 255];
}

function isBlockedHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase().replace(/^\[/, "").replace(/\]$/, "").replace(/\.$/, "");
  if (normalized === "localhost" || normalized.endsWith(".localhost")) return true;
  if (normalized === "metadata.google.internal" || normalized === "169.254.169.254") return true;

  const ipv4 = parseIPv4StyleHost(normalized);
  if (ipv4) return isBlockedIPv4(ipv4);

  if (normalized.includes(":")) {
    if (normalized === "::" || normalized === "::1") return true;
    const embedded = parseIPv6EmbeddedIPv4(normalized);
    if (embedded && isBlockedIPv4(embedded)) return true;
    const first = Number.parseInt(normalized.split(":")[0] || "0", 16);
    if (!Number.isNaN(first)) {
      if ((first & 0xfe00) === 0xfc00) return true; // unique local fc00::/7
      if ((first & 0xffc0) === 0xfe80) return true; // link-local fe80::/10
      if ((first & 0xff00) === 0xff00) return true; // multicast ff00::/8
    }
  }
  return false;
}

function assertLiteralPublicHttpUrl(rawUrl: string): URL {
  const url = new URL(rawUrl);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Only http and https URLs are supported.");
  }
  if (url.username || url.password) {
    throw new Error("URLs with embedded credentials are not allowed.");
  }
  const normalizedHostname = url.hostname.toLowerCase().replace(/^\[/, "").replace(/\]$/, "").replace(/\.$/, "");
  const literalIp = parseIPv4StyleHost(normalizedHostname) || (normalizedHostname.includes(":") && /^[0-9a-f:.]+$/i.test(normalizedHostname));
  if (!literalIp) {
    throw new Error("Direct web_fetch is limited to literal globally-routable IP URLs because this runtime cannot pin DNS before fetch. Use web_search for hostname discovery.");
  }
  if (isBlockedHostname(url.hostname)) {
    throw new Error("Private, loopback, link-local, metadata, and localhost URLs are not allowed.");
  }
  return url;
}

function resolveRedirectUrl(location: string | null, baseUrl: URL): URL | null {
  if (!location) return null;
  return assertLiteralPublicHttpUrl(new URL(location, baseUrl).toString());
}

async function limitedResponseText(response: Response): Promise<string> {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength && Number(declaredLength) > MAX_FETCH_BYTES) {
    throw new Error(`Response is too large: ${declaredLength} bytes`);
  }
  if (!response.body) {
    return await response.text();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > MAX_FETCH_BYTES) {
      await reader.cancel();
      throw new Error(`Response exceeded ${MAX_FETCH_BYTES} bytes`);
    }
    chunks.push(value);
  }
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(merged);
}

function decodeEntities(text: string): string {
  return text
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#x2F;/g, "/");
}

function stripTags(html: string): string {
  return decodeEntities(
    html
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(/<noscript[\s\S]*?<\/noscript>/gi, " ")
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<\/(p|div|section|article|li|h[1-6]|tr)>/gi, "\n")
      .replace(/<[^>]+>/g, " ")
      .replace(/[ \t]+/g, " ")
      .replace(/\n\s+/g, "\n")
      .replace(/\n{3,}/g, "\n\n")
      .trim(),
  );
}

function extractTitle(html: string, fallback: string): string {
  const title = html.match(/<title[^>]*>([\s\S]*?)<\/title>/i)?.[1];
  return stripTags(title || fallback).slice(0, 200) || fallback;
}

function normalizeDuckDuckGoUrl(raw: string): string | null {
  const decoded = decodeEntities(raw);
  try {
    const parsed = new URL(decoded, "https://duckduckgo.com");
    const uddg = parsed.searchParams.get("uddg");
    if (uddg) return decodeURIComponent(uddg);
    if (parsed.protocol === "http:" || parsed.protocol === "https:") return parsed.toString();
  } catch {
    if (decoded.startsWith("http://") || decoded.startsWith("https://")) return decoded;
  }
  return null;
}

function searchResultsFromHtml(html: string, maxResults: number): SearchResult[] {
  const results: SearchResult[] = [];
  const resultRegex = /<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>([\s\S]*?)<\/a>[\s\S]*?(?:<a[^>]+class="[^"]*result__snippet[^"]*"[^>]*>|<div[^>]+class="[^"]*result__snippet[^"]*"[^>]*>)([\s\S]*?)(?:<\/a>|<\/div>)/gi;
  let match: RegExpExecArray | null;
  while ((match = resultRegex.exec(html)) && results.length < maxResults) {
    const url = normalizeDuckDuckGoUrl(match[1]);
    if (!url || results.some((result) => result.url === url)) continue;
    results.push({
      title: stripTags(match[2]),
      url,
      content: stripTags(match[3]),
    });
  }
  return results;
}

async function fetchText(url: string, signal: AbortSignal): Promise<{ url: string; title: string; content: string; links: string[] }> {
  let safeUrl = assertLiteralPublicHttpUrl(url);
  let response: Response | null = null;
  for (let redirectCount = 0; redirectCount <= 5; redirectCount += 1) {
    response = await fetch(safeUrl, {
      headers: { "User-Agent": USER_AGENT, "Accept": "text/html,text/plain,application/json,*/*" },
      redirect: "manual",
      signal,
    });
    if (response.status < 300 || response.status >= 400) break;
    const nextUrl = resolveRedirectUrl(response.headers.get("location"), safeUrl);
    if (!nextUrl) throw new Error("Redirect target is missing or blocked.");
    safeUrl = nextUrl;
  }
  if (!response) {
    throw new Error("Fetch did not produce a response.");
  }
  if (response.status >= 300 && response.status < 400) {
    throw new Error("Too many redirects or blocked redirect target.");
  }
  if (!response.ok) {
    throw new Error(`Fetch API error (status ${response.status}): ${response.statusText}`);
  }
  const finalUrl = response.url || safeUrl.toString();
  assertLiteralPublicHttpUrl(finalUrl);
  const html = await limitedResponseText(response);
  const links = Array.from(html.matchAll(/<a[^>]+href="([^"]+)"/gi))
    .map((match) => {
      try { return new URL(decodeEntities(match[1]), finalUrl).toString(); } catch { return null; }
    })
    .filter((link): link is string => !!link)
    .slice(0, 30);
  return {
    url: finalUrl,
    title: extractTitle(html, finalUrl),
    content: stripTags(html).slice(0, 30000),
    links,
  };
}

export default function (pi: ExtensionAPI) {
  pi.registerTool({
    name: "web_search",
    label: "Web Search",
    description: "Search the web with Liquid's Pi web tool. Ollama is only used as the model provider; this tool does not require Ollama web search.",
    parameters: Type.Object({
      query: Type.String({ description: "The search query to execute" }),
      max_results: Type.Optional(Type.Number({ description: "Maximum number of search results to return (default: 5)", default: 5 })),
    }),
    async execute(_toolCallId, params, signal, _onUpdate, _ctx) {
      const maxResults = Math.max(1, Math.min(params.max_results ?? 5, 10));
      const searchUrl = new URL("https://html.duckduckgo.com/html/");
      searchUrl.searchParams.set("q", params.query);
      const response = await fetch(searchUrl, {
        method: "GET",
        headers: { "User-Agent": USER_AGENT, "Accept": "text/html" },
        signal,
      });
      if (!response.ok) {
        const errorText = await response.text().catch(() => "");
        throw new Error(`Search API error (status ${response.status}): ${errorText || response.statusText}`);
      }
      const html = await response.text();
      const results = searchResultsFromHtml(html, maxResults);
      const formatted = results
        .map((r, i) => `${i + 1}. ${r.title}\n   URL: ${r.url}\n   ${r.content}`)
        .join("\n\n");
      return {
        content: [{ type: "text", text: formatted || "No results found." }],
        details: { results, provider: "duckduckgo-html" },
      };
    },
  });

  pi.registerTool({
    name: "web_fetch",
    label: "Web Fetch",
    description: "Fetch and extract text content from a literal globally-routable IP URL with Liquid's Pi web tool. Hostnames are intentionally rejected because this runtime cannot pin DNS before fetch.",
    parameters: Type.Object({
      url: Type.String({ description: "Literal globally-routable IP http(s) URL to fetch and extract content from; hostnames are rejected for SSRF safety" }),
    }),
    async execute(_toolCallId, params, signal, _onUpdate, _ctx) {
      const data = await fetchText(params.url, signal);
      const formatted = [
        `Title: ${data.title}`,
        `URL: ${data.url}`,
        "",
        "Content:",
        data.content,
        "",
        `Links found: ${data.links?.length ?? 0}`,
        ...(data.links?.slice(0, 10).map((l) => `  - ${l}`) ?? []),
      ].join("\n");
      return {
        content: [{ type: "text", text: formatted }],
        details: {
          title: data.title,
          url: data.url,
          content: data.content,
          links: data.links,
          provider: "direct-fetch",
        },
      };
    },
  });
}
"#;

pub fn pi_web_search_extension_path(data_dir: &StdPath) -> PathBuf {
    pi_agent_dir(data_dir).join("liquid-web-search.ts")
}

pub async fn ensure_pi_web_search_extension(data_dir: &StdPath) -> Result<PathBuf, String> {
    let extension_path = pi_web_search_extension_path(data_dir);
    if let Some(parent) = extension_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Pi web search extension 디렉터리를 만들지 못했습니다: {e}"))?;
    }
    fs::write(&extension_path, PI_WEB_SEARCH_EXTENSION)
        .await
        .map_err(|e| format!("Pi web search extension을 쓰지 못했습니다: {e}"))?;
    Ok(extension_path)
}

pub fn pi_tool_args(allow_web_search: bool, extension_path: Option<&StdPath>) -> Vec<String> {
    if allow_web_search {
        let path = extension_path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                pi_web_search_extension_path(StdPath::new("."))
                    .to_string_lossy()
                    .to_string()
            });
        vec![
            "--no-builtin-tools".to_string(),
            "--extension".to_string(),
            path,
            "--tools".to_string(),
            "web_search,web_fetch".to_string(),
        ]
    } else {
        vec!["--no-tools".to_string()]
    }
}

pub fn contains_http_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

pub async fn pi_web_tool_failure(session_dir: &StdPath) -> Option<String> {
    let mut entries = fs::read_dir(session_dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(content) = fs::read_to_string(path).await else {
            continue;
        };
        for line in content.lines() {
            let is_web_tool = line.contains("\"toolName\":\"web_search\"")
                || line.contains("\"toolName\":\"web_fetch\"");
            if is_web_tool && line.contains("\"isError\":true") {
                return Some("Pi+Ollama 웹검색 도구 실행이 실패했습니다.".to_string());
            }
        }
    }
    None
}

pub async fn clear_pi_session_jsonl(session_dir: &StdPath) {
    let Ok(mut entries) = fs::read_dir(session_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            let _ = fs::remove_file(path).await;
        }
    }
}

pub async fn ensure_pi_ollama_models_config(
    data_dir: &StdPath,
    selected_model: &str,
) -> Result<Vec<String>, String> {
    let home_dir = pi_home_dir(data_dir);
    let agent_dir = pi_agent_dir(data_dir);
    fs::create_dir_all(&agent_dir)
        .await
        .map_err(|e| format!("Pi 격리 런타임 디렉터리를 만들지 못했습니다: {e}"))?;

    match list_pi_ollama_models(data_dir).await {
        Ok(models) if models.iter().any(|model| model == selected_model) => return Ok(models),
        _ => {}
    }

    let mut launch_cmd = tokio::process::Command::new("ollama");
    launch_cmd
        .arg("launch")
        .arg("pi")
        .arg("--model")
        .arg(selected_model)
        .arg("--yes")
        .arg("--")
        .arg("--provider")
        .arg("ollama")
        .env("HOME", &home_dir)
        .env("PI_CODING_AGENT_DIR", &agent_dir)
        .current_dir(&home_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let launch_result = match launch_cmd.spawn() {
        Ok(child) => {
            match tokio::time::timeout(std::time::Duration::from_secs(45), child.wait_with_output())
                .await
            {
                Ok(output) => output,
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "ollama launch pi가 45초 안에 끝나지 않았습니다.",
                )),
            }
        }
        Err(e) => Err(e),
    };

    match &launch_result {
        Ok(output) if output.status.success() => {
            if let Ok(models) = list_pi_ollama_models(data_dir).await {
                if models.iter().any(|model| model == selected_model) {
                    return Ok(models);
                }
            }
        }
        _ => {}
    }

    let launch_error = match &launch_result {
        Ok(output) => format_cli_failure(output),
        Err(e) => format!("ollama launch pi 실행 실패: {e}"),
    };
    match write_pi_ollama_config_fallback(data_dir, selected_model).await {
        Ok(models) => Ok(models),
        Err(fallback_error) => Err(format!(
            "ollama launch pi 부트스트랩 실패: {launch_error}; 직접 config fallback 실패: {fallback_error}"
        )),
    }
}

pub async fn list_pi_ollama_models(data_dir: &StdPath) -> Result<Vec<String>, String> {
    let home_dir = pi_home_dir(data_dir);
    let agent_dir = pi_agent_dir(data_dir);
    let mut cmd = tokio::process::Command::new("pi");
    cmd.arg("--list-models")
        .arg("ollama")
        .env("HOME", &home_dir)
        .env("PI_CODING_AGENT_DIR", &agent_dir)
        .current_dir(&home_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| format!("pi 모델 목록을 실행하지 못했습니다: {e}"))?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait_with_output())
        .await
        .map_err(|_| "pi 모델 목록 확인이 15초 안에 끝나지 않았습니다.".to_string())?
        .map_err(|e| format!("pi 모델 목록 확인에 실패했습니다: {e}"))?;
    if !output.status.success() {
        return Err(format_cli_failure(&output));
    }
    Ok(pi_listed_models(&String::from_utf8_lossy(&output.stdout)))
}

pub async fn write_pi_ollama_config_fallback(
    data_dir: &StdPath,
    selected_model: &str,
) -> Result<Vec<String>, String> {
    let agent_dir = pi_agent_dir(data_dir);
    fs::create_dir_all(&agent_dir)
        .await
        .map_err(|e| format!("Pi agent 디렉터리를 만들지 못했습니다: {e}"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Ollama 확인 클라이언트를 만들지 못했습니다: {e}"))?;
    let data = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Ollama에 연결할 수 없습니다: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Ollama 모델 목록을 가져오지 못했습니다: {e}"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Ollama 모델 목록을 해석하지 못했습니다: {e}"))?;

    let models: Vec<String> = data["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| model["name"].as_str())
        .map(str::to_string)
        .collect();
    if models.is_empty() {
        return Err("Ollama에 등록된 모델이 없습니다.".to_string());
    }
    if !models.iter().any(|model| model == selected_model) {
        return Err(format!("Ollama 모델 {selected_model}이 목록에 없습니다."));
    }

    let model_entries: Vec<serde_json::Value> = models
        .iter()
        .map(|model| {
            serde_json::json!({
                "_launch": true,
                "contextWindow": 262144,
                "id": model,
                "input": ["text", "image"],
                "reasoning": true
            })
        })
        .collect();
    let models_config = serde_json::json!({
        "providers": {
            "ollama": {
                "api": "openai-completions",
                "apiKey": "ollama",
                "baseUrl": "http://127.0.0.1:11434/v1",
                "models": model_entries
            }
        }
    });
    let config_text = serde_json::to_string_pretty(&models_config)
        .map_err(|e| format!("Pi 모델 설정을 만들지 못했습니다: {e}"))?;
    fs::write(agent_dir.join("models.json"), config_text)
        .await
        .map_err(|e| format!("Pi 모델 설정을 저장하지 못했습니다: {e}"))?;
    let settings_config = serde_json::json!({
        "defaultProvider": "ollama",
        "defaultModel": selected_model
    });
    let settings_text = serde_json::to_string_pretty(&settings_config)
        .map_err(|e| format!("Pi 설정을 만들지 못했습니다: {e}"))?;
    fs::write(agent_dir.join("settings.json"), settings_text)
        .await
        .map_err(|e| format!("Pi 설정을 저장하지 못했습니다: {e}"))?;

    Ok(models)
}

pub fn pi_listed_models(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            match (columns.next(), columns.next()) {
                (Some("ollama"), Some(model)) => Some(model.to_string()),
                _ => None,
            }
        })
        .collect()
}

pub fn format_cli_failure(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let status = output
        .status
        .code()
        .map(|code| format!("exit code {}", code))
        .unwrap_or_else(|| "terminated by signal".to_string());

    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{}\n\nstderr:\n{}", stdout, stderr),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => format!("CLI task failed with {}", status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path as StdPath;

    #[test]
    fn test_pi_model_list_parsing_uses_exact_ollama_model_column() {
        let output =
            "Provider Model Context\nollama llama3 131072\nollama llama3.1 131072\nopenai llama3 131072\n";
        let models = pi_listed_models(output);
        assert!(models.iter().any(|model| model == "llama3"));
        assert!(models.iter().any(|model| model == "llama3.1"));
        assert!(!models.iter().any(|model| model == "llama"));
    }

    #[test]
    fn test_pi_tool_args_scope_web_extension_only_when_requested() {
        assert_eq!(pi_tool_args(false, None), vec!["--no-tools"]);
        let extension_path = StdPath::new("/tmp/liquid-web-search.ts");
        let args = pi_tool_args(true, Some(extension_path));
        assert!(args.contains(&"--no-builtin-tools".to_string()));
        assert!(args.contains(&"--extension".to_string()));
        assert!(args.contains(&"/tmp/liquid-web-search.ts".to_string()));
        assert!(args.contains(&"web_search,web_fetch".to_string()));
        assert!(!args.contains(&"bash".to_string()));
        assert!(!args.contains(&"read,grep,find,ls".to_string()));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("https://html.duckduckgo.com/html/"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("duckduckgo-html"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("isBlockedHostname"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("assertLiteralPublicHttpUrl"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("limited to literal globally-routable IP URLs"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("parseIPv4StyleHost"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("parseIPv6EmbeddedIPv4"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("lastIndexOf(\"ffff:\")"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("(hi >>> 8) & 255"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("redirect: \"manual\""));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("resolveRedirectUrl"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("169.254.169.254"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("MAX_FETCH_BYTES"));
        assert!(!PI_WEB_SEARCH_EXTENSION.contains("pub"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("Private, loopback"));
        assert!(PI_WEB_SEARCH_EXTENSION.contains("metadata"));
        assert!(!PI_WEB_SEARCH_EXTENSION.contains("LIQUID_OLLAMA_API_KEY"));
        assert!(!PI_WEB_SEARCH_EXTENSION.contains("ollama.com/api"));
        assert!(!PI_WEB_SEARCH_EXTENSION.contains("api/experimental"));
        assert!(contains_http_url("출처: https://example.com"));
        assert!(!contains_http_url("외부 URL 출처 없음"));
    }
}
