//! Git merge operations for integrating worktree branches

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use super::branch::{branch_exists, current_branch};

/// Result of a merge operation
#[derive(Debug, Clone)]
pub enum MergeResult {
    /// Merge completed successfully
    Success {
        /// Number of files changed
        files_changed: u32,
        /// Number of insertions
        insertions: u32,
        /// Number of deletions
        deletions: u32,
    },
    /// Merge has conflicts that need resolution
    Conflict {
        /// List of files with conflicts
        conflicting_files: Vec<String>,
    },
    /// Fast-forward merge (no actual merge commit needed)
    FastForward,
    /// Nothing to merge (branches are identical)
    AlreadyUpToDate,
}

/// Merge a stage branch to target branch (typically main)
///
/// Steps:
/// 1. Checkout target branch
/// 2. Merge stage branch (loom/{stage_id})
/// 3. Return merge result
pub fn merge_stage(stage_id: &str, target_branch: &str, repo_root: &Path) -> Result<MergeResult> {
    let branch_name = format!("loom/{stage_id}");

    // First, check that the branch exists
    if !branch_exists(&branch_name, repo_root)? {
        bail!("Branch '{branch_name}' does not exist");
    }

    // Get current branch to restore later if needed
    let original_branch = current_branch(repo_root)?;

    // Checkout target branch
    checkout_branch(target_branch, repo_root)?;

    // Attempt merge
    let output = Command::new("git")
        .args(["merge", "--no-ff", "-m"])
        .arg(format!("Merge {branch_name} into {target_branch}"))
        .arg(&branch_name)
        .current_dir(repo_root)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute git merge {} in {}",
                branch_name,
                repo_root.display()
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        // Parse merge output to determine result type
        if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
            return Ok(MergeResult::AlreadyUpToDate);
        }

        if stdout.contains("Fast-forward") {
            return Ok(MergeResult::FastForward);
        }

        // Parse stats from merge output
        let stats = parse_merge_stats(&stdout);
        return Ok(MergeResult::Success {
            files_changed: stats.0,
            insertions: stats.1,
            deletions: stats.2,
        });
    }

    // Check for conflicts
    if stderr.contains("CONFLICT") || stdout.contains("CONFLICT") {
        let conflicts = get_conflicting_files(repo_root)?;

        // Abort the merge to leave repo in clean state
        abort_merge(repo_root).ok(); // Ignore abort errors

        // Restore original branch
        checkout_branch(&original_branch, repo_root).ok();

        return Ok(MergeResult::Conflict {
            conflicting_files: conflicts,
        });
    }

    // Some other error - include comprehensive diagnostics
    let exit_code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".to_string());

    bail!(
        "git merge failed (exit code {exit_code}):\n\
         Directory: {}\n\
         Stdout: {}\n\
         Stderr: {}",
        repo_root.display(),
        if stdout.is_empty() {
            "(empty)"
        } else {
            stdout.trim()
        },
        if stderr.is_empty() {
            "(empty)"
        } else {
            stderr.trim()
        }
    );
}

/// Parse merge statistics from git output
fn parse_merge_stats(output: &str) -> (u32, u32, u32) {
    let mut files_changed = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;

    for line in output.lines() {
        // Look for line like: "3 files changed, 10 insertions(+), 5 deletions(-)"
        if line.contains("files changed") || line.contains("file changed") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "files" || *part == "file") && i > 0 {
                    files_changed = parts[i - 1].parse().unwrap_or(0);
                }
                if part.contains("insertion") && i > 0 {
                    insertions = parts[i - 1].parse().unwrap_or(0);
                }
                if part.contains("deletion") && i > 0 {
                    deletions = parts[i - 1].parse().unwrap_or(0);
                }
            }
        }
    }

    (files_changed, insertions, deletions)
}

/// Get list of files with conflicts (during an active merge)
fn get_conflicting_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_root)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute git diff --name-only --diff-filter=U in {}",
                repo_root.display()
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}

/// Get list of files that would conflict if we merged a branch.
///
/// This uses `git merge --no-commit` to test the merge, then aborts.
/// Returns the list of conflicting files, or empty if merge would succeed.
pub fn get_conflicting_files_from_status(
    source_branch: &str,
    target_branch: &str,
    repo_root: &Path,
) -> Result<Vec<String>> {
    // Save current branch
    let original_branch = current_branch(repo_root)?;

    // Checkout target branch
    checkout_branch(target_branch, repo_root)?;

    // Try merge with --no-commit to test for conflicts
    let output = Command::new("git")
        .args(["merge", "--no-commit", "--no-ff", source_branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute test merge of {} in {}",
                source_branch,
                repo_root.display()
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let conflicts = if !output.status.success()
        && (stderr.contains("CONFLICT") || stdout.contains("CONFLICT"))
    {
        // Get the conflicting files
        get_conflicting_files(repo_root).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Always abort the test merge and restore original branch
    abort_merge(repo_root).ok();
    checkout_branch(&original_branch, repo_root).ok();

    Ok(conflicts)
}

/// Abort a merge in progress
pub fn abort_merge(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["merge", "--abort"])
        .current_dir(repo_root)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute git merge --abort in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());

        bail!(
            "git merge --abort failed (exit code {exit_code}):\n\
             Directory: {}\n\
             Stdout: {}\n\
             Stderr: {}",
            repo_root.display(),
            if stdout.is_empty() {
                "(empty)"
            } else {
                stdout.trim()
            },
            if stderr.is_empty() {
                "(empty)"
            } else {
                stderr.trim()
            }
        );
    }

    Ok(())
}

/// Checkout a branch
pub fn checkout_branch(branch_name: &str, repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", branch_name])
        .current_dir(repo_root)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute git checkout {} in {}",
                branch_name,
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());

        bail!(
            "git checkout '{branch_name}' failed (exit code {exit_code}):\n\
             Directory: {}\n\
             Stdout: {}\n\
             Stderr: {}",
            repo_root.display(),
            if stdout.is_empty() {
                "(empty)"
            } else {
                stdout.trim()
            },
            if stderr.is_empty() {
                "(empty)"
            } else {
                stderr.trim()
            }
        );
    }

    Ok(())
}

/// Get conflict resolution instructions
pub fn conflict_resolution_instructions(
    stage_id: &str,
    target_branch: &str,
    conflicts: &[String],
) -> String {
    let mut instructions = String::new();

    instructions.push_str(&format!(
        "Merge conflict detected when merging loom/{stage_id} into {target_branch}\n\n"
    ));
    instructions.push_str("Conflicting files:\n");
    for file in conflicts {
        instructions.push_str(&format!("  - {file}\n"));
    }
    instructions.push_str("\nTo resolve:\n");
    instructions.push_str("  1. cd to repository root\n");
    instructions.push_str(&format!("  2. git checkout {target_branch}\n"));
    instructions.push_str(&format!("  3. git merge loom/{stage_id}\n"));
    instructions.push_str("  4. Resolve conflicts in the listed files\n");
    instructions.push_str("  5. git add <resolved files>\n");
    instructions.push_str("  6. git commit\n");
    instructions.push_str(&format!(
        "  7. loom worktree remove {stage_id} (to clean up worktree and branch)\n"
    ));

    instructions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_merge_stats() {
        let output = " 3 files changed, 10 insertions(+), 5 deletions(-)\n";
        let (files, ins, del) = parse_merge_stats(output);
        assert_eq!(files, 3);
        assert_eq!(ins, 10);
        assert_eq!(del, 5);
    }

    #[test]
    fn test_parse_merge_stats_single_file() {
        let output = " 1 file changed, 2 insertions(+)\n";
        let (files, ins, del) = parse_merge_stats(output);
        assert_eq!(files, 1);
        assert_eq!(ins, 2);
        assert_eq!(del, 0);
    }

    #[test]
    fn test_conflict_resolution_instructions() {
        let instructions = conflict_resolution_instructions(
            "stage-1",
            "main",
            &["src/lib.rs".to_string(), "Cargo.toml".to_string()],
        );

        assert!(instructions.contains("loom/stage-1"));
        assert!(instructions.contains("src/lib.rs"));
        assert!(instructions.contains("Cargo.toml"));
        // Should reference worktree remove for cleanup, not loom merge
        assert!(instructions.contains("loom worktree remove stage-1"));
        assert!(!instructions.contains("loom merge stage-1"));
    }
}
