//! Stage type detection from plan definitions.

use super::types::{StageDefinition, StageType};

/// Detect the stage type from a definition.
///
/// Uses explicit `stage_type` field if set, otherwise falls back to
/// detecting stage type based on ID or name patterns (case-insensitive):
/// - "knowledge-distill" -> KnowledgeDistill (checked first)
/// - "knowledge" -> Knowledge
/// - "integration-verify" or "integration verify" -> IntegrationVerify
pub fn detect_stage_type(stage_def: &StageDefinition) -> StageType {
    if stage_def.stage_type != StageType::Standard {
        return stage_def.stage_type;
    }

    let id_lower = stage_def.id.to_lowercase();
    let name_lower = stage_def.name.to_lowercase();

    if id_lower.contains("knowledge-distill")
        || name_lower.contains("knowledge-distill")
        || name_lower.contains("knowledge distill")
    {
        return StageType::KnowledgeDistill;
    }

    if id_lower.contains("knowledge") || name_lower.contains("knowledge") {
        return StageType::Knowledge;
    }

    if id_lower.contains("integration-verify")
        || name_lower.contains("integration-verify")
        || name_lower.contains("integration verify")
    {
        return StageType::IntegrationVerify;
    }

    StageType::Standard
}
