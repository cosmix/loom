//! Ownership contract for finite subprocess operations.
//!
//! The process group belongs to one operation, including after its leader exits.
//! Leader exit and timeout retire the group immediately. A streaming adapter may
//! also retire on its protocol completion (quota replies); exit-status protocols
//! (verification/direct forwarding) require the leader status before success.
//! Reaping and pipe collection share one absolute cleanup deadline. Readers may
//! report partial output; they must never extend the operation with an unbounded join.
use std::process::{Child, Command, ExitStatus, Output};
use std::time::{Duration, Instant};

use anyhow::Result;

use super::errors::ProcessTimeoutError;
use super::{run_bounded_with, tail, BoundedOutput};

pub(crate) const CLEANUP_GRACE: Duration = Duration::from_secs(1);
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How a bounded run stops a group still running at its deadline.
#[derive(Clone, Copy, Debug)]
pub(super) enum Expiry {
    /// `SIGKILL` at once.
    Kill,
    /// `SIGTERM` first, so the group can clean up after itself (git removes its
    /// `index.lock` on `SIGTERM`, never on `SIGKILL`), then `SIGKILL` whatever is
    /// left once `grace` has passed.
    Terminate { grace: Duration },
}

/// [`super::run_bounded_output`] whose deadline sends `SIGTERM` to the group and
/// waits up to `grace` for the leader before the `SIGKILL`. For work that holds
/// locks a kill would leave behind; every other caller keeps the immediate kill.
pub(crate) fn run_bounded_terminating_output(
    command: &mut Command,
    timeout: Duration,
    grace: Duration,
    operation: impl Into<String>,
) -> Result<Output> {
    let operation = operation.into();
    let expiry = Expiry::Terminate { grace };
    match run_bounded_with(command, timeout, tail::Stdout::Whole, expiry)? {
        BoundedOutput::Completed(output) => Ok(output),
        BoundedOutput::TimedOut => Err(ProcessTimeoutError::new(operation, timeout).into()),
    }
}

pub(crate) struct OwnedProcess {
    child: Child,
    retired: Option<Instant>,
    owns_group: bool,
}

impl OwnedProcess {
    /// The caller must spawn the child in a fresh process group.
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child,
            retired: None,
            owns_group: true,
        }
    }

    /// The child joined a separately owned group whose anchor performs group
    /// retirement; this handle may retire only the direct child.
    pub(crate) fn child_only(child: Child) -> Self {
        Self {
            child,
            retired: None,
            owns_group: false,
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
            if self.owns_group {
                kill_group(&mut self.child);
            } else {
                let _ = self.child.kill();
            }
            let _ = self.wait(deadline.saturating_duration_since(Instant::now()));
        }
        deadline
    }

    /// Stop a run that reached its deadline as `expiry` says, then [`Self::retire`].
    pub(super) fn expire(&mut self, expiry: Expiry) -> Instant {
        if let Expiry::Terminate { grace } = expiry {
            if self.retired.is_none() && self.owns_group {
                signal_group(&self.child, nix::sys::signal::Signal::SIGTERM);
                let _ = self.wait(grace);
            }
        }
        self.retire()
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.retire();
    }
}

pub(super) fn kill_group(child: &mut Child) {
    signal_group(child, nix::sys::signal::Signal::SIGKILL);
    let _ = child.kill();
}

fn signal_group(child: &Child, signal: nix::sys::signal::Signal) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(-pid), signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::run_bounded_output;

    /// A shell that records a `SIGTERM` in `mark` before it exits, and would otherwise wait 30 s.
    fn trapping(mark: &std::path::Path) -> Command {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(r#"trap 'echo term > "$MARK"; exit 0' TERM; sleep 30 & wait"#)
            .env("MARK", mark);
        command
    }

    #[test]
    fn the_terminating_run_sends_sigterm_before_the_kill() {
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("mark");
        let started = Instant::now();
        let error = run_bounded_terminating_output(
            &mut trapping(&mark),
            Duration::from_millis(300),
            Duration::from_secs(5),
            "trap",
        )
        .unwrap_err();
        assert!(
            error.downcast_ref::<ProcessTimeoutError>().is_some(),
            "{error:#}"
        );
        assert_eq!(std::fs::read_to_string(&mark).unwrap(), "term\n");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the group was not stopped"
        );
    }

    #[test]
    fn the_plain_run_still_kills_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("mark");
        let error = run_bounded_output(&mut trapping(&mark), Duration::from_millis(300), "trap")
            .unwrap_err();
        assert!(
            error.downcast_ref::<ProcessTimeoutError>().is_some(),
            "{error:#}"
        );
        assert!(!mark.exists(), "SIGKILL leaves no chance to trap");
    }
}
