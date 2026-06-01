use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use liquid_files::{DocumentRelationships, FileMetadata, ResearchSourceDocument};
use liquid_protocol::{ResearchControllerArtifactsSummary, ResearchSourceDiagnosticsSummary};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ResearchRequestTask {
    pub id: i64,
    pub original_name: String,
    pub created_at: DateTime<Utc>,
    pub user_prompt: Option<String>,
    pub source_file_ids: Option<String>,
    pub source_filenames: Option<String>,
    pub research_type: Option<String>,
    pub research_mode: Option<String>,
    pub research_format: Option<String>,
    pub research_topic: Option<String>,
    pub research_instructions: Option<String>,
    pub model: Option<String>,
    pub engine_preset_name: Option<String>,
    pub engine_kind: Option<String>,
    pub resolved_model: Option<String>,
    pub research_intensity: Option<String>,
    pub quality_max_iterations: Option<i64>,
    pub quality_depth: Option<String>,
    pub quality_status: Option<String>,
    pub quality_last_failure: Option<String>,
    pub web_search_requested: Option<String>,
    pub web_search_provider: Option<String>,
    pub research_controller_artifacts_json: Option<String>,
    pub research_source_diagnostics_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskDocumentLinkContext {
    pub file_prefix: Option<String>,
    pub research_type: Option<String>,
    pub source_file_ids: Option<String>,
    pub source_filenames: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResearchRequestInfo {
    pub task_id: i64,
    pub original_name: String,
    pub created_at: DateTime<Utc>,
    pub request_prompt: Option<String>,
    pub research_topic: Option<String>,
    pub research_instructions: Option<String>,
    pub source_filenames: Vec<String>,
    pub source_documents: Vec<ResearchSourceDocument>,
    pub relationships: DocumentRelationships,
    pub research_type: Option<String>,
    pub research_mode: Option<String>,
    pub research_format: Option<String>,
    pub model: Option<String>,
    pub engine_preset_name: Option<String>,
    pub engine_kind: Option<String>,
    pub resolved_model: Option<String>,
    pub research_intensity: Option<String>,
    pub quality_max_iterations: Option<i64>,
    pub quality_depth: Option<String>,
    pub quality_status: Option<String>,
    pub quality_last_failure: Option<String>,
    pub web_search_requested: Option<String>,
    pub web_search_provider: Option<String>,
    pub research_controller_artifacts_summary: Option<ResearchControllerArtifactsSummary>,
    pub research_source_diagnostics_summary: Option<ResearchSourceDiagnosticsSummary>,
}

pub trait WorkspaceStore {
    type Error;

    fn has_research_request<'a>(
        &'a self,
        file_id: i64,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<bool, Self::Error>>;
    fn load_latest_research_request_task<'a>(
        &'a self,
        file_id: i64,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchRequestTask>, Self::Error>>;
    fn load_task_document_link_context<'a>(
        &'a self,
        task_id: i64,
    ) -> BoxFuture<'a, Result<Option<TaskDocumentLinkContext>, Self::Error>>;
    fn load_source_document_by_id<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>>;
    fn load_source_document_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>>;
    fn file_id_exists<'a>(&'a self, file_id: i64) -> BoxFuture<'a, Result<bool, Self::Error>>;
    fn file_id_for_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<i64>, Self::Error>>;
    fn load_relationships<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<DocumentRelationships, Self::Error>>;
    fn insert_document_link<'a>(
        &'a self,
        from_file_id: i64,
        to_file_id: i64,
        relation_type: &'a str,
        task_id: i64,
    ) -> BoxFuture<'a, Result<u64, Self::Error>>;
}

pub async fn has_research_request<S: WorkspaceStore>(
    store: &S,
    file: &FileMetadata,
) -> Result<bool, S::Error> {
    store.has_research_request(file.id, &file.filename).await
}

pub async fn load_research_request_info<S: WorkspaceStore>(
    store: &S,
    output_file: &FileMetadata,
    artifacts_summary: Option<ResearchControllerArtifactsSummary>,
    diagnostics_summary: Option<ResearchSourceDiagnosticsSummary>,
) -> Result<Option<ResearchRequestInfo>, S::Error> {
    let Some(task) = store
        .load_latest_research_request_task(output_file.id, &output_file.filename)
        .await?
    else {
        return Ok(None);
    };

    research_request_info_from_task(
        store,
        output_file.id,
        task,
        artifacts_summary,
        diagnostics_summary,
    )
    .await
    .map(Some)
}

pub async fn research_request_info_from_task<S: WorkspaceStore>(
    store: &S,
    output_file_id: i64,
    task: ResearchRequestTask,
    artifacts_summary: Option<ResearchControllerArtifactsSummary>,
    diagnostics_summary: Option<ResearchSourceDiagnosticsSummary>,
) -> Result<ResearchRequestInfo, S::Error> {
    let source_filenames = parse_json_string_array(task.source_filenames.as_deref());
    let source_file_ids = parse_json_i64_array(task.source_file_ids.as_deref());
    let source_documents =
        hydrate_research_source_documents(store, source_file_ids.as_deref(), &source_filenames)
            .await?;
    let relationships = store.load_relationships(output_file_id).await?;

    Ok(ResearchRequestInfo {
        task_id: task.id,
        original_name: task.original_name,
        created_at: task.created_at,
        request_prompt: task.user_prompt,
        research_topic: task.research_topic,
        research_instructions: task.research_instructions,
        source_filenames,
        source_documents,
        relationships,
        research_type: task.research_type,
        research_mode: task.research_mode,
        research_format: task.research_format,
        model: task.model,
        engine_preset_name: task.engine_preset_name,
        engine_kind: task.engine_kind,
        resolved_model: task.resolved_model,
        research_intensity: task.research_intensity,
        quality_max_iterations: task.quality_max_iterations,
        quality_depth: task.quality_depth,
        quality_status: task.quality_status,
        quality_last_failure: task.quality_last_failure,
        web_search_requested: task.web_search_requested,
        web_search_provider: task.web_search_provider,
        research_controller_artifacts_summary: artifacts_summary,
        research_source_diagnostics_summary: diagnostics_summary,
    })
}

pub async fn create_document_links_for_task_output<S: WorkspaceStore>(
    store: &S,
    task_id: i64,
    output_file_id: i64,
) -> Result<usize, S::Error> {
    let Some(task) = store.load_task_document_link_context(task_id).await? else {
        return Ok(0);
    };

    let mut source_ids = resolve_document_link_source_ids(
        store,
        task.source_file_ids.as_deref(),
        task.source_filenames.as_deref(),
    )
    .await?;
    source_ids.retain(|source_id| *source_id != output_file_id);
    source_ids.sort_unstable();
    source_ids.dedup();

    let Some(relation_type) = document_relation_type(
        task.file_prefix.as_deref(),
        task.research_type.as_deref(),
        source_ids.len(),
    ) else {
        return Ok(0);
    };

    let mut inserted = 0usize;
    for source_id in source_ids {
        inserted += store
            .insert_document_link(output_file_id, source_id, relation_type, task_id)
            .await? as usize;
    }

    Ok(inserted)
}

pub fn document_relation_type(
    file_prefix: Option<&str>,
    research_type: Option<&str>,
    source_count: usize,
) -> Option<&'static str> {
    if source_count == 0 {
        return None;
    }

    match file_prefix.unwrap_or_default() {
        "[KO]" => Some("translated_from"),
        "[Research]" | "[AI-Research]" => match research_type.unwrap_or_default() {
            "follow_up" => Some("followed_up_from"),
            "synthesis" => Some("synthesized_from"),
            _ if source_count > 1 => Some("synthesized_from"),
            _ => Some("derived_from"),
        },
        _ => None,
    }
}

async fn hydrate_research_source_documents<S: WorkspaceStore>(
    store: &S,
    source_file_ids: Option<&[i64]>,
    source_filenames: &[String],
) -> Result<Vec<ResearchSourceDocument>, S::Error> {
    let mut documents = Vec::new();
    let mut seen_filenames = std::collections::HashSet::new();

    if let Some(source_file_ids) = source_file_ids {
        for file_id in source_file_ids {
            if let Some(document) = store.load_source_document_by_id(*file_id).await? {
                seen_filenames.insert(document.filename.clone());
                documents.push(document);
            }
        }
    }

    if source_file_ids.is_none() || documents.len() < source_filenames.len() {
        for filename in source_filenames {
            if seen_filenames.contains(filename) {
                continue;
            }
            if let Some(document) = store.load_source_document_by_filename(filename).await? {
                seen_filenames.insert(document.filename.clone());
                documents.push(document);
            }
        }
    }

    Ok(documents)
}

async fn resolve_document_link_source_ids<S: WorkspaceStore>(
    store: &S,
    source_file_ids_json: Option<&str>,
    source_filenames_json: Option<&str>,
) -> Result<Vec<i64>, S::Error> {
    let parsed_ids = parse_json_i64_array(source_file_ids_json);
    let source_filenames = parse_json_string_array(source_filenames_json);
    let mut resolved = Vec::new();

    if let Some(source_ids) = parsed_ids.as_ref() {
        for source_id in source_ids {
            if store.file_id_exists(*source_id).await? {
                resolved.push(*source_id);
            }
        }
    }

    if parsed_ids.is_none() || resolved.len() < source_filenames.len() {
        for filename in source_filenames {
            if let Some(source_id) = store.file_id_for_filename(&filename).await? {
                resolved.push(source_id);
            }
        }
    }

    resolved.sort_unstable();
    resolved.dedup();
    Ok(resolved)
}

fn parse_json_string_array(value: Option<&str>) -> Vec<String> {
    value
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

fn parse_json_i64_array(value: Option<&str>) -> Option<Vec<i64>> {
    value.and_then(|raw| serde_json::from_str::<Vec<i64>>(raw).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_type_matches_existing_task_metadata_rules() {
        assert_eq!(
            document_relation_type(Some("[KO]"), None, 1),
            Some("translated_from")
        );
        assert_eq!(
            document_relation_type(Some("[AI-Research]"), Some("follow_up"), 1),
            Some("followed_up_from")
        );
        assert_eq!(
            document_relation_type(Some("[Research]"), Some("synthesis"), 1),
            Some("synthesized_from")
        );
        assert_eq!(
            document_relation_type(Some("[Research]"), Some("initial"), 2),
            Some("synthesized_from")
        );
        assert_eq!(
            document_relation_type(Some("[Research]"), Some("initial"), 1),
            Some("derived_from")
        );
        assert_eq!(document_relation_type(Some("[Scrape]"), None, 1), None);
    }
}
