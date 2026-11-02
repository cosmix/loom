//! Fast-forward the branch checked out in the operator's checkout while
//! keeping that checkout's uncommitted work.
//!
//! Git carries local changes through `merge --ff-only` when none sits on a
//! path the merge touches. When one does, the tracked changes are stashed
//! around the fast-forward after a dry run proves they reapply cleanly
//! (`checkout_stash`). `--autostash` is never used: it writes conflict markers
//! into working files.
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
use super::checkout_stash::{
    checkout_unchanged, reset_checkout, snapshot_changes, store_unchanged, ChangesSnapshot,
};
use super::checkout_state::{classify, Classification};
use super::fast_forward::{after_errored_fast_forward, fast_forward, StashedChanges};
use super::tree::{rev_parse, Advance, MergeBlock, PendingMerge, RollbackChanges};
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
        Classification::NoOverlap => land(repo, pending, &[]),
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
fn land(repo: &Path, pending: &mut PendingMerge, removed: &[String]) -> Result<Advance> {
    let new = pending.commit()?;
    fast_forward(repo, (pending.old(), &new), removed, None)
}

/// Remove the untracked files the merge replaces with identical bytes, then
/// land. An incomplete rollback is reported with the paths left to recover.
fn land_removing(repo: &Path, pending: &mut PendingMerge, paths: Vec<String>) -> Result<Advance> {
    pending.commit()?;
    if let Err(error) = remove_files(repo, pending.tree(), &paths) {
        tracing::warn!(%error, "Cannot remove untracked files the merge replaces");
        if !error.restore.failures.is_empty() {
            return Ok(Advance::Blocked(
                error
                    .restore
                    .block(pending.tree(), RollbackChanges::NoBackup),
            ));
        }
        return Ok(overlap(paths));
    }
    land(repo, pending, &paths)
}

/// Dry run, then stash, fast-forward and pop. The snapshot is a commit of the
/// tracked changes that touches nothing in the working tree.
/// A git failure before the fast-forward blocks on the tracked paths unless
/// rollback needs operator recovery, which is reported separately.
fn reapply(
    repo: &Path,
    stage_id: &str,
    pending: &mut PendingMerge,
    tracked: Vec<String>,
    remove_untracked: Vec<String>,
) -> Result<Advance> {
    let snapshot = match snapshot_changes(repo) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return if remove_untracked.is_empty() {
                land(repo, pending, &[])
            } else {
                land_removing(repo, pending, remove_untracked)
            };
        }
        Err(error) => {
            tracing::warn!(%error, "Cannot snapshot the local changes of the main checkout");
            return Ok(overlap(tracked));
        }
    };
    let new = pending.commit()?;
    if !dry_run_reapplies(repo, (pending.old(), &new), &snapshot.commit)? {
        return Ok(overlap(tracked));
    }

    let backup_ref = backup_ref_name(stage_id, &snapshot.commit);
    create_backup_ref(repo, &backup_ref, &snapshot.commit)?;
    let tree = pending.tree().to_string();
    if let Err(error) = remove_files(repo, &tree, &remove_untracked) {
        tracing::warn!(%error, "Cannot remove untracked files the merge replaces");
        if !error.restore.failures.is_empty() {
            return Ok(Advance::Blocked(
                error
                    .restore
                    .block(&tree, RollbackChanges::InCheckout { backup_ref }),
            ));
        }
        drop_backup(repo, &backup_ref);
        return Ok(overlap(tracked));
    }
    stash_and_land(
        repo,
        pending,
        &new,
        &snapshot,
        (tracked, remove_untracked),
        backup_ref,
    )
}

/// Stash only after the rollback backup exists and untracked overlaps are removed.
fn stash_and_land(
    repo: &Path,
    pending: &PendingMerge,
    new: &str,
    snapshot: &ChangesSnapshot,
    (tracked, remove_untracked): (Vec<String>, Vec<String>),
    backup_ref: String,
) -> Result<Advance> {
    let tree = pending.tree();
    let stored = stash_changes(repo, snapshot);
    if let Err(error) = stored {
        tracing::warn!(%error, "Cannot stash the local changes of the main checkout");
        let restore = restore_files(repo, tree, &remove_untracked);
        if !restore.failures.is_empty() {
            let changes = RollbackChanges::InCheckout { backup_ref };
            return Ok(Advance::Blocked(restore.block(tree, changes)));
        }
        drop_backup(repo, &backup_ref);
        return Ok(overlap(tracked));
    }
    #[cfg(test)]
    tests::before_stash_reset(repo);
    if let Err(error) = reset_checkout(repo) {
        let error = format!("{error:#}");
        tracing::warn!(%error, "Cannot clear the stashed changes from the main checkout");
        if checkout_unchanged(repo, snapshot) {
            return Ok(reset_changed_nothing(
                repo,
                new,
                &remove_untracked,
                backup_ref,
            ));
        }
        // The reset ran partway, or its effect cannot be read: this is the state
        // an errored fast-forward leaves.
        return Ok(after_errored_fast_forward(
            repo,
            new,
            &remove_untracked,
            Some(backup_ref),
            &error,
        ));
    }
    let stash = StashedChanges {
        commit: snapshot.commit.clone(),
        backup_ref,
    };
    fast_forward(repo, (pending.old(), new), &remove_untracked, Some(stash))
}

/// Store the checkout's changes in the stash, or fail where a test injected it.
fn stash_changes(repo: &Path, snapshot: &ChangesSnapshot) -> Result<()> {
    #[cfg(test)]
    let force_stash_failure = tests::before_stash_push(repo);
    #[cfg(not(test))]
    let force_stash_failure = false;
    if force_stash_failure {
        Err(anyhow::anyhow!("simulated stash failure"))
    } else {
        store_unchanged(repo, snapshot)
    }
}

/// The block after a reset that failed with the checkout as the snapshot recorded
/// it: the changes never left, and the stash entry and `backup_ref` are spare
/// copies.
fn reset_changed_nothing(
    repo: &Path,
    new: &str,
    remove_untracked: &[String],
    backup_ref: String,
) -> Advance {
    let restore = restore_files(repo, new, remove_untracked);
    if !restore.failures.is_empty() {
        let changes = RollbackChanges::ResetRefused { backup_ref };
        return Advance::Blocked(restore.block(new, changes));
    }
    Advance::Blocked(MergeBlock::StashResetRefused { backup_ref })
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
