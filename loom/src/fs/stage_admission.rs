//! Per-stage exclusion between manual reset and scheduler admission.

use anyhow::{bail, Context, Result};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::time::{Duration, Instant};

use super::safe_fs::{
    flock_exclusive, open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd,
};

/// Hold this through the scheduler's final stage check and launch, or through
/// reset's liveness scan, retirement, state write and host-job cancellation.
/// This is a per-stage flock, never the shared `stages/` directory lock.
pub(crate) fn lock(work_dir: &Path, stage_id: &str) -> Result<OwnedFd> {
    let fd = open_lock_file(work_dir, stage_id)?;
    flock_exclusive(&fd).with_context(|| format!("locking admission for stage '{stage_id}'"))?;
    Ok(fd)
}

/// How long a lock taken on the daemon's scheduling thread may wait. A holder
/// stopped with Ctrl-Z would otherwise stall every stage; the caller defers
/// this one and tries again next tick.
pub(super) const DAEMON_LOCK_WAIT: Duration = Duration::from_millis(250);

/// [`lock`] for the daemon's scheduling thread: an error after
/// [`DAEMON_LOCK_WAIT`] while another process holds the lock.
pub(crate) fn lock_for_daemon(work_dir: &Path, stage_id: &str) -> Result<OwnedFd> {
    let fd = open_lock_file(work_dir, stage_id)?;
    flock_within(&fd, DAEMON_LOCK_WAIT)
        .with_context(|| format!("locking admission for stage '{stage_id}'"))?;
    Ok(fd)
}

/// Poll a non-blocking flock until `limit` has passed.
pub(super) fn flock_within(fd: &OwnedFd, limit: Duration) -> Result<()> {
    let deadline = Instant::now() + limit;
    loop {
        // SAFETY: fd is valid; the flags are constants.
        if unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        match error.kind() {
            std::io::ErrorKind::Interrupted => {}
            std::io::ErrorKind::WouldBlock if Instant::now() >= deadline => {
                bail!("another process has held the lock for over {limit:?}");
            }
            std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            _ => return Err(error).context("flock LOCK_EX|LOCK_NB failed"),
        }
    }
}

fn open_lock_file(work_dir: &Path, stage_id: &str) -> Result<OwnedFd> {
    crate::validation::validate_id(stage_id)?;
    // Worktrees intentionally expose `.loom/work` as a symlink to the state
    // root. Resolve that root once; every component below it stays no-follow.
    let canonical = work_dir.canonicalize()?;
    let root = safe_open_dirfd(&canonical)?;
    let dir = Path::new("stage-admission");
    safe_create_dir_all_in_workdir(root.as_raw_fd(), dir, 0o700)?;
    let path = dir.join(format!("{stage_id}.lock"));
    open_safely(root.as_raw_fd(), &path, libc::O_RDWR | libc::O_CREAT, 0o600)
        .with_context(|| format!("opening admission lock for stage '{stage_id}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemons_lock_gives_up_on_a_holder_and_takes_a_free_lock() {
        let temp = tempfile::tempdir().unwrap();
        let holder = lock(temp.path(), "alpha").unwrap();

        let started = Instant::now();
        let error = lock_for_daemon(temp.path(), "alpha").unwrap_err();
        assert!(
            format!("{error:#}").contains("another process"),
            "{error:#}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));

        drop(holder);
        assert!(lock_for_daemon(temp.path(), "alpha").is_ok());
    }
}
