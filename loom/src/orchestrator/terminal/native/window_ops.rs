//! Window operations for terminal management
//!
//! Provides functions for closing and focusing terminal windows.

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

/// Check if a window with the given title exists
///
/// Uses wmctrl or xdotool to check if a window with the given title exists.
/// Returns true if found, false otherwise (or if tools unavailable).
pub fn window_exists_by_title(title: &str) -> bool {
    // Try wmctrl first (most reliable for window management)
    if which::which("wmctrl").is_ok() {
        // wmctrl -l lists all windows
        let output = Command::new("wmctrl").arg("-l").output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // Check if any line contains the title
                for line in stdout.lines() {
                    if line.contains(title) {
                        return true;
                    }
                }
            }
        }
    }

    // Try xdotool as fallback
    if which::which("xdotool").is_ok() {
        // Search for window by name match
        let search_output = Command::new("xdotool")
            .args(["search", "--name", title])
            .output();

        if let Ok(out) = search_output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // xdotool returns one window ID per line
                // If any window IDs are returned, the window exists
                for window_id in stdout.lines() {
                    let window_id = window_id.trim();
                    if !window_id.is_empty() {
                        return true;
                    }
                }
            }
        }
    }

    false
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

    #[test]
    fn test_window_exists_by_title_nonexistent() {
        // Test that checking for a non-existent window returns false gracefully
        // This should not panic or error, just return false
        let result = window_exists_by_title("nonexistent-window-title-98765");
        // If wmctrl/xdotool are not available, this will return false
        // If they are available, it should also return false for this non-existent window
        assert!(!result, "Non-existent window should not be found");
    }

    #[test]
    fn test_window_exists_by_title_format() {
        // Test that we can construct valid title strings for the function
        let stage_id = "check-stage";
        let title = format!("loom-{stage_id}");
        // This tests the function doesn't panic with valid input
        let _ = window_exists_by_title(&title);
        // The actual result depends on whether such a window exists
    }
}
