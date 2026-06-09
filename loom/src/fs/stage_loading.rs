//! Shared utilities for loading stage definitions from .work/stages/ files.
//!
//! On-disk `.work/stages/*.md` files carry a full serialized [`Stage`] in their
//! YAML frontmatter (written by `serialize_stage_to_markdown`). A
//! [`StageDefinition`] is a strict subset of those fields, so we deserialize the
//! frontmatter *directly* into a `StageDefinition` — serde ignores the runtime-only
//! keys (`status`, `created_at`, `merged`, …). This is deliberately NOT a partial
//! hand-rolled struct: an intermediate struct previously dropped `stage_type`,
//! `auto_merge`, `sandbox`, `context_budget`, and `before_stage`/`after_stage` on
//! every daemon restart (the loader prefers stage files over the plan).

use anyhow::{Context, Result};
use std::path::Path;

use crate::parser::frontmatter::parse_from_markdown;
use crate::plan::schema::StageDefinition;
use crate::validation::validate_id;

/// Deserialize a [`StageDefinition`] directly from a stage markdown file's YAML
/// frontmatter.
///
/// Every field a stage file can carry has a serde default on `StageDefinition`
/// (including `working_dir`, which falls back to `"."`), so older or partially
/// written stage files still load without error.
pub fn extract_stage_definition(content: &str) -> Result<StageDefinition> {
    parse_from_markdown(content, "StageDefinition")
}

/// Load stage definitions from .work/stages/ directory
pub fn load_stages_from_work_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
    let mut stages = Vec::new();

    for entry in std::fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Skip non-markdown files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the stage file
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Deserialize the full StageDefinition from frontmatter (lossless for
        // every field a StageDefinition carries — stage_type, auto_merge,
        // sandbox, context_budget, before/after_stage all survive).
        let stage_def = match extract_stage_definition(&content) {
            Ok(def) => {
                // Validate the stage ID before using it
                if let Err(e) = validate_id(&def.id) {
                    eprintln!("Warning: Invalid stage ID in {}: {}", path.display(), e);
                    continue;
                }
                def
            }
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                continue;
            }
        };

        stages.push(stage_def);
    }

    Ok(stages)
}
