//! Stage persistence operations
//!
//! This module handles:
//! - Loading and saving stage state to/from `.work/stages/` markdown files

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fs::locking::{atomic_write_locked, locked_dir_update, locked_read, locked_write};
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
/// mutating live stages, not creating them — use [`save_stage`] for creation).
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

        let content = std::fs::read_to_string(&stage_path)
            .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;
        let mut stage = parse_stage_from_markdown(&content)
            .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))?;

        // Apply the operation-owned delta to the fresh state. A closure error
        // (e.g. a refused transition) leaves the file untouched.
        modify(&mut stage)?;

        let new_content = serialize_stage_to_markdown(&stage)?;
        atomic_write_locked(&stage_path, &new_content)?;

        Ok(stage)
    })
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
mod update_stage_tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus};

    fn seed_stage(work_dir: &Path, id: &str) -> Stage {
        let stage = Stage {
            id: id.to_string(),
            name: format!("Stage {id}"),
            status: StageStatus::Executing,
            ..Stage::default()
        };
        save_stage(&stage, work_dir).unwrap();
        stage
    }

    #[test]
    fn update_stage_applies_delta_and_returns_written_stage() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        seed_stage(work_dir, "s1");

        let written = update_stage("s1", work_dir, |s| {
            s.dispute_count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(written.dispute_count, 1);

        let reloaded = load_stage("s1", work_dir).unwrap();
        assert_eq!(reloaded.dispute_count, 1);
    }

    #[test]
    fn update_stage_preserves_concurrent_field_written_after_load() {
        // Models the A-5 lost-update class: a long-running op loads the stage,
        // then a *different* writer commits a change to another field; the
        // long-running op must NOT revert that field. update_stage re-reads the
        // current on-disk state inside the lock, so it only touches its own
        // field and preserves the concurrent writer's change.
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stale = seed_stage(work_dir, "s2");

        // Concurrent writer (e.g. dispute thread) bumps dispute_count on disk.
        update_stage("s2", work_dir, |s| {
            s.dispute_count = 5;
            Ok(())
        })
        .unwrap();

        // The "long-running op" still holds `stale` (dispute_count == 0). It
        // applies its own owned field via update_stage, which re-reads disk.
        assert_eq!(stale.dispute_count, 0);
        update_stage("s2", work_dir, |s| {
            s.retry_count += 1;
            Ok(())
        })
        .unwrap();

        let reloaded = load_stage("s2", work_dir).unwrap();
        // Concurrent writer's field survived...
        assert_eq!(reloaded.dispute_count, 5);
        // ...and our owned field was applied.
        assert_eq!(reloaded.retry_count, 1);
    }

    #[test]
    fn update_stage_leaves_file_untouched_on_closure_error() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        seed_stage(work_dir, "s3");
        update_stage("s3", work_dir, |s| {
            s.dispute_count = 9;
            Ok(())
        })
        .unwrap();

        let err = update_stage("s3", work_dir, |s| {
            s.dispute_count = 99; // mutated, but the closure then fails
            anyhow::bail!("closure failed")
        });
        assert!(err.is_err());

        // The failed update must not have written the mutation.
        let reloaded = load_stage("s3", work_dir).unwrap();
        assert_eq!(reloaded.dispute_count, 9);
    }

    #[test]
    fn update_stage_errors_when_file_missing() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();
        let res = update_stage("does-not-exist", work_dir, |_s| Ok(()));
        assert!(res.is_err());
    }

    #[test]
    fn update_stage_concurrent_increments_have_no_lost_updates() {
        use std::thread;
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path().to_path_buf();
        seed_stage(&work_dir, "s4");

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let work_dir = work_dir.clone();
                thread::spawn(move || {
                    update_stage("s4", &work_dir, |s| {
                        s.dispute_count += 1;
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let reloaded = load_stage("s4", &work_dir).unwrap();
        // All 10 increments landed — the exclusive lock serializes the RMW.
        assert_eq!(reloaded.dispute_count, 10);
    }
}
