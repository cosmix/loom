//! File-based locking for merge operations

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

// Merge lock timeout for detecting stale locks (5 minutes)
const MERGE_LOCK_STALE_TIMEOUT_SECS: u64 = 300;

/// File-based merge lock to prevent concurrent merges
pub struct MergeLock {
    lock_path: std::path::PathBuf,
    held: bool,
}

impl MergeLock {
    /// Acquire the merge lock with a timeout
    ///
    /// The lock is a simple file-based lock stored at `.work/merge.lock`.
    /// If another process holds the lock, this will wait up to `timeout`
    /// before failing.
    pub fn acquire(work_dir: &Path, timeout: Duration) -> Result<Self> {
        let lock_path = work_dir.join("merge.lock");
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        loop {
            match Self::try_acquire(&lock_path) {
                Ok(lock) => return Ok(lock),
                Err(_) if start.elapsed() < timeout => {
                    std::thread::sleep(poll_interval);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Try to acquire the lock without waiting
    fn try_acquire(lock_path: &Path) -> Result<Self> {
        // Try to create the lock file exclusively
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                // Write our PID and timestamp to the lock file
                let pid = std::process::id();
                let timestamp = chrono::Utc::now().to_rfc3339();
                writeln!(file, "pid={pid}")?;
                writeln!(file, "timestamp={timestamp}")?;
                file.sync_all()?;
                Ok(Self {
                    lock_path: lock_path.to_path_buf(),
                    held: true,
                })
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // Lock is held by another process - check if it's stale
                if Self::is_lock_stale(lock_path)? {
                    // Remove stale lock and retry
                    fs::remove_file(lock_path).ok();
                    Self::try_acquire(lock_path)
                } else {
                    Err(anyhow::anyhow!("Merge lock is held by another process"))
                }
            }
            Err(e) => Err(e).context("Failed to acquire merge lock"),
        }
    }

    /// Check if an existing lock is stale (older than 5 minutes)
    fn is_lock_stale(lock_path: &Path) -> Result<bool> {
        let metadata = fs::metadata(lock_path)?;
        let modified = metadata.modified()?;
        let age = std::time::SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::ZERO);

        // Consider lock stale if older than MERGE_LOCK_STALE_TIMEOUT_SECS
        Ok(age > Duration::from_secs(MERGE_LOCK_STALE_TIMEOUT_SECS))
    }

    /// Release the lock
    pub fn release(mut self) -> Result<()> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<()> {
        if self.held {
            fs::remove_file(&self.lock_path).ok();
            self.held = false;
        }
        Ok(())
    }
}

impl Drop for MergeLock {
    fn drop(&mut self) {
        // Best-effort release on drop
        self.release_inner().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_merge_lock_acquire_release() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // First acquire should succeed
        let lock = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();
        assert!(work_dir.join("merge.lock").exists());

        // Release should remove the lock file
        lock.release().unwrap();
        assert!(!work_dir.join("merge.lock").exists());
    }

    #[test]
    fn test_merge_lock_concurrent_fails() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // First acquire should succeed
        let _lock1 = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();

        // Second acquire should fail (with short timeout)
        let result = MergeLock::acquire(work_dir, Duration::from_millis(100));
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_lock_drop_releases() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        {
            let _lock = MergeLock::acquire(work_dir, Duration::from_secs(1)).unwrap();
            assert!(work_dir.join("merge.lock").exists());
        }

        // Lock should be released on drop
        assert!(!work_dir.join("merge.lock").exists());
    }
}
