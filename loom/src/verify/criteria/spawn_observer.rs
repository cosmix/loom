//! Process-wide observer of confined spawns.
//!
//! The host child (`loom stage verify-host`) installs an observer that starts a
//! persistent group guardian before each confined command. Other processes
//! install no observer and retain the normal one-command process group.

use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Result;

/// Callback starts and records a guardian before the confined command spawns.
pub struct SpawnObserver(pub Box<dyn Fn() -> Result<GroupAnchor> + Send + Sync>);

/// A separate leader kept alive after the observed command exits. Its stop
/// callback addresses the leader through the host job's private control FIFO.
pub struct GroupAnchor {
    child: Child,
    stop: Box<dyn Fn() -> Result<()> + Send + Sync>,
    retired: bool,
}

impl GroupAnchor {
    pub fn new(child: Child, stop: Box<dyn Fn() -> Result<()> + Send + Sync>) -> Self {
        Self {
            child,
            stop,
            retired: false,
        }
    }

    pub fn pgid(&self) -> u32 {
        self.child.id()
    }

    pub fn retire(&mut self) {
        if self.retired {
            return;
        }
        self.retired = true;
        // This direct child has not been reaped, so the kernel cannot recycle
        // its PID. Its numeric process group is safe to signal while held.
        let mut signalled = false;
        if let Ok(raw) = i32::try_from(self.pgid()) {
            if raw > 1 && raw != nix::unistd::getpgrp().as_raw() {
                signalled = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(raw),
                    nix::sys::signal::Signal::SIGKILL,
                )
                .is_ok();
            }
        }
        if !signalled {
            let _ = (self.stop)();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        tracing::warn!(pgid = self.pgid(), "criterion group anchor did not stop");
        // A still-owned, unreaped direct child cannot have a recycled PID.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for GroupAnchor {
    fn drop(&mut self) {
        self.retire();
    }
}

impl std::fmt::Debug for SpawnObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnObserver").finish_non_exhaustive()
    }
}

/// Clears the process-wide observer when dropped.
#[derive(Debug)]
#[must_use = "dropping the guard clears the spawn observer"]
pub struct SpawnObserverGuard {
    _private: (),
}

impl Drop for SpawnObserverGuard {
    fn drop(&mut self) {
        *slot() = None;
    }
}

static OBSERVER: Mutex<Option<Arc<SpawnObserver>>> = Mutex::new(None);

fn slot() -> MutexGuard<'static, Option<Arc<SpawnObserver>>> {
    // A panic inside an observer runs after the lock is released, so a
    // poisoned lock still holds a consistent value.
    OBSERVER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Install `observer` for every later confined spawn in this process,
/// replacing any current one. Dropping the returned guard clears it.
pub fn set_spawn_observer(observer: SpawnObserver) -> SpawnObserverGuard {
    *slot() = Some(Arc::new(observer));
    SpawnObserverGuard { _private: () }
}

/// Start an observed command's guardian, if one is installed.
pub(crate) fn prepare_group() -> Result<Option<GroupAnchor>> {
    let observer = slot().clone();
    observer.map(|observer| (observer.0)()).transpose()
}
