//! Append/read/drain mechanics for the stage-request spool.
//!
//! Storage mechanics are shared with memory spools through `fs::jsonl_spool`.
//! Requests retain stable identities so application receipts suppress replay.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::types::StageRequest;
use crate::fs::jsonl_spool;
use crate::git::worktree::find_worktree_root_from_cwd;
use crate::stage_service::MAX_REQUEST_BYTES;

/// Spool location relative to a worktree root.
pub const SPOOL_RELPATH: &str = ".loom/stage-request-spool.jsonl";

/// Refuse further appends past this size (bytes). Bounds a runaway agent that
/// keeps queueing while the daemon (for whatever reason) isn't draining -
/// without this a stuck spool grows without limit. Same bound as the memory
/// spool, for the same reason.
pub const SPOOL_MAX_BYTES: u64 = 1024 * 1024;

/// What a drain pass accomplished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrainOutcome {
    /// Requests the stage actually took.
    pub applied: usize,
    /// Lines that could not be parsed, plus requests the daemon refused.
    /// Both are dead to the caller in the same way: nothing changed, and
    /// retrying the same bytes would not change that.
    pub skipped: usize,
}

/// Absolute path of a worktree's spool file.
pub fn spool_path(worktree_root: &Path) -> PathBuf {
    worktree_root.join(SPOOL_RELPATH)
}

/// The worktree whose spool this process should queue into.
///
/// Spooling only makes sense inside a worktree - that is the one place the
/// daemon knows to look, and the only thing it attributes a drained request
/// by. Off a worktree there is nowhere meaningful to write, so this is an
/// error naming why rather than a spool file nothing will ever read. Resolved
/// through the same [`find_worktree_root_from_cwd`] the memory CLI's spool
/// fallback uses, so both answer the question identically.
pub fn spool_target_from_cwd() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    find_worktree_root_from_cwd(&cwd).ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot reach the loom daemon over .loom/work/orchestrator.sock, and this is not a \
             stage worktree, so there is nowhere to queue the request. Run this from the \
             stage's worktree, or from a shell that can reach the daemon."
        )
    })
}

/// Append one request as a single JSON line, creating `.loom/` if needed.
///
/// Holds an exclusive lock for the whole read-size/write operation: `O_APPEND`
/// alone only guarantees atomicity up to the platform's atomic write size
/// (commonly 4096 bytes), and a dispute carrying 4 KiB of failure output plus a
/// reason exceeds that, so two concurrent appends could otherwise interleave
/// their bytes on one line.
pub fn append_to_spool(worktree_root: &Path, request: &StageRequest) -> Result<()> {
    jsonl_spool::append(
        worktree_root,
        SPOOL_RELPATH,
        &super::types::QueuedRequest::new(request.clone()),
        MAX_REQUEST_BYTES,
    )
}

/// Pending requests, WITHOUT applying or removing them. Empty vec when no
/// spool exists.
///
/// Missing-spool is the overwhelmingly common case (every daemon tick, for
/// every stage that has queued nothing) so this must stay cheap and must never
/// create the file or the `.loom/` directory. Exercised only by tests -
/// production code learns about spooled requests solely through
/// [`super::apply::drain_requests`].
#[cfg(test)]
pub(crate) fn read_pending(worktree_root: &Path) -> Result<Vec<StageRequest>> {
    Ok(
        jsonl_spool::read::<super::types::QueuedRequest>(worktree_root, SPOOL_RELPATH)?
            .into_iter()
            .map(|entry| entry.payload)
            .collect(),
    )
}

/// Hand every pending request to `sink`, then truncate the spool.
///
/// Delivery is at-least-once: requests are only removed after `sink` has
/// returned `Ok` for every one of them and the file is truncated, all under one
/// exclusive lock. If any `sink` call errors, the error propagates and the file
/// is left untouched - including requests `sink` already applied - so the whole
/// batch redelivers on the next pass. A crash between the last successful
/// `sink` call and the truncate has the same effect. That is why the sink
/// treats a REFUSAL as success (see [`super::apply::drain_requests`]): a stage
/// the daemon declines to block is answered, not failed, and redelivering it
/// forever would wedge every request behind it.
///
/// Malformed lines are skipped (counted, not retried) rather than blocking the
/// requests around them; they are discarded on the truncate that follows a
/// successful pass, since a line that couldn't parse this time never will.
///
/// A spool the session replaced with a symlink (at the spool or its `.loom/`
/// directory), a hard link, or a FIFO is refused with an `Err` and left
/// untouched (see `open_regular_no_follow` in `fs::safe_read`), so the
/// trusted daemon never reads or truncates a file outside the worktree.
#[cfg(test)]
pub(super) fn drain_spool(
    worktree_root: &Path,
    sink: &mut dyn FnMut(&StageRequest) -> Result<()>,
) -> Result<DrainOutcome> {
    let (applied, skipped) = jsonl_spool::drain::<super::types::QueuedRequest>(
        worktree_root,
        SPOOL_RELPATH,
        &mut |entry| sink(&entry.payload),
    )?;
    Ok(DrainOutcome { applied, skipped })
}

#[cfg(test)]
#[path = "tests/spool.rs"]
mod tests;

/// Production drain exposes stable identities to the application checkpoint.
pub(super) fn drain_identified(
    root: &Path,
    sink: &mut dyn FnMut(&super::types::QueuedRequest) -> Result<()>,
) -> Result<DrainOutcome> {
    let (applied, skipped) = jsonl_spool::drain(root, SPOOL_RELPATH, sink)?;
    Ok(DrainOutcome { applied, skipped })
}
