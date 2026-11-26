//! Observer of the confined spawns one thread makes.
//!
//! The host child (`loom stage verify-host`) installs an observer that starts a
//! persistent group guardian before each confined command. Other processes
//! install no observer and retain the normal one-command process group.
//!
//! The slot belongs to the installing thread: a confined command another
//! thread spawns meanwhile is not the job's and must not join its guarded
//! groups or fail when the job's guardian start fails. The host child runs every
//! criterion synchronously on the thread running `run_job`. Revisit if a job
//! ever spawns its criteria from worker threads: they would then need the
//! observer handed to them explicitly.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::process::Child;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;

use crate::process::{signal_group_id, wait_with_deadline};

/// Callback starts and records a guardian before the confined command spawns.
pub struct SpawnObserver(pub Box<dyn Fn() -> Result<GroupAnchor>>);

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

/// Clears the calling thread's observer when dropped. It cannot leave the
/// thread, so it clears the installing thread's slot; installs do not nest.
#[derive(Debug)]
#[must_use = "dropping the guard clears the spawn observer"]
pub struct SpawnObserverGuard {
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for SpawnObserverGuard {
    fn drop(&mut self) {
        OBSERVER.with_borrow_mut(|slot| *slot = None);
    }
}

thread_local! {
    static OBSERVER: RefCell<Option<Rc<SpawnObserver>>> = const { RefCell::new(None) };
}

/// Install `observer` for every later confined spawn this thread makes,
/// replacing any current one. Dropping the returned guard clears it.
pub fn set_spawn_observer(observer: SpawnObserver) -> SpawnObserverGuard {
    OBSERVER.with_borrow_mut(|slot| *slot = Some(Rc::new(observer)));
    SpawnObserverGuard {
        _not_send: PhantomData,
    }
}

/// Start an observed command's guardian, if this thread installed an observer.
pub(crate) fn prepare_group() -> Result<Option<GroupAnchor>> {
    // Cloned out first: the callback may spawn, and must not find the slot borrowed.
    let observer = OBSERVER.with_borrow(Option::clone);
    observer.map(|observer| (observer.0)()).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing_observer() -> SpawnObserver {
        SpawnObserver(Box::new(|| Err(anyhow::anyhow!("guardian refused"))))
    }

    #[test]
    fn another_threads_spawn_is_not_observed() {
        let _guard = set_spawn_observer(failing_observer());
        let elsewhere = std::thread::spawn(|| prepare_group().map(|anchor| anchor.is_none()));
        let unobserved = elsewhere.join().expect("the other thread panicked");
        assert!(
            unobserved.expect("another thread's spawn ran this thread's observer"),
            "another thread's spawn got this thread's anchor"
        );
        assert!(
            prepare_group().is_err(),
            "this thread's spawn was not observed"
        );
    }

    #[test]
    fn dropping_the_guard_clears_the_observer() {
        drop(set_spawn_observer(failing_observer()));
        assert!(prepare_group().expect("no observer").is_none());
    }
}
