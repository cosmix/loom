//! Worktree management commands
//! Usage: loom worktree [list|clean|remove <stage-id>]

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::commands::merge::mark_stage_merged;
use crate::git::cleanup::{cleanup_after_merge, CleanupConfig};

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
                let branch = format!("loom/{stage_name}");
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

/// Clean orphaned worktrees
pub fn clean() -> Result<()> {
    println!("Cleaning orphaned worktrees...");

    let worktrees_dir = std::env::current_dir()?.join(".worktrees");
    if !worktrees_dir.exists() {
        println!("No .worktrees/ directory to clean");
        return Ok(());
    }

    // Would run: git worktree prune
    println!("\nWould run: git worktree prune");
    println!("Note: Full clean requires Phase 4 (git module)");

    Ok(())
}

/// Get the base worktrees directory
pub fn worktrees_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
}

/// Remove a specific worktree and its branch after merge conflict resolution
///
/// This command is used after resolving merge conflicts manually (or via Claude Code).
/// It cleans up the worktree and branch WITHOUT attempting another merge.
///
/// # Use Case
/// When `loom merge` encounters conflicts:
/// 1. A CC session is spawned to resolve conflicts
/// 2. CC runs `git merge loom/<stage>` → resolves → `git add` → `git commit`
/// 3. The merge is complete but worktree/branch still exist
/// 4. Run `loom worktree remove <stage>` to clean up
pub fn remove(stage_id: String) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    println!();
    println!(
        "{} {} {}",
        "Cleaning up".cyan().bold(),
        "stage:".dimmed(),
        stage_id.cyan()
    );
    println!("{}", "─".repeat(50).dimmed());

    // Check if worktree or branch exists
    let worktree_path = repo_root.join(".worktrees").join(&stage_id);
    let branch_name = format!("loom/{stage_id}");

    let worktree_exists = worktree_path.exists();
    let branch_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &branch_name])
        .current_dir(&repo_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !worktree_exists && !branch_exists {
        println!(
            "  {} Stage '{}' is already cleaned up",
            "✓".green().bold(),
            stage_id
        );
        println!("    {} Worktree not found", "─".dimmed());
        println!(
            "    {} Branch '{}' does not exist",
            "─".dimmed(),
            branch_name
        );

        // Still mark as merged in case stage status wasn't updated
        mark_stage_merged(&stage_id, &work_dir)?;
        println!();
        return Ok(());
    }

    // Perform cleanup
    let config = CleanupConfig {
        force_worktree_removal: true,
        force_branch_deletion: true, // Force delete since merge is complete
        prune_worktrees: true,
        verbose: false,
    };

    let result = cleanup_after_merge(&stage_id, &repo_root, &config)?;

    // Report results
    if result.worktree_removed {
        println!(
            "  {} Removed worktree: {}",
            "✓".green().bold(),
            format!(".worktrees/{stage_id}").dimmed()
        );
    } else if worktree_exists {
        println!("  {} Worktree not found (already removed)", "─".dimmed());
    }

    if result.branch_deleted {
        println!(
            "  {} Deleted branch: {}",
            "✓".green().bold(),
            branch_name.dimmed()
        );
    } else if branch_exists {
        println!("  {} Branch not found (already deleted)", "─".dimmed());
    }

    // Report any warnings
    for warning in &result.warnings {
        println!("  {} {}", "⚠".yellow().bold(), warning.dimmed());
    }

    // Mark stage as merged
    mark_stage_merged(&stage_id, &work_dir)?;
    println!(
        "  {} Stage '{}' marked as merged",
        "✓".green().bold(),
        stage_id
    );

    println!();
    println!("{} Cleanup complete!", "✓".green().bold());
    println!();

    Ok(())
}
