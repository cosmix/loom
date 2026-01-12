//! Common utility functions shared across command implementations.
//!
//! This module provides utilities for:
//! - Work directory discovery
//! - Session ID detection (multiple strategies)
//! - Stage ID detection from various contexts
//! - String truncation for display

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

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

/// Detect session ID by matching stages to active sessions.
///
/// This strategy:
/// 1. Checks LOOM_SESSION_ID environment variable
/// 2. Extracts stage ID from current worktree path
/// 3. Searches for active session matching that stage
///
/// Use this when you need to find the session running in a specific worktree.
pub fn detect_session_from_sessions(work_dir: &Path) -> Result<String> {
    // First, try to get from environment (set by hooks)
    if let Ok(session_id) = std::env::var("LOOM_SESSION_ID") {
        return Ok(session_id);
    }

    // Try to detect from current worktree by looking at active sessions
    let sessions_dir = work_dir.join("sessions");
    if sessions_dir.exists() {
        // Get current working directory to determine which worktree we're in
        let cwd = std::env::current_dir()?;

        // Check if we're in a worktree by looking for .worktrees in the path
        if let Some(stage_id) = extract_stage_from_worktree_path(&cwd) {
            // Look for a session file that matches this stage
            for entry in std::fs::read_dir(&sessions_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let content = std::fs::read_to_string(&path)?;
                    // Check if session is for this stage and is active
                    if content.contains(&format!("stage: {stage_id}"))
                        && content.contains("status: Active")
                    {
                        if let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) {
                            return Ok(session_id.to_string());
                        }
                    }
                }
            }
        }
    }

    bail!(
        "Could not detect session ID. Please provide --session <session-id> explicitly, \
         or set LOOM_SESSION_ID environment variable."
    )
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

/// Extract stage ID from a worktree path like `.worktrees/stage-name/...`
///
/// Returns the stage ID if the path contains a `.worktrees/` directory,
/// None otherwise.
pub fn extract_stage_from_worktree_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find(".worktrees/") {
        let after_worktrees = &path_str[idx + ".worktrees/".len()..];
        // Take everything up to the next path separator
        let stage_id = after_worktrees.split(std::path::MAIN_SEPARATOR).next()?;
        if !stage_id.is_empty() {
            return Some(stage_id.to_string());
        }
    }
    None
}

/// Detect stage ID from current worktree branch.
///
/// Checks if the current git branch follows the loom worktree naming pattern
/// `loom/<stage-id>` and extracts the stage ID. Filters out special branches
/// like `loom/_base`.
pub fn detect_stage_id() -> Option<String> {
    // Check if we're in a worktree by looking at the branch name
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Worktree branches are named loom/<stage-id>
    if let Some(stage_id) = branch.strip_prefix("loom/") {
        // Filter out special branches like _base
        if !stage_id.starts_with('_') {
            return Some(stage_id.to_string());
        }
    }

    None
}

/// Truncate a string for display, using UTF-8 safe character-based truncation.
///
/// This converts multi-line strings to single lines and truncates by character
/// count (not byte count) to avoid breaking UTF-8 encoding.
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
    fn test_extract_stage_from_worktree_path() {
        let path = PathBuf::from("/home/user/project/.worktrees/my-stage/src/main.rs");
        assert_eq!(
            extract_stage_from_worktree_path(&path),
            Some("my-stage".to_string())
        );

        let path = PathBuf::from("/home/user/project/src/main.rs");
        assert_eq!(extract_stage_from_worktree_path(&path), None);

        let path = PathBuf::from(".worktrees/test-stage");
        assert_eq!(
            extract_stage_from_worktree_path(&path),
            Some("test-stage".to_string())
        );

        let path = PathBuf::from(".worktrees/");
        assert_eq!(extract_stage_from_worktree_path(&path), None);
    }

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
