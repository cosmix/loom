//! Clean command for loom resource cleanup
//! Usage: loom clean [--all] [--worktrees] [--sessions] [--state]

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Execute the clean command
///
/// # Arguments
/// * `all` - Remove all loom resources
/// * `worktrees` - Remove only worktrees
/// * `sessions` - Kill only tmux sessions
/// * `state` - Remove only .work/ state directory
///
/// If no flags are provided, cleans everything (same as --all)
pub fn execute(all: bool, worktrees: bool, sessions: bool, state: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    // If no specific flags provided, clean everything
    let clean_all = all || (!worktrees && !sessions && !state);

    let mut cleaned_something = false;

    // Clean worktrees
    if (clean_all || worktrees) && clean_worktrees(&repo_root)? {
        cleaned_something = true;
    }

    // Clean tmux sessions
    if (clean_all || sessions) && clean_tmux_sessions()? {
        cleaned_something = true;
    }

    // Clean state directory
    if (clean_all || state) && clean_state_directory(&repo_root)? {
        cleaned_something = true;
    }

    if cleaned_something {
        println!("\nloom cleanup complete.");
    } else {
        println!("Nothing to clean.");
    }

    Ok(())
}

/// Clean up all loom worktrees and their branches
///
/// Returns true if any cleanup was performed
fn clean_worktrees(repo_root: &Path) -> Result<bool> {
    let worktrees_dir = repo_root.join(".worktrees");

    // First, always prune stale git worktrees
    println!("Pruning stale git worktrees...");
    let prune_output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    match prune_output {
        Ok(result) if result.status.success() => {
            println!("  Stale worktrees pruned");
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            eprintln!("  Warning: Failed to prune worktrees: {}", stderr.trim());
        }
        Err(e) => {
            eprintln!("  Warning: Failed to prune worktrees: {e}");
        }
    }

    if !worktrees_dir.exists() {
        println!("No .worktrees/ directory to clean");
        return Ok(false);
    }

    println!("Removing loom worktrees...");

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
                        println!("  Removed worktree: {stage_id}");
                        removed_count += 1;
                    }
                    Ok(_) | Err(_) => {
                        // If git worktree remove fails, try manual removal
                        if let Err(e) = fs::remove_dir_all(&path) {
                            eprintln!("  Warning: Failed to remove {}: {e}", path.display());
                        } else {
                            println!("  Removed worktree directory: {stage_id}");
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
    if !branch_names.is_empty() {
        println!("Removing loom branches...");
        for branch_name in &branch_names {
            let delete_result = Command::new("git")
                .args(["branch", "-D", branch_name])
                .current_dir(repo_root)
                .output();

            match delete_result {
                Ok(result) if result.status.success() => {
                    println!("  Deleted branch: {branch_name}");
                }
                Ok(result) => {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    // Don't warn if branch doesn't exist
                    if !stderr.contains("not found") {
                        eprintln!("  Warning: Failed to delete branch '{branch_name}': {}", stderr.trim());
                    }
                }
                Err(e) => {
                    eprintln!("  Warning: Failed to delete branch '{branch_name}': {e}");
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
        println!("  Removed .worktrees/ directory");
    }

    if removed_count > 0 {
        println!("  Cleaned {removed_count} worktree(s)");
    }

    Ok(true)
}

/// Kill all loom-* tmux sessions
///
/// Returns true if any sessions were killed
fn clean_tmux_sessions() -> Result<bool> {
    println!("Cleaning loom tmux sessions...");

    // List all tmux sessions with loom- prefix
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    let sessions: Vec<String> = match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout
                .lines()
                .filter(|line| line.starts_with("loom-"))
                .map(|s| s.to_string())
                .collect()
        }
        Ok(_) => {
            // tmux returns non-zero when no sessions exist
            println!("  No tmux sessions found");
            return Ok(false);
        }
        Err(_) => {
            // tmux might not be installed
            println!("  tmux not available or no sessions");
            return Ok(false);
        }
    };

    if sessions.is_empty() {
        println!("  No loom tmux sessions found");
        return Ok(false);
    }

    let mut killed_count = 0;
    for session_name in &sessions {
        match Command::new("tmux")
            .args(["kill-session", "-t", session_name])
            .output()
        {
            Ok(result) if result.status.success() => {
                println!("  Killed session: {session_name}");
                killed_count += 1;
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                eprintln!(
                    "  Warning: Failed to kill session '{}': {}",
                    session_name,
                    stderr.trim()
                );
            }
            Err(e) => {
                eprintln!("  Warning: Failed to kill session '{session_name}': {e}");
            }
        }
    }

    if killed_count > 0 {
        println!("  Killed {killed_count} tmux session(s)");
    }

    Ok(killed_count > 0)
}

/// Remove the .work/ state directory
///
/// Returns true if the directory was removed
fn clean_state_directory(repo_root: &Path) -> Result<bool> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        println!("No .work/ directory to clean");
        return Ok(false);
    }

    println!("Removing .work/ state directory...");
    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!("  Removed .work/ directory");

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
        // Returns false when no .worktrees directory exists
        assert!(!result.unwrap());
    }
}
