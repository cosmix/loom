//! Whether a stage's work is provably contained in the target branch.
//!
//! This is the precondition for destroying a stage's worktree and branch, and
//! the reason `merge_lifecycle` exists at all: deleting `loom/<stage>` while it
//! still holds commits the target lacks destroys a user's work, and loom has
//! done exactly that before (`doc/loom/knowledge/mistakes/phantom-merges.md`).
//!
//! `require_merged_history` in `git/cleanup/removal.rs` is the stronger sibling
//! of this predicate — it makes the same checks and additionally verifies the
//! stage's base branch, and it is what `cleanup_verified_stage` gates on. It is
//! private and tied to `StageResources`, so the two are duplicated for now; they
//! must be kept in agreement, and consolidating them is recorded as a concern.

use std::path::Path;

use super::MergeLifecycle;
use crate::git::branch::{branch_name_for_stage, commits_ahead_of, is_ancestor_of};
use crate::git::cleanup::branch_exists_strict;
use crate::git::merge::verify_merge_succeeded;
use crate::verify::transitions::load_stage;

/// Refuse cleanup unless the stage's work is provably in `target_branch`.
///
/// `None` proceeds, `Some(reason)` refuses. Every requirement is conjunctive and
/// every unanswerable git query is a refusal: a predicate guarding deletion must
/// fail closed, never silently proceed.
///
/// Two independent requirements, BOTH of which must hold:
///
/// 1. the stage branch is contained in `target_branch` — or, when the branch is
///    gone, the worktree HEAD is — or there is nothing left to lose; AND
/// 2. if the stage record carries a `completed_commit`, that commit is in
///    `target_branch` too.
///
/// Requirement 2 is ADDITIONAL, never a short circuit. `completed_commit` is a
/// snapshot the daemon takes from the branch head *before* it merges, and the
/// branch can advance past it afterwards: a resolver session commits more work
/// after a conflict, or the agent commits again in the worktree once the stage
/// was already merged and marked. Treating a verified snapshot as proof about
/// the branch authorises deleting commits nobody has merged.
pub(super) fn containment_refusal(
    lifecycle: &MergeLifecycle<'_>,
    target_branch: &str,
) -> Option<String> {
    let MergeLifecycle {
        stage_id,
        repo_root,
        work_dir,
    } = *lifecycle;

    let detail = uncontained_refs(stage_id, repo_root, target_branch)
        .or_else(|| uncontained_record(stage_id, work_dir, repo_root, target_branch))?;
    Some(format!("stage '{stage_id}': {detail}"))
}

/// Requirement 1: the refs that can still hold commits are contained.
fn uncontained_refs(stage_id: &str, repo_root: &Path, target_branch: &str) -> Option<String> {
    let branch = branch_name_for_stage(stage_id);
    match branch_exists_strict(&branch, repo_root) {
        Ok(true) => match commits_ahead_of(&branch, target_branch, repo_root) {
            Ok(0) => None,
            Ok(ahead) => Some(format!(
                "branch '{branch}' still holds {ahead} commit(s) that are not in '{target_branch}'"
            )),
            Err(error) => Some(format!(
                "cannot compare branch '{branch}' against '{target_branch}' ({error})"
            )),
        },
        // With the branch gone the worktree HEAD is the only ref left that can
        // hold commits — a detached-HEAD worktree must not be removed unchecked.
        Ok(false) => uncontained_worktree_head(stage_id, repo_root, target_branch),
        Err(error) => Some(format!(
            "cannot determine whether branch '{branch}' exists ({error})"
        )),
    }
}

fn uncontained_worktree_head(
    stage_id: &str,
    repo_root: &Path,
    target_branch: &str,
) -> Option<String> {
    let worktree = repo_root.join(".worktrees").join(stage_id);
    if !worktree.exists() {
        // No branch and no worktree: there is nothing left to lose.
        return None;
    }
    match is_ancestor_of("HEAD", target_branch, &worktree) {
        Ok(true) => None,
        Ok(false) => Some(format!(
            "the worktree HEAD holds commits that are not in '{target_branch}'"
        )),
        Err(error) => Some(format!(
            "cannot verify the worktree HEAD against '{target_branch}' ({error})"
        )),
    }
}

/// Requirement 2: a recorded completion commit, if any, is in the target.
fn uncontained_record(
    stage_id: &str,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Option<String> {
    // An unreadable or commit-less record drops this requirement only.
    // Requirement 1 still had to hold to get here.
    let commit = recorded_commit(stage_id, work_dir)?;
    match verify_merge_succeeded(&commit, target_branch, repo_root) {
        Ok(true) => None,
        Ok(false) => Some(format!(
            "the recorded commit {commit} is not in '{target_branch}'"
        )),
        Err(error) => Some(format!(
            "cannot verify the recorded commit {commit} against '{target_branch}' ({error})"
        )),
    }
}

fn recorded_commit(stage_id: &str, work_dir: &Path) -> Option<String> {
    match load_stage(stage_id, work_dir) {
        Ok(stage) => stage
            .completed_commit
            .filter(|commit| !commit.trim().is_empty()),
        Err(error) => {
            tracing::debug!(stage = %stage_id, %error, "No stage record to read");
            None
        }
    }
}
