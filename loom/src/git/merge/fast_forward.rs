//! `merge --ff-only` in the operator's checkout. A refused fast-forward is
//! always a typed block, and the untracked files moved aside for it are put
//! back first.

use anyhow::Result;
use std::path::Path;

use super::checkout_files::Displaced;
use super::tree::{rev_parse, Advance, MergeBlock};
use crate::git::branch::branch_ref;
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

/// `merge --ff-only` from `old` to `new` in the checkout whose `HEAD` is the
/// branch `target`, then settle the untracked files moved aside for it.
///
/// Infallible, so no `?` can leave moved files in quarantine. The merge counts
/// as landed only when `target` itself now names `new`: a branch switch or a
/// detach after the caller's `HEAD` check makes git advance some other ref,
/// and that is a block, never an advance. A refusal puts the displaced files
/// back and names any it could not restore; a clean refusal that moved the
/// target is [`MergeBlock::TargetMoved`]; one where git advanced the ref
/// `HEAD` now points at names that ref in a [`MergeBlock::FastForwardRefused`].
pub(super) fn fast_forward(
    repo: &Path,
    target: &str,
    (old, new): (&str, &str),
    displaced: Displaced,
) -> Advance {
    #[cfg(test)]
    run_before_fast_forward(repo);
    match try_fast_forward(repo, new) {
        Ok(Ok(())) => {
            if rev_parse(repo, &branch_ref(target)).is_ok_and(|tip| tip == new) {
                displaced.discard();
                Advance::Advanced
            } else {
                Advance::Blocked(advanced_other_ref(repo, target, displaced))
            }
        }
        Ok(Err(stderr)) => Advance::Blocked(displaced.restore().unwrap_or_else(|| {
            if rev_parse(repo, "HEAD").is_ok_and(|head| head != old) {
                MergeBlock::TargetMoved
            } else {
                refused_block(&stderr)
            }
        })),
        // Git did not start or timed out, possibly mid-checkout.
        Err(error) => Advance::Blocked(
            displaced
                .restore()
                .unwrap_or_else(|| refused_block(&format!("{error:#}"))),
        ),
    }
}

/// The block for a fast-forward that succeeded on a ref other than `target`:
/// the operator's branch was advanced to the stage's merge commit and `target`
/// was left unchanged. When the displaced files cannot all be put back, the
/// restore's block keeps naming the quarantine and carries the advanced ref
/// in its `detail`.
fn advanced_other_ref(repo: &Path, target: &str, displaced: Displaced) -> MergeBlock {
    let detail = advanced_other_ref_detail(repo, target);
    match displaced.restore() {
        None => MergeBlock::FastForwardRefused { detail },
        Some(MergeBlock::CheckoutRestoreIncomplete {
            quarantine,
            failures,
            ..
        }) => MergeBlock::CheckoutRestoreIncomplete {
            quarantine,
            failures,
            detail: Some(detail),
        },
        Some(other) => other,
    }
}

/// Which ref git advanced instead of `target`, for the operator.
fn advanced_other_ref_detail(repo: &Path, target: &str) -> String {
    let head = run_git(&["symbolic-ref", "-q", "HEAD"], repo)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "a different branch".to_string());
    format!(
        "the checkout switched away from {target} before the fast-forward, so git advanced {head} to the merge commit and {target} was left unchanged"
    )
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
    // Not `WorktreeGit::pinned_main`, whose private git directory would hide the
    // operator's `index.lock` until the target had moved and lose settings from an
    // `includeIf "gitdir:"`. So a link planted in the git directory between the
    // runner's look and git's open is still written through. Revisit if pinned git
    // can hold the real index's lock for the whole command and carry those settings.
    let output = run_git(
        &[
            "merge",
            "--ff-only",
            "--no-overwrite-ignore",
            "--quiet",
            new,
        ],
        repo,
    )?;
    if output.status.success() {
        return Ok(Ok(()));
    }
    Ok(Err(String::from_utf8_lossy(&output.stderr).into_owned()))
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

/// Test-only switches that make the fast-forward refuse or error on the
/// current thread, to reach paths git does not fail on by itself.
#[cfg(test)]
pub(super) mod failpoint {
    use std::cell::Cell;

    #[derive(Clone, Copy, Default)]
    pub struct Failures {
        pub fast_forward: bool,
        pub fast_forward_error: bool,
    }

    thread_local! {
        static FAILURES: Cell<Failures> = const { Cell::new(Failures { fast_forward: false, fast_forward_error: false }) };
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
}

#[cfg(test)]
type CheckoutHook = Box<dyn FnOnce(&Path)>;

#[cfg(test)]
thread_local! {
    static BEFORE_FAST_FORWARD: std::cell::RefCell<Option<CheckoutHook>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `hook` once on this thread with the checkout, just before the next
/// fast-forward starts: the window in which an operator can switch branches.
#[cfg(test)]
pub(super) fn inject_before_fast_forward(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_FAST_FORWARD.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_before_fast_forward(repo: &Path) {
    if let Some(hook) = BEFORE_FAST_FORWARD.with(|slot| slot.borrow_mut().take()) {
        hook(repo);
    }
}

#[cfg(test)]
mod tests;
