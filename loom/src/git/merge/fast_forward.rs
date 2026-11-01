//! `merge --ff-only` in the operator's checkout, and the stash restore around
//! it. A refused fast-forward is always a typed block: rollback failures
//! name each path left to recover, and stashed changes retain a backup ref.

use anyhow::Result;
use std::path::Path;

use super::checkout_files::restore_files;
use super::tree::{rev_parse, Advance, MergeBlock, StashReapply};
use crate::git::runner::run_git;

/// Longest `FastForwardRefused` detail, in characters.
const DETAIL_LIMIT: usize = 300;

/// Headers git prints before the tab-indented paths of a refused merge.
const OVERLAP_HEADERS: [&str; 4] = [
    "Your local changes to the following files would be overwritten by merge:",
    "The following untracked working tree files would be overwritten by merge:",
    "The following untracked working tree files would be removed by merge:",
    "Updating the following directories would lose untracked files in them:",
];

/// `merge --ff-only` from `old` to `new`, then restore a stash made for it
/// (when `backup_ref` is set). After a clean refusal (git exited non-zero
/// having changed nothing) the stash is popped and `removed` is restored where
/// absent. Incomplete restoration names each path; a failed stash pop is a
/// [`MergeBlock::StashNotRestored`], never an `Err`.
/// A fast-forward that errors (git did not start, or timed out mid-checkout)
/// leaves the checkout in an unknown state, so see [`after_errored_fast_forward`].
pub(super) fn fast_forward(
    repo: &Path,
    (old, new): (&str, &str),
    removed: &[String],
    backup_ref: Option<String>,
) -> Result<Advance> {
    let stderr = match try_fast_forward(repo, new) {
        Ok(Ok(())) => {
            let stash = backup_ref.map(|backup_ref| StashReapply {
                restored: pop_stash(repo),
                backup_ref,
            });
            return Ok(Advance::Advanced { stash });
        }
        Ok(Err(stderr)) => stderr,
        Err(error) => {
            return Ok(after_errored_fast_forward(
                repo,
                new,
                removed,
                backup_ref,
                &format!("{error:#}"),
            ))
        }
    };
    let stash_restored = match backup_ref {
        Some(_) => pop_stash(repo),
        None => true,
    };
    let restore = restore_files(repo, new, removed);
    if !restore.failures.is_empty() {
        return Ok(Advance::Blocked(restore.block(new, backup_ref)));
    }
    if let (false, Some(backup_ref)) = (stash_restored, backup_ref) {
        return Ok(Advance::Blocked(MergeBlock::StashNotRestored {
            backup_ref,
        }));
    }
    if rev_parse(repo, "HEAD")? != old {
        return Ok(Advance::Blocked(MergeBlock::TargetMoved));
    }
    Ok(Advance::Blocked(refused_block(&stderr)))
}

/// The block after a fast-forward that errored. A killed checkout may have
/// left the working tree half updated, so the stash is not popped onto it: it
/// stays, named by [`MergeBlock::StashNotRestored`]. Rollback attempts every
/// removed path with exclusive creation; occupied paths are reported as
/// incomplete recovery. With no stash made, the error is a refusal.
fn after_errored_fast_forward(
    repo: &Path,
    new: &str,
    removed: &[String],
    backup_ref: Option<String>,
    error: &str,
) -> Advance {
    let restore = restore_files(repo, new, removed);
    if !restore.failures.is_empty() {
        return Advance::Blocked(restore.block(new, backup_ref));
    }
    match backup_ref {
        Some(backup_ref) => Advance::Blocked(MergeBlock::StashNotRestored { backup_ref }),
        None => Advance::Blocked(refused_block(error)),
    }
}

/// `Ok(Err(stderr))` when git refused the fast-forward.
fn try_fast_forward(repo: &Path, new: &str) -> Result<std::result::Result<(), String>> {
    #[cfg(test)]
    if failpoint::fast_forward() {
        return Ok(Err("simulated refusal".to_string()));
    }
    #[cfg(test)]
    if failpoint::fast_forward_error() {
        anyhow::bail!("simulated spawn failure");
    }
    let output = run_git(&["merge", "--ff-only", "--quiet", new], repo)?;
    if output.status.success() {
        return Ok(Ok(()));
    }
    Ok(Err(String::from_utf8_lossy(&output.stderr).into_owned()))
}

/// `stash pop --index`, then plain `stash pop`. Returns whether either
/// worked; on failure the stash entry stays.
fn pop_stash(repo: &Path) -> bool {
    #[cfg(test)]
    if failpoint::pop() {
        return false;
    }
    let attempts: [&[&str]; 2] = [
        &["stash", "pop", "--index", "--quiet"],
        &["stash", "pop", "--quiet"],
    ];
    attempts
        .iter()
        .any(|args| run_git(args, repo).is_ok_and(|o| o.status.success()))
}

/// The block for a refused fast-forward whose stderr is `stderr`: the paths
/// git names, or the flattened message when it names none.
pub(super) fn refused_block(stderr: &str) -> MergeBlock {
    let mut paths = Vec::new();
    let mut listing = false;
    for line in stderr.lines() {
        if OVERLAP_HEADERS.iter().any(|header| line.contains(header)) {
            listing = true;
        } else if let Some(path) = line.strip_prefix('\t').filter(|_| listing) {
            paths.push(path.trim_end().to_string());
        } else {
            listing = false;
        }
    }
    if !paths.is_empty() {
        return MergeBlock::UncommittedOverlap { paths };
    }
    let flat = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    MergeBlock::FastForwardRefused {
        detail: flat.chars().take(DETAIL_LIMIT).collect(),
    }
}

/// Test-only switches that make the fast-forward refuse or error, or the stash pop fail on
/// the current thread, to reach paths git does not fail on by itself.
#[cfg(test)]
pub(super) mod failpoint {
    use std::cell::Cell;

    #[derive(Clone, Copy, Default)]
    pub struct Failures {
        pub fast_forward: bool,
        pub fast_forward_error: bool,
        pub pop: bool,
    }

    thread_local! {
        static FAILURES: Cell<Failures> = const { Cell::new(Failures { fast_forward: false, fast_forward_error: false, pop: false }) };
    }

    /// Resets the switches when dropped.
    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            FAILURES.with(|cell| cell.set(Failures::default()));
        }
    }

    pub fn inject(failures: Failures) -> Guard {
        FAILURES.with(|cell| cell.set(failures));
        Guard
    }

    pub(super) fn fast_forward() -> bool {
        FAILURES.with(|cell| cell.get().fast_forward)
    }

    pub(super) fn fast_forward_error() -> bool {
        FAILURES.with(|cell| cell.get().fast_forward_error)
    }

    pub(super) fn pop() -> bool {
        FAILURES.with(|cell| cell.get().pop)
    }
}

#[cfg(test)]
mod tests;
