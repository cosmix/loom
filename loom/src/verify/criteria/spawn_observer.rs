//! Process-wide observer of confined spawns.
//!
//! The host child (`loom stage verify-host`) records the process group of
//! every confined command it starts, so the daemon can kill a job's criteria
//! when it retires the job: a criterion runs in its own process group, which
//! killing the child's group does not reach. Every other process installs no
//! observer, and a spawn then notifies nothing.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Callback told the pid of each confined command right after it spawns. The
/// pid is also the command's process-group id (`process_group(0)`).
pub struct SpawnObserver(pub Box<dyn Fn(u32) + Send + Sync>);

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

/// Tell the current observer, if any, that a confined command spawned.
pub(crate) fn notify_spawned(pid: u32) {
    let observer = slot().clone();
    if let Some(observer) = observer {
        (observer.0)(pid);
    }
}
