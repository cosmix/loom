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
                    // Atomically claim the stale lock by renaming it
                    let claimed_path = lock_path.with_extension("claimed");
                    match fs::rename(lock_path, &claimed_path) {
                        Ok(_) => {
                            // We successfully claimed the stale lock
                            fs::remove_file(&claimed_path).ok();
                            Self::try_acquire(lock_path)
                        }
                        Err(_) => {
                            // Another process already claimed it, lock is not stale anymore
                            Err(anyhow::anyhow!("Merge lock is held by another process"))
                        }
                    }
                } else {
                    Err(anyhow::anyhow!("Merge lock is held by another process"))
                }
            }
            Err(e) => Err(e).context("Failed to acquire merge lock"),
        }
    }

    /// Check if an existing lock is stale.
    ///
    /// Staleness is decided by the holder's liveness first, NOT by mtime alone.
    /// A legitimately long merge (>5min) must not have its lock stolen just
    /// because the holder never refreshed the file's mtime. So:
    /// - If the lock records a `pid=` and that process is **alive**, the lock is
    ///   held — never stale, regardless of age.
    /// - If the recorded holder process is **dead**, the lock is stale (the
    ///   holder crashed without releasing it).
    /// - If no PID can be read from the file (truncated/legacy/partial write),
    ///   fall back to the mtime ceiling so a genuinely abandoned lock still
    ///   eventually clears.
    fn is_lock_stale(lock_path: &Path) -> Result<bool> {
        match Self::read_holder_pid(lock_path) {
            Some(pid) => {
                if crate::process::is_process_alive(pid) {
                    // Holder is still running — the lock is valid no matter how old.
                    Ok(false)
                } else {
                    // Holder is gone — reclaim the lock.
                    Ok(true)
                }
            }
            None => {
                // Couldn't read the holder PID; use the mtime backstop.
                let metadata = fs::metadata(lock_path)?;
                let modified = metadata.modified()?;
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::ZERO);
                Ok(age > Duration::from_secs(MERGE_LOCK_STALE_TIMEOUT_SECS))
            }
        }
    }

    /// Parse the `pid=<n>` line written by [`Self::try_acquire`].
    ///
    /// Returns `None` if the file can't be read or has no parseable `pid=` line
    /// (e.g. a partially-written lock), signalling the caller to fall back to
    /// the mtime heuristic.
    fn read_holder_pid(lock_path: &Path) -> Option<u32> {
        let contents = fs::read_to_string(lock_path).ok()?;
        contents
            .lines()
            .find_map(|line| line.strip_prefix("pid="))
            .and_then(|v| v.trim().parse::<u32>().ok())
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

    #[test]
    fn test_lock_with_live_holder_is_not_stale() {
        // A lock held by a live PID must NOT be considered stale. The live-holder
        // branch short-circuits before mtime is even consulted, which is the
        // >5min legitimate-merge no-steal guarantee (no mtime backdating needed).
        use std::io::Write;
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("merge.lock");

        let mut f = fs::File::create(&lock_path).unwrap();
        // Our own PID is guaranteed alive.
        writeln!(f, "pid={}", std::process::id()).unwrap();
        writeln!(f, "timestamp=2000-01-01T00:00:00Z").unwrap();
        drop(f);

        assert!(
            !MergeLock::is_lock_stale(&lock_path).unwrap(),
            "a lock held by a live process must never be stale"
        );
    }

    #[test]
    fn test_lock_with_dead_holder_is_stale() {
        // A lock whose recorded holder PID is dead must be reclaimable.
        use std::io::Write;
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("merge.lock");

        let mut f = fs::File::create(&lock_path).unwrap();
        // A PID that will not exist.
        writeln!(f, "pid=4294967294").unwrap();
        writeln!(f, "timestamp=2000-01-01T00:00:00Z").unwrap();
        drop(f);

        assert!(
            MergeLock::is_lock_stale(&lock_path).unwrap(),
            "a lock whose holder is dead must be stale"
        );
    }
}
