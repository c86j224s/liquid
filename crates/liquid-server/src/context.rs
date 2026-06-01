use axum::http::StatusCode;
use futures::future::BoxFuture;
use liquid_files::{DocumentRelationshipGraph, DocumentRelationships, FileListItem, TagInfo};
use liquid_protocol::{
    AppConfig, DrawerAssignment, DrawerInfo, DrawerPayload, EnginePreset,
    EnginePresetCreatePayload, EnginePresetUpdatePayload, ModelOption, MultiResearchRequest,
    RetryTaskPayload, ScrapRequest, TaskInfo, TaskUpdateEvent, TopicResearchRequest,
};
use liquid_workspace::ResearchRequestInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct StaticAssets {
    pub root_dir: PathBuf,
}

pub struct ServerContext {
    pub files: Arc<dyn FileService>,
    pub tasks: Arc<dyn TaskService>,
    pub research: Arc<dyn ResearchSubmissionService>,
    pub scraping: Arc<dyn ScrapeSubmissionService>,
    pub translation: Arc<dyn TranslationSubmissionService>,
    pub config: Arc<dyn ConfigViewService>,
    pub engine_presets: Arc<dyn EnginePresetService>,
    pub drawers: Arc<dyn DrawerService>,
    pub static_assets: StaticAssets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerServiceError {
    BadRequest,
    NotFound,
    Conflict,
    MethodNotAllowed,
    Storage,
}

pub fn service_error_status(error: ServerServiceError) -> StatusCode {
    match error {
        ServerServiceError::BadRequest => StatusCode::BAD_REQUEST,
        ServerServiceError::NotFound => StatusCode::NOT_FOUND,
        ServerServiceError::Conflict => StatusCode::CONFLICT,
        ServerServiceError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        ServerServiceError::Storage => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Debug, Clone)]
pub struct LoadedFileContent {
    pub file_type: String,
    pub content: String,
    pub has_research_request: bool,
}

#[derive(Debug)]
pub struct ResearchRequestPayload {
    pub info: ResearchRequestInfo,
    pub research_controller_artifacts_json: Option<String>,
    pub research_source_diagnostics_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub original_name: String,
    pub file_type: String,
}

#[derive(Debug, Clone)]
pub enum ScrapeSubmissionError {
    BadRequest(&'static str),
    Storage,
}

pub trait FileService: Send + Sync {
    fn list_files(&self) -> BoxFuture<'_, Result<Vec<FileListItem>, ServerServiceError>>;
    fn upload_file(
        &self,
        original_name: String,
        file_type: String,
        data: Vec<u8>,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn push_content(
        &self,
        title: String,
        content: String,
        status: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn update_status(
        &self,
        filename: String,
        status: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn update_title(
        &self,
        filename: String,
        title: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn update_file_metadata(
        &self,
        filename: String,
        title: String,
        tags: Vec<String>,
    ) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>>;
    fn list_tags(&self) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>>;
    fn update_tags(
        &self,
        filename: String,
        tags: Vec<String>,
    ) -> BoxFuture<'_, Result<Vec<TagInfo>, ServerServiceError>>;
    fn delete_file(&self, filename: String) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn load_renderable_content(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<LoadedFileContent, ServerServiceError>>;
    fn load_raw_content(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<String, ServerServiceError>>;
    fn load_research_request(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<ResearchRequestPayload, ServerServiceError>>;
    fn load_file_relationships(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<DocumentRelationships, ServerServiceError>>;
    fn load_relationship_graph(
        &self,
        filename: String,
        direction: String,
    ) -> BoxFuture<'_, Result<DocumentRelationshipGraph, ServerServiceError>>;
    fn search_files(
        &self,
        query: String,
    ) -> BoxFuture<'_, Result<Vec<FileListItem>, ServerServiceError>>;
    fn load_translation_target(
        &self,
        filename: String,
    ) -> BoxFuture<'_, Result<ModelSelection, ServerServiceError>>;
}

pub trait TaskService: Send + Sync {
    fn list_tasks(&self) -> BoxFuture<'_, Result<Vec<TaskInfo>, ServerServiceError>>;
    fn subscribe_updates(&self) -> broadcast::Receiver<TaskUpdateEvent>;
    fn retry_task(
        &self,
        id: i64,
        payload: RetryTaskPayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn delete_task(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>>;
}

pub trait ResearchSubmissionService: Send + Sync {
    fn submit_multi_research(
        &self,
        payload: MultiResearchRequest,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn submit_topic_research(
        &self,
        payload: TopicResearchRequest,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
}

pub trait ScrapeSubmissionService: Send + Sync {
    fn submit_scrape(
        &self,
        payload: ScrapRequest,
    ) -> BoxFuture<'_, Result<i64, ScrapeSubmissionError>>;
}

pub trait TranslationSubmissionService: Send + Sync {
    fn submit_translation(
        &self,
        filename: String,
        model: String,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
}

pub trait ConfigViewService: Send + Sync {
    fn app_config(&self) -> AppConfig;
    fn available_models(&self) -> BoxFuture<'_, Vec<ModelOption>>;
}

pub trait EnginePresetService: Send + Sync {
    fn list_engine_presets(&self) -> BoxFuture<'_, Result<Vec<EnginePreset>, ServerServiceError>>;
    fn create_engine_preset(
        &self,
        payload: EnginePresetCreatePayload,
    ) -> BoxFuture<'_, Result<i64, ServerServiceError>>;
    fn update_engine_preset(
        &self,
        id: i64,
        payload: EnginePresetUpdatePayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn delete_engine_preset(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn test_engine_preset(
        &self,
        id: i64,
    ) -> BoxFuture<'_, Result<(String, String), ServerServiceError>>;
}

pub trait DrawerService: Send + Sync {
    fn list_drawers(&self) -> BoxFuture<'_, Result<Vec<DrawerInfo>, ServerServiceError>>;
    fn create_drawer(
        &self,
        payload: DrawerPayload,
    ) -> BoxFuture<'_, Result<i64, ServerServiceError>>;
    fn update_drawer(
        &self,
        id: i64,
        payload: DrawerPayload,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn delete_drawer(&self, id: i64) -> BoxFuture<'_, Result<(), ServerServiceError>>;
    fn update_file_drawer(
        &self,
        filename: String,
        payload: DrawerAssignment,
    ) -> BoxFuture<'_, Result<(), ServerServiceError>>;
}
