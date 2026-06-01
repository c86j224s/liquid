use futures::future::BoxFuture;
pub use liquid_files::{normalize_tag_slug, strip_legacy_title_metadata};
use liquid_files::{
    system_tag_labels_for_task, DocumentGraphEdge, DocumentGraphNode, DocumentLinkInfo,
    DocumentLinkStore, DocumentRelationshipGraph, DocumentRelationships, FileListItem,
    FileMetadata, FileStore, ResearchSourceDocument, TagInfo, TagStore,
};
use liquid_workspace::{ResearchRequestTask, TaskDocumentLinkContext, WorkspaceStore};
use sqlx::{
    sqlite::{SqlitePool, SqliteRow},
    FromRow, Row,
};
use std::collections::{HashMap, HashSet};
use std::path::Path as StdPath;

pub struct SqliteFileRepository<'a> {
    db: &'a SqlitePool,
}

impl<'a> SqliteFileRepository<'a> {
    pub fn new(db: &'a SqlitePool) -> Self {
        Self { db }
    }

    pub async fn list_files(&self) -> Result<Vec<FileMetadata>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, filename, original_name, file_type, status, drawer_id, uploaded_at
             FROM files
             ORDER BY uploaded_at DESC",
        )
        .fetch_all(self.db)
        .await?;

        Ok(rows.into_iter().map(file_metadata_from_row).collect())
    }

    pub async fn find_file_id_by_filename(
        &self,
        filename: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
            .bind(filename)
            .fetch_optional(self.db)
            .await
    }

    pub async fn create_file(
        &self,
        filename: &str,
        original_name: &str,
        file_type: &str,
        status: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query(
            "INSERT INTO files (filename, original_name, file_type, status) VALUES (?, ?, ?, ?)",
        )
        .bind(filename)
        .bind(original_name)
        .bind(file_type)
        .bind(status)
        .execute(self.db)
        .await
        .map(|result| result.last_insert_rowid())
    }

    pub async fn update_status_by_filename(
        &self,
        filename: &str,
        status: &str,
        clear_drawer: bool,
    ) -> Result<u64, sqlx::Error> {
        let sql = if clear_drawer {
            "UPDATE files SET status = ?, drawer_id = NULL WHERE filename = ?"
        } else {
            "UPDATE files SET status = ? WHERE filename = ?"
        };
        sqlx::query(sql)
            .bind(status)
            .bind(filename)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }

    pub async fn update_title_by_filename(
        &self,
        filename: &str,
        title: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query("UPDATE files SET original_name = ? WHERE filename = ?")
            .bind(title)
            .bind(filename)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }

    pub async fn load_file_by_filename(
        &self,
        filename: &str,
    ) -> Result<Option<FileMetadata>, sqlx::Error> {
        sqlx::query(
            "SELECT id, filename, original_name, file_type, status, drawer_id, uploaded_at
             FROM files
             WHERE filename = ?",
        )
        .bind(filename)
        .fetch_optional(self.db)
        .await
        .map(|row| row.map(file_metadata_from_row))
    }

    pub async fn delete_file_by_filename(&self, filename: &str) -> Result<u64, sqlx::Error> {
        sqlx::query("DELETE FROM files WHERE filename = ?")
            .bind(filename)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }

    pub async fn load_source_document_by_id(
        &self,
        file_id: i64,
    ) -> Result<Option<ResearchSourceDocument>, sqlx::Error> {
        sqlx::query("SELECT id, filename, original_name AS title FROM files WHERE id = ?")
            .bind(file_id)
            .fetch_optional(self.db)
            .await
            .map(|row| row.map(source_document_from_row))
    }

    pub async fn load_source_document_by_filename(
        &self,
        filename: &str,
    ) -> Result<Option<ResearchSourceDocument>, sqlx::Error> {
        sqlx::query("SELECT id, filename, original_name AS title FROM files WHERE filename = ?")
            .bind(filename)
            .fetch_optional(self.db)
            .await
            .map(|row| row.map(source_document_from_row))
    }
}

pub struct SqliteTagRepository<'a> {
    db: &'a SqlitePool,
}

impl<'a> SqliteTagRepository<'a> {
    pub fn new(db: &'a SqlitePool) -> Self {
        Self { db }
    }

    pub async fn hydrate_file_tags(
        &self,
        file_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<TagInfo>>, sqlx::Error> {
        if file_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders = vec!["?"; file_ids.len()].join(",");
        let sql = format!(
            "SELECT file_tags.file_id, tags.id, tags.label, tags.slug, tags.kind, file_tags.source
             FROM file_tags
             JOIN tags ON tags.id = file_tags.tag_id
             WHERE file_tags.file_id IN ({placeholders})
             ORDER BY tags.kind, tags.label"
        );
        let mut query = sqlx::query(&sql);
        for file_id in file_ids {
            query = query.bind(file_id);
        }

        let rows = query.fetch_all(self.db).await?;
        let mut by_file: HashMap<i64, Vec<TagInfo>> = HashMap::new();
        for row in rows {
            let file_id: i64 = row.get("file_id");
            by_file.entry(file_id).or_default().push(TagInfo {
                id: row.get("id"),
                label: row.get("label"),
                slug: row.get("slug"),
                kind: row.get("kind"),
                source: row.get("source"),
            });
        }

        Ok(by_file)
    }

    pub async fn list_tags(&self) -> Result<Vec<TagInfo>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, label, slug, kind, '' AS source FROM tags ORDER BY kind, label",
        )
        .fetch_all(self.db)
        .await?;

        Ok(rows.into_iter().map(tag_info_from_row).collect())
    }

    pub async fn load_file_tags(&self, file_id: i64) -> Result<Vec<TagInfo>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT tags.id, tags.label, tags.slug, tags.kind, file_tags.source
             FROM file_tags
             JOIN tags ON tags.id = file_tags.tag_id
             WHERE file_tags.file_id = ?
             ORDER BY tags.kind, tags.label",
        )
        .bind(file_id)
        .fetch_all(self.db)
        .await?;

        Ok(rows.into_iter().map(tag_info_from_row).collect())
    }
}

pub struct NewEnginePresetRecord {
    pub name: String,
    pub engine_kind: String,
    pub provider: String,
    pub model: Option<String>,
    pub command: Option<String>,
    pub args_json: Option<String>,
    pub base_url: Option<String>,
    pub runtime_profile: Option<String>,
    pub default_intensity: String,
    pub web_search_enabled: String,
    pub fallback_execution: String,
    pub install_hint: Option<String>,
    pub limits_json: Option<String>,
    pub allowed_tools_json: Option<String>,
    pub allowed_skills_json: Option<String>,
}

pub struct SqliteEnginePresetRepository<'a> {
    db: &'a SqlitePool,
}

impl<'a> SqliteEnginePresetRepository<'a> {
    pub fn new(db: &'a SqlitePool) -> Self {
        Self { db }
    }

    pub async fn list_custom_presets<T>(&self) -> Result<Vec<T>, sqlx::Error>
    where
        for<'r> T: FromRow<'r, SqliteRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(
            "SELECT * FROM engine_presets WHERE is_default != 'true' ORDER BY id ASC",
        )
        .fetch_all(self.db)
        .await
    }

    pub async fn load_custom_preset_by_id<T>(&self, id: i64) -> Result<Option<T>, sqlx::Error>
    where
        for<'r> T: FromRow<'r, SqliteRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>("SELECT * FROM engine_presets WHERE id = ? AND is_default != 'true'")
            .bind(id)
            .fetch_optional(self.db)
            .await
    }

    pub async fn load_enabled_custom_preset_by_id<T>(
        &self,
        id: i64,
    ) -> Result<Option<T>, sqlx::Error>
    where
        for<'r> T: FromRow<'r, SqliteRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(
            "SELECT * FROM engine_presets WHERE id = ? AND enabled = 'true' AND is_default != 'true'",
        )
        .bind(id)
        .fetch_optional(self.db)
        .await
    }

    pub async fn insert_custom_preset(
        &self,
        record: &NewEnginePresetRecord,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query("INSERT INTO engine_presets (name, engine_kind, provider, model, command, args_json, base_url, runtime_profile, default_intensity, web_search_enabled, fallback_execution, enabled, is_default, install_hint, limits_json, allowed_tools_json, allowed_skills_json, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'true', 'false', ?, ?, ?, ?, CURRENT_TIMESTAMP)")
            .bind(&record.name)
            .bind(&record.engine_kind)
            .bind(&record.provider)
            .bind(&record.model)
            .bind(&record.command)
            .bind(&record.args_json)
            .bind(&record.base_url)
            .bind(&record.runtime_profile)
            .bind(&record.default_intensity)
            .bind(&record.web_search_enabled)
            .bind(&record.fallback_execution)
            .bind(&record.install_hint)
            .bind(&record.limits_json)
            .bind(&record.allowed_tools_json)
            .bind(&record.allowed_skills_json)
            .execute(self.db)
            .await
            .map(|result| result.last_insert_rowid())
    }

    pub async fn update_custom_preset_name_and_model(
        &self,
        id: i64,
        name: &str,
        model: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query("UPDATE engine_presets SET name = ?, model = ?, last_test_status = 'unverified', last_test_at = NULL, last_test_message = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND is_default != 'true'")
            .bind(name)
            .bind(model)
            .bind(id)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }

    pub async fn delete_custom_preset(&self, id: i64) -> Result<u64, sqlx::Error> {
        sqlx::query("DELETE FROM engine_presets WHERE id = ? AND is_default != 'true'")
            .bind(id)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }

    pub async fn record_test_result(
        &self,
        id: i64,
        status: &str,
        message: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query("UPDATE engine_presets SET last_test_status = ?, last_test_at = CURRENT_TIMESTAMP, last_test_message = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(status)
            .bind(message)
            .bind(id)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
    }
}

pub struct SqliteDocumentLinkRepository<'a> {
    db: &'a SqlitePool,
}

impl<'a> SqliteDocumentLinkRepository<'a> {
    pub fn new(db: &'a SqlitePool) -> Self {
        Self { db }
    }

    pub async fn load_relationships(
        &self,
        file_id: i64,
    ) -> Result<DocumentRelationships, sqlx::Error> {
        let sources = self.load_document_links(file_id, true).await?;
        let derivatives = self.load_document_links(file_id, false).await?;
        Ok(DocumentRelationships {
            sources,
            derivatives,
        })
    }

    pub async fn load_relationship_graph(
        &self,
        root: DocumentGraphNode,
        direction: &str,
    ) -> Result<DocumentRelationshipGraph, sqlx::Error> {
        let edges = self.load_document_graph_edges(root.id, direction).await?;
        let mut node_ids = vec![root.id];
        let mut seen = HashSet::from([root.id]);
        for edge in &edges {
            if seen.insert(edge.from_file_id) {
                node_ids.push(edge.from_file_id);
            }
            if seen.insert(edge.to_file_id) {
                node_ids.push(edge.to_file_id);
            }
        }

        let mut node_lookup = self.load_document_graph_nodes(&node_ids).await?;
        let mut nodes = Vec::with_capacity(node_ids.len());
        nodes.push(root.clone());
        node_lookup.remove(&root.id);
        for node_id in node_ids.into_iter().skip(1) {
            if let Some(node) = node_lookup.remove(&node_id) {
                nodes.push(node);
            }
        }

        Ok(DocumentRelationshipGraph { root, nodes, edges })
    }

    async fn load_document_links(
        &self,
        file_id: i64,
        sources: bool,
    ) -> Result<Vec<DocumentLinkInfo>, sqlx::Error> {
        let (file_column, linked_column) = if sources {
            ("from_file_id", "to_file_id")
        } else {
            ("to_file_id", "from_file_id")
        };
        let sql = format!(
            "SELECT document_links.id,
                    document_links.from_file_id,
                    document_links.to_file_id,
                    document_links.relation_type,
                    document_links.created_by_task_id,
                    document_links.created_at,
                    linked.id AS document_id,
                    linked.filename AS document_filename,
                    linked.original_name AS document_title
             FROM document_links
             JOIN files AS linked ON linked.id = document_links.{linked_column}
             WHERE document_links.{file_column} = ?
             ORDER BY document_links.created_at DESC, document_links.id DESC"
        );
        let rows = sqlx::query(&sql).bind(file_id).fetch_all(self.db).await?;
        Ok(rows
            .into_iter()
            .map(|row| DocumentLinkInfo {
                id: row.get("id"),
                from_file_id: row.get("from_file_id"),
                to_file_id: row.get("to_file_id"),
                relation_type: row.get("relation_type"),
                created_by_task_id: row.get("created_by_task_id"),
                created_at: row.get("created_at"),
                document: source_document_from_link_row(&row),
            })
            .collect())
    }

    async fn load_document_graph_edges(
        &self,
        root_id: i64,
        direction: &str,
    ) -> Result<Vec<DocumentGraphEdge>, sqlx::Error> {
        let where_clause = match direction {
            "sources" => "from_file_id = ?",
            "derivatives" => "to_file_id = ?",
            _ => "from_file_id = ? OR to_file_id = ?",
        };
        let sql = format!(
            "SELECT id, from_file_id, to_file_id, relation_type, created_by_task_id, created_at
             FROM document_links
             WHERE {where_clause}
             ORDER BY created_at DESC, id DESC"
        );
        let mut query = sqlx::query(&sql).bind(root_id);
        if direction != "sources" && direction != "derivatives" {
            query = query.bind(root_id);
        }
        let rows = query.fetch_all(self.db).await?;
        Ok(rows
            .into_iter()
            .map(|row| DocumentGraphEdge {
                id: row.get("id"),
                from_file_id: row.get("from_file_id"),
                to_file_id: row.get("to_file_id"),
                relation_type: row.get("relation_type"),
                created_by_task_id: row.get("created_by_task_id"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn load_document_graph_nodes(
        &self,
        node_ids: &[i64],
    ) -> Result<HashMap<i64, DocumentGraphNode>, sqlx::Error> {
        if node_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders = vec!["?"; node_ids.len()].join(",");
        let sql = format!(
            "SELECT id, filename, original_name AS title
             FROM files
             WHERE id IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for node_id in node_ids {
            query = query.bind(node_id);
        }

        let rows = query.fetch_all(self.db).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                (
                    id,
                    DocumentGraphNode {
                        id,
                        filename: row.get("filename"),
                        title: row.get("title"),
                    },
                )
            })
            .collect())
    }
}

pub struct SqliteWorkspaceStore<'a> {
    db: &'a SqlitePool,
}

impl<'a> SqliteWorkspaceStore<'a> {
    pub fn new(db: &'a SqlitePool) -> Self {
        Self { db }
    }
}

fn file_metadata_from_row(row: SqliteRow) -> FileMetadata {
    FileMetadata {
        id: row.get("id"),
        filename: row.get("filename"),
        original_name: row.get("original_name"),
        file_type: row.get("file_type"),
        status: row.get("status"),
        drawer_id: row.get("drawer_id"),
        uploaded_at: row.get("uploaded_at"),
    }
}

fn source_document_from_row(row: SqliteRow) -> ResearchSourceDocument {
    ResearchSourceDocument {
        id: row.get("id"),
        filename: row.get("filename"),
        title: row.get("title"),
    }
}

fn source_document_from_link_row(row: &SqliteRow) -> ResearchSourceDocument {
    ResearchSourceDocument {
        id: row.get("document_id"),
        filename: row.get("document_filename"),
        title: row.get("document_title"),
    }
}

fn tag_info_from_row(row: SqliteRow) -> TagInfo {
    TagInfo {
        id: row.get("id"),
        label: row.get("label"),
        slug: row.get("slug"),
        kind: row.get("kind"),
        source: row.get("source"),
    }
}

impl FileStore for SqliteFileRepository<'_> {
    type Error = sqlx::Error;

    fn list_files<'a>(&'a self) -> BoxFuture<'a, Result<Vec<FileMetadata>, Self::Error>> {
        Box::pin(async move { self.list_files().await })
    }

    fn find_file_id_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<i64>, Self::Error>> {
        Box::pin(async move { self.find_file_id_by_filename(filename).await })
    }

    fn load_file_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<FileMetadata>, Self::Error>> {
        Box::pin(async move { self.load_file_by_filename(filename).await })
    }

    fn load_source_document_by_id<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>> {
        Box::pin(async move { self.load_source_document_by_id(file_id).await })
    }

    fn load_source_document_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>> {
        Box::pin(async move { self.load_source_document_by_filename(filename).await })
    }
}

impl TagStore for SqliteTagRepository<'_> {
    type Error = sqlx::Error;

    fn list_tags<'a>(&'a self) -> BoxFuture<'a, Result<Vec<TagInfo>, Self::Error>> {
        Box::pin(async move { self.list_tags().await })
    }

    fn load_file_tags<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Vec<TagInfo>, Self::Error>> {
        Box::pin(async move { self.load_file_tags(file_id).await })
    }

    fn hydrate_file_tags<'a>(
        &'a self,
        items: &'a mut [FileListItem],
    ) -> BoxFuture<'a, Result<(), Self::Error>> {
        Box::pin(async move {
            let file_ids = items
                .iter()
                .map(|item| item.metadata.id)
                .collect::<Vec<_>>();
            let mut by_file = self.hydrate_file_tags(&file_ids).await?;
            for item in items {
                item.tags = by_file.remove(&item.metadata.id).unwrap_or_default();
            }
            Ok(())
        })
    }
}

impl DocumentLinkStore for SqliteDocumentLinkRepository<'_> {
    type Error = sqlx::Error;

    fn load_relationships<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<DocumentRelationships, Self::Error>> {
        Box::pin(async move { self.load_relationships(file_id).await })
    }

    fn load_relationship_graph<'a>(
        &'a self,
        root: DocumentGraphNode,
        direction: &'a str,
    ) -> BoxFuture<'a, Result<DocumentRelationshipGraph, Self::Error>> {
        Box::pin(async move { self.load_relationship_graph(root, direction).await })
    }
}

impl WorkspaceStore for SqliteWorkspaceStore<'_> {
    type Error = sqlx::Error;

    fn has_research_request<'a>(
        &'a self,
        file_id: i64,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<bool, Self::Error>> {
        Box::pin(async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM tasks
                 WHERE (file_id = ? OR filename = ?)
                   AND file_prefix IN ('[Research]', '[AI-Research]')",
            )
            .bind(file_id)
            .bind(filename)
            .fetch_one(self.db)
            .await
            .map(|count| count > 0)
        })
    }

    fn load_latest_research_request_task<'a>(
        &'a self,
        file_id: i64,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchRequestTask>, Self::Error>> {
        Box::pin(async move {
            sqlx::query(
                "SELECT id, original_name, created_at, user_prompt, source_file_ids, source_filenames,
                        research_type, research_mode, research_format, research_topic, research_instructions,
                        model, engine_preset_name, engine_kind, resolved_model, research_intensity,
                        quality_max_iterations, quality_depth, quality_status, quality_last_failure,
                        web_search_requested, web_search_provider, research_controller_artifacts_json,
                        research_source_diagnostics_json
                 FROM tasks
                 WHERE (file_id = ? OR filename = ?)
                   AND file_prefix IN ('[Research]', '[AI-Research]')
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
            )
            .bind(file_id)
            .bind(filename)
            .fetch_optional(self.db)
            .await
            .map(|row| {
                row.map(|row| ResearchRequestTask {
                    id: row.get("id"),
                    original_name: row.get("original_name"),
                    created_at: row.get("created_at"),
                    user_prompt: row.get("user_prompt"),
                    source_file_ids: row.get("source_file_ids"),
                    source_filenames: row.get("source_filenames"),
                    research_type: row.get("research_type"),
                    research_mode: row.get("research_mode"),
                    research_format: row.get("research_format"),
                    research_topic: row.get("research_topic"),
                    research_instructions: row.get("research_instructions"),
                    model: row.get("model"),
                    engine_preset_name: row.get("engine_preset_name"),
                    engine_kind: row.get("engine_kind"),
                    resolved_model: row.get("resolved_model"),
                    research_intensity: row.get("research_intensity"),
                    quality_max_iterations: row.get("quality_max_iterations"),
                    quality_depth: row.get("quality_depth"),
                    quality_status: row.get("quality_status"),
                    quality_last_failure: row.get("quality_last_failure"),
                    web_search_requested: row.get("web_search_requested"),
                    web_search_provider: row.get("web_search_provider"),
                    research_controller_artifacts_json: row.get("research_controller_artifacts_json"),
                    research_source_diagnostics_json: row.get("research_source_diagnostics_json"),
                })
            })
        })
    }

    fn load_task_document_link_context<'a>(
        &'a self,
        task_id: i64,
    ) -> BoxFuture<'a, Result<Option<TaskDocumentLinkContext>, Self::Error>> {
        Box::pin(async move {
            sqlx::query(
                "SELECT file_prefix, research_type, source_file_ids, source_filenames
                 FROM tasks
                 WHERE id = ?",
            )
            .bind(task_id)
            .fetch_optional(self.db)
            .await
            .map(|row| {
                row.map(|row| TaskDocumentLinkContext {
                    file_prefix: row.get("file_prefix"),
                    research_type: row.get("research_type"),
                    source_file_ids: row.get("source_file_ids"),
                    source_filenames: row.get("source_filenames"),
                })
            })
        })
    }

    fn load_source_document_by_id<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>> {
        Box::pin(async move {
            SqliteFileRepository::new(self.db)
                .load_source_document_by_id(file_id)
                .await
        })
    }

    fn load_source_document_by_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<ResearchSourceDocument>, Self::Error>> {
        Box::pin(async move {
            SqliteFileRepository::new(self.db)
                .load_source_document_by_filename(filename)
                .await
        })
    }

    fn file_id_exists<'a>(&'a self, file_id: i64) -> BoxFuture<'a, Result<bool, Self::Error>> {
        Box::pin(async move {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM files WHERE id = ?")
                .bind(file_id)
                .fetch_one(self.db)
                .await
                .map(|count| count > 0)
        })
    }

    fn file_id_for_filename<'a>(
        &'a self,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<Option<i64>, Self::Error>> {
        Box::pin(async move {
            sqlx::query_scalar::<_, i64>("SELECT id FROM files WHERE filename = ?")
                .bind(filename)
                .fetch_optional(self.db)
                .await
        })
    }

    fn load_relationships<'a>(
        &'a self,
        file_id: i64,
    ) -> BoxFuture<'a, Result<DocumentRelationships, Self::Error>> {
        Box::pin(async move {
            SqliteDocumentLinkRepository::new(self.db)
                .load_relationships(file_id)
                .await
        })
    }

    fn insert_document_link<'a>(
        &'a self,
        from_file_id: i64,
        to_file_id: i64,
        relation_type: &'a str,
        task_id: i64,
    ) -> BoxFuture<'a, Result<u64, Self::Error>> {
        Box::pin(async move {
            sqlx::query(
                "INSERT OR IGNORE INTO document_links
                 (from_file_id, to_file_id, relation_type, created_by_task_id)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(from_file_id)
            .bind(to_file_id)
            .bind(relation_type)
            .bind(task_id)
            .execute(self.db)
            .await
            .map(|result| result.rows_affected())
        })
    }
}

pub async fn setup_db(data_dir: &StdPath) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let db_path = data_dir.join("liquid.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());
    let db = SqlitePool::connect(&db_url).await?;

    sqlx::query("CREATE TABLE IF NOT EXISTS files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL, file_type TEXT NOT NULL DEFAULT 'md', status TEXT NOT NULL DEFAULT 'draft', drawer_id INTEGER NULL, uploaded_at DATETIME DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS drawers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, description TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS engine_presets (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, engine_kind TEXT NOT NULL, provider TEXT NOT NULL, model TEXT, command TEXT, args_json TEXT, base_url TEXT, runtime_profile TEXT, default_intensity TEXT NOT NULL DEFAULT 'medium', web_search_enabled TEXT NOT NULL DEFAULT 'false', fallback_execution TEXT NOT NULL DEFAULT 'false', enabled TEXT NOT NULL DEFAULT 'true', is_default TEXT NOT NULL DEFAULT 'false', install_hint TEXT, limits_json TEXT, allowed_tools_json TEXT, allowed_skills_json TEXT, last_test_status TEXT NOT NULL DEFAULT 'unverified', last_test_at DATETIME, last_test_message TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER, filename TEXT, original_name TEXT NOT NULL, status TEXT NOT NULL, error_message TEXT, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS tags (id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, kind TEXT NOT NULL CHECK(kind IN ('user','system')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS file_tags (file_id INTEGER NOT NULL, tag_id INTEGER NOT NULL, source TEXT NOT NULL CHECK(source IN ('user','system','migration','task')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(file_id, tag_id))").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS document_links (id INTEGER PRIMARY KEY AUTOINCREMENT, from_file_id INTEGER NOT NULL, to_file_id INTEGER NOT NULL, relation_type TEXT NOT NULL CHECK(relation_type IN ('derived_from','translated_from','followed_up_from','synthesized_from')), created_by_task_id INTEGER, created_at DATETIME DEFAULT CURRENT_TIMESTAMP, CHECK(from_file_id != to_file_id))").execute(&db).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_file_tags_tag_id ON file_tags(tag_id)")
        .execute(&db)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_file_tags_file_id ON file_tags(file_id)")
        .execute(&db)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tags_kind_slug ON tags(kind, slug)")
        .execute(&db)
        .await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_document_links_unique ON document_links(from_file_id, to_file_id, relation_type)")
        .execute(&db)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_document_links_from_file_id ON document_links(from_file_id)")
        .execute(&db)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_document_links_to_file_id ON document_links(to_file_id)",
    )
    .execute(&db)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_document_links_created_by_task_id ON document_links(created_by_task_id)")
        .execute(&db)
        .await?;

    ensure_column(&db, "files", "file_type", "TEXT NOT NULL DEFAULT 'md'").await?;
    ensure_column(&db, "files", "status", "TEXT NOT NULL DEFAULT 'draft'").await?;
    ensure_column(&db, "files", "drawer_id", "INTEGER NULL").await?;
    ensure_column(
        &db,
        "files",
        "uploaded_at",
        "DATETIME DEFAULT '1970-01-01 00:00:00'",
    )
    .await?;
    sqlx::query("UPDATE files SET file_type = 'md' WHERE file_type IS NULL OR file_type = ''")
        .execute(&db)
        .await?;
    sqlx::query("UPDATE files SET status = 'draft' WHERE status IS NULL OR status = ''")
        .execute(&db)
        .await?;
    sqlx::query("UPDATE files SET drawer_id = NULL WHERE status != 'published'")
        .execute(&db)
        .await?;
    sqlx::query("UPDATE files SET uploaded_at = CURRENT_TIMESTAMP WHERE uploaded_at IS NULL OR uploaded_at = ''").execute(&db).await?;

    ensure_column(&db, "tasks", "error_message", "TEXT").await?;
    ensure_column(&db, "tasks", "deleted_at", "DATETIME").await?;
    ensure_column(&db, "tasks", "model", "TEXT").await?;
    ensure_column(&db, "tasks", "system_prompt", "TEXT").await?;
    ensure_column(&db, "tasks", "user_prompt", "TEXT").await?;
    ensure_column(&db, "tasks", "source_file_ids", "TEXT").await?;
    ensure_column(&db, "tasks", "source_filenames", "TEXT").await?;
    ensure_column(&db, "tasks", "file_prefix", "TEXT").await?;
    ensure_column(&db, "tasks", "file_type", "TEXT").await?;
    ensure_column(&db, "tasks", "cleanup_files", "TEXT").await?;
    ensure_column(&db, "tasks", "research_type", "TEXT").await?;
    ensure_column(&db, "tasks", "research_mode", "TEXT").await?;
    ensure_column(&db, "tasks", "research_format", "TEXT").await?;
    ensure_column(&db, "tasks", "research_topic", "TEXT").await?;
    ensure_column(&db, "tasks", "research_instructions", "TEXT").await?;
    ensure_column(&db, "tasks", "prompt_version", "TEXT").await?;
    ensure_column(&db, "tasks", "resolved_system_prompt", "TEXT").await?;
    ensure_column(&db, "tasks", "resolved_user_prompt", "TEXT").await?;
    ensure_column(&db, "tasks", "web_search_requested", "TEXT").await?;
    ensure_column(&db, "tasks", "web_search_provider", "TEXT").await?;
    ensure_column(&db, "tasks", "engine_preset_id", "INTEGER").await?;
    ensure_column(&db, "tasks", "engine_preset_name", "TEXT").await?;
    ensure_column(&db, "tasks", "engine_kind", "TEXT").await?;
    ensure_column(&db, "tasks", "resolved_model", "TEXT").await?;
    ensure_column(&db, "tasks", "research_intensity", "TEXT").await?;
    ensure_column(&db, "tasks", "fallback_used", "TEXT").await?;
    ensure_column(&db, "tasks", "fallback_reason", "TEXT").await?;
    ensure_column(&db, "tasks", "quality_current_iteration", "INTEGER").await?;
    ensure_column(&db, "tasks", "quality_max_iterations", "INTEGER").await?;
    ensure_column(&db, "tasks", "quality_status", "TEXT").await?;
    ensure_column(&db, "tasks", "quality_depth", "TEXT").await?;
    ensure_column(&db, "tasks", "quality_last_failure", "TEXT").await?;
    ensure_column(&db, "tasks", "research_controller_stage", "TEXT").await?;
    ensure_column(&db, "tasks", "research_controller_iteration", "INTEGER").await?;
    ensure_column(
        &db,
        "tasks",
        "research_controller_max_iterations",
        "INTEGER",
    )
    .await?;
    ensure_column(&db, "tasks", "research_controller_artifacts_json", "TEXT").await?;
    ensure_column(&db, "tasks", "research_source_diagnostics_json", "TEXT").await?;

    sqlx::query("UPDATE tasks SET status = 'interrupted' WHERE status IN ('processing', 'translating', 'researching', 'scraping')").execute(&db).await?;
    migrate_legacy_title_markers(&db).await?;
    migrate_task_metadata_tags(&db).await?;
    migrate_research_system_tag_aliases(&db).await?;
    backfill_document_links(&db).await?;

    Ok(db)
}

pub async fn ensure_tag_id(db: &SqlitePool, label: &str, kind: &str) -> Result<i64, sqlx::Error> {
    let Some(slug) = normalize_tag_slug(label) else {
        return Err(sqlx::Error::Protocol("empty tag slug".to_string()));
    };

    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM tags WHERE slug = ? AND kind = ?")
            .bind(&slug)
            .bind(kind)
            .fetch_optional(db)
            .await?
    {
        return Ok(id);
    }

    let existing_kind = sqlx::query_scalar::<_, String>("SELECT kind FROM tags WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(db)
        .await?;
    let insert_slug = if existing_kind
        .as_deref()
        .is_some_and(|existing| existing != kind)
    {
        format!("{kind}-{slug}")
    } else {
        slug
    };

    sqlx::query("INSERT OR IGNORE INTO tags (label, slug, kind) VALUES (?, ?, ?)")
        .bind(label)
        .bind(&insert_slug)
        .bind(kind)
        .execute(db)
        .await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM tags WHERE slug = ? AND kind = ?")
        .bind(insert_slug)
        .bind(kind)
        .fetch_one(db)
        .await
}

async fn ensure_system_tag(db: &SqlitePool, label: &str) -> Result<i64, sqlx::Error> {
    ensure_tag_id(db, label, "system").await
}

async fn migrate_legacy_title_markers(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query("SELECT id, original_name FROM files")
        .fetch_all(db)
        .await?;

    for row in rows {
        let file_id: i64 = row.get("id");
        let original_name: String = row.get("original_name");
        let (pure_title, tag_labels) = strip_legacy_title_metadata(&original_name);

        if pure_title != original_name {
            sqlx::query("UPDATE files SET original_name = ? WHERE id = ?")
                .bind(&pure_title)
                .bind(file_id)
                .execute(db)
                .await?;
        }

        for label in tag_labels {
            let tag_id = ensure_system_tag(db, label).await?;
            sqlx::query(
                "INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, 'migration')",
            )
            .bind(file_id)
            .bind(tag_id)
            .execute(db)
            .await?;
        }
    }

    Ok(())
}

async fn migrate_task_metadata_tags(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT files.id AS file_id,
                tasks.file_prefix,
                tasks.quality_status,
                tasks.model,
                tasks.resolved_model,
                tasks.engine_kind
         FROM files
         JOIN tasks ON tasks.file_id = files.id OR tasks.filename = files.filename
         WHERE tasks.file_prefix IN ('[Research]', '[AI-Research]', '[Scrape]', '[Scrape+KO]', '[KO]')",
    )
    .fetch_all(db)
    .await?;

    for row in rows {
        let file_id: i64 = row.get("file_id");
        let file_prefix: Option<String> = row.get("file_prefix");
        let quality_status: Option<String> = row.get("quality_status");
        let model: Option<String> = row.get("model");
        let resolved_model: Option<String> = row.get("resolved_model");
        let engine_kind: Option<String> = row.get("engine_kind");
        let labels = system_tag_labels_for_task(
            file_prefix.as_deref().unwrap_or_default(),
            quality_status.as_deref(),
            model.as_deref(),
            resolved_model.as_deref(),
            engine_kind.as_deref(),
        );
        for label in labels {
            let tag_id = ensure_system_tag(db, &label).await?;
            sqlx::query(
                "INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, 'task')",
            )
            .bind(file_id)
            .bind(tag_id)
            .execute(db)
            .await?;
        }
    }

    Ok(())
}

async fn migrate_research_system_tag_aliases(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let research_tag_id = ensure_system_tag(db, "Research").await?;
    let ai_research_tag_id = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM tags WHERE label = 'AI Research' AND kind = 'system' LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    let Some(ai_research_tag_id) = ai_research_tag_id else {
        return Ok(());
    };

    let associations = sqlx::query("SELECT file_id, source FROM file_tags WHERE tag_id = ?")
        .bind(ai_research_tag_id)
        .fetch_all(db)
        .await?;
    for association in associations {
        let file_id: i64 = association.get("file_id");
        let source: String = association.get("source");
        sqlx::query("INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, ?)")
            .bind(file_id)
            .bind(research_tag_id)
            .bind(source)
            .execute(db)
            .await?;
    }

    sqlx::query("DELETE FROM file_tags WHERE tag_id = ?")
        .bind(ai_research_tag_id)
        .execute(db)
        .await?;
    sqlx::query(
        "DELETE FROM tags
         WHERE id = ?
           AND kind = 'system'
           AND NOT EXISTS (SELECT 1 FROM file_tags WHERE tag_id = tags.id)",
    )
    .bind(ai_research_tag_id)
    .execute(db)
    .await?;

    Ok(())
}

pub fn document_relation_type(
    file_prefix: Option<&str>,
    research_type: Option<&str>,
    source_count: usize,
) -> Option<&'static str> {
    liquid_workspace::document_relation_type(file_prefix, research_type, source_count)
}

pub async fn create_document_links_for_task_output(
    db: &SqlitePool,
    task_id: i64,
    output_file_id: i64,
) -> Result<usize, sqlx::Error> {
    liquid_workspace::create_document_links_for_task_output(
        &SqliteWorkspaceStore::new(db),
        task_id,
        output_file_id,
    )
    .await
}

async fn backfill_document_links(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT tasks.id AS task_id,
                COALESCE(output_by_id.id, output_by_filename.id) AS output_file_id
         FROM tasks
         LEFT JOIN files AS output_by_id ON output_by_id.id = tasks.file_id
         LEFT JOIN files AS output_by_filename ON output_by_filename.filename = tasks.filename
         WHERE tasks.status = 'completed'
           AND COALESCE(output_by_id.id, output_by_filename.id) IS NOT NULL",
    )
    .fetch_all(db)
    .await?;

    for row in rows {
        let task_id: i64 = row.get("task_id");
        let output_file_id: i64 = row.get("output_file_id");
        create_document_links_for_task_output(db, task_id, output_file_id).await?;
    }

    Ok(())
}

pub async fn ensure_column(
    db: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), sqlx::Error> {
    if table_has_column(db, table, column).await? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition);
    sqlx::query(&sql).execute(db).await?;
    Ok(())
}

pub async fn table_has_column(
    db: &SqlitePool,
    table: &str,
    column: &str,
) -> Result<bool, sqlx::Error> {
    let sql = format!("PRAGMA table_info({})", table);
    let rows = sqlx::query(&sql).fetch_all(db).await?;
    Ok(rows.iter().any(|row| {
        let name: String = row.get("name");
        name == column
    }))
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{schema_snapshot, table_exists, temp_test_dir};
    use sqlx::Connection;
    use std::fs as std_fs;

    #[tokio::test]
    async fn db_schema_snapshot_matches_current_startup_migrations() {
        let dir = temp_test_dir("schema-snapshot");
        let db = setup_db(&dir).await.unwrap();
        let snapshot = schema_snapshot(&db).await;

        assert_eq!(
            snapshot,
            vec![
                "document_links|id:INTEGER:0:NULL:1,from_file_id:INTEGER:1:NULL:0,to_file_id:INTEGER:1:NULL:0,relation_type:TEXT:1:NULL:0,created_by_task_id:INTEGER:0:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "drawers|id:INTEGER:0:NULL:1,name:TEXT:1:NULL:0,description:TEXT:0:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "engine_presets|id:INTEGER:0:NULL:1,name:TEXT:1:NULL:0,engine_kind:TEXT:1:NULL:0,provider:TEXT:1:NULL:0,model:TEXT:0:NULL:0,command:TEXT:0:NULL:0,args_json:TEXT:0:NULL:0,base_url:TEXT:0:NULL:0,runtime_profile:TEXT:0:NULL:0,default_intensity:TEXT:1:'medium':0,web_search_enabled:TEXT:1:'false':0,fallback_execution:TEXT:1:'false':0,enabled:TEXT:1:'true':0,is_default:TEXT:1:'false':0,install_hint:TEXT:0:NULL:0,limits_json:TEXT:0:NULL:0,allowed_tools_json:TEXT:0:NULL:0,allowed_skills_json:TEXT:0:NULL:0,last_test_status:TEXT:1:'unverified':0,last_test_at:DATETIME:0:NULL:0,last_test_message:TEXT:0:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0,updated_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "file_tags|file_id:INTEGER:1:NULL:1,tag_id:INTEGER:1:NULL:2,source:TEXT:1:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "files|id:INTEGER:0:NULL:1,filename:TEXT:1:NULL:0,original_name:TEXT:1:NULL:0,file_type:TEXT:1:'md':0,status:TEXT:1:'draft':0,drawer_id:INTEGER:0:NULL:0,uploaded_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "tags|id:INTEGER:0:NULL:1,label:TEXT:1:NULL:0,slug:TEXT:1:NULL:0,kind:TEXT:1:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0",
                "tasks|id:INTEGER:0:NULL:1,file_id:INTEGER:0:NULL:0,filename:TEXT:0:NULL:0,original_name:TEXT:1:NULL:0,status:TEXT:1:NULL:0,error_message:TEXT:0:NULL:0,created_at:DATETIME:0:CURRENT_TIMESTAMP:0,deleted_at:DATETIME:0:NULL:0,model:TEXT:0:NULL:0,system_prompt:TEXT:0:NULL:0,user_prompt:TEXT:0:NULL:0,source_file_ids:TEXT:0:NULL:0,source_filenames:TEXT:0:NULL:0,file_prefix:TEXT:0:NULL:0,file_type:TEXT:0:NULL:0,cleanup_files:TEXT:0:NULL:0,research_type:TEXT:0:NULL:0,research_mode:TEXT:0:NULL:0,research_format:TEXT:0:NULL:0,research_topic:TEXT:0:NULL:0,research_instructions:TEXT:0:NULL:0,prompt_version:TEXT:0:NULL:0,resolved_system_prompt:TEXT:0:NULL:0,resolved_user_prompt:TEXT:0:NULL:0,web_search_requested:TEXT:0:NULL:0,web_search_provider:TEXT:0:NULL:0,engine_preset_id:INTEGER:0:NULL:0,engine_preset_name:TEXT:0:NULL:0,engine_kind:TEXT:0:NULL:0,resolved_model:TEXT:0:NULL:0,research_intensity:TEXT:0:NULL:0,fallback_used:TEXT:0:NULL:0,fallback_reason:TEXT:0:NULL:0,quality_current_iteration:INTEGER:0:NULL:0,quality_max_iterations:INTEGER:0:NULL:0,quality_status:TEXT:0:NULL:0,quality_depth:TEXT:0:NULL:0,quality_last_failure:TEXT:0:NULL:0,research_controller_stage:TEXT:0:NULL:0,research_controller_iteration:INTEGER:0:NULL:0,research_controller_max_iterations:INTEGER:0:NULL:0,research_controller_artifacts_json:TEXT:0:NULL:0,research_source_diagnostics_json:TEXT:0:NULL:0",
            ]
        );

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_setup_db_empty_db_has_expected_columns() {
        let dir = temp_test_dir("empty-db");
        let db = setup_db(&dir).await.unwrap();

        assert!(table_exists(&db, "drawers").await);
        assert!(table_exists(&db, "document_links").await);
        assert!(table_exists(&db, "engine_presets").await);
        assert!(table_exists(&db, "tags").await);
        assert!(table_exists(&db, "file_tags").await);
        assert!(table_has_column(&db, "files", "file_type").await.unwrap());
        assert!(table_has_column(&db, "files", "status").await.unwrap());
        assert!(table_has_column(&db, "files", "drawer_id").await.unwrap());
        assert!(table_has_column(&db, "files", "uploaded_at").await.unwrap());
        assert!(table_has_column(&db, "tasks", "cleanup_files")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "source_file_ids")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "deleted_at").await.unwrap());
        assert!(table_has_column(&db, "tasks", "research_type")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "resolved_user_prompt")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "web_search_requested")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "web_search_provider")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "engine_preset_id")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "research_intensity")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "quality_current_iteration")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "quality_max_iterations")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "quality_status")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "quality_depth")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "quality_last_failure")
            .await
            .unwrap());
        assert!(table_has_column(&db, "tasks", "research_controller_stage")
            .await
            .unwrap());
        assert!(
            table_has_column(&db, "tasks", "research_controller_artifacts_json")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&db, "tasks", "research_source_diagnostics_json")
                .await
                .unwrap()
        );
        let preset_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_presets")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(preset_count, 0);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_setup_db_upgrades_main_shaped_db() {
        let dir = temp_test_dir("main-db");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db).await.unwrap();
        sqlx::query("INSERT INTO files (filename, original_name) VALUES ('legacy.md', 'Legacy')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER, filename TEXT, original_name TEXT NOT NULL, status TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&mut db).await.unwrap();
        sqlx::query("INSERT INTO tasks (original_name, status) VALUES ('Running', 'processing')")
            .execute(&mut db)
            .await
            .unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        let file = sqlx::query_as::<_, (String, String, Option<i64>)>(
            "SELECT file_type, status, drawer_id FROM files WHERE filename = 'legacy.md'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        let task = sqlx::query_as::<_, (String,)>(
            "SELECT status FROM tasks WHERE original_name = 'Running'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();

        assert_eq!(file.0, "md");
        assert_eq!(file.1, "draft");
        assert_eq!(file.2, None);
        assert_eq!(task.0, "interrupted");
        assert!(table_exists(&upgraded, "drawers").await);
        assert!(table_exists(&upgraded, "document_links").await);
        assert!(table_exists(&upgraded, "engine_presets").await);
        assert!(table_exists(&upgraded, "tags").await);
        assert!(table_exists(&upgraded, "file_tags").await);
        assert!(table_has_column(&upgraded, "files", "drawer_id")
            .await
            .unwrap());
        assert!(table_has_column(&upgraded, "tasks", "model").await.unwrap());
        assert!(table_has_column(&upgraded, "tasks", "deleted_at")
            .await
            .unwrap());
        assert!(table_has_column(&upgraded, "tasks", "source_file_ids")
            .await
            .unwrap());
        assert!(table_has_column(&upgraded, "tasks", "cleanup_files")
            .await
            .unwrap());
        assert!(table_has_column(&upgraded, "tasks", "research_type")
            .await
            .unwrap());
        assert!(
            table_has_column(&upgraded, "tasks", "resolved_system_prompt")
                .await
                .unwrap()
        );
        assert!(table_has_column(&upgraded, "tasks", "web_search_provider")
            .await
            .unwrap());
        assert!(table_has_column(&upgraded, "tasks", "engine_kind")
            .await
            .unwrap());
        assert!(
            table_has_column(&upgraded, "tasks", "quality_current_iteration")
                .await
                .unwrap()
        );
        assert!(table_has_column(&upgraded, "tasks", "quality_last_failure")
            .await
            .unwrap());
        assert!(
            table_has_column(&upgraded, "tasks", "research_controller_stage")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&upgraded, "tasks", "research_controller_artifacts_json")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&upgraded, "tasks", "research_source_diagnostics_json")
                .await
                .unwrap()
        );
        let preset_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM engine_presets WHERE is_default = 'true'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        assert_eq!(preset_count, 0);

        upgraded.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_setup_db_is_idempotent_for_current_shape() {
        let dir = temp_test_dir("current-db");
        let db = setup_db(&dir).await.unwrap();
        db.close().await;

        let reopened = setup_db(&dir).await.unwrap();
        assert!(table_exists(&reopened, "drawers").await);
        assert!(table_exists(&reopened, "document_links").await);
        assert!(table_exists(&reopened, "engine_presets").await);
        assert!(table_exists(&reopened, "tags").await);
        assert!(table_exists(&reopened, "file_tags").await);
        assert!(table_has_column(&reopened, "files", "drawer_id")
            .await
            .unwrap());
        assert!(table_has_column(&reopened, "files", "uploaded_at")
            .await
            .unwrap());
        assert!(table_has_column(&reopened, "tasks", "file_prefix")
            .await
            .unwrap());
        let preset_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_presets")
            .fetch_one(&reopened)
            .await
            .unwrap();
        assert_eq!(preset_count, 0);

        reopened.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_title_metadata_is_stripped_into_system_tags() {
        let (title, tags) =
            strip_legacy_title_metadata("[NO CONFIDENCE] [AI-Research] Market scan (재시도)");

        assert_eq!(title, "Market scan");
        assert_eq!(tags, vec!["Low Confidence", "Research", "Retry"]);
        assert_eq!(normalize_tag_slug("Research").as_deref(), Some("research"));
    }

    #[test]
    fn combined_scrape_translation_marker_maps_to_both_tags() {
        let (title, tags) = strip_legacy_title_metadata("[Scrape+KO] Zig release notes");

        assert_eq!(title, "Zig release notes");
        assert_eq!(tags, vec!["Scrape", "Translation"]);
    }

    #[tokio::test]
    async fn setup_db_migrates_legacy_title_markers_to_tags_without_user_tag_mutation() {
        let dir = temp_test_dir("legacy-tags");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db).await.unwrap();
        sqlx::query("INSERT INTO files (filename, original_name) VALUES ('research.md', '[NO CONFIDENCE] [AI-Research] Market scan')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, kind TEXT NOT NULL CHECK(kind IN ('user','system')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE file_tags (file_id INTEGER NOT NULL, tag_id INTEGER NOT NULL, source TEXT NOT NULL CHECK(source IN ('user','system','migration','task')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(file_id, tag_id))")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tags (label, slug, kind) VALUES ('Pinned', 'pinned', 'user')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO file_tags (file_id, tag_id, source) VALUES (1, 1, 'user')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER, filename TEXT, original_name TEXT NOT NULL, status TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&mut db).await.unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        let title = sqlx::query_scalar::<_, String>(
            "SELECT original_name FROM files WHERE filename = 'research.md'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        let tags = sqlx::query_as::<_, (String, String, String)>(
            "SELECT tags.label, tags.kind, file_tags.source
             FROM tags
             JOIN file_tags ON file_tags.tag_id = tags.id
             WHERE file_tags.file_id = 1
             ORDER BY tags.label",
        )
        .fetch_all(&upgraded)
        .await
        .unwrap();

        assert_eq!(title, "Market scan");
        assert_eq!(
            tags,
            vec![
                (
                    "Low Confidence".to_string(),
                    "system".to_string(),
                    "migration".to_string()
                ),
                ("Pinned".to_string(), "user".to_string(), "user".to_string()),
                (
                    "Research".to_string(),
                    "system".to_string(),
                    "migration".to_string()
                ),
            ]
        );

        upgraded.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn setup_db_backfills_task_metadata_tags_for_existing_pure_titles() {
        let dir = temp_test_dir("task-metadata-tags");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (filename, original_name) VALUES ('aurelian.md', '아우렐리아누스 복원 정책 분석')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER,
                filename TEXT,
                original_name TEXT NOT NULL,
                status TEXT NOT NULL,
                file_prefix TEXT,
                model TEXT,
                resolved_model TEXT,
                engine_kind TEXT,
                quality_status TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks
             (file_id, filename, original_name, status, file_prefix, model, resolved_model, engine_kind, quality_status)
             VALUES (1, 'aurelian.md', '아우렐리아누스 복원 정책 분석', 'completed', '[Research]', 'pi:gemma4:e4b', 'gemma4:e4b', 'pi_ollama', 'untrusted')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        let title = sqlx::query_scalar::<_, String>(
            "SELECT original_name FROM files WHERE filename = 'aurelian.md'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        let tags = sqlx::query_as::<_, (String, String, String)>(
            "SELECT tags.label, tags.kind, file_tags.source
             FROM tags
             JOIN file_tags ON file_tags.tag_id = tags.id
             WHERE file_tags.file_id = 1
             ORDER BY tags.label",
        )
        .fetch_all(&upgraded)
        .await
        .unwrap();

        assert_eq!(title, "아우렐리아누스 복원 정책 분석");
        assert_eq!(
            tags,
            vec![
                (
                    "Engine: pi_ollama".to_string(),
                    "system".to_string(),
                    "task".to_string()
                ),
                (
                    "Low Confidence".to_string(),
                    "system".to_string(),
                    "task".to_string()
                ),
                (
                    "Model: gemma4:e4b".to_string(),
                    "system".to_string(),
                    "task".to_string()
                ),
                (
                    "Research".to_string(),
                    "system".to_string(),
                    "task".to_string()
                ),
            ]
        );

        upgraded.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn setup_db_merges_ai_research_system_tag_into_research_without_touching_user_tags() {
        let dir = temp_test_dir("merge-ai-research-system-tag");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (filename, original_name) VALUES ('one.md', 'One'), ('two.md', 'Two'), ('three.md', 'Three')")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, kind TEXT NOT NULL CHECK(kind IN ('user','system')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE file_tags (file_id INTEGER NOT NULL, tag_id INTEGER NOT NULL, source TEXT NOT NULL CHECK(source IN ('user','system','migration','task')), created_at DATETIME DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(file_id, tag_id))")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tags (id, label, slug, kind) VALUES
             (1, 'AI Research', 'system-ai-research', 'system'),
             (2, 'Research', 'research', 'system'),
             (3, 'AI Research', 'ai-research', 'user')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO file_tags (file_id, tag_id, source) VALUES
             (1, 1, 'task'),
             (2, 1, 'migration'),
             (2, 2, 'task'),
             (3, 3, 'user')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER, filename TEXT, original_name TEXT NOT NULL, status TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&mut db)
            .await
            .unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        let system_ai_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tags WHERE label = 'AI Research' AND kind = 'system'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        let user_ai_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tags WHERE label = 'AI Research' AND kind = 'user'",
        )
        .fetch_one(&upgraded)
        .await
        .unwrap();
        let associations = sqlx::query_as::<_, (i64, String, String)>(
            "SELECT file_tags.file_id, tags.label, file_tags.source
             FROM file_tags
             JOIN tags ON tags.id = file_tags.tag_id
             ORDER BY file_tags.file_id, tags.kind, tags.label",
        )
        .fetch_all(&upgraded)
        .await
        .unwrap();

        assert_eq!(system_ai_count, 0);
        assert_eq!(user_ai_count, 1);
        assert_eq!(
            associations,
            vec![
                (1, "Research".to_string(), "task".to_string()),
                (2, "Research".to_string(), "task".to_string()),
                (3, "AI Research".to_string(), "user".to_string()),
            ]
        );

        upgraded.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ai_research_task_metadata_maps_to_research_system_tag() {
        let labels = system_tag_labels_for_task(
            "[AI-Research]",
            Some("trusted"),
            Some("cli:codex"),
            None,
            Some("cli"),
        );

        assert!(labels.contains(&"Research".to_string()));
        assert!(!labels.contains(&"AI Research".to_string()));
    }

    #[test]
    fn document_relation_type_maps_task_metadata() {
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
        assert_eq!(document_relation_type(Some("[Research]"), None, 0), None);
    }

    #[tokio::test]
    async fn setup_db_backfills_document_links_from_completed_tasks_idempotently() {
        let dir = temp_test_dir("document-link-backfill");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO files (id, filename, original_name) VALUES
             (1, 'source.md', 'Source'),
             (2, 'research.md', 'Research'),
             (3, 'ko.md', 'Translation'),
             (4, 'follow.md', 'Follow-up'),
             (5, 'synthesis.md', 'Synthesis'),
             (6, 'second.md', 'Second Source'),
             (7, 'draft.md', 'Draft')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER,
                filename TEXT,
                original_name TEXT NOT NULL,
                status TEXT NOT NULL,
                source_file_ids TEXT,
                source_filenames TEXT,
                file_prefix TEXT,
                research_type TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks
             (id, file_id, filename, original_name, status, source_file_ids, source_filenames, file_prefix, research_type)
             VALUES
             (1, 2, 'research.md', 'Research', 'completed', '[1]', '[\"source.md\"]', '[Research]', 'initial'),
             (2, 3, 'ko.md', 'Translation', 'completed', '[1]', '[\"source.md\"]', '[KO]', NULL),
             (3, 4, 'follow.md', 'Follow-up', 'completed', '[1]', '[\"source.md\"]', '[AI-Research]', 'follow_up'),
             (4, 5, 'synthesis.md', 'Synthesis', 'completed', '[1,6]', '[\"source.md\",\"second.md\"]', '[Research]', 'synthesis'),
             (5, 7, 'draft.md', 'Draft', 'queued', '[1]', '[\"source.md\"]', '[Research]', 'initial')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        upgraded.close().await;
        let reopened = setup_db(&dir).await.unwrap();
        let links = sqlx::query_as::<_, (i64, i64, String, Option<i64>)>(
            "SELECT from_file_id, to_file_id, relation_type, created_by_task_id
             FROM document_links
             ORDER BY from_file_id, to_file_id",
        )
        .fetch_all(&reopened)
        .await
        .unwrap();

        assert_eq!(
            links,
            vec![
                (2, 1, "derived_from".to_string(), Some(1)),
                (3, 1, "translated_from".to_string(), Some(2)),
                (4, 1, "followed_up_from".to_string(), Some(3)),
                (5, 1, "synthesized_from".to_string(), Some(4)),
                (5, 6, "synthesized_from".to_string(), Some(4)),
            ]
        );

        reopened.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn setup_db_backfills_document_links_with_filename_fallback_and_skips_self_links() {
        let dir = temp_test_dir("document-link-filename-fallback");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("liquid.db").to_string_lossy()
        );
        let mut db = sqlx::SqliteConnection::connect(&db_url).await.unwrap();
        sqlx::query("CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL UNIQUE, original_name TEXT NOT NULL)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO files (id, filename, original_name) VALUES
             (1, 'source.md', 'Source'),
             (2, 'research.md', 'Research')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER,
                filename TEXT,
                original_name TEXT NOT NULL,
                status TEXT NOT NULL,
                source_file_ids TEXT,
                source_filenames TEXT,
                file_prefix TEXT,
                research_type TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks
             (id, file_id, filename, original_name, status, source_file_ids, source_filenames, file_prefix, research_type)
             VALUES
             (1, 2, 'research.md', 'Research', 'completed', '[999,2]', '[\"source.md\",\"missing.md\"]', '[Research]', 'initial')",
        )
        .execute(&mut db)
        .await
        .unwrap();
        db.close().await.unwrap();

        let upgraded = setup_db(&dir).await.unwrap();
        let links = sqlx::query_as::<_, (i64, i64, String)>(
            "SELECT from_file_id, to_file_id, relation_type FROM document_links",
        )
        .fetch_all(&upgraded)
        .await
        .unwrap();

        assert_eq!(links, vec![(2, 1, "derived_from".to_string())]);

        upgraded.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }
}
