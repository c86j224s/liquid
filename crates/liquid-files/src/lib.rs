use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMetadata {
    pub id: i64,
    pub filename: String,
    pub original_name: String,
    pub file_type: String,
    pub status: String,
    pub drawer_id: Option<i64>,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileListItem {
    #[serde(flatten)]
    pub metadata: FileMetadata,
    pub content_preview: Option<String>,
    pub has_research_request: bool,
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSourceDocument {
    pub id: i64,
    pub filename: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentLinkInfo {
    pub id: i64,
    pub from_file_id: i64,
    pub to_file_id: i64,
    pub relation_type: String,
    pub created_by_task_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub document: ResearchSourceDocument,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRelationships {
    pub sources: Vec<DocumentLinkInfo>,
    pub derivatives: Vec<DocumentLinkInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentGraphNode {
    pub id: i64,
    pub filename: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentGraphEdge {
    pub id: i64,
    pub from_file_id: i64,
    pub to_file_id: i64,
    pub relation_type: String,
    pub created_by_task_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRelationshipGraph {
    pub root: DocumentGraphNode,
    pub nodes: Vec<DocumentGraphNode>,
    pub edges: Vec<DocumentGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagInfo {
    pub id: i64,
    pub label: String,
    pub slug: String,
    pub kind: String,
    pub source: String,
}

pub trait FileStore {
    type Error;

    fn list_files<'a>(&'a self) -> BoxFuture<'a, Result<Vec<FileMetadata>, Self::Error>>;
    fn find_file_id_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<i64>, Self::Error>>;
    fn load_file_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<FileMetadata>, Self::Error>>;
    fn load_source_document_by_id<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>>;
    fn load_source_document_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>>;
}

pub trait TagStore {
    type Error;

    fn list_tags<'a>(&'a self) -> BoxFuture<'a, Result<Vec<TagInfo>, Self::Error>>;
    fn load_file_tags<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Vec<TagInfo>, Self::Error>>;
    fn hydrate_file_tags<'a>(
        &'a self,
        items: &'a mut [FileListItem],
    ) -> BoxFuture<'a, Result<(), Self::Error>>;
}

pub trait DocumentLinkStore {
    type Error;

    fn load_relationships<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<DocumentRelationships, Self::Error>>;
    fn load_relationship_graph<'a>(
        &'a self,
        root: DocumentGraphNode,
        direction: &'a str,
    ) -> BoxFuture<'a, Result<DocumentRelationshipGraph, Self::Error>>;
}

pub fn normalize_tag_slug(label: &str) -> Option<String> {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in label.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            slug.push(ch);
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    (!slug.is_empty()).then_some(slug)
}

pub fn strip_legacy_title_metadata(title: &str) -> (String, Vec<&'static str>) {
    let mut remaining = title.trim().to_string();
    let mut tags = Vec::new();

    loop {
        let trimmed = remaining.trim_start();
        let Some(end) = trimmed.find(']') else {
            break;
        };
        if !trimmed.starts_with('[') {
            break;
        }

        let marker = &trimmed[..=end];
        let marker_tags: &[&str] = match marker.to_ascii_lowercase().as_str() {
            "[ai-research]" => &["Research"],
            "[research]" => &["Research"],
            "[no confidence]" => &["Low Confidence"],
            "[ko]" => &["Translation"],
            "[scrape]" => &["Scrape"],
            "[scrape+ko]" => &["Scrape", "Translation"],
            _ => &[],
        };

        if marker_tags.is_empty() {
            break;
        }
        for tag in marker_tags {
            if !tags.contains(tag) {
                tags.push(*tag);
            }
        }
        remaining = trimmed[end + 1..].trim_start().to_string();
    }

    if let Some(stripped) = remaining.strip_suffix(" (재시도)") {
        remaining = stripped.trim_end().to_string();
        if !tags.contains(&"Retry") {
            tags.push("Retry");
        }
    }

    if remaining.is_empty() {
        remaining = title.trim().to_string();
    }

    (remaining, tags)
}

pub fn system_tags_for_file_prefix(file_prefix: &str) -> Vec<&'static str> {
    match file_prefix {
        "[AI-Research]" => vec!["Research"],
        "[Research]" => vec!["Research"],
        "[KO]" => vec!["Translation"],
        "[Scrape]" => vec!["Scrape"],
        "[Scrape+KO]" => vec!["Scrape", "Translation"],
        _ => Vec::new(),
    }
}

pub fn system_tag_labels_for_task(
    file_prefix: &str,
    quality_status: Option<&str>,
    model: Option<&str>,
    resolved_model: Option<&str>,
    engine_kind: Option<&str>,
) -> Vec<String> {
    let mut labels: Vec<String> = system_tags_for_file_prefix(file_prefix)
        .into_iter()
        .map(str::to_string)
        .collect();

    if matches!(
        quality_status.unwrap_or_default(),
        "untrusted" | "low_confidence" | "no_confidence"
    ) {
        labels.push("Low Confidence".to_string());
    }

    let model_label = resolved_model
        .filter(|value| !value.trim().is_empty())
        .or_else(|| model.and_then(normalized_model_label));
    if let Some(model_label) = model_label {
        labels.push(format!("Model: {}", model_label.trim()));
    }

    if let Some(engine_kind) = engine_kind.filter(|value| !value.trim().is_empty()) {
        labels.push(format!("Engine: {}", engine_kind.trim()));
    }

    labels.sort();
    labels.dedup();
    labels
}

pub fn markdown_preview_text(content: &str, limit: usize) -> String {
    let mut text = String::new();
    let mut in_fence = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches(['-', '*', '+', '>'])
            .trim();
        if cleaned.is_empty() {
            continue;
        }

        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(cleaned);
        if text.chars().count() >= limit {
            break;
        }
    }

    let mut clipped: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        clipped.push('…');
    }
    clipped
}

fn normalized_model_label(model: &str) -> Option<&str> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .strip_prefix("cli:")
            .or_else(|| trimmed.strip_prefix("pi:"))
            .unwrap_or(trimmed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_tags_cover_prefix_quality_and_engine_metadata() {
        let tags = system_tag_labels_for_task(
            "[AI-Research]",
            Some("untrusted"),
            Some("cli:codex"),
            None,
            Some("cli"),
        );

        assert_eq!(
            tags,
            vec![
                "Engine: cli".to_string(),
                "Low Confidence".to_string(),
                "Model: codex".to_string(),
                "Research".to_string(),
            ]
        );
    }

    #[test]
    fn markdown_preview_ignores_fences_and_headings() {
        let preview = markdown_preview_text(
            "# Title\n\n```rust\nlet hidden = true;\n```\n- Visible point\n> Quoted\n",
            40,
        );

        assert_eq!(preview, "Title Visible point Quoted");
    }

    #[test]
    fn normalize_slug_and_strip_legacy_markers_preserve_expected_tags() {
        assert_eq!(
            normalize_tag_slug("  Model: GPT-5  ").as_deref(),
            Some("model-gpt-5")
        );

        let (title, tags) =
            strip_legacy_title_metadata("[NO CONFIDENCE] [Scrape+KO] Market scan (재시도)");
        assert_eq!(title, "Market scan");
        assert_eq!(
            tags,
            vec!["Low Confidence", "Scrape", "Translation", "Retry"]
        );
    }
}
