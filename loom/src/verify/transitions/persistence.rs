//! Stage persistence operations
//!
//! This module handles:
//! - Loading and saving stage state to/from `.work/stages/` markdown files

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fs::locking::{atomic_write_locked, locked_dir_update, locked_read};
use crate::fs::stage_files::{find_stage_file, stage_file_path};
use crate::models::stage::Stage;
use crate::plan::graph::levels::compute_all_levels;

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

/// Compatibility name for creation-only stage persistence.
///
/// This function deliberately refuses to overwrite an existing stage. Live
/// records must be changed through [`update_stage`] so concurrent fields cannot
/// be lost. New code should prefer [`create_stage`] for clarity.
pub fn save_stage(stage: &Stage, work_dir: &Path) -> Result<()> {
    create_stage(stage, work_dir)
}

/// Create a new stage record and refuse to overwrite an existing stage.
///
/// Initialization paths should use this API instead of a whole-record save.
/// The existence check and atomic write share the stages-directory lock, so two
/// creators cannot both claim the same stage ID.
pub fn create_stage(stage: &Stage, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).with_context(|| {
        format!(
            "Failed to create stages directory: {}",
            stages_dir.display()
        )
    })?;
    let depth = compute_stage_depth(stage, work_dir)?;
    let stage_path = stage_file_path(&stages_dir, depth, &stage.id);
    let content = serialize_stage_to_markdown(stage)?;

    locked_dir_update(&stages_dir, || {
        if find_stage_file(&stages_dir, &stage.id)?.is_some() {
            anyhow::bail!("Stage already exists: {}", stage.id);
        }
        atomic_write_locked(&stage_path, &content)
    })
}

/// Atomically read-modify-write a stage file under a single exclusive lock.
///
/// This is the lost-update-safe alternative to the load → mutate → `save_stage`
/// pattern. The whole-`Stage` save approach reverts any field a *concurrent*
/// writer changed between this caller's load and its save, because each save
/// serializes the entire in-memory `Stage`. With three writer classes racing on
/// the same file (the orchestrator main loop, the daemon dispute IPC thread, and
/// agent-run CLI commands), the last writer silently clobbers the others'
/// changes (status reverted, `dispute_count`/`retry_count`/`close_reason`/
/// `session` lost).
///
/// `update_stage` closes the window: it holds the `stages/` directory lock across
/// a *fresh* on-disk read, the `modify` closure, and the write. The closure
/// therefore mutates the **current** persisted state, so it only needs to touch
/// the fields the operation owns — it never reverts a sibling writer's field.
///
/// The directory lock is the same inode every `locked_read`/`locked_write` of a
/// stage file takes (they lock the file's parent, which is `stages/`), so this
/// critical section is mutually exclusive with all other stage-file reads and
/// writes — across processes, since these are advisory `flock`s.
///
/// The stage file MUST already exist; a missing file is an error (this API is for
/// mutating live stages, not creating them — use [`create_stage`] for creation).
///
/// # Arguments
/// * `stage_id` - The ID of the stage to update
/// * `work_dir` - Path to the `.work` directory
/// * `modify` - Closure applied to the freshly-read on-disk `Stage`. It may fail
///   (e.g. a state-machine transition is refused); on `Err` the file is left
///   untouched.
///
/// # Returns
/// The post-modification `Stage` (a clone of what was written) on success.
pub fn update_stage<F>(stage_id: &str, work_dir: &Path, modify: F) -> Result<Stage>
where
    F: FnOnce(&mut Stage) -> Result<()>,
{
    let stages_dir = work_dir.join("stages");

    locked_dir_update(&stages_dir, || {
        // Re-read the CURRENT on-disk stage under the lock. Anything a concurrent
        // writer committed before we took the lock is visible here and preserved.
        let stage_path = find_stage_file(&stages_dir, stage_id)?
            .ok_or_else(|| anyhow::anyhow!("Stage file not found for update: {stage_id}"))?;

        update_stage_file_locked(stage_id, &stage_path, modify)
    })
}

/// Update an already-enumerated canonical stage path without rescanning `stages/`.
///
/// Recovery builds one path index per pass to avoid an O(stages²) directory scan.
/// This variant retains the exact locking and fail-closed parsing guarantees of
/// [`update_stage`], while also verifying that the indexed path still contains
/// the requested stage record before applying the delta.
pub(crate) fn update_stage_at_path<F>(
    stage_id: &str,
    stage_path: &Path,
    work_dir: &Path,
    modify: F,
) -> Result<Stage>
where
    F: FnOnce(&mut Stage) -> Result<()>,
{
    let stages_dir = work_dir.join("stages");
    if stage_path.parent() != Some(stages_dir.as_path()) {
        anyhow::bail!(
            "Stage path is outside canonical stages directory: {}",
            stage_path.display()
        );
    }

    locked_dir_update(&stages_dir, || {
        update_stage_file_locked(stage_id, stage_path, modify)
    })
}

fn update_stage_file_locked<F>(stage_id: &str, stage_path: &Path, modify: F) -> Result<Stage>
where
    F: FnOnce(&mut Stage) -> Result<()>,
{
    let content = std::fs::read_to_string(stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;
    let mut stage = parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))?;
    if stage.id != stage_id {
        anyhow::bail!(
            "Stage identity mismatch at {}: expected '{stage_id}', found '{}'",
            stage_path.display(),
            stage.id
        );
    }

    modify(&mut stage)?;
    let new_content = serialize_stage_to_markdown(&stage)?;
    atomic_write_locked(stage_path, &new_content)?;
    Ok(stage)
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
    let mut existing_stages = list_all_stages(work_dir).unwrap_or_default();

    // Add the current stage if not already present
    if !existing_stages.iter().any(|s| s.id == stage.id) {
        existing_stages.push(stage.clone());
    }

    // Compute depths for all stages
    let depths = compute_all_levels(&existing_stages, |s| s.id.as_str(), |s| &s.dependencies);

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

#[cfg(test)]
#[path = "tests/update_stage.rs"]
mod update_stage_tests;
