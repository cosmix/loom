//! Stage file naming and lookup utilities
//!
//! Stage files are named with a topological depth prefix for human readability:
//! - `01-core-architecture.md` (depth 0, no dependencies)
//! - `02-math-core.md` (depth 1, depends on depth-0 stages)
//! - `02-ui-framework.md` (depth 1, parallel to math-core)
//! - `03-canvas-enhancements.md` (depth 2, depends on depth-1 stages)
//!
//! This module provides utilities for:
//! - Finding stage files by ID (regardless of prefix)
//! - Computing topological depth for stages
//! - Generating consistent stage filenames

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Find a stage file by stage ID, regardless of its depth prefix.
///
/// Searches for files matching the pattern `*-{stage_id}.md` or `{stage_id}.md`
/// in the stages directory.
///
/// # Arguments
/// * `stages_dir` - Path to the `.work/stages/` directory
/// * `stage_id` - The stage ID to find
///
/// # Returns
/// The path to the stage file if found, None otherwise
pub fn find_stage_file(stages_dir: &Path, stage_id: &str) -> Result<Option<PathBuf>> {
    // Defensive validation - stage_id should already be validated at entry points,
    // but verify here for defense in depth
    crate::validation::validate_id(stage_id)
        .context("Invalid stage ID passed to find_stage_file")?;

    if !stages_dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // Check for exact match (no prefix)
            if stem == stage_id {
                return Ok(Some(path));
            }

            // Check for prefixed match: XX-{stage_id} where XX is digits
            if let Some(suffix) = stem.strip_prefix(|c: char| c.is_ascii_digit()) {
                if let Some(suffix) = suffix.strip_prefix(|c: char| c.is_ascii_digit()) {
                    if let Some(id) = suffix.strip_prefix('-') {
                        if id == stage_id {
                            return Ok(Some(path));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Extract stage ID from a stage filename.
///
/// Handles both prefixed (`01-stage-id.md`) and non-prefixed (`stage-id.md`) formats.
///
/// # Arguments
/// * `filename` - The filename (with or without path)
///
/// # Returns
/// The stage ID if parseable, None otherwise
pub fn extract_stage_id(filename: &str) -> Option<String> {
    let stem = Path::new(filename).file_stem().and_then(|s| s.to_str())?;

    // Check for prefixed format: XX-{stage_id}
    if let Some(suffix) = stem.strip_prefix(|c: char| c.is_ascii_digit()) {
        if let Some(suffix) = suffix.strip_prefix(|c: char| c.is_ascii_digit()) {
            if let Some(id) = suffix.strip_prefix('-') {
                return Some(id.to_string());
            }
        }
    }

    // Non-prefixed format
    Some(stem.to_string())
}

/// Generate a stage filename with depth prefix.
///
/// # Arguments
/// * `depth` - The topological depth (0-indexed)
/// * `stage_id` - The stage ID
///
/// # Returns
/// Filename in format `{depth+1:02}-{stage_id}.md`
pub fn stage_filename(depth: usize, stage_id: &str) -> String {
    format!("{:02}-{}.md", depth + 1, stage_id)
}

/// Get the stage file path, creating a new path with depth prefix.
///
/// # Arguments
/// * `stages_dir` - Path to the stages directory
/// * `depth` - The topological depth (0-indexed)
/// * `stage_id` - The stage ID
///
/// # Returns
/// Full path to the stage file
pub fn stage_file_path(stages_dir: &Path, depth: usize, stage_id: &str) -> PathBuf {
    stages_dir.join(stage_filename(depth, stage_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_stage_file_exact_match() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path();

        fs::write(stages_dir.join("stage-1.md"), "content").unwrap();

        let result = find_stage_file(stages_dir, "stage-1").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("stage-1.md"));
    }

    #[test]
    fn test_find_stage_file_with_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path();

        fs::write(stages_dir.join("01-core-architecture.md"), "content").unwrap();
        fs::write(stages_dir.join("02-math-core.md"), "content").unwrap();

        let result = find_stage_file(stages_dir, "core-architecture").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("01-core-architecture.md"));

        let result = find_stage_file(stages_dir, "math-core").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("02-math-core.md"));
    }

    #[test]
    fn test_find_stage_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path();

        fs::write(stages_dir.join("01-stage-1.md"), "content").unwrap();

        let result = find_stage_file(stages_dir, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_stage_file_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let result = find_stage_file(temp_dir.path(), "stage-1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_stage_file_nonexistent_dir() {
        let result = find_stage_file(Path::new("/nonexistent/path"), "stage-1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_stage_id_with_prefix() {
        assert_eq!(
            extract_stage_id("01-core-architecture.md"),
            Some("core-architecture".to_string())
        );
        assert_eq!(
            extract_stage_id("12-some-stage.md"),
            Some("some-stage".to_string())
        );
    }

    #[test]
    fn test_extract_stage_id_without_prefix() {
        assert_eq!(extract_stage_id("stage-1.md"), Some("stage-1".to_string()));
        assert_eq!(
            extract_stage_id("my-stage.md"),
            Some("my-stage".to_string())
        );
    }

    #[test]
    fn test_stage_filename() {
        assert_eq!(stage_filename(0, "core-arch"), "01-core-arch.md");
        assert_eq!(stage_filename(1, "math-core"), "02-math-core.md");
        assert_eq!(stage_filename(9, "final-stage"), "10-final-stage.md");
    }

    #[test]
    fn test_stage_file_path() {
        let stages_dir = Path::new("/work/stages");
        let path = stage_file_path(stages_dir, 0, "core-arch");
        assert_eq!(path, PathBuf::from("/work/stages/01-core-arch.md"));
    }
}
