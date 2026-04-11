//! Plan file lifecycle management - renaming plan files based on execution state.
//!
//! This module handles:
//! - Adding `IN_PROGRESS-` prefix when execution starts
//! - Replacing `IN_PROGRESS-` with `DONE-` when all stages are merged
//! - Checking plan source paths in config.toml
//! - Checking if all stages are merged

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::parser::frontmatter::extract_frontmatter_field;

// Filename prefix constants
pub const IN_PROGRESS_PREFIX: &str = "IN_PROGRESS-";
pub const DONE_PREFIX: &str = "DONE-";

// ===== Filename Operations =====

/// Add a prefix to the plan filename, preserving the directory
pub fn add_prefix_to_filename(path: &Path, prefix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plan.md");

    parent.join(format!("{prefix}{filename}"))
}

/// Remove a prefix from the plan filename if present
pub fn remove_prefix_from_filename(path: &Path, prefix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plan.md");

    if let Some(stripped) = filename.strip_prefix(prefix) {
        parent.join(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Check if the filename has a specific prefix
pub fn has_prefix(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with(prefix))
}

// ===== Config Operations =====

/// Get the plan source path from config.toml
///
/// This is a convenience wrapper around `crate::fs::get_source_path` that accepts
/// a `WorkDir` reference instead of a raw path.
pub fn get_plan_source_path(work_dir: &WorkDir) -> Result<Option<PathBuf>> {
    crate::fs::get_source_path(work_dir.root())
}

/// Update the plan source path in config.toml
pub fn update_plan_source_path(work_dir: &WorkDir, new_path: &Path) -> Result<()> {
    let mut config = work_dir.load_config_required()?;

    if let Some(plan) = config.as_toml_mut().get_mut("plan") {
        if let Some(table) = plan.as_table_mut() {
            table.insert(
                "source_path".to_string(),
                toml::Value::String(new_path.display().to_string()),
            );
        }
    }

    // Serialize back to TOML with proper formatting
    let new_content = config.to_toml_string()?;
    fs::write(work_dir.config_path(), new_content).context("Failed to write config.toml")?;

    Ok(())
}

// ===== Merge Status =====

/// Check if all stages are merged by reading stage files.
pub fn all_stages_merged(work_dir: &WorkDir) -> Result<bool> {
    let stages_dir = work_dir.root().join("stages");

    if !stages_dir.exists() {
        return Ok(false);
    }

    let entries = fs::read_dir(&stages_dir).context("Failed to read stages directory")?;

    let mut found_any_stage = false;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        found_any_stage = true;

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Parse YAML frontmatter to check merged status
        match extract_frontmatter_field(&content, "merged") {
            Ok(Some(value)) if value == "true" => {
                // Stage is merged, continue checking others
            }
            _ => {
                // Not merged or error parsing
                return Ok(false);
            }
        }
    }

    // Must have at least one stage to be considered "all merged"
    Ok(found_any_stage)
}

// ===== Post-Completion Commit =====

/// Commit tracked changes to keep the default branch clean after plan completion.
///
/// After all stages are merged and the plan is renamed to DONE, this commits:
/// - The plan file rename (IN_PROGRESS → DONE)
/// - Any other tracked modifications (e.g., knowledge files updated by integration-verify)
fn commit_post_completion_changes(
    work_dir: &WorkDir,
    old_plan_path: &Path,
    new_plan_path: &Path,
) -> Result<()> {
    use crate::git::runner::run_git_checked;

    let repo_root = work_dir
        .project_root()
        .context("Failed to determine project root")?;

    // Stage the plan file rename: add the new DONE file and stage deletion of old
    run_git_checked(&["add", &new_plan_path.display().to_string()], repo_root)?;
    run_git_checked(&["add", &old_plan_path.display().to_string()], repo_root)?;

    // Stage any other tracked modifications (modified + deleted tracked files).
    // Does NOT add untracked files — safe for automated use.
    // Typically catches knowledge files updated by integration-verify.
    run_git_checked(&["add", "-u"], repo_root)?;

    // Check if there's anything staged to commit
    let staged = run_git_checked(&["diff", "--cached", "--name-only"], repo_root)?;
    if staged.is_empty() {
        return Ok(());
    }

    // Commit with a descriptive message
    let plan_name = new_plan_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan");
    run_git_checked(
        &[
            "commit",
            "-m",
            &format!("chore(loom): mark plan complete — {plan_name}"),
        ],
        repo_root,
    )?;

    println!(
        "  {} Committed post-completion changes to default branch",
        "✓".green().bold(),
    );

    Ok(())
}

// ===== Plan Lifecycle Functions =====

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

    // Commit tracked changes to leave the default branch clean
    if let Err(e) = commit_post_completion_changes(work_dir, &current_path, &new_path) {
        eprintln!(
            "  {} Warning: Failed to commit post-completion changes: {e}",
            "⚠".yellow().bold()
        );
    }

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

    // Filename operations tests
    #[test]
    fn test_add_prefix_to_filename() {
        let path = PathBuf::from("doc/plans/PLAN-feature.md");
        let result = add_prefix_to_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(
            result,
            PathBuf::from("doc/plans/IN_PROGRESS-PLAN-feature.md")
        );
    }

    #[test]
    fn test_add_prefix_preserves_nested_path() {
        let path = PathBuf::from("/home/user/project/doc/plans/PLAN-auth.md");
        let result = add_prefix_to_filename(&path, DONE_PREFIX);
        assert_eq!(
            result,
            PathBuf::from("/home/user/project/doc/plans/DONE-PLAN-auth.md")
        );
    }

    #[test]
    fn test_remove_prefix_from_filename() {
        let path = PathBuf::from("doc/plans/IN_PROGRESS-PLAN-feature.md");
        let result = remove_prefix_from_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(result, PathBuf::from("doc/plans/PLAN-feature.md"));
    }

    #[test]
    fn test_remove_prefix_not_present() {
        let path = PathBuf::from("doc/plans/PLAN-feature.md");
        let result = remove_prefix_from_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(result, PathBuf::from("doc/plans/PLAN-feature.md"));
    }

    #[test]
    fn test_has_prefix() {
        assert!(has_prefix(
            Path::new("doc/plans/IN_PROGRESS-PLAN.md"),
            IN_PROGRESS_PREFIX
        ));
        assert!(!has_prefix(
            Path::new("doc/plans/PLAN.md"),
            IN_PROGRESS_PREFIX
        ));
        assert!(has_prefix(Path::new("doc/plans/DONE-PLAN.md"), DONE_PREFIX));
    }

    // Merge status tests
    #[test]
    fn test_all_stages_merged_empty_stages_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);
        // No stages directory

        let result = all_stages_merged(&work_dir).unwrap();

        assert!(!result); // Empty = not merged
    }

    #[test]
    fn test_all_stages_merged_ignores_non_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);

        let stages_dir = work_dir.root().join("stages");
        fs::create_dir_all(&stages_dir).unwrap();
        fs::write(stages_dir.join("readme.txt"), "Not a stage").unwrap();

        // With only non-markdown files, returns false (no stages found)
        let result = all_stages_merged(&work_dir).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_all_stages_merged_true() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);

        create_stage_file(&work_dir, "stage-1", true);
        create_stage_file(&work_dir, "stage-2", true);

        let result = all_stages_merged(&work_dir).unwrap();
        assert!(result);
    }

    #[test]
    fn test_all_stages_merged_false_when_one_not_merged() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = create_test_work_dir(&temp_dir);

        create_stage_file(&work_dir, "stage-1", true);
        create_stage_file(&work_dir, "stage-2", false);

        let result = all_stages_merged(&work_dir).unwrap();
        assert!(!result);
    }

    // Plan lifecycle tests
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
