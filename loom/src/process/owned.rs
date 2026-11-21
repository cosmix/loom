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

    pub(crate) fn wait(&mut self, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
        wait_with_deadline(&mut self.child, timeout)
    }

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Waits up to `timeout` for the child to exit WITHOUT reaping it
    /// (`waitid` with `WNOWAIT`): while it stays a zombie its pid, and the
    /// id of the group it leads, cannot be reused, so [`Self::retire`] and a
    /// group scan still name only this child's group. `None` at the timeout.
    #[cfg(target_os = "macos")]
    pub(crate) fn wait_unreaped(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<ExitStatus>> {
        use std::os::unix::process::ExitStatusExt;
        let deadline = Instant::now() + timeout;
        let id = libc::id_t::from(self.child.id());
        loop {
            // SAFETY: an all-zero `siginfo_t` is a valid value for the call
            // to overwrite, and `info` outlives the call.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let flags = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
            // SAFETY: `info` is a valid, writable `siginfo_t`; WNOWAIT leaves
            // the child waitable, so `Child` still reaps it later.
            let result = unsafe { libc::waitid(libc::P_PID, id, &mut info, flags) };
            if result != 0 {
                match std::io::Error::last_os_error() {
                    error if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    // Already reaped through `Child`, which kept the status.
                    error if error.raw_os_error() == Some(libc::ECHILD) => {
                        return self.child.try_wait()
                    }
                    error => return Err(error),
                }
            }
            if info.si_pid != 0 {
                // A wait status: the exit code in bits 8-15, or the signal.
                let raw = match info.si_code {
                    libc::CLD_EXITED => (info.si_status & 0xff) << 8,
                    _ => info.si_status & 0x7f,
                };
                return Ok(Some(ExitStatus::from_raw(raw)));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(WAIT_POLL_INTERVAL);
        }
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

/// Reaps `child` or gives up once `limit` has passed, returning `None` then.
///
/// Polls `try_wait` instead of installing `wait_timeout`'s SIGCHLD handler. Some
/// sandboxes reject its signal-pipe write and abort the whole process.
pub(crate) fn wait_with_deadline(
    child: &mut Child,
    limit: Duration,
) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        std::thread::sleep(remaining.min(WAIT_POLL_INTERVAL));
    }
}

/// `SIGKILL` to the group `child` leads, then to `child` itself. Prefer calling
/// it before the child is reaped: once the leader is reaped and its group is
/// empty, its id may be reused by an unrelated group. A caller that sweeps
/// descendants after the leader exited accepts that window.
pub(crate) fn kill_group(child: &mut Child) {
    signal_group(child, nix::sys::signal::Signal::SIGKILL);
    let _ = child.kill();
}

fn signal_group(child: &Child, signal: nix::sys::signal::Signal) {
    if let Ok(pid) = i32::try_from(child.id()) {
        signal_group_id(pid, signal);
    }
}

/// Sends `signal` to process group `group`; returns whether `killpg` succeeded.
///
/// Refuses, sending nothing, a group `<= 1` (`killpg(0)` and `killpg(1)` reach
/// the caller's group or every process) or the caller's own group.
pub(crate) fn signal_group_id(group: i32, signal: nix::sys::signal::Signal) -> bool {
    if group <= 1 || group == nix::unistd::getpgrp().as_raw() {
        return false;
    }
    nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group), signal).is_ok()
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

    #[cfg(target_os = "macos")]
    #[test]
    fn wait_unreaped_reports_the_exit_and_leaves_the_leader_a_zombie() {
        use crate::process::{process_is_zombie, process_start_time};
        use std::os::unix::process::CommandExt;

        let child = Command::new("sh")
            .args(["-c", "exit 3"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut owned = OwnedProcess::new(child);
        let status = owned
            .wait_unreaped(Duration::from_secs(10))
            .unwrap()
            .expect("the child exits within the timeout");
        assert_eq!(status.code(), Some(3));
        assert!(process_is_zombie(pid), "the leader was reaped");
        assert!(process_start_time(pid).is_some(), "the pid was released");
        let reaped = owned
            .try_wait()
            .unwrap()
            .expect("the status is still there");
        assert_eq!(reaped.code(), Some(3));
        owned.retire();
    }
}
