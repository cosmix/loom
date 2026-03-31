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
    complete_stage_ids_filtered(cwd, prefix, &[])
}

/// Complete stage IDs filtered by allowed statuses.
///
/// Reads stage files and parses YAML frontmatter to extract the `status:`
/// field. Only returns stages whose status matches one of `allowed_statuses`.
/// If `allowed_statuses` is empty, returns all stages (same as `complete_stage_ids`).
pub fn complete_stage_ids_filtered(
    cwd: &Path,
    prefix: &str,
    allowed_statuses: &[&str],
) -> Result<Vec<String>> {
    let stages_dir = cwd.join(".work/stages");

    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(stage_id) = crate::fs::stage_files::extract_stage_id(filename) {
                if !prefix.is_empty() && !stage_id.starts_with(prefix) {
                    continue;
                }

                if allowed_statuses.is_empty() || status_matches(&path, allowed_statuses) {
                    results.push(stage_id);
                }
            }
        }
    }

    results.sort();
    results.dedup();
    Ok(results)
}

/// Check if a stage file's status matches any of the allowed statuses.
///
/// Parses the YAML frontmatter to find a `status:` field.
fn status_matches(path: &Path, allowed_statuses: &[&str]) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Look for status in YAML frontmatter (between --- delimiters)
    let mut in_frontmatter = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_frontmatter {
                break; // End of frontmatter
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if let Some(value) = trimmed.strip_prefix("status:") {
                let status = value.trim().trim_matches('"').trim_matches('\'');
                return allowed_statuses.contains(&status);
            }
        }
    }

    false
}
