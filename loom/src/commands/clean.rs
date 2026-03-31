//! Clean command for loom resource cleanup
//! Usage: loom clean [--all] [--worktrees] [--sessions] [--state]

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::git::cleanup::{
    cleanup_all_base_branches, cleanup_multiple_stages, prune_worktrees, CleanupConfig,
};

/// Statistics for cleanup operations
#[derive(Default)]
struct CleanStats {
    worktrees_removed: usize,
    branches_removed: usize,
    sessions_killed: usize,
    state_removed: bool,
}

/// Execute the clean command
///
/// # Arguments
/// * `all` - Remove all loom resources
/// * `worktrees` - Remove only worktrees
/// * `sessions` - Kill only sessions
/// * `state` - Remove only .work/ state directory
///
/// If no flags are provided, cleans everything (same as --all)
pub fn execute(all: bool, worktrees: bool, sessions: bool, state: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    // Print header
    print_header();

    // If no specific flags provided, clean everything
    let clean_all = all || (!worktrees && !sessions && !state);

    let mut stats = CleanStats::default();

    // Clean worktrees
    if clean_all || worktrees {
        println!("\n{}", "Worktrees".bold());
        println!("{}", "─".repeat(40).dimmed());
        let (wt_count, br_count) = clean_worktrees(&repo_root)?;
        stats.worktrees_removed = wt_count;
        stats.branches_removed = br_count;
    }

    // Clean sessions
    if clean_all || sessions {
        println!("\n{}", "Sessions".bold());
        println!("{}", "─".repeat(40).dimmed());
        stats.sessions_killed = clean_sessions()?;
    }

    // Clean state directory
    if clean_all || state {
        println!("\n{}", "State".bold());
        println!("{}", "─".repeat(40).dimmed());
        stats.state_removed = clean_state_directory(&repo_root)?;
    }

    print_summary(&stats);

    Ok(())
}

/// Print the loom clean header
fn print_header() {
    crate::utils::print_logo_header("Cleaning...");
}

/// Print the final summary
fn print_summary(stats: &CleanStats) {
    println!();
    println!("{}", "═".repeat(40).dimmed());

    let has_cleanup = stats.worktrees_removed > 0
        || stats.branches_removed > 0
        || stats.sessions_killed > 0
        || stats.state_removed;

    if has_cleanup {
        println!("{} Cleanup complete", "✓".green().bold());

        let mut items: Vec<String> = Vec::new();
        if stats.worktrees_removed > 0 {
            items.push(format!(
                "{} worktree{}",
                stats.worktrees_removed,
                if stats.worktrees_removed == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if stats.branches_removed > 0 {
            items.push(format!(
                "{} branch{}",
                stats.branches_removed,
                if stats.branches_removed == 1 {
                    ""
                } else {
                    "es"
                }
            ));
        }
        if stats.sessions_killed > 0 {
            items.push(format!(
                "{} session{}",
                stats.sessions_killed,
                if stats.sessions_killed == 1 { "" } else { "s" }
            ));
        }
        if stats.state_removed {
            items.push("state directory".to_string());
        }

        println!("  Removed: {}", items.join(", ").dimmed());
    } else {
        println!("{} Nothing to clean", "✓".green().bold());
    }
    println!();
}

/// Clean up all loom worktrees and their branches
///
/// Returns (worktrees_removed, branches_removed) counts
fn clean_worktrees(repo_root: &Path) -> Result<(usize, usize)> {
    let worktrees_dir = repo_root.join(".worktrees");

    // First, always prune stale git worktrees
    match prune_worktrees(repo_root) {
        Ok(()) => {
            println!("  {} Stale worktrees pruned", "✓".green().bold());
        }
        Err(e) => {
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    if !worktrees_dir.exists() {
        println!("  {} No {} directory", "─".dimmed(), ".worktrees/".dimmed());
        return Ok((0, 0));
    }

    // Collect all stage IDs from .worktrees/ directory
    let mut stage_ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let stage_id = entry.file_name().to_string_lossy().to_string();
                stage_ids.push(stage_id);
            }
        }
    }

    // Use shared cleanup utilities for batch cleanup
    let config = CleanupConfig::forced();
    let stage_id_refs: Vec<&str> = stage_ids.iter().map(|s| s.as_str()).collect();
    let results = cleanup_multiple_stages(&stage_id_refs, repo_root, &config);

    // Count successes and print results
    let mut worktrees_removed = 0;
    let mut branches_removed = 0;

    for (stage_id, result) in results {
        if result.worktree_removed {
            println!(
                "  {} Removed worktree: {}",
                "✓".green().bold(),
                stage_id.dimmed()
            );
            worktrees_removed += 1;
        }

        if result.branch_deleted {
            println!(
                "  {} Deleted branch: {}",
                "✓".green().bold(),
                format!("loom/{stage_id}").dimmed()
            );
            branches_removed += 1;
        }

        // Print warnings if any
        for warning in &result.warnings {
            println!(
                "  {} {}: {}",
                "⚠".yellow().bold(),
                stage_id,
                warning.dimmed()
            );
        }
    }

    // Clean up base branches as well
    match cleanup_all_base_branches(repo_root) {
        Ok(deleted) => {
            for branch in deleted {
                println!(
                    "  {} Deleted base branch: {}",
                    "✓".green().bold(),
                    branch.dimmed()
                );
            }
        }
        Err(e) => {
            println!(
                "  {} Failed to clean base branches: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    // Remove the .worktrees directory itself if it still exists
    if worktrees_dir.exists() {
        fs::remove_dir_all(&worktrees_dir).with_context(|| {
            format!(
                "Failed to remove .worktrees/ directory at {}",
                worktrees_dir.display()
            )
        })?;
        println!(
            "  {} Removed {}",
            "✓".green().bold(),
            ".worktrees/".dimmed()
        );
    }

    Ok((worktrees_removed, branches_removed))
}

/// Kill all loom sessions
///
/// Returns the number of sessions killed
fn clean_sessions() -> Result<usize> {
    println!(
        "  {} Session cleanup not yet implemented for native backend",
        "─".dimmed()
    );
    println!(
        "  {} Use 'loom sessions kill' to kill specific sessions",
        "→".dimmed()
    );
    Ok(0)
}

/// Remove the .work/ state directory
///
/// Returns true if the directory was removed
fn clean_state_directory(repo_root: &Path) -> Result<bool> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        println!("  {} No {} directory", "─".dimmed(), ".work/".dimmed());
        return Ok(false);
    }

    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!("  {} Removed {}", "✓".green().bold(), ".work/".dimmed());

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_clean_state_directory_when_exists() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir(&work_dir).unwrap();
        fs::write(work_dir.join("test.txt"), "test").unwrap();

        let result = clean_state_directory(temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!work_dir.exists());
    }

    #[test]
    fn test_clean_state_directory_when_not_exists() {
        let temp_dir = TempDir::new().unwrap();

        let result = clean_state_directory(temp_dir.path());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_clean_worktrees_when_no_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize as a git repo so git commands don't fail
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let result = clean_worktrees(temp_dir.path());
        assert!(result.is_ok());
        // Returns (0, 0) when no .worktrees directory exists
        assert_eq!(result.unwrap(), (0, 0));
    }
}
