//! Clean command for loom resource cleanup
//! Usage: loom clean [--all] [--worktrees] [--sessions] [--state]

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

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
    println!();
    println!("{}", "╭──────────────────────────────────────╮".cyan());
    println!(
        "{}",
        "│         Cleaning Loom...             │".cyan().bold()
    );
    println!("{}", "╰──────────────────────────────────────╯".cyan());
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
    let prune_output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    match prune_output {
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

    if !worktrees_dir.exists() {
        println!("  {} No {} directory", "─".dimmed(), ".worktrees/".dimmed());
        return Ok((0, 0));
    }

    // Collect worktree entries and their branches
    let mut removed_count = 0;
    let mut branch_names: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let stage_id = entry.file_name().to_string_lossy().to_string();

                // Try git worktree remove first (handles git metadata cleanup)
                let remove_result = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .current_dir(repo_root)
                    .output();

                match remove_result {
                    Ok(result) if result.status.success() => {
                        println!(
                            "  {} Removed worktree: {}",
                            "✓".green().bold(),
                            stage_id.dimmed()
                        );
                        removed_count += 1;
                    }
                    Ok(_) | Err(_) => {
                        // If git worktree remove fails, try manual removal
                        if let Err(e) = fs::remove_dir_all(&path) {
                            println!(
                                "  {} Failed to remove {}: {}",
                                "✗".red().bold(),
                                path.display(),
                                e.to_string().dimmed()
                            );
                        } else {
                            println!(
                                "  {} Removed worktree: {}",
                                "✓".green().bold(),
                                stage_id.dimmed()
                            );
                            removed_count += 1;
                        }
                    }
                }

                // Track the branch for deletion
                branch_names.push(format!("loom/{stage_id}"));
            }
        }
    }

    // Delete loom branches
    let mut branches_deleted = 0;
    if !branch_names.is_empty() {
        for branch_name in &branch_names {
            let delete_result = Command::new("git")
                .args(["branch", "-D", branch_name])
                .current_dir(repo_root)
                .output();

            match delete_result {
                Ok(result) if result.status.success() => {
                    println!(
                        "  {} Deleted branch: {}",
                        "✓".green().bold(),
                        branch_name.dimmed()
                    );
                    branches_deleted += 1;
                }
                Ok(result) => {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    // Don't warn if branch doesn't exist
                    if !stderr.contains("not found") {
                        println!(
                            "  {} Branch '{}': {}",
                            "⚠".yellow().bold(),
                            branch_name,
                            stderr.trim().dimmed()
                        );
                    }
                }
                Err(e) => {
                    println!(
                        "  {} Branch '{}': {}",
                        "⚠".yellow().bold(),
                        branch_name,
                        e.to_string().dimmed()
                    );
                }
            }
        }
    }

    // Final prune after removing worktrees
    let _ = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

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

    Ok((removed_count, branches_deleted))
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
