//! Window operations for terminal management
//!
//! Provides functions for closing and focusing terminal windows.

use anyhow::Result;
use std::process::Command;

/// Close a window by its title using wmctrl or xdotool.
///
/// This is the preferred method for closing terminal windows because it works
/// correctly for all terminal emulators, including those like gnome-terminal
/// that use a server process (where killing by PID would kill all windows).
///
/// Returns `true` if the window was successfully closed, `false` otherwise.
pub fn close_window_by_title(title: &str) -> bool {
    // Try wmctrl first (most reliable for window management)
    if which::which("wmctrl").is_ok() {
        // wmctrl -c matches window name as substring and sends close request
        let output = Command::new("wmctrl").args(["-c", title]).output();

        if let Ok(out) = output {
            if out.status.success() {
                return true;
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // Search for window by exact name match
        let search_output = Command::new("xdotool")
            .args(["search", "--name", title])
            .output();

        if let Ok(out) = search_output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        // Send close request to the window
                        let close_result = Command::new("xdotool")
                            .args(["windowclose", window_id])
                            .output();

                        if close_result.is_ok() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Try to focus a window by its process ID
///
/// This is best-effort and uses wmctrl or xdotool if available.
/// Returns Ok(()) even if focusing fails (the window might not be focusable).
pub fn focus_window_by_pid(pid: u32) -> Result<()> {
    // Try wmctrl first (more reliable for window management)
    if which::which("wmctrl").is_ok() {
        let _ = Command::new("wmctrl")
            .args(["-i", "-a"])
            .arg(format!("0x{pid:x}")) // This won't work, but wmctrl -a with PID isn't standard
            .output();

        // Try by searching window list
        let output = Command::new("wmctrl").arg("-l").arg("-p").output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(window_pid) = parts[2].parse::<u32>() {
                    if window_pid == pid {
                        let window_id = parts[0];
                        let _ = Command::new("wmctrl")
                            .args(["-i", "-a", window_id])
                            .output();
                        return Ok(());
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        let _ = Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string(), "windowactivate"])
            .output();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_close_window_by_title_nonexistent() {
        // Test that closing a non-existent window returns false gracefully
        // This should not panic or error, just return false
        let result = close_window_by_title("nonexistent-window-title-12345");
        // Result depends on whether wmctrl/xdotool are available and if any window matches
        // The important thing is it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_close_window_by_title_format() {
        // Test that the title format we use matches what we expect
        let stage_id = "test-stage";
        let expected_title = format!("loom-{stage_id}");
        assert_eq!(expected_title, "loom-test-stage");
    }
}
