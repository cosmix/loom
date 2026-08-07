//! Cleanup functions for loom init command.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::runner::run_git;
use crate::orchestrator::terminal::tmux::{
    kill_socket_server, list_loom_sockets, socket_session_is_alive,
};

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

/// Kill any orphaned loom tmux sessions from previous runs.
///
/// A socket is "orphaned" when it is attributed to THIS work dir (a matching
/// `.work/sessions/<id>.md` exists) but its session is no longer alive.
/// Sockets that cannot be attributed to this work dir are reported but never
/// touched — the tmux socket directory is per-user, not per-repository, so an
/// unattributed socket may belong to a colleague's, or another checkout's,
/// live session.
pub fn cleanup_orphaned_sessions(repo_root: &Path) -> Result<()> {
    let work_dir = repo_root.join(".work");

    let mut reaped = 0;
    let mut unattributed = 0;

    for socket in list_loom_sockets(&work_dir) {
        if !socket.attributed {
            unattributed += 1;
            continue;
        }

        if socket_session_is_alive(&work_dir, &socket.session_id) {
            continue;
        }

        kill_socket_server(&socket.path);
        let _ = fs::remove_file(&socket.path);
        reaped += 1;
    }

    if reaped == 0 {
        println!("  {} No orphaned sessions to clean", "✓".green().bold());
    } else {
        println!(
            "  {} Reaped {} orphaned tmux session{}",
            "✓".green().bold(),
            reaped,
            if reaped == 1 { "" } else { "s" }
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
