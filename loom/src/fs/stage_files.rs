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
use std::collections::HashMap;
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

/// Dependency information for computing stage depth
#[derive(Debug, Clone)]
pub struct StageDependencies {
    pub id: String,
    pub dependencies: Vec<String>,
}

/// Compute topological depths for all stages.
///
/// Stages with no dependencies get depth 0 (prefix 01-).
/// Stages depending only on depth-N stages get depth N+1.
///
/// # Arguments
/// * `stages` - List of stage dependency information
///
/// # Returns
/// Map from stage_id to depth (0-indexed, but filenames use 1-indexed: depth 0 -> prefix 01)
pub fn compute_stage_depths(stages: &[StageDependencies]) -> Result<HashMap<String, usize>> {
    let mut depths: HashMap<String, usize> = HashMap::new();

    // Build adjacency information
    let stage_set: std::collections::HashSet<&str> = stages.iter().map(|s| s.id.as_str()).collect();

    // Iteratively compute depths
    // A stage's depth = max(depth of dependencies) + 1, or 0 if no dependencies
    let mut changed = true;
    let mut iterations = 0;
    let max_iterations = stages.len() + 1;

    // Initialize stages with no dependencies to depth 0
    for stage in stages {
        if stage.dependencies.is_empty() {
            depths.insert(stage.id.clone(), 0);
        }
    }

    while changed && iterations < max_iterations {
        changed = false;
        iterations += 1;

        for stage in stages {
            if depths.contains_key(&stage.id) {
                continue;
            }

            // Check if all dependencies have computed depths
            let all_deps_resolved = stage
                .dependencies
                .iter()
                .filter(|dep| stage_set.contains(dep.as_str()))
                .all(|dep| depths.contains_key(dep));

            if all_deps_resolved {
                let max_dep_depth = stage
                    .dependencies
                    .iter()
                    .filter(|dep| stage_set.contains(dep.as_str()))
                    .filter_map(|dep| depths.get(dep))
                    .max()
                    .copied()
                    .unwrap_or(0);

                let depth = if stage.dependencies.is_empty() {
                    0
                } else {
                    max_dep_depth + 1
                };

                depths.insert(stage.id.clone(), depth);
                changed = true;
            }
        }
    }

    // Any remaining stages (possibly in cycles or with missing deps) get depth 0
    for stage in stages {
        depths.entry(stage.id.clone()).or_insert(0);
    }

    Ok(depths)
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
    fn test_compute_stage_depths_no_deps() {
        let stages = vec![
            StageDependencies {
                id: "stage-1".to_string(),
                dependencies: vec![],
            },
            StageDependencies {
                id: "stage-2".to_string(),
                dependencies: vec![],
            },
        ];

        let depths = compute_stage_depths(&stages).unwrap();
        assert_eq!(depths.get("stage-1"), Some(&0));
        assert_eq!(depths.get("stage-2"), Some(&0));
    }

    #[test]
    fn test_compute_stage_depths_linear() {
        let stages = vec![
            StageDependencies {
                id: "stage-1".to_string(),
                dependencies: vec![],
            },
            StageDependencies {
                id: "stage-2".to_string(),
                dependencies: vec!["stage-1".to_string()],
            },
            StageDependencies {
                id: "stage-3".to_string(),
                dependencies: vec!["stage-2".to_string()],
            },
        ];

        let depths = compute_stage_depths(&stages).unwrap();
        assert_eq!(depths.get("stage-1"), Some(&0));
        assert_eq!(depths.get("stage-2"), Some(&1));
        assert_eq!(depths.get("stage-3"), Some(&2));
    }

    #[test]
    fn test_compute_stage_depths_diamond() {
        // Diamond: A -> B, A -> C, B -> D, C -> D
        let stages = vec![
            StageDependencies {
                id: "a".to_string(),
                dependencies: vec![],
            },
            StageDependencies {
                id: "b".to_string(),
                dependencies: vec!["a".to_string()],
            },
            StageDependencies {
                id: "c".to_string(),
                dependencies: vec!["a".to_string()],
            },
            StageDependencies {
                id: "d".to_string(),
                dependencies: vec!["b".to_string(), "c".to_string()],
            },
        ];

        let depths = compute_stage_depths(&stages).unwrap();
        assert_eq!(depths.get("a"), Some(&0));
        assert_eq!(depths.get("b"), Some(&1));
        assert_eq!(depths.get("c"), Some(&1));
        assert_eq!(depths.get("d"), Some(&2));
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
