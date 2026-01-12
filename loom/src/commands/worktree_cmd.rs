//! Worktree management commands
//! Usage: loom worktree [list|clean|remove <stage-id>]

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::commands::merge::mark_stage_merged;
use crate::fs::stage_files::find_stage_file;
use crate::git::cleanup::{cleanup_after_merge, prune_worktrees, CleanupConfig};
use crate::git::worktree::find_worktree_by_prefix;
use crate::models::stage::StageStatus;
use crate::verify::transitions::parse_stage_from_markdown;

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
///
/// A worktree is considered orphaned if:
/// - The corresponding stage file doesn't exist
/// - The stage is in a terminal state (Completed, Blocked, Skipped)
/// - The stage is waiting for dependencies (not actively executing)
///
/// Active states that keep worktrees alive: Executing, NeedsHandoff, MergeConflict, WaitingForInput
pub fn clean() -> Result<()> {
    println!("Cleaning orphaned worktrees...");
    println!("{}", "─".repeat(50).dimmed());

    let repo_root = std::env::current_dir()?;
    let worktrees_dir = repo_root.join(".worktrees");
    let work_dir = repo_root.join(".work");
    let stages_dir = work_dir.join("stages");

    if !worktrees_dir.exists() {
        println!("No .worktrees/ directory to clean");
        return Ok(());
    }

    // Collect worktree stage IDs
    let mut worktree_ids: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name();
                worktree_ids.push(name.to_string_lossy().to_string());
            }
        }
    }

    if worktree_ids.is_empty() {
        println!("No worktrees found");
        prune_worktrees(&repo_root)?;
        println!("{} Pruned stale worktree references", "✓".green().bold());
        return Ok(());
    }

    println!(
        "Found {} worktree(s): {}",
        worktree_ids.len(),
        worktree_ids.join(", ").dimmed()
    );
    println!();

    // Check each worktree for orphan status
    let mut orphaned: Vec<String> = Vec::new();
    let mut active: Vec<String> = Vec::new();

    for stage_id in &worktree_ids {
        let is_orphan = match find_stage_file(&stages_dir, stage_id)? {
            None => {
                // No stage file exists - orphaned
                println!(
                    "  {} {} (no stage file)",
                    "orphan:".yellow(),
                    stage_id.cyan()
                );
                true
            }
            Some(stage_path) => {
                // Parse stage file to check status
                match std::fs::read_to_string(&stage_path) {
                    Ok(content) => match parse_stage_from_markdown(&content) {
                        Ok(stage) => {
                            let status = &stage.status;
                            let is_active = matches!(
                                status,
                                StageStatus::Executing
                                    | StageStatus::NeedsHandoff
                                    | StageStatus::MergeConflict
                                    | StageStatus::WaitingForInput
                                    | StageStatus::Queued
                            );

                            if is_active {
                                println!(
                                    "  {} {} ({})",
                                    "active:".green(),
                                    stage_id.cyan(),
                                    format!("{status}").dimmed()
                                );
                                false
                            } else {
                                println!(
                                    "  {} {} ({})",
                                    "orphan:".yellow(),
                                    stage_id.cyan(),
                                    format!("{status}").dimmed()
                                );
                                true
                            }
                        }
                        Err(_) => {
                            // Can't parse stage - treat as orphan
                            println!(
                                "  {} {} (unparseable stage file)",
                                "orphan:".yellow(),
                                stage_id.cyan()
                            );
                            true
                        }
                    },
                    Err(_) => {
                        // Can't read stage file - treat as orphan
                        println!(
                            "  {} {} (unreadable stage file)",
                            "orphan:".yellow(),
                            stage_id.cyan()
                        );
                        true
                    }
                }
            }
        };

        if is_orphan {
            orphaned.push(stage_id.clone());
        } else {
            active.push(stage_id.clone());
        }
    }

    println!();

    if orphaned.is_empty() {
        println!(
            "{} No orphaned worktrees to clean ({} active)",
            "✓".green().bold(),
            active.len()
        );
    } else {
        println!(
            "Cleaning {} orphaned worktree(s)...",
            orphaned.len().to_string().yellow()
        );

        let config = CleanupConfig {
            force_worktree_removal: true,
            force_branch_deletion: true,
            prune_worktrees: false, // We'll prune at the end
            verbose: false,
        };

        for stage_id in &orphaned {
            match cleanup_after_merge(stage_id, &repo_root, &config) {
                Ok(result) => {
                    let mut actions = Vec::new();
                    if result.worktree_removed {
                        actions.push("worktree");
                    }
                    if result.branch_deleted {
                        actions.push("branch");
                    }
                    if result.base_branch_deleted {
                        actions.push("base branch");
                    }

                    if actions.is_empty() {
                        println!("  {} {} (already clean)", "─".dimmed(), stage_id);
                    } else {
                        println!(
                            "  {} {} (removed: {})",
                            "✓".green().bold(),
                            stage_id,
                            actions.join(", ")
                        );
                    }

                    for warning in &result.warnings {
                        println!("    {} {}", "⚠".yellow(), warning.dimmed());
                    }
                }
                Err(e) => {
                    println!("  {} {} ({})", "✗".red().bold(), stage_id, e);
                }
            }
        }
    }

    // Always prune stale worktree references
    println!();
    prune_worktrees(&repo_root)?;
    println!("{} Pruned stale worktree references", "✓".green().bold());

    println!();
    println!("{} Cleanup complete!", "✓".green().bold());

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
///
/// Supports prefix matching: `loom worktree remove pref` will match `prefix-matching`
/// if it's the only worktree starting with "pref".
pub fn remove(stage_id: String) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    // Resolve stage_id using prefix matching
    let (worktree_path, actual_stage_id) = match find_worktree_by_prefix(&repo_root, &stage_id)? {
        Some(path) => {
            let actual_id = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&stage_id)
                .to_string();
            (path, actual_id)
        }
        None => {
            // No worktree found, but branch might still exist
            // Fall back to the provided stage_id
            (
                repo_root.join(".worktrees").join(&stage_id),
                stage_id.clone(),
            )
        }
    };

    println!();
    println!(
        "{} {} {}",
        "Cleaning up".cyan().bold(),
        "stage:".dimmed(),
        actual_stage_id.cyan()
    );
    println!("{}", "─".repeat(50).dimmed());

    let branch_name = format!("loom/{actual_stage_id}");

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
            actual_stage_id
        );
        println!("    {} Worktree not found", "─".dimmed());
        println!(
            "    {} Branch '{}' does not exist",
            "─".dimmed(),
            branch_name
        );

        // Still mark as merged in case stage status wasn't updated
        mark_stage_merged(&actual_stage_id, &work_dir)?;
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

    let result = cleanup_after_merge(&actual_stage_id, &repo_root, &config)?;

    // Report results
    if result.worktree_removed {
        println!(
            "  {} Removed worktree: {}",
            "✓".green().bold(),
            format!(".worktrees/{actual_stage_id}").dimmed()
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
    mark_stage_merged(&actual_stage_id, &work_dir)?;
    println!(
        "  {} Stage '{}' marked as merged",
        "✓".green().bold(),
        actual_stage_id
    );

    println!();
    println!("{} Cleanup complete!", "✓".green().bold());
    println!();

    Ok(())
}
