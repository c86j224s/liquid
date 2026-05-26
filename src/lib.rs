mod ai_runtime;
mod app;
mod cli_launcher;
mod config;
mod db;
mod diagnostics;
mod drawers;
mod engine_presets;
mod files;
mod models;
mod pi_runtime;
mod research;
mod research_design;
mod research_quality;
mod research_sources;
mod scraping;
mod state;
pub mod tasks;
#[cfg(test)]
mod test_support;
mod translate;

pub use research_quality::{has_visible_final_answer_section, strip_research_artifact_blocks};
pub use tasks::{
    run_research_benchmark_case, ResearchBenchmarkCaseInput, ResearchBenchmarkCaseResult,
    ResearchBenchmarkMode,
};
