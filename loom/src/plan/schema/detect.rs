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

    detect_stage_type_from_id_name(&stage_def.id, &stage_def.name)
}

/// Detect the stage type from an id and name alone, using the same id/name
/// pattern heuristics as [`detect_stage_type`].
///
/// This is the heuristic half of detection — it does NOT consider an explicit
/// `stage_type` field (callers that have a full definition should use
/// [`detect_stage_type`]). It exists so that paths which only have a stage id
/// and name (e.g. reconstructing a stage from a graph node when its file is
/// missing) can still classify a knowledge / knowledge-distill / integration-verify
/// stage correctly instead of defaulting everything to Standard.
pub fn detect_stage_type_from_id_name(id: &str, name: &str) -> StageType {
    let id_lower = id.to_lowercase();
    let name_lower = name.to_lowercase();

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
