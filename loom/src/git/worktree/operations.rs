//! Worktree operations
//!
//! Core CRUD operations for git worktrees: create, remove, list, get_or_create.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::permissions::{trust_worktree, untrust_worktree};
use crate::git::branch::{
    branch_name_for_stage, branch_ref, commits_ahead_of, commits_between, default_branch,
};
use crate::git::runner::{run_git, run_git_checked, run_git_checked_bytes};
use crate::models::worktree::Worktree;
use crate::validation::validate_id;

use super::checks::is_valid_git_worktree;
use super::parser::{parse_worktree_list, WorktreeInfo};
use super::settings::{
    cleanup_worktree_settings, ensure_work_symlink, setup_claude_directory, setup_root_claude_md,
};

/// Create a new worktree for a stage
///
/// Creates: .worktrees/{stage_id}/ with branch loom/{stage_id}
/// Also creates the state-root symlink into the main repo's shared state —
/// .worktrees/{stage_id}/.loom/work -> main .loom/work/ (or, on a legacy
/// workspace, .worktrees/{stage_id}/.work -> main .work/)
///
/// If `start_point` is Some(rev), the new branch is created at that revision:
///   git worktree add -b loom/{stage_id} .worktrees/{stage_id} {rev}
/// Otherwise, if `base_branch` is Some(branch), it is created from that branch:
///   git worktree add -b loom/{stage_id} .worktrees/{stage_id} {branch}
/// If both are None, the new branch is created from HEAD.
///
/// Also excludes `.claude/settings.local.json` and loom's own runtime paths
/// from git's view of this worktree, by writing to the repo's common
/// `.git/info/exclude` (see `add_settings_local_to_worktree_gitignore`'s doc
/// comment for why it is NOT the per-worktree gitdir). This prevents agents
/// from accidentally staging session-specific hook/sandbox settings, and
/// keeps the memory spool / context cache out of `git status` for every
/// worktree.
pub fn create_worktree(
    stage_id: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
    start_point: Option<&str>,
) -> Result<Worktree> {
    // Validate stage_id before using in paths
    validate_id(stage_id).context("Invalid stage ID for worktree")?;

    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = branch_name_for_stage(stage_id);

    // Ensure .worktrees directory exists
    let worktrees_dir = repo_root.join(".worktrees");
    if !worktrees_dir.exists() {
        std::fs::create_dir_all(&worktrees_dir)
            .with_context(|| "Failed to create .worktrees directory")?;
    }

    // Check if worktree already exists
    if worktree_path.exists() {
        bail!("Worktree already exists at {}", worktree_path.display());
    }

    // Create the worktree with a new branch at the start point, else the base
    // branch, else HEAD:
    // git worktree add -b loom/{stage_id} .worktrees/{stage_id} [{start}]
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    let mut args: Vec<&str> = vec!["worktree", "add", "-b", &branch_name, &worktree_path_str];
    if let Some(start) = start_point.or(base_branch) {
        args.push(start);
    }

    let output = run_git(&args, repo_root)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            bail!("git worktree add failed: {stderr}");
        }
        recreate_or_reuse_branch(stage_id, repo_root, &args, base_branch, start_point)?;
    }

    scaffold_worktree(&worktree_path, repo_root)?;

    let mut worktree = Worktree::new(stage_id.to_string(), worktree_path, branch_name);
    worktree.mark_active();

    Ok(worktree)
}

/// Set up a freshly added worktree: state-root symlink, `.claude/`, the root
/// `CLAUDE.md`, Claude Code trust, and the shared git excludes. Trust and
/// exclude failures only warn.
fn scaffold_worktree(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    // Create symlink to the main repo's state root (.loom/work, or .work on
    // a legacy workspace)
    ensure_work_symlink(worktree_path, repo_root)?;

    // Set up .claude/ directory for worktree.
    setup_claude_directory(worktree_path, repo_root)?;

    // Symlink project-root CLAUDE.md
    setup_root_claude_md(worktree_path, repo_root)?;

    // Register worktree as trusted so Claude Code skips the "trust this folder?" prompt
    if let Err(e) = trust_worktree(worktree_path) {
        eprintln!("Warning: Failed to register worktree trust: {e}");
    }

    if let Err(e) = super::settings::add_settings_local_to_worktree_gitignore(repo_root) {
        eprintln!("Warning: Failed to add settings.local.json to worktree gitignore: {e}");
    }

    Ok(())
}

/// Finish a `worktree add -b` (`add_args`) that failed because the branch
/// `loom/<stage_id>` already exists with no worktree: cleanup or manual
/// removal left the branch behind, then the stage was re-queued.
///
/// Force-deleting the branch unconditionally would silently destroy any
/// unmerged commits it holds (reflog-only recovery), and the daemon's orphan
/// recovery routes commits-ahead branches to NeedsHandoff precisely because
/// they hold value. So the branch is deleted and `add_args` rerun only when
/// it has no commits beyond its base; otherwise the existing branch is
/// checked out into the new worktree (`worktree add` without `-b`),
/// preserving the work and letting the stage resume against it.
fn recreate_or_reuse_branch(
    stage_id: &str,
    repo_root: &Path,
    add_args: &[&str],
    base_branch: Option<&str>,
    start_point: Option<&str>,
) -> Result<()> {
    let branch_name = branch_name_for_stage(stage_id);
    let (base, ahead) = commits_beyond_base(&branch_name, repo_root, base_branch, start_point);
    if ahead == 0 {
        // No unmerged work — safe to recreate from the correct base.
        run_git_checked(&["branch", "-D", &branch_name], repo_root)?;
        let retry_output = run_git(add_args, repo_root)?;
        if !retry_output.status.success() {
            let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
            bail!("git worktree add failed after branch deletion: {retry_stderr}");
        }
        return Ok(());
    }
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let worktree_path_str = worktree_path.to_string_lossy();
    let reuse_args: [&str; 4] = ["worktree", "add", &worktree_path_str, &branch_name];
    let reuse_output = run_git(&reuse_args, repo_root)?;
    if !reuse_output.status.success() {
        let reuse_stderr = String::from_utf8_lossy(&reuse_output.stderr);
        bail!(
            "branch '{branch_name}' has {ahead} unmerged commit(s) ahead of \
             '{base}'; refusing to delete it. Tried to reuse the existing branch \
             but `git worktree add` failed: {reuse_stderr}. \
             Resolve via `loom stage merge {stage_id}` or retry canonical completion \
             with `loom stage complete {stage_id}`."
        );
    }
    Ok(())
}

/// The base `branch_name` is measured against and its count of commits
/// beyond that base. With a start point the base is that revision: measured
/// against a held target moved onto the branch's own commit instead, the
/// branch would count zero and lose its only ref. Otherwise the base is
/// `base_branch`, else the repository's default branch. Fails closed: a
/// count that errors is 1, so the branch is reused, never deleted.
fn commits_beyond_base(
    branch_name: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
    start_point: Option<&str>,
) -> (String, usize) {
    if let Some(start) = start_point {
        let ahead = commits_between(&branch_ref(branch_name), start, repo_root);
        return (start.to_string(), ahead.unwrap_or(1));
    }
    let base = match base_branch {
        Some(b) => b.to_string(),
        None => default_branch(repo_root).unwrap_or_else(|_| "main".to_string()),
    };
    let ahead = commits_ahead_of(branch_name, &base, repo_root).unwrap_or(1);
    (base, ahead)
}

/// Remove a worktree
///
/// Runs: git worktree remove .worktrees/{stage_id}
pub fn remove_worktree(stage_id: &str, repo_root: &Path, force: bool) -> Result<()> {
    // Validate stage_id before using in paths
    validate_id(stage_id).context("Invalid stage ID for worktree removal")?;

    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_path.exists() {
        bail!("Worktree does not exist: {}", worktree_path.display());
    }

    // Clean up settings and symlinks first
    cleanup_worktree_settings(&worktree_path);

    // Remove worktree from Claude Code's trusted projects
    if let Err(e) = untrust_worktree(&worktree_path) {
        eprintln!("Warning: Failed to remove worktree trust: {e}");
    }

    let mut args: Vec<&str> = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let wt_str = worktree_path.to_string_lossy().to_string();
    args.push(&wt_str);

    let output = run_git(&args, repo_root)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree remove failed: {stderr}");
    }

    Ok(())
}

/// List all worktrees
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
    let stdout = run_git_checked_bytes(&["worktree", "list", "--porcelain", "-z"], repo_root)?;
    parse_worktree_list(&stdout)
}

/// Clean orphaned worktrees (prune)
pub fn clean_worktrees(repo_root: &Path) -> Result<()> {
    run_git_checked(&["worktree", "prune"], repo_root)?;
    Ok(())
}

/// Get an existing worktree or create a new one
///
/// If a valid worktree exists at .worktrees/{stage_id}/, reuses it.
/// An unregistered directory is preserved unless it is empty.
/// Otherwise, creates a new worktree.
///
/// A new worktree branches from `start_point` when given, else from
/// `base_branch`, else from HEAD (see [`create_worktree`]).
///
/// This function is idempotent and safe to call multiple times for the same stage.
pub fn get_or_create_worktree(
    stage_id: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
    start_point: Option<&str>,
) -> Result<Worktree> {
    // Validate stage_id before using in paths
    validate_id(stage_id).context("Invalid stage ID for worktree")?;

    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = branch_name_for_stage(stage_id);

    if worktree_path.exists() {
        // Check if it's a valid git worktree by looking for the .git file
        // Git worktrees have a .git file (not directory) that points to the main repo
        let git_file = worktree_path.join(".git");
        if git_file.exists() {
            // Verify it's actually tracked by git worktree list
            if is_valid_git_worktree(&worktree_path, repo_root)? {
                // Valid worktree exists, return it
                let mut worktree = Worktree::new(stage_id.to_string(), worktree_path, branch_name);
                worktree.mark_active();
                return Ok(worktree);
            }
        }

        remove_empty_remnant(&worktree_path, repo_root)?;
    }

    // Create new worktree
    create_worktree(stage_id, repo_root, base_branch, start_point)
}

fn remove_empty_remnant(path: &Path, repo_root: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_dir() || std::fs::read_dir(path)?.next().transpose()?.is_some() {
        bail!("Cannot reuse unregistered worktree {}; its contents were preserved. Inspect and move or remove the directory before retrying", path.display());
    }
    clean_worktrees(repo_root).context("Failed to prune stale worktree registrations")?;
    std::fs::remove_dir(path)
        .with_context(|| format!("Failed to remove empty worktree remnant {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "operations_start_point_tests.rs"]
mod start_point_tests;

#[cfg(test)]
#[path = "operations_recovery_tests.rs"]
mod recovery_tests;
