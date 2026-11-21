//! Liveness probes and the plain `SIGTERM` for a single pid.

use anyhow::{Context, Result};
use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

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
    let exists = || match kill(Pid::from_raw(pid_i32), None) {
        Ok(()) => true,             // Process exists and we can signal it
        Err(Errno::EPERM) => true,  // Process exists but we lack permission
        Err(Errno::ESRCH) => false, // No such process
        Err(_) => false,            // Other error, treat as non-existent
    };

    // A zombie still answers the null signal: the PID entry survives until the
    // parent reaps it. It cannot run, so it is not alive for any question loom
    // asks (crash detection, session liveness, daemon singleton ownership).
    // A zombie reaped between the two probes reads as "not a zombie", so the
    // null signal is sent again before the process is called alive.
    exists() && !super::identity::process_is_zombie(pid) && exists()
}
