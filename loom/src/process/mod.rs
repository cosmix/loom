//! Process utilities for loom
//!
//! This module provides common process management functions used across the codebase.

/// Check if a process with the given PID is alive
///
/// Uses libc::kill with signal 0 to check if the process exists.
/// This doesn't actually send a signal to the process, it only checks if
/// it exists and is owned by the current user (or we have permission to signal it).
///
/// # Arguments
/// * `pid` - The process ID to check
///
/// # Returns
/// * `true` - The process exists and we can signal it
/// * `false` - The process doesn't exist or we can't signal it
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
    // Use libc::kill directly for efficiency (avoids spawning a subprocess)
    // Signal 0 (null signal) is used to check process existence without sending a real signal
    //
    // Safety: kill with signal 0 is safe - it doesn't terminate any process,
    // only checks if the PID exists and we have permission to signal it
    match i32::try_from(pid) {
        Ok(pid_i32) => {
            // SAFETY: kill(pid, 0) only checks process existence, doesn't send a real signal
            let result = unsafe { libc::kill(pid_i32, 0) };
            result == 0
        }
        Err(_) => {
            // PID exceeds i32::MAX, treat as non-existent
            false
        }
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
}
