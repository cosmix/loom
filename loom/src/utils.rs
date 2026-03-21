use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};

use crate::models::constants::display::{CONTEXT_HEALTHY_PCT, CONTEXT_WARNING_PCT};

/// Global path to the TUI marker file, set when TUI mode is entered.
static TUI_MARKER_PATH: OnceLock<PathBuf> = OnceLock::new();

/// ANSI escape codes for terminal control
const CURSOR_SHOW: &str = "\x1B[?25h";
const ATTR_RESET: &str = "\x1B[0m";
const CLEAR_LINE: &str = "\r\x1B[K";

static PANIC_HOOK_INSTALLED: Once = Once::new();

/// Format elapsed time in compact human-readable format.
///
/// Produces output like: `30s`, `1m30s`, `1h1m`
///
/// # Arguments
/// * `seconds` - The number of seconds elapsed
///
/// # Returns
/// A compact string representation of the duration
pub fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Format elapsed time in verbose human-readable format.
///
/// Produces output like: `30s`, `1m 30s`, `1h 1m 1s`
///
/// # Arguments
/// * `seconds` - The number of seconds elapsed
///
/// # Returns
/// A verbose string representation of the duration with spaces between units
pub fn format_elapsed_verbose(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {secs}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// Write a TUI marker file so we can detect unclean exits.
///
/// The marker contains the current PID. On the next loom command,
/// if the PID is dead, we know the TUI exited without cleanup
/// (e.g., SIGKILL) and can restore the terminal.
pub fn write_tui_marker(work_path: &Path) {
    let marker = work_path.join("tui.pid");
    let _ = std::fs::write(&marker, std::process::id().to_string());
    TUI_MARKER_PATH.set(marker).ok();
}

/// Remove the TUI marker file (called during normal cleanup).
pub fn remove_tui_marker() {
    if let Some(path) = TUI_MARKER_PATH.get() {
        let _ = std::fs::remove_file(path);
    }
}

/// Check for a stale TUI marker and recover terminal state if needed.
///
/// If a previous `loom status --live` was killed (SIGKILL, OOM, etc.)
/// without cleanup, the terminal may still be in raw mode with mouse
/// capture enabled. This function detects that situation and resets
/// the terminal.
///
/// Call this early in main() before dispatching any command.
pub fn recover_terminal_if_needed() {
    let marker = Path::new(".work/tui.pid");
    if !marker.exists() {
        return;
    }

    let contents = match std::fs::read_to_string(marker) {
        Ok(c) => c,
        Err(_) => {
            // Can't read marker — remove it and reset just in case
            let _ = std::fs::remove_file(marker);
            cleanup_terminal_crossterm();
            return;
        }
    };

    if let Ok(pid) = contents.trim().parse::<u32>() {
        if !crate::process::is_process_alive(pid) {
            // Previous TUI process is dead — terminal likely corrupted
            cleanup_terminal_crossterm();
            let _ = std::fs::remove_file(marker);
        }
        // If still alive, another TUI instance is running — don't interfere
    } else {
        // Malformed marker — clean up
        let _ = std::fs::remove_file(marker);
        cleanup_terminal_crossterm();
    }
}

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

/// Restore terminal from crossterm TUI state.
///
/// This handles the full crossterm cleanup:
/// - Disables raw mode
/// - Disables mouse capture
/// - Leaves alternate screen
/// - Shows cursor
///
/// Use this for TUI applications that use crossterm.
/// Safe to call even if terminal wasn't in TUI mode.
pub fn cleanup_terminal_crossterm() {
    use crossterm::{
        cursor::Show,
        event::DisableMouseCapture,
        execute,
        terminal::{disable_raw_mode, LeaveAlternateScreen},
    };

    // Ignore errors - best effort cleanup
    let _ = disable_raw_mode();
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen, Show);

    // Belt-and-suspenders: raw escape sequences for mouse disable
    // in case crossterm state tracking is confused
    let _ = stdout.write_all(b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l");
    let _ = stdout.flush();

    // Also do basic cleanup
    cleanup_terminal();

    // Remove TUI marker so the next loom command doesn't try to recover again
    remove_tui_marker();
}

/// Install a panic hook that restores crossterm terminal state before panicking.
///
/// This is the TUI-aware version that handles alternate screen and raw mode.
/// Safe to call multiple times - only installs once.
pub fn install_crossterm_panic_hook() {
    use std::sync::Once;
    static CROSSTERM_HOOK_INSTALLED: Once = Once::new();

    CROSSTERM_HOOK_INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Restore crossterm terminal state first
            cleanup_terminal_crossterm();
            // Then call the default panic handler
            default_hook(panic_info);
        }));
    });
}

/// Get the terminal color for a context percentage.
///
/// Returns red if >= WARNING threshold, yellow if >= HEALTHY threshold, green otherwise.
pub fn context_pct_terminal_color(pct: f32) -> colored::Color {
    if pct >= CONTEXT_WARNING_PCT {
        colored::Color::Red
    } else if pct >= CONTEXT_HEALTHY_PCT {
        colored::Color::Yellow
    } else {
        colored::Color::Green
    }
}

/// Get the TUI color for a context percentage.
///
/// Returns red if >= WARNING threshold, yellow if >= HEALTHY threshold, green otherwise.
pub fn context_pct_tui_color(pct: f32) -> ratatui::style::Color {
    if pct >= CONTEXT_WARNING_PCT {
        ratatui::style::Color::Red
    } else if pct >= CONTEXT_HEALTHY_PCT {
        ratatui::style::Color::Yellow
    } else {
        ratatui::style::Color::Green
    }
}

/// Truncate a string safely by character count, not byte count.
///
/// This ensures we don't break UTF-8 encoding by cutting mid-character.
/// Adds "..." ellipsis (3 characters) when truncating.
///
/// Use this for simple single-line string truncation.
/// For multi-line strings that need collapsing, use `truncate_for_display()`.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

/// Truncate a string for display, using UTF-8 safe character-based truncation.
///
/// This converts multi-line strings to single lines and truncates by character
/// count (not byte count) to avoid breaking UTF-8 encoding.
/// Uses "…" ellipsis (1 character) when truncating.
pub fn truncate_for_display(s: &str, max_len: usize) -> String {
    // First, collapse multi-line strings to single line
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    // Use character-based truncation to be UTF-8 safe
    if single_line.chars().count() <= max_len {
        single_line
    } else {
        // Take max_len - 1 characters and add ellipsis
        let truncated: String = single_line
            .chars()
            .take(max_len.saturating_sub(1))
            .collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_seconds() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(59), "59s");
    }

    #[test]
    fn test_format_elapsed_minutes() {
        assert_eq!(format_elapsed(60), "1m0s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3599), "59m59s");
    }

    #[test]
    fn test_format_elapsed_hours() {
        assert_eq!(format_elapsed(3600), "1h0m");
        assert_eq!(format_elapsed(3661), "1h1m");
        assert_eq!(format_elapsed(7200), "2h0m");
    }

    #[test]
    fn test_format_elapsed_verbose_seconds() {
        assert_eq!(format_elapsed_verbose(0), "0s");
        assert_eq!(format_elapsed_verbose(30), "30s");
        assert_eq!(format_elapsed_verbose(59), "59s");
    }

    #[test]
    fn test_format_elapsed_verbose_minutes() {
        assert_eq!(format_elapsed_verbose(60), "1m 0s");
        assert_eq!(format_elapsed_verbose(90), "1m 30s");
        assert_eq!(format_elapsed_verbose(3599), "59m 59s");
    }

    #[test]
    fn test_format_elapsed_verbose_hours() {
        assert_eq!(format_elapsed_verbose(3600), "1h 0m 0s");
        assert_eq!(format_elapsed_verbose(3661), "1h 1m 1s");
        assert_eq!(format_elapsed_verbose(7200), "2h 0m 0s");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("12345", 5), "12345");
        assert_eq!(truncate("12345", 6), "12345");
    }

    #[test]
    fn test_truncate_utf8() {
        let emoji_str = "Hello 🦀 world";
        let result = truncate(emoji_str, 10);
        assert_eq!(result, "Hello 🦀...");
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_very_short() {
        assert_eq!(truncate("hello", 3), "...");
        assert_eq!(truncate("hello", 2), "...");
    }

    #[test]
    fn test_truncate_for_display() {
        assert_eq!(truncate_for_display("short", 10), "short");
        assert_eq!(
            truncate_for_display("this is a longer string", 10),
            "this is a…"
        );
        assert_eq!(
            truncate_for_display("line1\nline2\nline3", 20),
            "line1 line2 line3"
        );
    }

    #[test]
    fn test_truncate_for_display_utf8() {
        let emoji_str = "Hello 🦀 world!";
        let result = truncate_for_display(emoji_str, 10);
        assert_eq!(result, "Hello 🦀 w…");
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_for_display_exact_length() {
        let s = "12345";
        assert_eq!(truncate_for_display(s, 5), "12345");
        assert_eq!(truncate_for_display(s, 6), "12345");
    }
}
