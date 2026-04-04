//! Plan file completions for shell tab-completion.

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Complete plan file paths from doc/plans/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial filename prefix to filter results
///
/// # Returns
///
/// List of matching plan file paths
pub fn complete_plan_files(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let plans_dir = cwd.join("doc/plans");

    if !plans_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&plans_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Only include files we can form a relative path for
        if let Ok(rel_path) = path.strip_prefix(cwd) {
            let rel_str = rel_path.to_string_lossy().to_string();
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            // Match prefix against full relative path or just filename
            if prefix.is_empty() || rel_str.starts_with(prefix) || filename.starts_with(prefix) {
                results.push(rel_str);
            }
        }
    }

    results.sort();
    Ok(results)
}
