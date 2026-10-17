//! Ownership contract for finite subprocess operations.
//!
//! The process group belongs to one operation, including after its leader exits.
//! Leader exit and timeout retire the group immediately. A streaming adapter may
//! also retire on its protocol completion (quota replies); exit-status protocols
//! (verification/direct forwarding) require the leader status before success.
//! Reaping and pipe collection share one absolute cleanup deadline. Readers may
//! report partial output; they must never extend the operation with an unbounded join.
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

pub(crate) const CLEANUP_GRACE: Duration = Duration::from_secs(1);
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) struct OwnedProcess {
    child: Child,
    retired: Option<Instant>,
}

impl OwnedProcess {
    /// The caller must spawn the child in a fresh process group.
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child,
            retired: None,
        }
    }

    pub(crate) fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Polls instead of installing `wait_timeout`'s SIGCHLD handler. Some
    /// sandboxes reject its signal-pipe write and abort the whole process.
    pub(crate) fn wait(&mut self, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(Some(status));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            std::thread::sleep(remaining.min(WAIT_POLL_INTERVAL));
        }
    }

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Kill descendants even when the direct child was already reaped.
    pub(crate) fn retire(&mut self) -> Instant {
        if let Some(deadline) = self.retired {
            return deadline;
        }
        let deadline = Instant::now() + CLEANUP_GRACE;
        {
            self.retired = Some(deadline);
            kill_group(&mut self.child);
            let _ = self.wait(deadline.saturating_duration_since(Instant::now()));
        }
        deadline
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.retire();
    }
}

pub(super) fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(-pid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    let _ = child.kill();
}
