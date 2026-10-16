//! Shared lock for dispute filing and retirement.
use crate::fs::safe_fs::flock_exclusive;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::os::fd::OwnedFd;
use std::path::Path;

/// Take the per-stage lock that serialises dispute filings for the stage
/// whose dispute directory is `stage_dir`, creating the directory and its
/// `.lock` file if needed. The lock is held until the descriptor drops.
pub(crate) fn lock_stage_dispute_dir(stage_dir: &Path) -> Result<OwnedFd> {
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
    let lock = OwnedFd::from(lock_file);
    flock_exclusive(&lock)
        .with_context(|| format!("Failed to acquire dispute lock at {}", lock_path.display()))?;
    Ok(lock)
}
