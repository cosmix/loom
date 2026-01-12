//! Session file lookup utilities with prefix matching support
//!
//! Session files are stored in `.work/sessions/` with the naming pattern `{session_id}.md`.
//! This module provides utilities for finding session files by exact ID or prefix match.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Find a session file by ID or prefix.
///
/// First attempts an exact match: `{id}.md`
/// If not found, scans the sessions directory for files starting with the given prefix.
///
/// # Arguments
/// * `work_dir` - Path to the `.work/` directory
/// * `id` - The session ID or prefix to find
///
/// # Returns
/// * `Ok(Some(path))` - Single match found
/// * `Ok(None)` - No matches found
/// * `Err` - Multiple matches found (ambiguous prefix) or filesystem error
pub fn find_session_file(work_dir: &Path, id: &str) -> Result<Option<PathBuf>> {
    let sessions_dir = work_dir.join("sessions");

    if !sessions_dir.exists() {
        return Ok(None);
    }

    // Try exact match first
    let exact_path = sessions_dir.join(format!("{id}.md"));
    if exact_path.exists() {
        return Ok(Some(exact_path));
    }

    // Scan for prefix matches
    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    let mut matches: Vec<PathBuf> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.starts_with(id) {
                matches.push(path);
            }
        }
    }

    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        _ => {
            let match_names: Vec<String> = matches
                .iter()
                .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
                .collect();
            bail!(
                "Ambiguous session prefix '{}': matches {} sessions ({})",
                id,
                matches.len(),
                match_names.join(", ")
            );
        }
    }
}

/// Extract session ID from a session file path.
///
/// # Arguments
/// * `path` - Path to the session file
///
/// # Returns
/// The session ID (filename without extension)
pub fn extract_session_id(path: &Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_session_file_exact_match() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        fs::write(sessions_dir.join("session-abc123.md"), "content").unwrap();

        let result = find_session_file(work_dir, "session-abc123").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("session-abc123.md"));
    }

    #[test]
    fn test_find_session_file_prefix_match() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        fs::write(sessions_dir.join("session-abc123.md"), "content").unwrap();
        fs::write(sessions_dir.join("session-xyz789.md"), "content").unwrap();

        // Prefix "session-abc" should match "session-abc123"
        let result = find_session_file(work_dir, "session-abc").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("session-abc123.md"));
    }

    #[test]
    fn test_find_session_file_ambiguous_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        fs::write(sessions_dir.join("session-abc123.md"), "content").unwrap();
        fs::write(sessions_dir.join("session-abc456.md"), "content").unwrap();

        // Prefix "session-abc" matches both
        let result = find_session_file(work_dir, "session-abc");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Ambiguous"));
    }

    #[test]
    fn test_find_session_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        fs::write(sessions_dir.join("session-abc123.md"), "content").unwrap();

        let result = find_session_file(work_dir, "session-xyz").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_no_sessions_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = find_session_file(work_dir, "session-abc").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_exact_match_preferred() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        // Create files where one is an exact match and another starts with same prefix
        fs::write(sessions_dir.join("session-abc.md"), "exact").unwrap();
        fs::write(sessions_dir.join("session-abc123.md"), "prefix").unwrap();

        // Exact match "session-abc" should be preferred
        let result = find_session_file(work_dir, "session-abc").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("session-abc.md"));
    }

    #[test]
    fn test_extract_session_id() {
        let path = PathBuf::from("/work/sessions/session-abc123.md");
        assert_eq!(
            extract_session_id(&path),
            Some("session-abc123".to_string())
        );
    }
}
