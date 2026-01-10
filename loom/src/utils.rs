use std::io::{self, Write};
use std::path::Path;
use std::sync::Once;

/// ANSI escape codes for terminal control
const CURSOR_SHOW: &str = "\x1B[?25h";
const ATTR_RESET: &str = "\x1B[0m";
const CLEAR_LINE: &str = "\r\x1B[K";

static PANIC_HOOK_INSTALLED: Once = Once::new();

/// Restore terminal to a clean state.
///
/// This function:
/// - Shows the cursor (if hidden)
/// - Resets text attributes (colors, bold, etc.)
/// - Clears the current line (removes partial output from \r updates)
/// - Moves cursor to a new line
/// - Flushes stdout to ensure all escape codes are written
///
/// Call this before exiting to prevent leaving terminal in a weird state.
pub fn cleanup_terminal() {
    let mut stdout = io::stdout();

    // Build cleanup sequence:
    // 1. Clear current line (in case of \r-based status updates)
    // 2. Show cursor
    // 3. Reset attributes
    // 4. Ensure we're on a new line
    let cleanup = format!("{CLEAR_LINE}{CURSOR_SHOW}{ATTR_RESET}\n");

    // Ignore errors - we're cleaning up, best effort
    let _ = stdout.write_all(cleanup.as_bytes());
    let _ = stdout.flush();
}

/// Install a panic hook that restores terminal state before panicking.
///
/// This ensures the terminal is usable even if the program panics.
/// Safe to call multiple times - only installs once.
pub fn install_terminal_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Restore terminal first
            cleanup_terminal();
            // Then call the default panic handler
            default_hook(panic_info);
        }));
    });
}

/// Display a path relative to work_dir, or just filename if outside.
/// This prevents exposing full system paths to users.
pub fn display_path(path: &Path, work_dir: &Path) -> String {
    path.strip_prefix(work_dir)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "[path]".to_string())
        })
}

/// Truncate a string safely by character count, not byte count.
/// This ensures we don't break UTF-8 encoding by cutting mid-character.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_display_path_within_work_dir() {
        let work_dir = PathBuf::from("/home/user/project");
        let path = PathBuf::from("/home/user/project/.work/runners/se-001.md");
        let result = display_path(&path, &work_dir);
        assert_eq!(result, ".work/runners/se-001.md");
    }

    #[test]
    fn test_display_path_outside_work_dir() {
        let work_dir = PathBuf::from("/home/user/project");
        let path = PathBuf::from("/etc/passwd");
        let result = display_path(&path, &work_dir);
        assert_eq!(result, "passwd");
    }

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_utf8() {
        // Test with emoji (multi-byte UTF-8 characters)
        let emoji_str = "Hello 🦀 world";
        let result = truncate(emoji_str, 10);
        // Should truncate by character count, not byte count
        assert_eq!(result, "Hello 🦀...");

        // Verify the result is valid UTF-8
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_exact_length() {
        let s = "12345";
        assert_eq!(truncate(s, 5), "12345");
        assert_eq!(truncate(s, 6), "12345");
    }

    #[test]
    fn test_truncate_very_short() {
        // When max_chars is less than 3, we should still get "..."
        assert_eq!(truncate("hello", 3), "...");
        assert_eq!(truncate("hello", 2), "...");
    }
}
