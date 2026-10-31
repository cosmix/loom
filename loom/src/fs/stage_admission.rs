//! Per-stage exclusion between manual reset and scheduler admission.

use anyhow::{Context, Result};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;

use super::safe_fs::{
    flock_exclusive, open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd,
};

/// Hold this through the scheduler's final stage check and launch, or through
/// reset's liveness scan, retirement, state write and host-job cancellation.
/// This is a per-stage flock, never the shared `stages/` directory lock.
pub(crate) fn lock(work_dir: &Path, stage_id: &str) -> Result<OwnedFd> {
    crate::validation::validate_id(stage_id)?;
    // Worktrees intentionally expose `.loom/work` as a symlink to the state
    // root. Resolve that root once; every component below it stays no-follow.
    let canonical = work_dir.canonicalize()?;
    let root = safe_open_dirfd(&canonical)?;
    let dir = Path::new("stage-admission");
    safe_create_dir_all_in_workdir(root.as_raw_fd(), dir, 0o700)?;
    let path = dir.join(format!("{stage_id}.lock"));
    let fd = open_safely(root.as_raw_fd(), &path, libc::O_RDWR | libc::O_CREAT, 0o600)
        .with_context(|| format!("opening admission lock for stage '{stage_id}'"))?;
    flock_exclusive(&fd).with_context(|| format!("locking admission for stage '{stage_id}'"))?;
    Ok(fd)
}
