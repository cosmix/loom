//! Plan file lifecycle management - renaming plan files based on execution state.
//!
//! This module handles:
//! - Adding `IN_PROGRESS-` prefix when execution starts
//! - Replacing `IN_PROGRESS-` with `DONE-` when all stages are merged

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;

use super::config_ops::{get_plan_source_path, update_plan_source_path};
use super::filename_ops::{
    add_prefix_to_filename, has_prefix, remove_prefix_from_filename, DONE_PREFIX,
    IN_PROGRESS_PREFIX,
};
use super::merge_status::all_stages_merged;

/// Mark the plan file as in-progress by adding `IN_PROGRESS-` prefix.
///
/// This is called when `loom run` starts execution.
/// If the plan already has `IN_PROGRESS-` prefix, this is a no-op.
/// If the plan has `DONE-` prefix, we skip (user re-running a completed plan).
///
/// Returns the new path if renamed, None if no rename was needed.
pub fn mark_plan_in_progress(work_dir: &WorkDir) -> Result<Option<PathBuf>> {
    let Some(current_path) = get_plan_source_path(work_dir)? else {
        return Ok(None);
    };

    // Already marked as in-progress
    if has_prefix(&current_path, IN_PROGRESS_PREFIX) {
        return Ok(None);
    }

    // Already done - user is re-running, leave as-is
    if has_prefix(&current_path, DONE_PREFIX) {
        println!(
            "  {} Plan already marked as DONE, skipping prefix update",
            "→".dimmed()
        );
        return Ok(None);
    }

    // Check file exists before renaming
    if !current_path.exists() {
        return Ok(None);
    }

    let new_path = add_prefix_to_filename(&current_path, IN_PROGRESS_PREFIX);

    // Rename the file
    fs::rename(&current_path, &new_path).with_context(|| {
        format!(
            "Failed to rename plan file from {} to {}",
            current_path.display(),
            new_path.display()
        )
    })?;

    // Update config.toml with new path
    update_plan_source_path(work_dir, &new_path)?;

    println!(
        "  {} Plan marked as in-progress: {}",
        "→".cyan().bold(),
        new_path.file_name().unwrap_or_default().to_string_lossy()
    );

    Ok(Some(new_path))
}

/// Mark the plan file as done by replacing `IN_PROGRESS-` with `DONE-` prefix.
///
/// This is called after the orchestrator completes successfully.
/// Only renames if all stages are merged. If not all merged, leaves as `IN_PROGRESS-`.
///
/// Returns the new path if renamed, None if no rename was needed.
pub fn mark_plan_done_if_all_merged(work_dir: &WorkDir) -> Result<Option<PathBuf>> {
    let Some(current_path) = get_plan_source_path(work_dir)? else {
        return Ok(None);
    };

    // Only process IN_PROGRESS files
    if !has_prefix(&current_path, IN_PROGRESS_PREFIX) {
        return Ok(None);
    }

    // Check if all stages are merged
    if !all_stages_merged(work_dir)? {
        println!(
            "  {} Not all stages merged, leaving plan as IN_PROGRESS",
            "→".yellow().bold()
        );
        return Ok(None);
    }

    // Check file exists before renaming
    if !current_path.exists() {
        return Ok(None);
    }

    // Remove IN_PROGRESS- and add DONE-
    let without_prefix = remove_prefix_from_filename(&current_path, IN_PROGRESS_PREFIX);
    let new_path = add_prefix_to_filename(&without_prefix, DONE_PREFIX);

    // Rename the file
    fs::rename(&current_path, &new_path).with_context(|| {
        format!(
            "Failed to rename plan file from {} to {}",
            current_path.display(),
            new_path.display()
        )
    })?;

    // Update config.toml with new path
    update_plan_source_path(work_dir, &new_path)?;

    println!(
        "  {} Plan marked as done: {}",
        "✓".green().bold(),
        new_path.file_name().unwrap_or_default().to_string_lossy()
    );

    Ok(Some(new_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_work_dir(temp_dir: &TempDir) -> WorkDir {
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();
        work_dir
    }

    fn create_plan_file(temp_dir: &TempDir, filename: &str) -> PathBuf {
        let plans_dir = temp_dir.path().join("doc/plans");
        fs::create_dir_all(&plans_dir).unwrap();
        let plan_path = plans_dir.join(filename);
        fs::write(&plan_path, "# Test Plan\n\nPlan content").unwrap();
        plan_path
    }

    fn write_config(work_dir: &WorkDir, plan_path: &std::path::Path) {
        let config_content = format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"test\"\nplan_name = \"Test\"\nbase_branch = \"main\"\n",
            plan_path.display()
        );
        fs::write(work_dir.root().join("config.toml"), config_content).unwrap();
    }

    fn create_stage_file(work_dir: &WorkDir, stage_id: &str, merged: bool) {
        let stages_dir = work_dir.root().join("stages");
        fs::create_dir_all(&stages_dir).unwrap();
        let content = format!(
            "---\nid: {stage_id}\nname: Test Stage\nstatus: Completed\nmerged: {merged}\n---\n# Stage\n"
        );
        fs::write(stages_dir.join(format!("0-{stage_id}.md")), content).unwrap();
    }

    #[test]
    fn test_mark_plan_in_progress_renames_file() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        let result = mark_plan_in_progress(&work_dir).unwrap();

        assert!(result.is_some());
        let new_path = result.unwrap();
        assert!(new_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("IN_PROGRESS-"));
        assert!(new_path.exists());
        assert!(!plan_path.exists()); // Original file should be gone
    }

    #[test]
    fn test_mark_plan_in_progress_updates_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        mark_plan_in_progress(&work_dir).unwrap();

        // Verify config was updated
        let new_source_path = get_plan_source_path(&work_dir).unwrap().unwrap();
        assert!(new_source_path.to_str().unwrap().contains("IN_PROGRESS-"));
    }

    #[test]
    fn test_mark_plan_in_progress_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "IN_PROGRESS-PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        let result = mark_plan_in_progress(&work_dir).unwrap();

        assert!(result.is_none()); // No rename needed
        assert!(plan_path.exists()); // File unchanged
    }

    #[test]
    fn test_mark_plan_in_progress_skips_done_plans() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "DONE-PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        let result = mark_plan_in_progress(&work_dir).unwrap();

        assert!(result.is_none()); // No rename for DONE plans
        assert!(plan_path.exists()); // File unchanged
    }

    #[test]
    fn test_mark_plan_in_progress_no_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        // No config.toml created

        let result = mark_plan_in_progress(&work_dir).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_mark_plan_done_when_all_merged() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "IN_PROGRESS-PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        // Create merged stage files
        create_stage_file(&work_dir, "stage-1", true);
        create_stage_file(&work_dir, "stage-2", true);

        let result = mark_plan_done_if_all_merged(&work_dir).unwrap();

        assert!(result.is_some());
        let new_path = result.unwrap();
        assert!(new_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("DONE-"));
        assert!(!new_path.to_str().unwrap().contains("IN_PROGRESS"));
        assert!(new_path.exists());
    }

    #[test]
    fn test_mark_plan_done_skips_when_not_all_merged() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "IN_PROGRESS-PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        // Create one merged and one not merged
        create_stage_file(&work_dir, "stage-1", true);
        create_stage_file(&work_dir, "stage-2", false);

        let result = mark_plan_done_if_all_merged(&work_dir).unwrap();

        assert!(result.is_none()); // Should not rename
        assert!(plan_path.exists()); // Original still exists
    }

    #[test]
    fn test_mark_plan_done_only_processes_in_progress() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "PLAN-feature.md"); // No prefix
        write_config(&work_dir, &plan_path);

        create_stage_file(&work_dir, "stage-1", true);

        let result = mark_plan_done_if_all_merged(&work_dir).unwrap();

        assert!(result.is_none()); // Should not process non-IN_PROGRESS files
    }

    #[test]
    fn test_mark_plan_done_updates_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let plan_path = create_plan_file(&temp_dir, "IN_PROGRESS-PLAN-feature.md");
        write_config(&work_dir, &plan_path);

        create_stage_file(&work_dir, "stage-1", true);

        mark_plan_done_if_all_merged(&work_dir).unwrap();

        // Verify config was updated
        let new_source_path = get_plan_source_path(&work_dir).unwrap().unwrap();
        assert!(new_source_path.to_str().unwrap().contains("DONE-"));
        assert!(!new_source_path.to_str().unwrap().contains("IN_PROGRESS"));
    }

    #[test]
    fn test_full_lifecycle_plan_to_done() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        let original_plan = create_plan_file(&temp_dir, "PLAN-my-feature.md");
        write_config(&work_dir, &original_plan);

        // Step 1: Mark as in-progress (simulates loom run start)
        let in_progress = mark_plan_in_progress(&work_dir).unwrap().unwrap();
        assert_eq!(
            in_progress.file_name().unwrap().to_str().unwrap(),
            "IN_PROGRESS-PLAN-my-feature.md"
        );

        // Step 2: Create merged stages (simulates execution completing)
        create_stage_file(&work_dir, "stage-1", true);

        // Step 3: Mark as done (simulates successful completion)
        let done = mark_plan_done_if_all_merged(&work_dir).unwrap().unwrap();
        assert_eq!(
            done.file_name().unwrap().to_str().unwrap(),
            "DONE-PLAN-my-feature.md"
        );

        // Verify final state
        assert!(!original_plan.exists());
        assert!(!in_progress.exists());
        assert!(done.exists());
    }
}
