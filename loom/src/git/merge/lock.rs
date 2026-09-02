//! OS-backed locking for merge operations.

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// An exclusive OS lock held on the stable `.loom/work/merge.lock` inode.
///
/// The file is intentionally never unlinked. Process termination releases the
/// OS lock, so stale-path reclamation is unnecessary and a departing owner
/// cannot remove a successor's lock file.
#[derive(Debug)]
pub struct MergeLock {
    file: File,
    held: bool,
}

impl MergeLock {
    /// Acquire the merge lock, waiting at most `timeout` for another owner.
    pub fn acquire(work_dir: &Path, timeout: Duration) -> Result<Self> {
        let lock_path = work_dir.join("merge.lock");
        let started = Instant::now();

        loop {
            if let Some(lock) = Self::try_acquire(&lock_path)? {
                return Ok(lock);
            }
            if started.elapsed() >= timeout {
                bail!(
                    "Timed out after {:.3}s waiting for merge lock at {}",
                    timeout.as_secs_f64(),
                    lock_path.display()
                );
            }
            std::thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(started.elapsed())));
        }
    }

    fn try_acquire(lock_path: &Path) -> Result<Option<Self>> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .with_context(|| format!("Failed to open merge lock at {}", lock_path.display()))?;

        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => {
                Self::record_holder(&mut file).with_context(|| {
                    format!(
                        "Failed to record merge-lock owner at {}",
                        lock_path.display()
                    )
                })?;
                Ok(Some(Self { file, held: true }))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("Failed to lock merge lock at {}", lock_path.display())),
        }
    }

    fn record_holder(file: &mut File) -> Result<()> {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "pid={}", std::process::id())?;
        writeln!(file, "timestamp={}", chrono::Utc::now().to_rfc3339())?;
        file.sync_all()?;
        Ok(())
    }

    /// Release this owner's OS lock. The stable lock file remains in place.
    pub fn release(mut self) -> Result<()> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<()> {
        if self.held {
            FileExt::unlock(&self.file).context("Failed to release merge lock")?;
            self.held = false;
        }
        Ok(())
    }
}

impl Drop for MergeLock {
    fn drop(&mut self) {
        if let Err(error) = self.release_inner() {
            tracing::warn!(%error, "Failed to release merge lock during drop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn contention_is_reported_without_reclaiming_the_path() {
        let temp = TempDir::new().unwrap();
        let lock_path = temp.path().join("merge.lock");
        let _owner = MergeLock::try_acquire(&lock_path).unwrap().unwrap();

        assert!(MergeLock::try_acquire(&lock_path).unwrap().is_none());
        assert!(lock_path.exists());
    }

    #[test]
    fn drop_releases_only_owner_and_never_unlinks_successor_path() {
        let temp = TempDir::new().unwrap();
        let lock_path = temp.path().join("merge.lock");
        let first = MergeLock::try_acquire(&lock_path).unwrap().unwrap();

        drop(first);
        assert!(lock_path.exists(), "the stable lock inode must be retained");

        let successor = MergeLock::try_acquire(&lock_path).unwrap().unwrap();
        assert!(
            MergeLock::try_acquire(&lock_path).unwrap().is_none(),
            "a contender must not own the lock while the successor holds it"
        );
        assert!(
            lock_path.exists(),
            "successor's lock path must remain present"
        );

        drop(successor);
        assert!(MergeLock::try_acquire(&lock_path).unwrap().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn lock_handoffs_keep_the_same_inode() {
        use std::os::unix::fs::MetadataExt;

        let temp = TempDir::new().unwrap();
        let lock_path = temp.path().join("merge.lock");
        let first = MergeLock::try_acquire(&lock_path).unwrap().unwrap();
        let inode = std::fs::metadata(&lock_path).unwrap().ino();
        drop(first);

        let successor = MergeLock::try_acquire(&lock_path).unwrap().unwrap();
        assert_eq!(std::fs::metadata(&lock_path).unwrap().ino(), inode);
        assert!(MergeLock::try_acquire(&lock_path).unwrap().is_none());
        drop(successor);
    }

    #[test]
    fn acquire_times_out_while_an_owner_holds_the_lock() {
        let temp = TempDir::new().unwrap();
        let _owner = MergeLock::acquire(temp.path(), Duration::ZERO).unwrap();

        let error = MergeLock::acquire(temp.path(), Duration::ZERO).unwrap_err();
        assert!(error.to_string().contains("Timed out"));
    }
}
