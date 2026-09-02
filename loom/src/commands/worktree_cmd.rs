//! Worktree management commands
//! Usage: `loom worktree [list|remove <stage-id>]`

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::git::branch::{branch_name_for_stage, default_branch};
use crate::git::cleanup::{
    cleanup_destructive_stage, cleanup_orphaned_worktrees, cleanup_verified_stage,
    stage_resources_exist, CleanupResult,
};
use crate::git::merge::lock::MergeLock;
use crate::git::worktree::find_worktree_by_prefix;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, update_stage};
use std::time::Duration;

/// List all worktrees
pub fn list() -> Result<()> {
    println!("Git worktrees:");
    println!("─────────────────────────────────────────────────────────");

    let worktrees_dir = std::env::current_dir()?.join(".worktrees");
    if !worktrees_dir.exists() {
        println!("(no .worktrees/ directory)");
        return Ok(());
    }

    if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
        let mut found = false;
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name();
                let stage_name = name.to_string_lossy();
                let branch = branch_name_for_stage(&stage_name);
                println!("  {stage_name} -> {branch}");
                found = true;
            }
        }
        if !found {
            println!("(no worktrees found)");
        }
    }

    Ok(())
}

/// Clean worktrees whose branch history is proven to be retained by the target.
pub fn clean() -> Result<()> {
    let repo_root = std::env::current_dir()?;
    cleanup_orphaned_worktrees(&repo_root)
}

/// Get the base worktrees directory
pub fn worktrees_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
}

/// Remove a specific worktree and its branch after merge conflict resolution
///
/// This command is used after resolving merge conflicts manually or in a resolver session.
/// It cleans up the worktree and branch WITHOUT attempting another merge.
///
/// # Use Case
/// When auto-merge encounters conflicts:
/// 1. A resolver session is spawned to resolve conflicts
/// 2. The resolver merges `loom/<stage>`, resolves conflicts, and commits
/// 3. The merge is complete but worktree/branch still exist
/// 4. Run `loom worktree remove <stage>` to clean up
///
/// Supports prefix matching: `loom worktree remove pref` will match `prefix-matching`
/// if it's the only worktree starting with "pref".
pub fn remove(stage_id: String, force: bool, confirmation: Option<String>) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    let work_dir = crate::commands::common::work_dir_path()
        .context("state directory not found. Run 'loom init' first.")?;

    let actual_stage_id = resolve_stage_id(&repo_root, &stage_id)?;
    print_removal_header(&actual_stage_id, force);
    let _lock = MergeLock::acquire(&work_dir, Duration::from_secs(30))
        .context("Could not acquire merge lock for worktree removal")?;

    if force {
        return remove_destructively(
            &actual_stage_id,
            confirmation.as_deref(),
            &repo_root,
            &work_dir,
        );
    }

    clear_stale_warning_when_nothing_remains(&actual_stage_id, &repo_root, &work_dir)?;

    let stage = load_stage(&actual_stage_id, &work_dir)
        .with_context(|| format!("Cannot verify safe removal for stage '{actual_stage_id}'"))?;
    let completed_commit = validate_completed_stage(&stage)?.to_string();
    let target_branch = removal_target_branch(&work_dir, &repo_root)?;
    let result = cleanup_verified_stage(
        &actual_stage_id,
        &completed_commit,
        &target_branch,
        &repo_root,
    )?;
    mark_verified_merge(&actual_stage_id, &completed_commit, &work_dir)?;
    print_cleanup_result(&actual_stage_id, &result);
    println!(
        "  {} Stage marked as merged after ancestry verification",
        "✓".green()
    );
    Ok(())
}

/// Destructive removal path for `force`: bypasses ancestry verification and
/// removes the stage's git resources unconditionally after the exact
/// confirmation check inside `cleanup_destructive_stage`. Deliberately makes
/// no claim that the stage was merged.
fn remove_destructively(
    stage_id: &str,
    confirmation: Option<&str>,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    let result = cleanup_destructive_stage(stage_id, confirmation.unwrap_or_default(), repo_root)?;
    // Best-effort: a destructive removal proves nothing about the merge, but
    // it does remove the worktree/branch a stale cleanup warning pointed at.
    let _ = update_stage(stage_id, work_dir, |stage| {
        stage.cleanup_warning = None;
        Ok(())
    });
    print_cleanup_result(stage_id, &result);
    println!("  Merge state left unchanged; destructive removal does not prove a merge.");
    Ok(())
}

/// When none of the stage's git resources remain (worktree, `loom/<id>`,
/// `loom/_base/<id>`), a stale `cleanup_warning` from a hand-removal has
/// nothing left to warn about; clear it as a side effect. Deliberately does
/// NOT short-circuit removal: normal removal must still flow into
/// `cleanup_verified_stage`, whose `require_resources` bails with "refusing
/// to infer that it was merged" — a stage with no resources left is exactly
/// the case that guard exists to refuse, not a success.
fn clear_stale_warning_when_nothing_remains(
    stage_id: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    if stage_resources_exist(stage_id, repo_root)? {
        return Ok(());
    }
    let _ = update_stage(stage_id, work_dir, |stage| {
        stage.cleanup_warning = None;
        Ok(())
    });
    println!("  Nothing left on disk for stage '{stage_id}'; cleared its stale cleanup warning.");
    Ok(())
}

fn resolve_stage_id(repo_root: &Path, requested: &str) -> Result<String> {
    crate::validation::validate_id(requested).context("Invalid worktree stage ID or prefix")?;
    let Some(path) = find_worktree_by_prefix(repo_root, requested)? else {
        return Ok(requested.to_string());
    };
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Worktree path has no valid stage ID: {}", path.display()))
}

fn validate_completed_stage(stage: &Stage) -> Result<&str> {
    if stage.status != StageStatus::Completed {
        bail!(
            "Stage '{}' is {}; normal removal requires Completed status",
            stage.id,
            stage.status
        );
    }
    stage
        .completed_commit
        .as_deref()
        .filter(|commit| !commit.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Stage '{}' has no retained completed commit", stage.id))
}

fn removal_target_branch(work_dir: &Path, repo_root: &Path) -> Result<String> {
    let configured = crate::fs::work_dir::load_config(work_dir)
        .context("Failed to load Loom configuration for removal verification")?
        .and_then(|config| config.base_branch());
    match configured {
        Some(branch) => Ok(branch),
        None => default_branch(repo_root).context("Failed to resolve removal target branch"),
    }
}

fn mark_verified_merge(stage_id: &str, completed_commit: &str, work_dir: &Path) -> Result<()> {
    update_stage(stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Completed {
            bail!("Stage changed status during cleanup; refusing merge-state update");
        }
        if stage.completed_commit.as_deref() != Some(completed_commit) {
            bail!("Stage completed commit changed during cleanup; refusing merge-state update");
        }
        stage.merged = true;
        stage.cleanup_warning = None;
        Ok(())
    })?;
    Ok(())
}

fn print_removal_header(stage_id: &str, force: bool) {
    let mode = if force {
        "destructively removing"
    } else {
        "safely cleaning"
    };
    println!();
    println!("{} {}", mode.cyan().bold(), stage_id.cyan());
    println!("{}", "─".repeat(50).dimmed());
}

fn print_cleanup_result(stage_id: &str, result: &CleanupResult) {
    if result.worktree_removed {
        println!("  {} Removed .worktrees/{stage_id}", "✓".green().bold());
    }
    if result.branch_deleted {
        println!(
            "  {} Deleted {}",
            "✓".green().bold(),
            branch_name_for_stage(stage_id).dimmed()
        );
    }
    if result.base_branch_deleted {
        println!("  {} Deleted loom/_base/{stage_id}", "✓".green().bold());
    }
}
