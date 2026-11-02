//! Shared lock for dispute filing and retirement.
use crate::fs::safe_fs::flock_exclusive;
use crate::fs::stage_admission::{flock_within, DAEMON_LOCK_WAIT};
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::os::fd::OwnedFd;
use std::path::Path;

/// Take the per-stage lock that serialises dispute filings for the stage
/// whose dispute directory is `stage_dir`, creating the directory and its
/// `.lock` file if needed. The lock is held until the descriptor drops.
pub(crate) fn lock_stage_dispute_dir(stage_dir: &Path) -> Result<OwnedFd> {
    let lock = open_lock_file(stage_dir)?;
    flock_exclusive(&lock)
        .with_context(|| format!("Failed to acquire dispute lock at {}", stage_dir.display()))?;
    Ok(lock)
}

/// [`lock_stage_dispute_dir`] for the daemon's scheduling thread: an error once
/// another process has held the lock for `DAEMON_LOCK_WAIT`.
pub(crate) fn lock_stage_dispute_dir_for_daemon(stage_dir: &Path) -> Result<OwnedFd> {
    let lock = open_lock_file(stage_dir)?;
    flock_within(&lock, DAEMON_LOCK_WAIT)
        .with_context(|| format!("Failed to acquire dispute lock at {}", stage_dir.display()))?;
    Ok(lock)
}

fn open_lock_file(stage_dir: &Path) -> Result<OwnedFd> {
    std::fs::create_dir_all(stage_dir)
        .with_context(|| format!("Failed to create {}", stage_dir.display()))?;
    let lock_path = stage_dir.join(".lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open {}", lock_path.display()))?;
    Ok(OwnedFd::from(lock_file))
}
