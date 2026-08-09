//! Process utilities for loom
//!
//! This module provides common process management functions used across the codebase.

mod environment;
mod identity;

use anyhow::{Context, Result};
use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

pub use environment::apply_stage_environment;
pub use identity::{
    process_start_time, terminate_verified, verify_process_identity, IdentityStatus,
    ProcessIdentity, UnverifiedProcessIdentity,
};

/// Structured wall-clock timeout returned by bounded command helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTimeoutError {
    operation: String,
    timeout: Duration,
}

impl ProcessTimeoutError {
    pub fn new(operation: impl Into<String>, timeout: Duration) -> Self {
        Self {
            operation: operation.into(),
            timeout,
        }
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl std::fmt::Display for ProcessTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} timed out after {:?}", self.operation, self.timeout)
    }
}

impl std::error::Error for ProcessTimeoutError {}

/// Outcome of a subprocess run under a wall-clock bound.
#[derive(Debug)]
pub enum BoundedOutput {
    /// The child exited on its own before the deadline.
    Completed(Output),
    /// The deadline elapsed first; the child was killed and reaped.
    TimedOut,
}

impl BoundedOutput {
    /// The child's output, or `None` if it was killed at the deadline.
    pub fn completed(self) -> Option<Output> {
        match self {
            BoundedOutput::Completed(output) => Some(output),
            BoundedOutput::TimedOut => None,
        }
    }
}

/// Run a command to completion under a wall-clock bound, killing it if the
/// deadline elapses.
///
/// # Why this exists
///
/// The orchestrator's poll loop is single-threaded: it syncs stage state,
/// spawns ready stages, and handles session teardown in one sequence. A
/// subprocess that never returns therefore does not merely fail one
/// operation — it stops *all* orchestration, silently, with no further log
/// output, while the daemon's socket thread keeps answering `loom status`
/// as if everything were healthy.
///
/// Window management is the dangerous case. On macOS `osascript` sends an
/// Apple Event and blocks with no timeout of its own: a TCC Automation
/// prompt (which a detached daemon cannot surface), a terminal-side modal,
/// or an unresponsive terminal app all park the call indefinitely. Linux is
/// less exposed — the `wmctrl`/`xdotool` paths are `which`-guarded and no-op
/// when the tools are absent — but an unresponsive X server can stall them
/// the same way.
///
/// Every external command issued from the orchestrator loop must therefore be
/// bounded. A timed-out teardown is a warning; an unbounded one is a hang.
///
/// # Output size
///
/// stdout/stderr are piped but only drained after the child exits, so a child
/// that writes more than the OS pipe buffer (~64KB) before exiting will block
/// on write and be killed at the deadline. That is acceptable here: this
/// helper is for control commands with negligible output. Use
/// `verify::criteria::executor` for commands whose output matters.
pub fn run_bounded(command: &mut Command, timeout: Duration) -> Result<BoundedOutput> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn {:?}", command.get_program()))?;

    match child
        .wait_timeout(timeout)
        .with_context(|| format!("Failed to wait for {:?}", command.get_program()))?
    {
        Some(_) => {
            let output = child.wait_with_output().with_context(|| {
                format!("Failed to collect output of {:?}", command.get_program())
            })?;
            Ok(BoundedOutput::Completed(output))
        }
        None => {
            // Kill the whole child process group so a control command cannot
            // leave descendants behind after its direct child times out.
            #[cfg(unix)]
            if let Ok(pid) = i32::try_from(child.id()) {
                let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
            Ok(BoundedOutput::TimedOut)
        }
    }
}

/// Run a command under a deadline and turn expiry into a typed error.
pub fn run_bounded_output(
    command: &mut Command,
    timeout: Duration,
    operation: impl Into<String>,
) -> Result<Output> {
    let operation = operation.into();
    match run_bounded(command, timeout)? {
        BoundedOutput::Completed(output) => Ok(output),
        BoundedOutput::TimedOut => Err(ProcessTimeoutError::new(operation, timeout).into()),
    }
}

/// Send `SIGTERM` to a process.
///
/// Returns `Ok(true)` when the signal was delivered, `Ok(false)` when the
/// process no longer exists (`ESRCH` — already gone, which is a success for
/// every caller here), and `Err` for any other failure.
///
/// Prefer this over shelling out to `kill(1)`: it cannot block, and it avoids
/// a fork+exec on a path that runs for every session teardown.
pub fn terminate(pid: u32) -> Result<bool> {
    let pid_i32 = i32::try_from(pid).with_context(|| format!("PID {pid} exceeds i32::MAX"))?;

    match kill(Pid::from_raw(pid_i32), Signal::SIGTERM) {
        Ok(()) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(e) => Err(anyhow::anyhow!("Failed to signal process {pid}: {e}")),
    }
}

/// Check if a process with the given PID is alive
///
/// Uses `nix::sys::signal::kill` with signal `None` (null signal / signal 0) to check
/// process existence. This properly distinguishes between:
/// - Process exists and we can signal it (`Ok(())`)
/// - Process exists but we lack permission (`EPERM`)
/// - Process does not exist (`ESRCH`)
///
/// # PID Reuse Warning
///
/// **RACE CONDITION HAZARD**: This function is subject to PID reuse races. Between the time
/// you check if a PID is alive and the time you act on that information, the process may
/// have exited and the OS may have reassigned that PID to a new, unrelated process.
///
/// This means `is_process_alive(pid) == true` only tells you "a process with this PID exists
/// right now", NOT "the process I'm tracking is still running". The new process may be
/// completely unrelated to the one you're monitoring.
///
/// **Safe usage patterns:**
/// - Only use this for informational purposes (logging, diagnostics)
/// - DO NOT use this as the sole basis for sending signals or making state transitions
/// - Prefer tracking processes via process handles or file locks when possible
/// - If you must use PIDs, combine with additional identity checks (start time, parent PID, etc.)
///
/// # Arguments
/// * `pid` - The process ID to check
///
/// # Returns
/// * `true` - The process exists (regardless of signal permission)
/// * `false` - The process doesn't exist or the PID is invalid
///
/// # Example
/// ```ignore
/// use loom::process::is_process_alive;
///
/// let our_pid = std::process::id();
/// assert!(is_process_alive(our_pid));
///
/// // Non-existent PID
/// assert!(!is_process_alive(999999999));
/// ```
pub fn is_process_alive(pid: u32) -> bool {
    let pid_i32 = match i32::try_from(pid) {
        Ok(v) => v,
        Err(_) => {
            // PID exceeds i32::MAX, treat as non-existent
            return false;
        }
    };

    // Send null signal (signal 0) to check process existence without
    // actually delivering a signal. The kernel returns different errors
    // depending on whether the process exists vs. permission denied.
    match kill(Pid::from_raw(pid_i32), None) {
        Ok(()) => true,             // Process exists and we can signal it
        Err(Errno::EPERM) => true,  // Process exists but we lack permission
        Err(Errno::ESRCH) => false, // No such process
        Err(_) => false,            // Other error, treat as non-existent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_process_is_alive() {
        // Our own process should be alive
        let our_pid = std::process::id();
        assert!(is_process_alive(our_pid));
    }

    #[test]
    fn test_nonexistent_process_is_not_alive() {
        // A very high PID is unlikely to exist
        assert!(!is_process_alive(999999999));
    }

    #[test]
    fn test_pid_one_behavior() {
        // PID 1 is init/systemd, we may or may not be able to signal it
        // depending on permissions, so we just test it doesn't panic
        let _ = is_process_alive(1);
    }

    #[test]
    fn test_pid_zero_kernel_process() {
        // PID 0 is the kernel scheduler process. We don't have permission to
        // signal it, so kill returns EPERM. The function should return true
        // because the process exists (EPERM means "exists but no permission").
        let result = is_process_alive(0);
        // On macOS and Linux, PID 0 exists (kernel) but we get EPERM.
        // With our EPERM handling, this should return true.
        assert!(
            result,
            "PID 0 (kernel) should be detected as alive via EPERM"
        );
    }

    #[test]
    fn test_run_bounded_returns_output_when_command_completes() {
        let mut cmd = Command::new("echo");
        cmd.arg("loom");

        let output = run_bounded(&mut cmd, Duration::from_secs(10))
            .expect("echo should run")
            .completed()
            .expect("echo should complete well inside the deadline");

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "loom");
    }

    #[test]
    fn test_run_bounded_kills_command_that_outlives_deadline() {
        let mut cmd = Command::new("sleep");
        cmd.arg("60");

        let started = std::time::Instant::now();
        let outcome =
            run_bounded(&mut cmd, Duration::from_millis(200)).expect("sleep should spawn");

        assert!(
            matches!(outcome, BoundedOutput::TimedOut),
            "a 60s sleep must not report completion under a 200ms bound"
        );
        // The whole point is that the caller regains control promptly.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "run_bounded returned after {:?}; it must not wait for the child",
            started.elapsed()
        );
    }

    #[test]
    fn bounded_output_returns_structured_timeout() {
        let mut cmd = Command::new("sleep");
        cmd.arg("60");

        let error = run_bounded_output(&mut cmd, Duration::from_millis(100), "git status")
            .expect_err("sleep must exceed the deadline");
        let timeout = error
            .downcast_ref::<ProcessTimeoutError>()
            .expect("timeout must be machine-identifiable");
        assert_eq!(timeout.operation(), "git status");
        assert_eq!(timeout.timeout(), Duration::from_millis(100));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_descendants_in_the_child_process_group() {
        let temp = tempfile::tempdir().unwrap();
        let pid_path = temp.path().join("descendant.pid");
        let pid_arg = shell_escape::escape(pid_path.display().to_string().into());
        let script = format!("sleep 60 & printf '%s' $! > {pid_arg}; wait");
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &script]);

        let outcome = run_bounded(&mut cmd, Duration::from_millis(200)).unwrap();
        assert!(matches!(outcome, BoundedOutput::TimedOut));
        let descendant_pid: u32 = std::fs::read_to_string(pid_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();

        for _ in 0..40 {
            if !is_process_alive(descendant_pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("timed-out command left descendant PID {descendant_pid} alive");
    }

    #[test]
    fn test_terminate_reports_missing_process_without_error() {
        // ESRCH is success-with-nothing-to-do, not a failure.
        assert!(!terminate(999999999).expect("ESRCH must not error"));
    }

    #[test]
    fn test_terminate_signals_live_process() {
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep should spawn");

        assert!(terminate(child.id()).expect("signal should be delivered"));

        let status = child.wait().expect("child should be reapable");
        assert!(!status.success(), "SIGTERM'd sleep should not exit cleanly");
    }

    #[test]
    fn test_u32_max_overflow_returns_false() {
        // u32::MAX exceeds i32::MAX, so the conversion fails.
        // The function should return false without panicking.
        assert!(
            !is_process_alive(u32::MAX),
            "u32::MAX should return false due to i32 overflow"
        );
    }
}
