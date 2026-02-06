//! Stage persistence operations
//!
//! This module handles:
//! - Loading and saving stage state to/from `.work/stages/` markdown files

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fs::locking::{locked_read, locked_write};
use crate::fs::stage_files::{
    compute_stage_depths, find_stage_file, stage_file_path, StageDependencies,
};
use crate::models::stage::Stage;

use super::serialization::{parse_stage_from_markdown, serialize_stage_to_markdown};

/// Load a stage from disk
///
/// Finds and reads the stage file from `.work/stages/`, handling both
/// prefixed (e.g., `01-stage-id.md`) and non-prefixed (`stage-id.md`) formats.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to load
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The loaded stage
pub fn load_stage(stage_id: &str, work_dir: &Path) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?
        .ok_or_else(|| anyhow::anyhow!("Stage file not found for: {stage_id}"))?;

    let content = locked_read(&stage_path)?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))
}

/// Save a stage to disk
///
/// Serializes the stage to YAML frontmatter + markdown body and writes
/// to `.work/stages/`. Uses depth-prefixed filenames (e.g., `01-stage-id.md`)
/// for topological ordering visibility.
///
/// If the stage file already exists (with any prefix), updates it in place.
/// For new stages, computes the topological depth based on dependencies.
///
/// # Arguments
/// * `stage` - The stage to save
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// Ok(()) on success
pub fn save_stage(stage: &Stage, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");
    if !stages_dir.exists() {
        fs::create_dir_all(&stages_dir).with_context(|| {
            format!(
                "Failed to create stages directory: {}",
                stages_dir.display()
            )
        })?;
    }

    // Check if a file already exists for this stage (with any prefix)
    let stage_path = if let Some(existing_path) = find_stage_file(&stages_dir, &stage.id)? {
        // Update existing file in place
        existing_path
    } else {
        // New stage - compute depth and create with prefix
        let depth = compute_stage_depth(stage, work_dir)?;
        stage_file_path(&stages_dir, depth, &stage.id)
    };

    let content = serialize_stage_to_markdown(stage)?;

    locked_write(&stage_path, &content)?;

    Ok(())
}

/// Compute the topological depth for a single stage based on its dependencies
/// and existing stages in the work directory.
///
/// # Arguments
/// * `stage` - The stage to compute depth for
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The depth (0-indexed)
fn compute_stage_depth(stage: &Stage, work_dir: &Path) -> Result<usize> {
    // Load all existing stages to get their dependency info
    let existing_stages = list_all_stages(work_dir).unwrap_or_default();

    // Build dependency info including the new stage
    let mut stage_deps: Vec<StageDependencies> = existing_stages
        .iter()
        .map(|s| StageDependencies {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    // Add the current stage if not already present
    if !stage_deps.iter().any(|s| s.id == stage.id) {
        stage_deps.push(StageDependencies {
            id: stage.id.clone(),
            dependencies: stage.dependencies.clone(),
        });
    }

    // Compute depths for all stages
    let depths = compute_stage_depths(&stage_deps)?;

    // Return depth for this stage
    Ok(depths.get(&stage.id).copied().unwrap_or(0))
}

/// List all stages from `.work/stages/`
///
/// Reads all `.md` files in the stages directory and parses them into
/// Stage structs.
///
/// # Arguments
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// List of all stages
pub fn list_all_stages(work_dir: &Path) -> Result<Vec<Stage>> {
    let stages_dir = work_dir.join("stages");

    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut stages = Vec::new();

    let entries = fs::read_dir(&stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_stage_from_path(&path) {
                Ok(stage) => stages.push(stage),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load stage from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(stages)
}

/// Load a stage from a specific file path
fn load_stage_from_path(path: &Path) -> Result<Stage> {
    let content = locked_read(path)?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", path.display()))
}
