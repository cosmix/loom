//! Fast-forward the branch checked out in the operator's checkout while
//! keeping that checkout's uncommitted work.
//!
//! Git carries local changes through `merge --ff-only` when none sits on a
//! path the merge touches. When one does, the tracked changes are stashed
//! around the fast-forward after a dry run proves they reapply cleanly.
//! `reset --hard` and `--autostash` are never used: the latter writes
//! conflict markers into working files.
//!
//! The checkout is classified against the merged TREE, so a blocked attempt
//! writes no commit; the commit is written once the fast-forward is certain
//! to be tried (the dry run needs it too).

use anyhow::Result;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::checkout_files::{
    checked_stdout, porcelain_status, remove_files, restore_files, DiskProbe,
};
use super::checkout_state::{classify, Classification};
use super::fast_forward::fast_forward;
use super::tree::{rev_parse, Advance, MergeBlock, PendingMerge};
use crate::git::runner::{run_git, run_git_checked};

/// Fast-forward the checked-out branch to the merge commit of `pending`.
pub(super) fn advance_in_checkout(
    repo: &Path,
    stage_id: &str,
    pending: &mut PendingMerge,
) -> Result<Advance> {
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
    let probe = DiskProbe {
        repo,
        base: &old,
        tree: &tree,
    };

    match classify(
        &String::from_utf8_lossy(&status),
        &String::from_utf8_lossy(&diff),
        &probe,
    ) {
        Classification::NoOverlap => land(repo, pending, &[], None),
        Classification::RemoveUntracked { paths } => land_removing(repo, pending, paths),
        Classification::Blocked { paths } => Ok(overlap(paths)),
        Classification::Reapply {
            tracked,
            remove_untracked,
        } => reapply(repo, stage_id, pending, tracked, remove_untracked),
    }
}

fn overlap(paths: Vec<String>) -> Advance {
    Advance::Blocked(MergeBlock::UncommittedOverlap { paths })
}

/// Write the merge commit, then fast-forward to it.
fn land(
    repo: &Path,
    pending: &mut PendingMerge,
    removed: &[String],
    backup_ref: Option<String>,
) -> Result<Advance> {
    let new = pending.commit()?;
    fast_forward(repo, (pending.old(), &new), removed, backup_ref)
}

/// Remove the untracked files the merge replaces with identical bytes, then
/// land. An incomplete rollback is reported with the paths left to recover.
fn land_removing(repo: &Path, pending: &mut PendingMerge, paths: Vec<String>) -> Result<Advance> {
    pending.commit()?;
    if let Err(error) = remove_files(repo, pending.tree(), &paths) {
        tracing::warn!(%error, "Cannot remove untracked files the merge replaces");
        if !error.restore.failures.is_empty() {
            return Ok(Advance::Blocked(error.restore.block(pending.tree(), None)));
        }
        return Ok(overlap(paths));
    }
    land(repo, pending, &paths, None)
}

/// Dry run, then stash, fast-forward and pop. `S` is `git stash create`, a
/// commit of the tracked changes that touches nothing in the working tree.
/// A git failure before the fast-forward blocks on the tracked paths unless
/// rollback needs operator recovery, which is reported separately.
fn reapply(
    repo: &Path,
    stage_id: &str,
    pending: &mut PendingMerge,
    tracked: Vec<String>,
    remove_untracked: Vec<String>,
) -> Result<Advance> {
    let stash_commit = match run_git_checked(&["stash", "create"], repo) {
        Ok(stash_commit) => stash_commit,
        Err(error) => {
            tracing::warn!(%error, "Cannot snapshot the local changes of the main checkout");
            return Ok(overlap(tracked));
        }
    };
    if stash_commit.is_empty() {
        return if remove_untracked.is_empty() {
            land(repo, pending, &[], None)
        } else {
            land_removing(repo, pending, remove_untracked)
        };
    }
    let new = pending.commit()?;
    if !dry_run_reapplies(repo, (pending.old(), &new), &stash_commit)? {
        return Ok(overlap(tracked));
    }

    let backup_ref = backup_ref_name(stage_id, &stash_commit);
    create_backup_ref(repo, &backup_ref, &stash_commit)?;
    let tree = pending.tree().to_string();
    if let Err(error) = remove_files(repo, &tree, &remove_untracked) {
        tracing::warn!(%error, "Cannot remove untracked files the merge replaces");
        if !error.restore.failures.is_empty() {
            return Ok(Advance::Blocked(
                error.restore.block(&tree, Some(backup_ref)),
            ));
        }
        drop_backup(repo, &backup_ref);
        return Ok(overlap(tracked));
    }
    stash_and_land(repo, pending, &new, tracked, remove_untracked, backup_ref)
}

/// Stash only after the rollback backup exists and untracked overlaps are removed.
fn stash_and_land(
    repo: &Path,
    pending: &PendingMerge,
    new: &str,
    tracked: Vec<String>,
    remove_untracked: Vec<String>,
    backup_ref: String,
) -> Result<Advance> {
    let tree = pending.tree();
    #[cfg(test)]
    let force_stash_failure = tests::before_stash_push(repo);
    #[cfg(not(test))]
    let force_stash_failure = false;
    let stash_result = if force_stash_failure {
        Err(anyhow::anyhow!("simulated stash failure"))
    } else {
        run_git_checked(&["stash", "push", "--quiet"], repo)
    };
    if let Err(error) = stash_result {
        tracing::warn!(%error, "Cannot stash the local changes of the main checkout");
        let restore = restore_files(repo, tree, &remove_untracked);
        if !restore.failures.is_empty() {
            return Ok(Advance::Blocked(restore.block(tree, Some(backup_ref))));
        }
        drop_backup(repo, &backup_ref);
        return Ok(overlap(tracked));
    }
    fast_forward(
        repo,
        (pending.old(), new),
        &remove_untracked,
        Some(backup_ref),
    )
}

/// Whether the stashed changes `stash_commit` merge cleanly onto `new`, with
/// `old` as the base.
fn dry_run_reapplies(repo: &Path, (old, new): (&str, &str), stash_commit: &str) -> Result<bool> {
    let base = format!("--merge-base={old}");
    let dry = run_git(
        &["merge-tree", "--write-tree", &base, new, stash_commit],
        repo,
    )?;
    Ok(dry.status.success())
}

/// `refs/loom/autostash/<stage>-<unix secs>-<first 12 hex of the stash
/// commit>`: two attempts in one second keep distinct backups.
fn backup_ref_name(stage_id: &str, stash_commit: &str) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let short: String = stash_commit.chars().take(12).collect();
    format!("refs/loom/autostash/{stage_id}-{secs}-{short}")
}

/// Create `backup_ref` at `stash_commit`; the empty old value makes git
/// refuse a ref that already exists instead of overwriting its backup.
fn create_backup_ref(repo: &Path, backup_ref: &str, stash_commit: &str) -> Result<()> {
    run_git_checked(&["update-ref", backup_ref, stash_commit, ""], repo).map(|_| ())
}

/// Delete a backup ref made for a stash that was never pushed.
fn drop_backup(repo: &Path, backup_ref: &str) {
    let _ = run_git(&["update-ref", "-d", backup_ref], repo);
}

#[cfg(test)]
mod tests;
