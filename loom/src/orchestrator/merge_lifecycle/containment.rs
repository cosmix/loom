//! Whether a stage's work is provably contained in the target branch.
//!
//! This is the precondition for destroying a stage's worktree and branch, and
//! the reason `merge_lifecycle` exists at all: deleting `loom/<stage>` while it
//! still holds commits the target lacks destroys a user's work, and loom has
//! done exactly that before (`doc/loom/knowledge/mistakes/phantom-merges.md`).
//!
//! The predicate is `git::cleanup::require_stage_contained`, shared with
//! `cleanup_verified_stage`.

use std::path::Path;

use super::MergeLifecycle;
use crate::git::cleanup::require_stage_contained;
use crate::verify::transitions::load_stage;

/// Refuse cleanup unless the stage's work is provably in `target_branch`.
///
/// `None` proceeds, `Some(reason)` refuses. Every requirement is conjunctive and
/// every unanswerable git query is a refusal: a predicate guarding deletion must
/// fail closed, never silently proceed.
///
/// All of these must hold:
///
/// 1. the stage branch, if it exists, holds no commit the target lacks;
/// 2. the worktree HEAD, if a registered worktree exists, is in the target,
///    whether or not the branch exists (a detached HEAD can hold commits no
///    ref names);
/// 3. the base branch `loom/_base/<stage>`, if it exists, is in the target;
/// 4. the stage record's `completed_commit`, if it carries one, is in
///    `target_branch`.
///
/// The recorded commit is ADDITIONAL, never a short circuit. `completed_commit`
/// is a snapshot the daemon takes from the branch head *before* it merges, and
/// the branch can advance past it afterwards: a resolver session commits more
/// work after a conflict, or the agent commits again in the worktree once the
/// stage was already merged and marked. Treating a verified snapshot as proof
/// about the branch authorises deleting commits nobody has merged.
pub(super) fn containment_refusal(
    lifecycle: &MergeLifecycle<'_>,
    target_branch: &str,
) -> Option<String> {
    let MergeLifecycle {
        stage_id,
        repo_root,
        work_dir,
    } = *lifecycle;

    let commit = recorded_commit(stage_id, work_dir);
    require_stage_contained(stage_id, commit.as_deref(), target_branch, repo_root)
        .err()
        .map(|error| format!("stage '{stage_id}': {error:#}"))
}

/// An unreadable or commit-less record drops the recorded-commit check only;
/// the ref checks still have to hold.
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

#[cfg(test)]
mod base_tests;
#[cfg(test)]
mod detached_contract_tests;
#[cfg(test)]
mod tag_tests;
