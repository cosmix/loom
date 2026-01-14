//! Git utility functions for merge operations
//!
//! Contains helpers for checking/managing git status, stashing changes,
//! auto-committing, and cleaning up loom directories from branches.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// Ensure .work and .worktrees are in .gitignore
///
/// This prevents git merge from failing with "would lose untracked files"
/// when .work directory exists in the main repo.
pub fn ensure_work_gitignored(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    let entries_to_add = [".work/", ".worktrees/", ".claude/"];

    let existing_content = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path).with_context(|| "Failed to read .gitignore")?
    } else {
        String::new()
    };

    let mut missing_entries = Vec::new();
    for entry in &entries_to_add {
        // Check if entry (or variant without trailing slash) is already present
        let entry_no_slash = entry.trim_end_matches('/');
        let is_present = existing_content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == *entry || trimmed == entry_no_slash
        });
        if !is_present {
            missing_entries.push(*entry);
        }
    }

    if missing_entries.is_empty() {
        return Ok(());
    }

    // Check if we need to add a comment header
    let needs_comment = !existing_content.contains("# loom");

    // Append missing entries to .gitignore
    let mut new_content = existing_content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    // Add a comment if we're adding loom entries
    if needs_comment {
        new_content.push_str("\n# loom working directories\n");
    }

    for entry in &missing_entries {
        new_content.push_str(entry);
        new_content.push('\n');
    }

    std::fs::write(&gitignore_path, new_content).with_context(|| "Failed to update .gitignore")?;

    println!("Added {} to .gitignore", missing_entries.join(", "));

    Ok(())
}

/// Remove .work, .worktrees, and .claude from a branch if they were accidentally committed
///
/// This can happen if .gitignore wasn't set up before worktree creation.
/// We detect if these files exist in the branch and create a fixup commit to remove them.
pub fn remove_loom_dirs_from_branch(stage_id: &str, worktree_path: &Path) -> Result<()> {
    // Check if .work, .worktrees, or .claude exist in the branch's tree
    let output = Command::new("git")
        .args(["ls-files", ".work", ".worktrees", ".claude"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to check for .work in branch")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files_to_remove: Vec<&str> = stdout.lines().filter(|s| !s.is_empty()).collect();

    if files_to_remove.is_empty() {
        return Ok(());
    }

    println!("\nRemoving loom directories from branch (accidentally committed):");
    for file in &files_to_remove {
        println!("  - {file}");
    }

    // Remove from git index
    let mut rm_args = vec!["rm", "-r", "--cached", "--"];
    rm_args.extend(files_to_remove.iter());

    let output = Command::new("git")
        .args(&rm_args)
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to remove .work from index")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rm failed: {stderr}");
    }

    // Commit the removal
    let output = Command::new("git")
        .args([
            "commit",
            "-m",
            &format!("chore: remove loom working directories from branch loom/{stage_id}"),
        ])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to commit .work removal")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "nothing to commit" - might already be clean
        if !stderr.contains("nothing to commit") {
            bail!("git commit failed: {stderr}");
        }
    }

    println!("Loom directories removed from branch.");

    Ok(())
}

/// Check if worktree has uncommitted changes (staged, unstaged, or untracked)
/// Excludes .work and .worktrees directories (loom internal state)
pub fn has_uncommitted_changes(worktree_path: &Path) -> Result<bool> {
    // Check for staged/unstaged changes
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to check git status in worktree")?;

    let stdout = String::from_utf8_lossy(&status_output.stdout);

    // Filter out loom internal directories and .claude
    let has_changes = stdout.lines().filter(|l| !l.is_empty()).any(|l| {
        let file = if l.len() > 3 { &l[3..] } else { l };
        !file.starts_with(".work")
            && !file.starts_with(".worktrees")
            && !file.starts_with(".claude")
    });

    Ok(has_changes)
}

/// Get list of uncommitted files in worktree
/// Excludes .work and .worktrees directories (loom internal state)
pub fn get_uncommitted_files(worktree_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to check git status in worktree")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            // Format is "XY filename" where XY is the status
            if l.len() > 3 {
                l[3..].to_string()
            } else {
                l.to_string()
            }
        })
        // Exclude loom internal directories and .claude
        .filter(|f| {
            !f.starts_with(".work") && !f.starts_with(".worktrees") && !f.starts_with(".claude")
        })
        .collect();

    Ok(files)
}

/// Check if there are unmerged paths (merge conflicts) in the repository
pub fn has_merge_conflicts(repo_path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .with_context(|| "Failed to check for merge conflicts")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

/// Stash all changes (including untracked) in a repository
/// Returns true if changes were stashed, false if nothing to stash
/// NOTE: .work and .worktrees are excluded via .gitignore (ensured before merge)
pub fn stash_changes(repo_path: &Path, message: &str) -> Result<bool> {
    // Check for merge conflicts first - git can't stash with unmerged paths
    if has_merge_conflicts(repo_path)? {
        bail!(
            "Cannot stash: repository has unresolved merge conflicts.\n\n\
             Resolve conflicts first:\n\
               git status                    # See conflicting files\n\
               git checkout --theirs <file>  # Keep merged version\n\
               git checkout --ours <file>    # Keep your version\n\
               git add <file>                # Mark as resolved\n\
               git stash drop                # Drop the conflicted stash\n\n\
             Then retry the merge."
        );
    }

    // Check if there are changes to stash
    if !has_uncommitted_changes(repo_path)? {
        return Ok(false);
    }

    // Stash including untracked files (-u flag is critical for untracked files)
    // .work and .worktrees are excluded via .gitignore, so git skips them automatically
    let output = Command::new("git")
        .args(["stash", "push", "-u", "-m", message])
        .current_dir(repo_path)
        .output()
        .with_context(|| "Failed to stash changes")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "No local changes to save" is not an error
        if stderr.contains("No local changes to save") {
            return Ok(false);
        }
        bail!("git stash failed: {stderr}");
    }

    Ok(true)
}

/// Pop the most recent stash
/// Returns Ok even if pop has conflicts (just warns user)
pub fn pop_stash(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["stash", "pop"])
        .current_dir(repo_path)
        .output()
        .with_context(|| "Failed to pop stash")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check for conflicts
        if stderr.contains("CONFLICT") || stdout.contains("CONFLICT") {
            eprintln!("Warning: Stash pop had merge conflicts.");
            eprintln!("Your stashed changes have been partially applied.");
            eprintln!("Resolve conflicts and run 'git stash drop' when done.");
        } else {
            eprintln!("Warning: git stash pop had issues: {stderr}");
            eprintln!("Your changes may still be in the stash. Check 'git stash list'.");
        }
    }

    Ok(())
}

/// Auto-commit all changes in worktree before merge
pub fn auto_commit_changes(stage_id: &str, worktree_path: &Path) -> Result<()> {
    // Add all changes including untracked files
    let add_output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to stage changes")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        bail!("git add failed: {stderr}");
    }

    // Commit with a descriptive message
    let commit_output = Command::new("git")
        .args([
            "commit",
            "-m",
            &format!("loom: auto-commit before merge of stage '{stage_id}'"),
        ])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to commit changes")?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        // "nothing to commit" is not an error
        if !stderr.contains("nothing to commit") {
            bail!("git commit failed: {stderr}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_git_repo(path: &Path) -> Result<()> {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .with_context(|| "Failed to init git repo")?;
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()?;
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()?;
        Ok(())
    }

    #[test]
    fn test_has_uncommitted_changes_empty_repo() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        // Empty repo has no uncommitted changes
        let result = has_uncommitted_changes(temp_dir.path()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_has_uncommitted_changes_with_changes() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        // Create an untracked file
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let result = has_uncommitted_changes(temp_dir.path()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_has_uncommitted_changes_excludes_work_dir() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        // Create .work directory with a file
        std::fs::create_dir_all(temp_dir.path().join(".work")).unwrap();
        std::fs::write(temp_dir.path().join(".work/state.md"), "content").unwrap();

        // .work should be excluded
        let result = has_uncommitted_changes(temp_dir.path()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_get_uncommitted_files() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        std::fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "content").unwrap();

        let files = get_uncommitted_files(temp_dir.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"file1.txt".to_string()));
        assert!(files.contains(&"file2.txt".to_string()));
    }

    #[test]
    fn test_has_merge_conflicts_no_conflicts() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).unwrap();

        let result = has_merge_conflicts(temp_dir.path()).unwrap();
        assert!(!result);
    }
}
