//! Fast-forward the branch checked out in the operator's checkout while
//! keeping that checkout's uncommitted work.
//!
//! Git carries local changes through `merge --ff-only` when none sits on a
//! path the merge touches. A tracked change on a touched path refuses the
//! merge: loom never stashes, resets or rewrites the operator's tracked work.
//! Untracked files the merge replaces with identical bytes are moved aside by
//! `displace_files` and put back if the fast-forward fails.
//!
//! The checkout is classified against the merged TREE, so a blocked attempt
//! writes no commit; the commit is written once the fast-forward is certain
//! to be tried.

use anyhow::Result;
use std::path::Path;

use super::checkout_files::{
    checked_stdout, displace_files, porcelain_status, DiskProbe, Displaced,
};
use super::checkout_state::{classify, Classification};
use super::fast_forward::fast_forward;
use super::tree::{rev_parse, Advance, MergeBlock, PendingMerge};
use crate::git::branch::branch_ref;
use crate::git::runner::run_git;

/// Fast-forward `target`, which must be the branch checked out in `repo`, to
/// the merge commit of `pending`.
pub(super) fn advance_in_checkout(
    repo: &Path,
    target: &str,
    stage_id: &str,
    pending: &mut PendingMerge,
) -> Result<Advance> {
    if !head_is_target(repo, target)? {
        return Ok(Advance::Blocked(MergeBlock::TargetMoved));
    }
    let old = pending.old().to_string();
    if rev_parse(repo, "HEAD")? != old {
        return Ok(Advance::Blocked(MergeBlock::TargetMoved));
    }
    let tree = pending.tree().to_string();
    let status = porcelain_status(repo)?;
    let diff = checked_stdout(
        &["diff", "--name-status", "-z", "--no-renames", &old, &tree],
        &[],
        repo,
    )?;
    // Touched paths are decoded strictly: a lossy name would be probed, and
    // its ignored file overwritten, under the wrong name.
    let Ok(diff) = String::from_utf8(diff) else {
        return Ok(Advance::Blocked(MergeBlock::FastForwardRefused {
            detail: "a path the merge touches is not valid UTF-8; merge it by hand".into(),
        }));
    };
    let probe = DiskProbe {
        repo,
        base: &old,
        tree: &tree,
    };

    // Touched paths are then all UTF-8, and a non-UTF-8 status path can only
    // overlap one through a UTF-8 prefix, which the lossy text keeps. Decoding
    // the status strictly would block every merge while any unrelated
    // untracked file has such a name.
    match classify(&String::from_utf8_lossy(&status), &diff, &probe) {
        Classification::NoOverlap => land(repo, target, pending, Displaced::default()),
        Classification::RemoveUntracked { paths } => {
            land_displacing(repo, (target, stage_id), pending, &paths)
        }
        Classification::Blocked { paths } => Ok(overlap(paths)),
    }
}

/// Whether `HEAD` is the symbolic ref of branch `target`: a detached `HEAD`
/// or another branch would have the fast-forward advance the wrong ref.
fn head_is_target(repo: &Path, target: &str) -> Result<bool> {
    let output = run_git(&["symbolic-ref", "-q", "HEAD"], repo)?;
    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout).trim() == branch_ref(target))
}

fn overlap(paths: Vec<String>) -> Advance {
    Advance::Blocked(MergeBlock::UncommittedOverlap { paths })
}

/// Write the merge commit, then fast-forward to it.
fn land(
    repo: &Path,
    target: &str,
    pending: &mut PendingMerge,
    displaced: Displaced,
) -> Result<Advance> {
    let new = pending.commit()?;
    Ok(fast_forward(repo, target, (pending.old(), &new), displaced))
}

/// Move the untracked files the merge replaces with identical bytes aside,
/// then land. A file that cannot be moved or checked blocks the merge.
fn land_displacing(
    repo: &Path,
    (target, stage_id): (&str, &str),
    pending: &mut PendingMerge,
    paths: &[String],
) -> Result<Advance> {
    pending.commit()?;
    match displace_files(repo, stage_id, pending.tree(), paths) {
        Err(block) => Ok(Advance::Blocked(block)),
        Ok(displaced) => land(repo, target, pending, displaced),
    }
}

#[cfg(test)]
mod tests;
