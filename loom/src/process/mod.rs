//! Process utilities for loom
//!
//! This module provides common process management functions used across the codebase.

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

/// Check if a process with the given PID is alive
///
/// Uses `nix::sys::signal::kill` with signal `None` (null signal / signal 0) to check
/// process existence. This properly distinguishes between:
/// - Process exists and we can signal it (`Ok(())`)
/// - Process exists but we lack permission (`EPERM`)
/// - Process does not exist (`ESRCH`)
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
        Ok(()) => true,           // Process exists and we can signal it
        Err(Errno::EPERM) => true, // Process exists but we lack permission
        Err(Errno::ESRCH) => false, // No such process
        Err(_) => false,           // Other error, treat as non-existent
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
        assert!(result, "PID 0 (kernel) should be detected as alive via EPERM");
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
