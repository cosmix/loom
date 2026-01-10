//! Session ID completions for shell tab-completion.

use anyhow::Result;
use std::fs;
use std::path::Path;

use super::stages::complete_stage_ids;

/// Complete session IDs from .work/sessions/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial session ID prefix to filter results
///
/// # Returns
///
/// List of matching session IDs
pub fn complete_session_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let sessions_dir = cwd.join(".work/sessions");

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Session ID is the filename stem (without .md extension)
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // Match against prefix
            if prefix.is_empty() || stem.starts_with(prefix) {
                results.push(stem.to_string());
            }
        }
    }

    results.sort();
    Ok(results)
}

/// Complete both stage and session IDs
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial ID prefix to filter results
///
/// # Returns
///
/// Combined list of matching stage and session IDs
pub fn complete_stage_or_session_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let mut results = Vec::new();

    results.extend(complete_stage_ids(cwd, prefix)?);
    results.extend(complete_session_ids(cwd, prefix)?);

    results.sort();
    results.dedup(); // Remove duplicates if a stage and session share an ID
    Ok(results)
}
