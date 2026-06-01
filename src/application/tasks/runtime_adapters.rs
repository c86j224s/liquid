use super::*;

pub(super) struct SqliteTaskRepository<'a> {
    pub(super) db: &'a sqlx::SqlitePool,
}

impl<'a> SqliteTaskRepository<'a> {
    pub(super) fn new(db: &'a sqlx::SqlitePool) -> Self {
        Self { db }
    }
}

impl TaskRepository for SqliteTaskRepository<'_> {
    fn list_recent_active<'a>(
        &'a self,
    ) -> futures::future::BoxFuture<'a, Result<Vec<TaskInfo>, sqlx::Error>> {
        Box::pin(async move {
            sqlx::query_as::<_, TaskInfo>(
                "SELECT * FROM tasks WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 30",
            )
            .fetch_all(self.db)
            .await
        })
    }

    fn load_active_task<'a>(
        &'a self,
        id: i64,
    ) -> futures::future::BoxFuture<'a, Result<Option<TaskInfo>, sqlx::Error>> {
        Box::pin(async move {
            sqlx::query_as::<_, TaskInfo>("SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL")
                .bind(id)
                .fetch_optional(self.db)
                .await
        })
    }
}

pub(super) struct AppModelRuntime<'a> {
    pub(super) state: &'a AppState,
}

impl<'a> AppModelRuntime<'a> {
    pub(super) fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl ModelRuntime for AppModelRuntime<'_> {
    fn execute<'a>(
        &'a self,
        request: ModelRuntimeRequest<'a>,
    ) -> futures::future::BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            execute_task_logic(
                self.state,
                request.task_id,
                request.filenames,
                request.model_name,
                request.source,
                request.system_prompt,
                request.user_prompt,
                request.research_subject_prompt,
                request.file_prefix,
                request.web_search_requested,
                request.web_search_provider_override,
                request.research_intensity,
                request.fallback_used,
                request.fallback_reason,
            )
            .await
        })
    }
}

pub(super) struct DefaultArtifactProcessor;

impl ArtifactFinalizer for DefaultArtifactProcessor {
    fn finalize(
        &self,
        draft: &str,
        artifacts: &ResearchControllerArtifacts,
        diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
        context: &ResearchQualityContext<'_>,
    ) -> liquid_research_core::ResearchFinalizationResult {
        finalize_research_output(draft, artifacts, diagnostics, context)
    }
}

impl ArtifactValidator for DefaultArtifactProcessor {
    fn validate_output(
        &self,
        output: &str,
        context: &ResearchQualityContext<'_>,
    ) -> Result<(), String> {
        validate_research_output(output, context).map(|_| ())
    }

    fn validate_artifacts(
        &self,
        artifacts: &ResearchControllerArtifacts,
        research_intensity: Option<&str>,
        quality_depth: Option<&str>,
    ) -> Result<(), Vec<String>> {
        validate_research_artifacts(artifacts, research_intensity, quality_depth)
    }
}

pub(super) struct DefaultSourceAcquisition;

impl SourceAcquisition for DefaultSourceAcquisition {
    fn collect_repair_search_hints<'a>(
        &'a self,
        subject: &'a str,
        queries: &'a [String],
        known_urls: &'a std::collections::HashSet<String>,
    ) -> futures::future::BoxFuture<'a, Vec<RepairSearchHint>> {
        Box::pin(async move {
            collect_transient_repair_search_hints(subject, queries, known_urls).await
        })
    }

    fn scrape_to_markdown<'a>(
        &'a self,
        url: &'a str,
        references: &'a [String],
    ) -> futures::future::BoxFuture<'a, Result<ScrapeResult, crate::contracts::ScrapeDiagnostics>>
    {
        Box::pin(async move { scrape_url_to_markdown(url, references).await })
    }
}
