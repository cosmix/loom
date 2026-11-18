//! Process-wide observer of confined spawns.
//!
//! The host child (`loom stage verify-host`) installs an observer that starts a
//! persistent group guardian before each confined command. Other processes
//! install no observer and retain the normal one-command process group.

use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Result;

use crate::process::{signal_group_id, wait_with_deadline};

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
        let signalled = i32::try_from(self.pgid())
            .is_ok_and(|raw| signal_group_id(raw, nix::sys::signal::Signal::SIGKILL));
        if !signalled {
            let _ = (self.stop)();
        }
        if wait_with_deadline(&mut self.child, Duration::from_secs(1))
            .ok()
            .flatten()
            .is_some()
        {
            return;
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
