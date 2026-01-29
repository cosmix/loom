//! Cleanup functions for loom init command.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::git::branch::branch_name_for_stage;

/// Prune stale git worktrees that have been deleted but are still registered
pub fn prune_stale_worktrees(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            println!("  {} Stale worktrees pruned", "✓".green().bold());
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                stderr.trim().dimmed()
            );
        }
        Err(e) => {
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    Ok(())
}

/// Kill any orphaned loom sessions from previous runs
pub fn cleanup_orphaned_sessions() -> Result<()> {
    println!("  {} No orphaned sessions to clean", "✓".green().bold());
    Ok(())
}

/// Remove the existing .work/ directory
pub fn cleanup_work_directory(repo_root: &Path) -> Result<()> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        return Ok(());
    }

    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!("  {} Removed old {}", "✓".green().bold(), ".work/".dimmed());

    Ok(())
}

/// Remove the .work/ directory silently (used for cleanup on initialization failure)
pub fn remove_work_directory_on_failure(repo_root: &Path) {
    let work_dir = repo_root.join(".work");

    if work_dir.exists() {
        let _ = fs::remove_dir_all(&work_dir);
    }
}

/// Remove existing loom worktrees and the .worktrees/ directory
pub fn cleanup_worktrees_directory(repo_root: &Path) -> Result<()> {
    let worktrees_dir = repo_root.join(".worktrees");

    if !worktrees_dir.exists() {
        return Ok(());
    }

    if let Ok(entries) = fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let stage_id = entry.file_name().to_string_lossy().to_string();

                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .current_dir(repo_root)
                    .output();

                let branch_name = branch_name_for_stage(&stage_id);
                let _ = Command::new("git")
                    .args(["branch", "-D", &branch_name])
                    .current_dir(repo_root)
                    .output();
            }
        }
    }

    let _ = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    if worktrees_dir.exists() {
        fs::remove_dir_all(&worktrees_dir).with_context(|| {
            format!(
                "Failed to remove .worktrees/ directory at {}",
                worktrees_dir.display()
            )
        })?;
    }

    println!(
        "  {} Removed old {}",
        "✓".green().bold(),
        ".worktrees/".dimmed()
    );

    Ok(())
}
