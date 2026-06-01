//! Shared research artifact vocabulary.
//!
//! This crate owns stable artifact warning/detail tokens that are shared by
//! acquisition, research-quality checks, and the Liquid application. The tokens
//! are intentionally kept out of `liquid-protocol`: they describe scaffold and
//! repair policy in generated research artifacts, not the API wire grammar.

pub const PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING: &str =
    "pi_local_source_pack_provenance_source_cards_scaffolded";
pub const PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING: &str =
    "pi_local_source_pack_claim_log_repaired_from_visible_output";
pub const PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF: &str =
    "internal:pi_local_source_pack_scaffold";
pub const PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT: &str =
    "pre-collected source-pack provenance only";
pub const PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION: &str =
    "provenance only; no model-emitted claim linkage was available";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_local_source_pack_tokens_are_stable() {
        assert_eq!(
            PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_WARNING,
            "pi_local_source_pack_provenance_source_cards_scaffolded"
        );
        assert_eq!(
            PI_LOCAL_SOURCE_PACK_CLAIM_LOG_REPAIR_WARNING,
            "pi_local_source_pack_claim_log_repaired_from_visible_output"
        );
        assert_eq!(
            PI_LOCAL_SOURCE_PACK_SOURCE_CARD_SCAFFOLD_DIAGNOSTICS_REF,
            "internal:pi_local_source_pack_scaffold"
        );
        assert_eq!(
            PI_LOCAL_SOURCE_PACK_SCAFFOLD_EXTRACTED_FACT,
            "pre-collected source-pack provenance only"
        );
        assert_eq!(
            PI_LOCAL_SOURCE_PACK_SCAFFOLD_LIMITATION,
            "provenance only; no model-emitted claim linkage was available"
        );
    }
}
