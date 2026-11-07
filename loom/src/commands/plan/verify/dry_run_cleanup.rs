//! Keeps `<repository>/.loom/dry-run/` from filling the disk: every dry run
//! removes the checkouts earlier runs left behind, and a SIGINT, SIGTERM or
//! SIGHUP removes the run's own checkout. A cold build of one checkout is tens
//! of gigabytes, and the checkout's `Drop` does not run when the process dies.
//!
//! Each run holds a shared advisory lock on `<parent>/.lock` for its whole
//! life. A run sweeps only while it can take that lock exclusively, so no live
//! run's checkout is ever removed; a run killed without a handler (SIGKILL)
//! releases its lock with the process and its checkout is swept by the next
//! run that finds the directory idle.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, PoisonError};

use anyhow::{Context, Result};
use fs2::FileExt;

use crate::verify::impact_tests::base::DRY_RUN_PREFIX;

const LOCK_FILE: &str = ".lock";
/// Exit status after SIGINT. The `ctrlc` handler cannot tell which signal
/// arrived, so SIGTERM and SIGHUP exit with it too.
const INTERRUPTED: i32 = 130;

/// The checkout directory the signal handler removes.
static ACTIVE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// A run's shared lock on the dry-run directory; dropping it releases the lock.
pub(super) struct RunLock {
    _file: File,
}

/// Sweep the checkouts no live run owns, then hold the shared lock for the
/// calling run.
pub(super) fn begin(parent: &Path) -> Result<RunLock> {
    let path = parent.join(LOCK_FILE);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    if FileExt::try_lock_exclusive(&file).is_ok() {
        sweep_stale(parent);
    }
    FileExt::lock_shared(&file).with_context(|| format!("failed to lock {}", path.display()))?;
    Ok(RunLock { _file: file })
}

/// Remove every `loom-dry-run-*` directory under `parent`, stopping any process
/// still running inside it. The caller holds the exclusive lock.
fn sweep_stale(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .file_name()
            .to_string_lossy()
            .starts_with(DRY_RUN_PREFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_dir());
        if stale {
            remove(&path);
        }
    }
}

fn remove(dir: &Path) {
    kill_processes_in(dir);
    if let Err(error) = std::fs::remove_dir_all(dir) {
        tracing::warn!(path = %dir.display(), %error, "failed to remove a dry-run checkout");
    }
}

/// Registers `dir` for removal by the signal handler until dropped.
pub(super) struct Registered;

pub(super) fn register(dir: &Path) -> Registered {
    *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = Some(dir.to_path_buf());
    Registered
}

impl Drop for Registered {
    fn drop(&mut self) {
        *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// On SIGINT, SIGTERM or SIGHUP (the `ctrlc` crate's `termination` feature),
/// remove the registered checkout and exit 130. A signal that arrives before
/// the checkout exists, or after the run's `Registered` is dropped, leaves
/// nothing to remove. The crate allows one handler per process; when another
/// owns it the run keeps only the next-run sweep.
pub(super) fn retire_on_signal() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let installed = ctrlc::set_handler(|| {
            let active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(dir) = active {
                remove(&dir);
            }
            std::process::exit(INTERRUPTED);
        });
        if let Err(error) = installed {
            tracing::warn!(%error, "dry run: no signal handler; an interrupt leaves its checkout");
        }
    });
}

/// SIGKILL every process whose working directory is inside `dir`. An
/// unconfined command leads its own process group and keeps building into the
/// checkout after loom stops. Linux only (reads `/proc`); elsewhere the
/// directory is removed under any such process.
#[cfg(target_os = "linux")]
fn kill_processes_in(dir: &Path) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let real = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let inside =
            std::fs::read_link(entry.path().join("cwd")).is_ok_and(|cwd| cwd.starts_with(&real));
        if let (true, true, Ok(raw)) = (inside, pid != std::process::id(), i32::try_from(pid)) {
            let _ = kill(Pid::from_raw(raw), Signal::SIGKILL);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn kill_processes_in(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn checkout(parent: &Path, name: &str) -> PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(dir.join("checkout/target/debug")).expect("dirs");
        std::fs::write(dir.join("checkout/target/debug/blob"), b"x").expect("file");
        dir
    }

    #[test]
    fn begin_removes_checkouts_left_by_earlier_runs_and_nothing_else() {
        let parent = TempDir::new().expect("tempdir");
        let stale = checkout(parent.path(), &format!("{DRY_RUN_PREFIX}abc123"));
        let other = checkout(parent.path(), "keep-me");

        let _lock = begin(parent.path()).expect("lock");

        assert!(!stale.exists(), "a stale checkout stays");
        assert!(other.exists(), "an unrelated directory was removed");
    }

    #[test]
    fn a_live_run_keeps_its_checkout_from_a_concurrent_sweep() {
        let parent = TempDir::new().expect("tempdir");
        let first = begin(parent.path()).expect("first run");
        let live = checkout(parent.path(), &format!("{DRY_RUN_PREFIX}live"));

        let second = begin(parent.path()).expect("second run");
        assert!(live.exists(), "a second run removed a live checkout");

        drop((first, second));
        // A test thread forking a child can hold a copy of a dropped lock
        // until the child execs, which only delays the sweep to a later run.
        for _ in 0..100 {
            let _third = begin(parent.path()).expect("third run");
            if !live.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the idle checkout was not swept");
    }
}
