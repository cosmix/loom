//! Merge completed stage worktree back to main
//! Usage: loom merge <stage_id> [--force]

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::stage_files::find_stage_file;
use crate::git::{
    cleanup_merged_branches, conflict_resolution_instructions, default_branch, ensure_work_symlink,
    merge_stage, remove_worktree, MergeResult,
};
use crate::models::stage::StageStatus;
use crate::orchestrator::session_is_running;
use crate::verify::transitions::{load_stage, transition_stage};

/// Find the tmux session name for a stage by checking session files
///
/// Looks for a session file in `.work/sessions/` that is assigned to the given stage
/// and returns its tmux_session name if found.
fn find_tmux_session_for_stage(stage_id: &str, work_dir: &Path) -> Result<Option<String>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse YAML frontmatter to check stage_id and get tmux_session
        if let Some(session_stage_id) = extract_frontmatter_field(&content, "stage_id") {
            if session_stage_id == stage_id {
                if let Some(tmux_session) = extract_frontmatter_field(&content, "tmux_session") {
                    return Ok(Some(tmux_session));
                }
            }
        }
    }

    Ok(None)
}

/// Extract a field value from YAML frontmatter
fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    // Check for frontmatter delimiter
    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        return None;
    }

    // Find end of frontmatter
    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx?;

    // Search for field in frontmatter
    for line in &lines[1..end_idx] {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == field {
                let value = value.trim();
                // Handle null values
                if value == "null" || value == "~" || value.is_empty() {
                    return None;
                }
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Validate that a stage is in an acceptable state for merging
fn validate_stage_status(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // If no stage file exists, skip validation (worktree without loom tracking)
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        return Ok(());
    }

    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    let status_ok = matches!(stage.status, StageStatus::Completed | StageStatus::Verified);

    if !status_ok {
        if force {
            println!(
                "Warning: Stage '{}' is in '{:?}' status (not Completed/Verified). Proceeding due to --force.",
                stage_id, stage.status
            );
        } else {
            bail!(
                "Stage '{}' is in '{:?}' status. Only Completed or Verified stages can be merged.\n\
                 \n\
                 To mark the stage as complete, run:\n\
                   loom stage complete {}\n\
                 \n\
                 To force merge anyway (DANGEROUS - may lose work):\n\
                   loom merge {} --force",
                stage_id,
                stage.status,
                stage_id,
                stage_id
            );
        }
    }

    Ok(())
}

/// Check if there's an active session for this stage
fn check_active_tmux_session(stage_id: &str, work_dir: &Path, force: bool) -> Result<()> {
    // First, check the standard naming convention: loom-{stage_id}
    let standard_tmux_name = format!("loom-{stage_id}");

    if session_is_running(&standard_tmux_name).unwrap_or(false) {
        if force {
            eprintln!(
                "Warning: Stage '{stage_id}' has an active session. Proceeding due to --force."
            );
        } else {
            bail!(
                "Stage '{stage_id}' has an active session.\n\
                 \n\
                 The worktree may be in use by a running Claude Code session.\n\
                 \n\
                 To complete the stage first:\n\
                   loom stage complete {stage_id}\n\
                 \n\
                 To kill the session:\n\
                   loom clean --sessions\n\
                 \n\
                 To force merge anyway (DANGEROUS - will delete worktree from under active session):\n\
                   loom merge {stage_id} --force"
            );
        }
        return Ok(());
    }

    // Also check if there's a session file that references this stage with a different name
    if let Some(tmux_name) = find_tmux_session_for_stage(stage_id, work_dir)? {
        if tmux_name != standard_tmux_name && session_is_running(&tmux_name).unwrap_or(false) {
            if force {
                eprintln!(
                    "Warning: Stage '{stage_id}' has an active session. Proceeding due to --force."
                );
            } else {
                bail!(
                    "Stage '{stage_id}' has an active session.\n\
                     \n\
                     The worktree may be in use by a running Claude Code session.\n\
                     \n\
                     To complete the stage first:\n\
                       loom stage complete {stage_id}\n\
                     \n\
                     To kill the session:\n\
                       loom clean --sessions\n\
                     \n\
                     To force merge anyway (DANGEROUS - will delete worktree from under active session):\n\
                       loom merge {stage_id} --force"
                );
            }
        }
    }

    Ok(())
}

/// Update stage status to Verified after successful merge
fn mark_stage_merged(stage_id: &str, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");

    // Only update if stage file exists
    if find_stage_file(&stages_dir, stage_id)?.is_none() {
        // Stage file doesn't exist (might be a worktree without loom tracking)
        return Ok(());
    }

    // Transition to Verified status (if not already)
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Verified {
        transition_stage(stage_id, StageStatus::Verified, work_dir)
            .with_context(|| format!("Failed to update stage status for: {stage_id}"))?;
        println!("Updated stage status to Verified");
    }

    Ok(())
}

/// Ensure .work and .worktrees are in .gitignore
///
/// This prevents git merge from failing with "would lose untracked files"
/// when .work directory exists in the main repo.
fn ensure_work_gitignored(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    let entries_to_add = [".work/", ".worktrees/"];

    let existing_content = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)
            .with_context(|| "Failed to read .gitignore")?
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

    std::fs::write(&gitignore_path, new_content)
        .with_context(|| "Failed to update .gitignore")?;

    println!("Added {} to .gitignore", missing_entries.join(", "));

    Ok(())
}

/// Remove .work and .worktrees from a branch if they were accidentally committed
///
/// This can happen if .gitignore wasn't set up before worktree creation.
/// We detect if these files exist in the branch and create a fixup commit to remove them.
fn remove_loom_dirs_from_branch(stage_id: &str, worktree_path: &Path) -> Result<()> {
    // Check if .work or .worktrees exist in the branch's tree
    let output = Command::new("git")
        .args(["ls-files", ".work", ".worktrees"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to check for .work in branch")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files_to_remove: Vec<&str> = stdout.lines().filter(|s| !s.is_empty()).collect();

    if files_to_remove.is_empty() {
        return Ok(());
    }

    println!(
        "\nRemoving loom directories from branch (accidentally committed):"
    );
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
fn has_uncommitted_changes(worktree_path: &Path) -> Result<bool> {
    // Check for staged/unstaged changes
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .with_context(|| "Failed to check git status in worktree")?;

    let stdout = String::from_utf8_lossy(&status_output.stdout);

    // Filter out loom internal directories
    let has_changes = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .any(|l| {
            let file = if l.len() > 3 { &l[3..] } else { l };
            !file.starts_with(".work") && !file.starts_with(".worktrees")
        });

    Ok(has_changes)
}

/// Get list of uncommitted files in worktree
/// Excludes .work and .worktrees directories (loom internal state)
fn get_uncommitted_files(worktree_path: &Path) -> Result<Vec<String>> {
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
        // Exclude loom internal directories
        .filter(|f| !f.starts_with(".work") && !f.starts_with(".worktrees"))
        .collect();

    Ok(files)
}

/// Check if there are unmerged paths (merge conflicts) in the repository
fn has_merge_conflicts(repo_path: &Path) -> Result<bool> {
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
fn stash_changes(repo_path: &Path, message: &str) -> Result<bool> {
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
fn pop_stash(repo_path: &Path) -> Result<()> {
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
fn auto_commit_changes(stage_id: &str, worktree_path: &Path) -> Result<()> {
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

/// Merge worktree branch to main, remove worktree on success
///
/// # Safety Checks (unless --force is used)
/// - Stage must be in Completed or Verified status
/// - No active tmux sessions for this stage
///
/// # Arguments
/// * `stage_id` - The ID of the stage to merge
/// * `force` - If true, skip safety checks (DANGEROUS)
pub fn execute(stage_id: String, force: bool) -> Result<()> {
    println!("Merging stage: {stage_id}");

    let repo_root = std::env::current_dir()?;
    let work_dir = repo_root.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    // Check worktree exists
    let worktree_path = repo_root.join(".worktrees").join(&stage_id);
    if !worktree_path.exists() {
        bail!(
            "Worktree for stage '{stage_id}' not found at {}",
            worktree_path.display()
        );
    }

    // Safety check 1: Validate stage status
    validate_stage_status(&stage_id, &work_dir, force)?;

    // Safety check 2: Check for active tmux sessions
    check_active_tmux_session(&stage_id, &work_dir, force)?;

    println!("Worktree path: {}", worktree_path.display());
    println!("Branch to merge: loom/{stage_id}");

    // Check for uncommitted changes and auto-commit them
    if has_uncommitted_changes(&worktree_path)? {
        let files = get_uncommitted_files(&worktree_path)?;
        println!("\nFound {} uncommitted file(s) in worktree:", files.len());
        for file in &files {
            println!("  - {file}");
        }
        println!("\nAuto-committing changes before merge...");
        auto_commit_changes(&stage_id, &worktree_path)?;
        println!("Changes committed.");
    }

    // Remove .work symlink from worktree before merge
    // This prevents "would lose untracked files" errors during merge
    let work_symlink = worktree_path.join(".work");
    if work_symlink.is_symlink() {
        std::fs::remove_file(&work_symlink)
            .with_context(|| format!("Failed to remove .work symlink from worktree: {}", work_symlink.display()))?;
    }

    // Determine target branch
    let target_branch = default_branch(&repo_root)
        .with_context(|| "Failed to detect default branch (main/master)")?;
    println!("Target branch: {target_branch}");

    // Ensure .work is in .gitignore to prevent merge conflicts
    // Git can fail with "would lose untracked files" if .work isn't ignored
    ensure_work_gitignored(&repo_root)?;

    // Remove .work and .worktrees from the branch if accidentally committed
    // This can happen if the gitignore wasn't set up before worktree creation
    remove_loom_dirs_from_branch(&stage_id, &worktree_path)?;

    // Auto-stash uncommitted changes in main repo (required for checkout)
    // Uses -u flag to include untracked files
    let main_repo_stashed = if has_uncommitted_changes(&repo_root)? {
        let files = get_uncommitted_files(&repo_root)?;
        println!("\nMain repository has {} uncommitted file(s):", files.len());
        for file in files.iter().take(5) {
            println!("  - {file}");
        }
        if files.len() > 5 {
            println!("  ... and {} more", files.len() - 5);
        }
        println!("\nAuto-stashing changes (will restore after merge)...");
        stash_changes(&repo_root, "loom: auto-stash before merge")?;
        println!("Changes stashed.");
        true
    } else {
        false
    };

    // Perform the merge (restore stash on error)
    println!("\nMerging loom/{stage_id} into {target_branch}...");
    let merge_result = match merge_stage(&stage_id, &target_branch, &repo_root) {
        Ok(result) => result,
        Err(e) => {
            // Restore .work symlink so worktree remains functional
            if let Err(restore_err) = ensure_work_symlink(&worktree_path, &repo_root) {
                eprintln!("Warning: Failed to restore .work symlink: {restore_err}");
            }
            // Restore stash before returning error
            if main_repo_stashed {
                eprintln!("\nMerge failed, restoring stashed changes...");
                pop_stash(&repo_root)?;
            }
            return Err(e);
        }
    };

    match merge_result {
        MergeResult::Success {
            files_changed,
            insertions,
            deletions,
        } => {
            println!("Merge successful!");
            println!("  {files_changed} files changed, +{insertions} -{deletions}");

            // Remove worktree (force=true since work is safely merged)
            println!("\nRemoving worktree...");
            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            // Clean up merged branch
            let cleaned = cleanup_merged_branches(&target_branch, &repo_root)?;
            if !cleaned.is_empty() {
                println!("Cleaned up branches: {}", cleaned.join(", "));
            }

            // Update stage status
            mark_stage_merged(&stage_id, &work_dir)?;
            println!("\nStage '{stage_id}' merged successfully!");
        }
        MergeResult::FastForward => {
            println!("Fast-forward merge completed!");

            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            let cleaned = cleanup_merged_branches(&target_branch, &repo_root)?;
            if !cleaned.is_empty() {
                println!("Cleaned up branches: {}", cleaned.join(", "));
            }

            mark_stage_merged(&stage_id, &work_dir)?;
            println!("\nStage '{stage_id}' merged successfully!");
        }
        MergeResult::AlreadyUpToDate => {
            println!("Branch is already up to date with {target_branch}.");
            println!("Removing worktree anyway...");

            remove_worktree(&stage_id, &repo_root, true)?;
            println!("Worktree removed: {}", worktree_path.display());

            mark_stage_merged(&stage_id, &work_dir)?;
        }
        MergeResult::Conflict { conflicting_files } => {
            // Restore .work symlink so worktree remains functional for conflict resolution
            if let Err(restore_err) = ensure_work_symlink(&worktree_path, &repo_root) {
                eprintln!("Warning: Failed to restore .work symlink: {restore_err}");
            }
            // Restore stash before showing conflict instructions
            if main_repo_stashed {
                eprintln!("\nRestoring stashed changes...");
                pop_stash(&repo_root)?;
            }
            let instructions =
                conflict_resolution_instructions(&stage_id, &target_branch, &conflicting_files);
            eprintln!("\n{instructions}");
            bail!(
                "Merge conflicts detected. Resolve manually, then run 'loom merge {stage_id}' again."
            );
        }
    }

    // Restore stashed changes after successful merge
    if main_repo_stashed {
        println!("\nRestoring stashed changes...");
        pop_stash(&repo_root)?;
    }

    Ok(())
}

/// Get the worktree path for a stage
pub fn worktree_path(stage_id: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
        .join(stage_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_frontmatter_field() {
        let content = r#"---
id: session-123
stage_id: my-stage
tmux_session: loom-my-stage
status: running
---

# Session content
"#;

        assert_eq!(
            extract_frontmatter_field(content, "id"),
            Some("session-123".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "stage_id"),
            Some("my-stage".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "tmux_session"),
            Some("loom-my-stage".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "status"),
            Some("running".to_string())
        );
        assert_eq!(extract_frontmatter_field(content, "nonexistent"), None);
    }

    #[test]
    fn test_extract_frontmatter_field_null_values() {
        let content = r#"---
id: session-123
stage_id: null
tmux_session: ~
empty_field:
---
"#;

        assert_eq!(extract_frontmatter_field(content, "stage_id"), None);
        assert_eq!(extract_frontmatter_field(content, "tmux_session"), None);
        assert_eq!(extract_frontmatter_field(content, "empty_field"), None);
    }

    #[test]
    fn test_extract_frontmatter_field_no_frontmatter() {
        let content = "# Just a markdown file\nNo frontmatter here.";
        assert_eq!(extract_frontmatter_field(content, "id"), None);
    }

    #[test]
    fn test_find_tmux_session_for_stage_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = find_tmux_session_for_stage("stage-1", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_tmux_session_for_stage_found() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create sessions directory and a session file
        let sessions_dir = work_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_content = r#"---
id: session-abc-123
stage_id: my-target-stage
tmux_session: loom-session-abc
status: running
---

# Session details
"#;
        std::fs::write(sessions_dir.join("session-abc-123.md"), session_content).unwrap();

        let result = find_tmux_session_for_stage("my-target-stage", work_dir).unwrap();
        assert_eq!(result, Some("loom-session-abc".to_string()));

        // Different stage should not match
        let result = find_tmux_session_for_stage("other-stage", work_dir).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_worktree_path() {
        let path = worktree_path("stage-1");
        assert!(path.to_string_lossy().contains(".worktrees"));
        assert!(path.to_string_lossy().contains("stage-1"));
    }
}
