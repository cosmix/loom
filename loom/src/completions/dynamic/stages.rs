//! Stage ID completions for shell tab-completion.

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Complete stage IDs from .work/stages/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial stage ID prefix to filter results
///
/// # Returns
///
/// List of matching stage IDs
pub fn complete_stage_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let stages_dir = cwd.join(".work/stages");

    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Extract stage ID using the stage_files module
            if let Some(stage_id) = crate::fs::stage_files::extract_stage_id(filename) {
                // Match against prefix
                if prefix.is_empty() || stage_id.starts_with(prefix) {
                    results.push(stage_id);
                }
            }
        }
    }

    results.sort();
    results.dedup(); // Remove duplicates in case of multiple matches
    Ok(results)
}
