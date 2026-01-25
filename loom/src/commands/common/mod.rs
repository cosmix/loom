//! Common utility functions shared across command implementations.
//!
//! This module provides utilities for:
//! - Work directory discovery
//! - Session ID detection (multiple strategies)
//! - Stage ID detection from various contexts
//! - String truncation for display

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::git::branch::stage_id_from_branch;

/// Find the .work directory by walking up from current directory.
///
/// Searches the current directory and all parent directories until it finds
/// a `.work` directory. This allows commands to work from any subdirectory
/// within a loom workspace.
pub fn find_work_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let work_path = current.join(".work");
        if work_path.exists() && work_path.is_dir() {
            return Ok(work_path);
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => bail!("Could not find .work directory. Are you in a loom workspace?"),
        }
    }
}

/// Detect session ID by finding most recent signal file.
///
/// This strategy:
/// 1. Checks LOOM_SESSION_ID environment variable
/// 2. Scans .work/signals/ for most recently modified signal file
///
/// Use this when you want to find the most recent active session.
pub fn detect_session_from_signals(work_dir: &Path) -> Result<String> {
    // Check environment variable first (set by loom when spawning)
    if let Ok(session_id) = std::env::var("LOOM_SESSION_ID") {
        return Ok(session_id);
    }

    // Try to detect from signal file in .work/signals/
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        bail!("No session ID provided or detected. Use --session <id>");
    }

    // Find most recent signal file
    let mut most_recent: Option<(String, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&signals_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(stem) = path.file_stem() {
                let session_id = stem.to_string_lossy().to_string();
                let metadata = entry.metadata()?;
                let modified = metadata.modified()?;

                match &most_recent {
                    None => most_recent = Some((session_id, modified)),
                    Some((_, prev_time)) if modified > *prev_time => {
                        most_recent = Some((session_id, modified));
                    }
                    _ => {}
                }
            }
        }
    }

    most_recent
        .map(|(id, _)| id)
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))
}

/// Detect stage ID from current worktree branch.
///
/// Checks if the current git branch follows the loom worktree naming pattern
/// `loom/<stage-id>` and extracts the stage ID. Filters out special branches
/// like `loom/_base`.
pub fn detect_stage_id() -> Option<String> {
    // Get current branch name
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Extract stage ID from branch name using centralized logic
    let stage_id = stage_id_from_branch(&branch)?;

    // Filter out special branches like _base
    if stage_id.starts_with('_') {
        return None;
    }

    Some(stage_id)
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
    fn test_detect_stage_id_format() {
        let parse_branch = |branch: &str| -> Option<String> {
            branch.strip_prefix("loom/").and_then(|s| {
                if !s.starts_with('_') {
                    Some(s.to_string())
                } else {
                    None
                }
            })
        };

        assert_eq!(
            parse_branch("loom/implement-auth"),
            Some("implement-auth".to_string())
        );
        assert_eq!(
            parse_branch("loom/stage-123"),
            Some("stage-123".to_string())
        );
        assert_eq!(parse_branch("loom/_base"), None);
        assert_eq!(parse_branch("main"), None);
        assert_eq!(parse_branch("feature/test"), None);
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
        // Test with emoji (multi-byte UTF-8 characters)
        let emoji_str = "Hello 🦀 world";
        let result = truncate(emoji_str, 10);
        assert_eq!(result, "Hello 🦀...");
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_very_short() {
        // When max_chars is less than 3, we should still get "..."
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
        // Test with emoji (multi-byte UTF-8 characters)
        let emoji_str = "Hello 🦀 world!";
        let result = truncate_for_display(emoji_str, 10);
        // Should truncate by character count, not byte count
        assert_eq!(result, "Hello 🦀 w…");

        // Verify the result is valid UTF-8
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_for_display_exact_length() {
        let s = "12345";
        assert_eq!(truncate_for_display(s, 5), "12345");
        assert_eq!(truncate_for_display(s, 6), "12345");
    }
}
