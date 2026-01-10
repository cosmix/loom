//! Session file operations for worktrees

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::parser::markdown::MarkdownDocument;

/// Internal result for session cleanup
pub(crate) struct SessionCleanupResult {
    pub sessions_removed: usize,
    pub session_ids: Vec<String>,
    pub warnings: Vec<String>,
}

/// Clean up session files for a specific stage
pub(crate) fn cleanup_sessions_for_stage(
    stage_id: &str,
    work_dir: &Path,
    verbose: bool,
) -> Result<SessionCleanupResult> {
    let sessions_dir = work_dir.join("sessions");
    let mut result = SessionCleanupResult {
        sessions_removed: 0,
        session_ids: Vec::new(),
        warnings: Vec::new(),
    };

    if !sessions_dir.exists() {
        return Ok(result);
    }

    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Try to read and parse the session file
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                result.warnings.push(format!(
                    "Failed to read session file '{}': {e}",
                    path.display()
                ));
                continue;
            }
        };

        // Parse to check stage_id
        let doc = match MarkdownDocument::parse(&content) {
            Ok(d) => d,
            Err(_) => continue, // Skip invalid files
        };

        let session_stage_id = doc.get_frontmatter("stage_id");
        if session_stage_id.map(|s| s.as_str()) != Some(stage_id) {
            continue;
        }

        // Get session ID before removing
        if let Some(session_id) = doc.get_frontmatter("id").cloned() {
            result.session_ids.push(session_id.clone());

            // Remove the session file
            match fs::remove_file(&path) {
                Ok(()) => {
                    result.sessions_removed += 1;
                    if verbose {
                        println!(
                            "  Removed session file: {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        );
                    }
                }
                Err(e) => {
                    result.warnings.push(format!(
                        "Failed to remove session file '{}': {e}",
                        path.display()
                    ));
                }
            }
        }
    }

    Ok(result)
}

/// Find all session IDs associated with a stage
///
/// This is useful for cleaning up sessions without needing to parse each file
pub fn find_sessions_for_stage(stage_id: &str, work_dir: &Path) -> Result<Vec<String>> {
    let sessions_dir = work_dir.join("sessions");
    let mut session_ids = Vec::new();

    if !sessions_dir.exists() {
        return Ok(session_ids);
    }

    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let doc = match MarkdownDocument::parse(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let session_stage_id = doc.get_frontmatter("stage_id");
        if session_stage_id.map(|s| s.as_str()) == Some(stage_id) {
            if let Some(session_id) = doc.get_frontmatter("id").cloned() {
                session_ids.push(session_id);
            }
        }
    }

    Ok(session_ids)
}

/// Remove a single session file by session ID
pub fn remove_session_file(session_id: &str, work_dir: &Path) -> Result<bool> {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));

    if !session_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&session_path)
        .with_context(|| format!("Failed to remove session file: {}", session_path.display()))?;

    Ok(true)
}
