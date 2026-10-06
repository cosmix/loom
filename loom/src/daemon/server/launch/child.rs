//! Waiting for, signalling and describing the daemon child process.

use super::POLL_SLICE;
use anyhow::{Context, Result};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

/// How long a terminated child gets between SIGTERM and SIGKILL.
const TERMINATE_WAIT: Duration = Duration::from_secs(2);

/// Whether `child` is writing a core dump. Linux reports `CoreDumping: 1` in
/// `/proc/<pid>/status` until a piped core handler (apport, systemd-coredump)
/// finishes, which can outlast the grace, and the child is not reapable until
/// then. Always false without `/proc`.
pub(super) fn core_dumping(child: &Child) -> bool {
    std::fs::read_to_string(format!("/proc/{}/status", child.id())).is_ok_and(|status| {
        status.lines().any(|line| {
            line.strip_prefix("CoreDumping:")
                .is_some_and(|v| v.trim() == "1")
        })
    })
}

/// Poll `child` until it exits or `limit` passes.
pub(super) fn wait_for_exit(child: &mut Child, limit: Duration) -> Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .context("Failed to check the daemon child")?
        {
            return Ok(Some(status));
        }
        if started.elapsed() >= limit {
            return Ok(None);
        }
        thread::sleep(POLL_SLICE);
    }
}

/// SIGTERM, then SIGKILL after [`TERMINATE_WAIT`]; the child is reaped either
/// way. Best effort: the launch has already failed, and that failure is what
/// the caller reports. A child already reaped, or one whose state cannot be
/// read, gets no signal: its pid may name another process by now.
pub(super) fn terminate(child: &mut Child) {
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
    }
    if matches!(wait_for_exit(child, TERMINATE_WAIT), Ok(Some(_))) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `exit status N`, `signal SIGABRT`, or `signal N` for an unknown signal.
pub(super) fn describe_exit(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit status {code}");
    }
    match status.signal() {
        Some(number) => match Signal::try_from(number) {
            Ok(signal) => format!("signal {}", signal.as_str()),
            Err(_) => format!("signal {number}"),
        },
        None => status.to_string(),
    }
}
