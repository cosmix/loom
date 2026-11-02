//! Per-worktree fallback spool for telemetry emitted by sandboxed sessions.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{append_record, TelemetryRecord};
use crate::fs::jsonl_spool;

/// Telemetry spool location relative to a worktree root.
pub const TELEMETRY_SPOOL_RELPATH: &str = ".loom/telemetry-spool.jsonl";

const RECORD_MAX_BYTES: usize = jsonl_spool::MAX_BYTES as usize - 1;

/// What a telemetry drain pass accomplished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrainOutcome {
    pub drained: usize,
    pub skipped_malformed: usize,
}

/// Absolute path of a worktree's telemetry spool.
pub fn spool_path(worktree_root: &Path) -> PathBuf {
    worktree_root.join(TELEMETRY_SPOOL_RELPATH)
}

/// Append one timestamped record under an exclusive lock.
pub fn append_to_spool(worktree_root: &Path, record: &TelemetryRecord) -> Result<()> {
    jsonl_spool::append(
        worktree_root,
        TELEMETRY_SPOOL_RELPATH,
        record,
        RECORD_MAX_BYTES,
    )
    .with_context(|| {
        format!(
            "Failed to append telemetry spool: {}",
            spool_path(worktree_root).display()
        )
    })
}

/// Read pending well-formed records without removing them. Exercised only by
/// tests - production code learns about spooled telemetry solely through
/// [`drain_into_events`].
#[cfg(test)]
pub(crate) fn read_pending(worktree_root: &Path) -> Result<Vec<TelemetryRecord>> {
    jsonl_spool::read(worktree_root, TELEMETRY_SPOOL_RELPATH).with_context(|| {
        format!(
            "Failed to read telemetry spool: {}",
            spool_path(worktree_root).display()
        )
    })
}

/// Drain every valid record into the canonical event file, then truncate the
/// spool while still holding its exclusive lock. Malformed lines are counted
/// and discarded. A spool the session replaced with a symlink (at the spool or
/// its `.loom/` directory), a hard link, or a FIFO is refused with an `Err` and
/// left untouched, so the trusted daemon never reads or truncates a file
/// outside the worktree.
pub fn drain_into_events(work_dir: &Path, worktree_root: &Path) -> Result<DrainOutcome> {
    let (drained, skipped_malformed) =
        jsonl_spool::drain(worktree_root, TELEMETRY_SPOOL_RELPATH, &mut |record| {
            append_record(work_dir, record)
        })
        .with_context(|| {
            format!(
                "Telemetry spool {} was not drained",
                spool_path(worktree_root).display()
            )
        })?;
    Ok(DrainOutcome {
        drained,
        skipped_malformed,
    })
}

pub(super) fn is_write_denied(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_err| {
                io_err.kind() == std::io::ErrorKind::PermissionDenied
                    || io_err.raw_os_error() == Some(libc::EROFS)
            })
    })
}
