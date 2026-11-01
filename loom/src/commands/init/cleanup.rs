//! Cleanup functions for loom init command.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::commands::common::resolve_state_dir;
use crate::fs::memory::archive_state_dir;
use crate::git::branch::branch_name_for_stage;
use crate::git::runner::run_git;
use crate::orchestrator::terminal::tmux::reap_sockets;
pub use crate::orchestrator::terminal::tmux::SessionReapMode;

/// Prune stale git worktrees that have been deleted but are still registered
pub fn prune_stale_worktrees(repo_root: &Path) -> Result<()> {
    let result = run_git(&["worktree", "prune"], repo_root);

    match result {
        Ok(output) if output.status.success() => {
            println!("  {} Stale worktrees pruned", "✓".green().bold());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
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

/// Reap sockets attributed to this work directory.
pub fn cleanup_orphaned_sessions(repo_root: &Path, mode: SessionReapMode) -> Result<()> {
    let work_dir = resolve_state_dir(repo_root);

    let stats = reap_sockets(&work_dir, mode)?;
    let orphaned_reaped = stats.orphaned;
    let live_reaped = stats.live;
    let unattributed = stats.unattributed;

    let total_reaped = orphaned_reaped + live_reaped;
    if total_reaped == 0 {
        println!("  {} No orphaned sessions to clean", "✓".green().bold());
    } else if live_reaped == 0 {
        println!(
            "  {} Reaped {} orphaned tmux session{}",
            "✓".green().bold(),
            orphaned_reaped,
            if orphaned_reaped == 1 { "" } else { "s" }
        );
    } else {
        println!(
            "  {} Reaped {} tmux session{} ({} orphaned, {} still live — about to be \
             orphaned by --clean)",
            "✓".green().bold(),
            total_reaped,
            if total_reaped == 1 { "" } else { "s" },
            orphaned_reaped,
            live_reaped
        );
    }

    if unattributed > 0 {
        println!(
            "  {} {} unattributable tmux socket{} left untouched",
            "─".dimmed(),
            unattributed,
            if unattributed == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Remove the existing state directory
///
/// The caller must have already run [`crate::commands::stop::ensure_daemon_stopped`]
/// on this directory; this function does not check for a live daemon itself.
pub fn cleanup_work_directory(repo_root: &Path) -> Result<()> {
    let work_dir = resolve_state_dir(repo_root);

    if !work_dir.exists() {
        return Ok(());
    }

    archive_state_dir(&work_dir, repo_root)?;

    fs::remove_dir_all(&work_dir)
        .with_context(|| format!("Failed to remove state directory at {}", work_dir.display()))?;
    println!(
        "  {} Removed old {}",
        "✓".green().bold(),
        work_dir.display().to_string().dimmed()
    );

    Ok(())
}

/// Remove the state directory silently (used for cleanup on initialization failure)
pub fn remove_work_directory_on_failure(repo_root: &Path) {
    let work_dir = resolve_state_dir(repo_root);

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

                let path_str = path.to_string_lossy().to_string();
                let _ = run_git(&["worktree", "remove", "--force", &path_str], repo_root);

                let branch_name = branch_name_for_stage(&stage_id);
                let _ = run_git(&["branch", "-D", &branch_name], repo_root);
            }
        }
    }

    let _ = run_git(&["worktree", "prune"], repo_root);

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

#[cfg(test)]
mod archive_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cleanup_work_directory_archives_memory_before_removal() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join(".loom/work");
        fs::create_dir_all(work_dir.join("memory")).unwrap();
        fs::write(work_dir.join("memory/stage.md"), "journal").unwrap();

        cleanup_work_directory(temp.path()).unwrap();

        let archive_root = temp.path().join(".loom/memory/archive");
        let archive = fs::read_dir(archive_root).unwrap().next().unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(archive.path().join("memory/stage.md")).unwrap(),
            "journal"
        );
    }

    #[test]
    fn cleanup_work_directory_keeps_state_when_archiving_fails() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join(".loom/work");
        fs::create_dir_all(work_dir.join("memory")).unwrap();
        fs::write(work_dir.join("memory/stage.md"), "journal").unwrap();
        fs::create_dir_all(temp.path().join(".loom/memory")).unwrap();
        fs::write(temp.path().join(".loom/memory/archive"), "blocker").unwrap();

        assert!(cleanup_work_directory(temp.path()).is_err());

        assert!(work_dir.join("memory/stage.md").exists());
    }
}
