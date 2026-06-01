use crate::application::scraping::ScrapeResult;
use crate::contracts::{
    ResearchContextPackingDiagnostics, ResearchControllerArtifacts,
    ResearchSourceDiagnosticsEnvelope, ResearchSourcePackReport, ScrapeDiagnostics, TaskInfo,
};
use futures::future::BoxFuture;
use liquid_acquisition::RepairSearchHint;
use liquid_research_classic::{
    build_document_research_user_prompt as classic_build_document_research_user_prompt,
    build_follow_up_research_user_prompt as classic_build_follow_up_research_user_prompt,
    build_research_context_pack as classic_build_research_context_pack,
    build_research_system_prompt as classic_build_research_system_prompt,
    build_topic_research_user_prompt as classic_build_topic_research_user_prompt,
    fallback_disclosure_prompt as classic_fallback_disclosure_prompt,
    intensity_prompt as classic_intensity_prompt,
    normalize_research_mode as classic_normalize_research_mode,
    normalize_research_type as classic_normalize_research_type,
    raw_fallback_context_diagnostics as classic_raw_fallback_context_diagnostics,
    research_allows_web_search as classic_research_allows_web_search,
    research_focus_section as classic_research_focus_section,
    web_search_audit_prompt as classic_web_search_audit_prompt,
    web_search_provider_for as classic_web_search_provider_for, ClassicResearchContextPackOptions,
    ClassicResearchImplementation, CLASSIC_RESEARCH_IMPLEMENTATION_ID,
};
use liquid_research_core::{ResearchFinalizationResult, ResearchQualityContext};
use std::sync::Arc;

pub(crate) trait TaskRepository {
    fn list_recent_active<'a>(&'a self) -> BoxFuture<'a, Result<Vec<TaskInfo>, sqlx::Error>>;
    fn load_active_task<'a>(
        &'a self,
        id: i64,
    ) -> BoxFuture<'a, Result<Option<TaskInfo>, sqlx::Error>>;
}

pub(crate) trait ResearchDiagnosticsRepository {
    fn load<'a>(&'a self, task_id: i64)
        -> BoxFuture<'a, Option<ResearchSourceDiagnosticsEnvelope>>;
    fn persist<'a>(
        &'a self,
        task_id: i64,
        diagnostics: ResearchSourceDiagnosticsEnvelope,
    ) -> BoxFuture<'a, Result<(), sqlx::Error>>;
}

pub(crate) trait SourcePackBuilder {
    fn build<'a>(
        &'a self,
        user_prompt: &'a str,
        source_documents: Option<&'a str>,
    ) -> BoxFuture<'a, ResearchSourcePackReport>;
}

pub(crate) trait SourceAcquisition {
    fn collect_repair_search_hints<'a>(
        &'a self,
        subject: &'a str,
        queries: &'a [String],
        known_urls: &'a std::collections::HashSet<String>,
    ) -> BoxFuture<'a, Vec<RepairSearchHint>>;
    fn scrape_to_markdown<'a>(
        &'a self,
        url: &'a str,
        references: &'a [String],
    ) -> BoxFuture<'a, Result<ScrapeResult, ScrapeDiagnostics>>;
}

pub(crate) struct ModelRuntimeRequest<'a> {
    pub(crate) task_id: i64,
    pub(crate) filenames: Vec<String>,
    pub(crate) model_name: &'a str,
    pub(crate) source: &'a str,
    pub(crate) system_prompt: &'a str,
    pub(crate) user_prompt: &'a str,
    pub(crate) research_subject_prompt: Option<&'a str>,
    pub(crate) file_prefix: &'a str,
    pub(crate) web_search_requested: Option<&'a str>,
    pub(crate) web_search_provider_override: Option<&'a str>,
    pub(crate) research_intensity: Option<&'a str>,
    pub(crate) fallback_used: bool,
    pub(crate) fallback_reason: Option<&'a str>,
}

pub(crate) trait ModelRuntime {
    fn execute<'a>(&'a self, request: ModelRuntimeRequest<'a>) -> BoxFuture<'a, Option<String>>;
}

pub(crate) trait ArtifactFinalizer {
    fn finalize(
        &self,
        draft: &str,
        artifacts: &ResearchControllerArtifacts,
        diagnostics: Option<&ResearchSourceDiagnosticsEnvelope>,
        context: &ResearchQualityContext<'_>,
    ) -> ResearchFinalizationResult;
}

pub(crate) trait ArtifactValidator {
    fn validate_output(
        &self,
        output: &str,
        context: &ResearchQualityContext<'_>,
    ) -> Result<(), String>;
    fn validate_artifacts(
        &self,
        artifacts: &ResearchControllerArtifacts,
        research_intensity: Option<&str>,
        quality_depth: Option<&str>,
    ) -> Result<(), Vec<String>>;
}

pub(crate) struct ResearchContextPackRequest<'a> {
    pub(crate) user_prompt: &'a str,
    pub(crate) raw_documents: &'a str,
    pub(crate) artifacts: &'a ResearchControllerArtifacts,
    pub(crate) diagnostics: Option<&'a ResearchSourceDiagnosticsEnvelope>,
    pub(crate) source_card_limit: usize,
    pub(crate) excerpt_chars: usize,
    pub(crate) prompt_safe_ledger_text_chars: usize,
    pub(crate) prompt_safe_ledger_list_items: usize,
    pub(crate) historical_narrative_state_prompt_hint: &'a str,
    pub(crate) historical_reader_quality_prompt_hint: &'a str,
}

pub(crate) trait ResearchImplementation: Send + Sync {
    fn id(&self) -> &'static str;
    fn research_allows_web_search(&self, file_prefix: &str) -> bool;
    fn web_search_provider_for(
        &self,
        source: &str,
        model_name: &str,
        requested: bool,
    ) -> &'static str;
    fn web_search_audit_prompt(&self, requested: bool, provider: &str) -> String;
    fn intensity_prompt(&self, intensity: &str) -> &'static str;
    fn fallback_disclosure_prompt(&self, reason: Option<&str>) -> String;
    fn normalize_research_mode(&self, mode: &str) -> &'static str;
    fn normalize_research_type(
        &self,
        research_type: Option<&str>,
        default_type: &str,
    ) -> &'static str;
    fn build_research_system_prompt(
        &self,
        mode: &str,
        format: &str,
        html_design_prompt: Option<&str>,
    ) -> String;
    fn research_focus_section(&self, instructions: Option<&str>) -> String;
    fn build_document_research_user_prompt(&self, instructions: Option<&str>) -> String;
    fn build_follow_up_research_user_prompt(&self, instructions: Option<&str>) -> String;
    fn build_topic_research_user_prompt(&self, topic: &str, instructions: Option<&str>) -> String;
    fn build_research_context_pack(
        &self,
        request: ResearchContextPackRequest<'_>,
    ) -> (String, ResearchContextPackingDiagnostics);
    fn raw_fallback_context_diagnostics(
        &self,
        raw_documents: &str,
    ) -> ResearchContextPackingDiagnostics;
}

pub(crate) fn classic_research_implementation() -> Arc<dyn ResearchImplementation> {
    Arc::new(ClassicResearchImplementation)
}

impl ResearchImplementation for ClassicResearchImplementation {
    fn id(&self) -> &'static str {
        CLASSIC_RESEARCH_IMPLEMENTATION_ID
    }

    fn research_allows_web_search(&self, file_prefix: &str) -> bool {
        classic_research_allows_web_search(file_prefix)
    }

    fn web_search_provider_for(
        &self,
        source: &str,
        model_name: &str,
        requested: bool,
    ) -> &'static str {
        classic_web_search_provider_for(source, model_name, requested)
    }

    fn web_search_audit_prompt(&self, requested: bool, provider: &str) -> String {
        classic_web_search_audit_prompt(requested, provider)
    }

    fn intensity_prompt(&self, intensity: &str) -> &'static str {
        classic_intensity_prompt(intensity)
    }

    fn fallback_disclosure_prompt(&self, reason: Option<&str>) -> String {
        classic_fallback_disclosure_prompt(reason)
    }

    fn normalize_research_mode(&self, mode: &str) -> &'static str {
        classic_normalize_research_mode(mode)
    }

    fn normalize_research_type(
        &self,
        research_type: Option<&str>,
        default_type: &str,
    ) -> &'static str {
        classic_normalize_research_type(research_type, default_type)
    }

    fn build_research_system_prompt(
        &self,
        mode: &str,
        format: &str,
        html_design_prompt: Option<&str>,
    ) -> String {
        classic_build_research_system_prompt(mode, format, html_design_prompt)
    }

    fn research_focus_section(&self, instructions: Option<&str>) -> String {
        classic_research_focus_section(instructions)
    }

    fn build_document_research_user_prompt(&self, instructions: Option<&str>) -> String {
        classic_build_document_research_user_prompt(instructions)
    }

    fn build_follow_up_research_user_prompt(&self, instructions: Option<&str>) -> String {
        classic_build_follow_up_research_user_prompt(instructions)
    }

    fn build_topic_research_user_prompt(&self, topic: &str, instructions: Option<&str>) -> String {
        classic_build_topic_research_user_prompt(topic, instructions)
    }

    fn build_research_context_pack(
        &self,
        request: ResearchContextPackRequest<'_>,
    ) -> (String, ResearchContextPackingDiagnostics) {
        classic_build_research_context_pack(
            request.user_prompt,
            request.raw_documents,
            request.artifacts,
            request.diagnostics,
            ClassicResearchContextPackOptions {
                source_card_limit: request.source_card_limit,
                excerpt_chars: request.excerpt_chars,
                prompt_safe_ledger_text_chars: request.prompt_safe_ledger_text_chars,
                prompt_safe_ledger_list_items: request.prompt_safe_ledger_list_items,
                historical_narrative_state_prompt_hint: request
                    .historical_narrative_state_prompt_hint,
                historical_reader_quality_prompt_hint: request
                    .historical_reader_quality_prompt_hint,
            },
        )
    }

    fn raw_fallback_context_diagnostics(
        &self,
        raw_documents: &str,
    ) -> ResearchContextPackingDiagnostics {
        classic_raw_fallback_context_diagnostics(raw_documents)
    }
}
