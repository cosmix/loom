//! Worktree management commands
//! Usage: `loom worktree [list|remove <stage-id>]`

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::git::branch::{branch_name_for_stage, default_branch};
use crate::git::cleanup::{
    cleanup_destructive_stage, cleanup_orphaned_worktrees, cleanup_verified_stage, CleanupResult,
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
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let actual_stage_id = resolve_stage_id(&repo_root, &stage_id)?;
    print_removal_header(&actual_stage_id, force);
    let _lock = MergeLock::acquire(&work_dir, Duration::from_secs(30))
        .context("Could not acquire merge lock for worktree removal")?;

    if force {
        let result = cleanup_destructive_stage(
            &actual_stage_id,
            confirmation.as_deref().unwrap_or_default(),
            &repo_root,
        )?;
        print_cleanup_result(&actual_stage_id, &result);
        println!("  Stage state left unchanged; destructive removal does not prove a merge.");
        return Ok(());
    }

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
