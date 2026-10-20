//! Knowledge-bootstrap recommendation for plans.

use std::path::Path;

use crate::fs::knowledge::KnowledgeDir;
use crate::plan::schema::types::StageDefinition;

/// Check for knowledge-related recommendations (non-fatal warnings)
///
/// Returns a warning when the plan has root stages (no dependencies) but no
/// knowledge-bootstrap stage, and the repository's knowledge tree is missing or
/// holds only the scaffold. `repo_root` is `None` when no repository root was
/// found; the check then cannot see a tree and still warns.
pub fn check_knowledge_recommendations(
    stages: &[StageDefinition],
    repo_root: Option<&Path>,
) -> Vec<String> {
    // Check if any stage has "knowledge" in its ID or name (case-insensitive),
    // but exclude knowledge-distill stages which are not knowledge-bootstrap stages.
    let has_knowledge_stage = stages.iter().any(|stage| {
        let id_lower = stage.id.to_lowercase();
        let name_lower = stage.name.to_lowercase();
        (id_lower.contains("knowledge") && !id_lower.contains("knowledge-distill"))
            || (name_lower.contains("knowledge")
                && !name_lower.contains("knowledge-distill")
                && !name_lower.contains("knowledge distill"))
    });

    let has_root_stages = stages.iter().any(|stage| stage.dependencies.is_empty());
    let tree_populated = repo_root.is_some_and(|root| KnowledgeDir::new(root).has_content());

    if has_root_stages && !has_knowledge_stage && !tree_populated {
        return vec![
            "Consider adding a 'knowledge-bootstrap' stage to capture codebase knowledge. \
             The repository's knowledge tree is missing or holds only the scaffold. \
             This stage can run first (no dependencies) to document entry points, patterns, \
             and conventions for subsequent stages."
                .to_string(),
        ];
    }
    Vec::new()
}
