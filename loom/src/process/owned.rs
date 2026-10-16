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
use wait_timeout::ChildExt;

pub(crate) const CLEANUP_GRACE: Duration = Duration::from_secs(1);

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

    /// `wait_timeout` rounds its poll down to whole milliseconds and can return
    /// `None` up to 1 ms early, so wait again until the deadline has passed.
    pub(crate) fn wait(&mut self, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let status = self.child.wait_timeout(remaining)?;
            if status.is_some() || remaining.is_zero() {
                return Ok(status);
            }
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
            let _ = self
                .child
                .wait_timeout(deadline.saturating_duration_since(Instant::now()));
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
