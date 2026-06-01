use crate::research_design::build_html_design_prompt;
use liquid_research_classic::{
    build_document_research_user_prompt as classic_build_document_research_user_prompt,
    build_follow_up_research_user_prompt as classic_build_follow_up_research_user_prompt,
    build_research_system_prompt as classic_build_research_system_prompt,
    build_topic_research_user_prompt as classic_build_topic_research_user_prompt,
    fallback_disclosure_prompt as classic_fallback_disclosure_prompt,
    intensity_prompt as classic_intensity_prompt,
    normalize_research_mode as classic_normalize_research_mode,
    normalize_research_type as classic_normalize_research_type,
    research_allows_web_search as classic_research_allows_web_search,
    research_controller_contract_prompt as classic_research_controller_contract_prompt,
    research_focus_section as classic_research_focus_section,
    research_mode_lens as classic_research_mode_lens,
    web_search_audit_prompt as classic_web_search_audit_prompt,
    web_search_provider_for as classic_web_search_provider_for,
};

pub(crate) fn research_allows_web_search(file_prefix: &str) -> bool {
    classic_research_allows_web_search(file_prefix)
}

pub(crate) fn web_search_provider_for(
    source: &str,
    model_name: &str,
    requested: bool,
) -> &'static str {
    classic_web_search_provider_for(source, model_name, requested)
}

pub(crate) fn web_search_audit_prompt(requested: bool, provider: &str) -> String {
    classic_web_search_audit_prompt(requested, provider)
}

pub(crate) fn intensity_prompt(intensity: &str) -> &'static str {
    classic_intensity_prompt(intensity)
}

pub(crate) fn research_controller_contract_prompt(format: &str) -> String {
    classic_research_controller_contract_prompt(format)
}

pub(crate) fn fallback_disclosure_prompt(reason: Option<&str>) -> String {
    classic_fallback_disclosure_prompt(reason)
}

pub(crate) fn normalize_research_mode(mode: &str) -> &'static str {
    classic_normalize_research_mode(mode)
}

pub(crate) fn normalize_research_type(
    research_type: Option<&str>,
    default_type: &str,
) -> &'static str {
    classic_normalize_research_type(research_type, default_type)
}

pub(crate) fn research_mode_lens(mode: &str) -> &'static str {
    classic_research_mode_lens(mode)
}

pub(crate) fn build_research_system_prompt(mode: &str, format: &str) -> String {
    let html_design_prompt = (format == "html").then(build_html_design_prompt);
    classic_build_research_system_prompt(mode, format, html_design_prompt.as_deref())
}

pub(crate) fn research_focus_section(instructions: Option<&str>) -> String {
    classic_research_focus_section(instructions)
}

pub(crate) fn build_document_research_user_prompt(instructions: Option<&str>) -> String {
    classic_build_document_research_user_prompt(instructions)
}

pub(crate) fn build_follow_up_research_user_prompt(instructions: Option<&str>) -> String {
    classic_build_follow_up_research_user_prompt(instructions)
}

pub(crate) fn build_topic_research_user_prompt(topic: &str, instructions: Option<&str>) -> String {
    classic_build_topic_research_user_prompt(topic, instructions)
}
