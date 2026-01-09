//! Worktree-related file operations
//!
//! This module provides utilities for managing files associated with worktrees
//! and stages in the `.work/` directory. It handles cleanup of session files,
//! signal files, and other stage-related metadata after merge operations.
//!
//! ## File Types
//!
//! - **Session files** (`.work/sessions/{session-id}.md`) - Track active sessions
//! - **Signal files** (`.work/signals/{session-id}.md`) - Assignment signals for agents
//! - **Stage files** (`.work/stages/{depth}-{stage-id}.md`) - Stage definitions and status
//! - **Handoff files** (`.work/handoffs/...`) - Context handoffs between sessions

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::parser::markdown::MarkdownDocument;

/// Configuration for stage file cleanup
#[derive(Debug, Clone)]
pub struct StageFileCleanupConfig {
    /// Remove session files for the stage
    pub cleanup_sessions: bool,
    /// Remove signal files for the stage
    pub cleanup_signals: bool,
    /// Archive stage file instead of deleting
    pub archive_stage: bool,
    /// Print progress messages
    pub verbose: bool,
}

impl Default for StageFileCleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_sessions: true,
            cleanup_signals: true,
            archive_stage: false,
            verbose: true,
        }
    }
}

impl StageFileCleanupConfig {
    /// Create a quiet config (no verbose output)
    pub fn quiet() -> Self {
        Self {
            verbose: false,
            ..Self::default()
        }
    }

    /// Create a config that archives instead of deleting
    pub fn with_archive() -> Self {
        Self {
            archive_stage: true,
            ..Self::default()
        }
    }
}

/// Result of stage file cleanup
#[derive(Debug, Clone, Default)]
pub struct StageFileCleanupResult {
    /// Number of session files removed
    pub sessions_removed: usize,
    /// Number of signal files removed
    pub signals_removed: usize,
    /// Whether the stage file was archived or removed
    pub stage_file_handled: bool,
    /// Session IDs that were cleaned up
    pub cleaned_session_ids: Vec<String>,
    /// Warnings that occurred during cleanup
    pub warnings: Vec<String>,
}

impl StageFileCleanupResult {
    /// Check if any cleanup was performed
    pub fn any_cleanup_done(&self) -> bool {
        self.sessions_removed > 0 || self.signals_removed > 0 || self.stage_file_handled
    }
}

/// Clean up all files associated with a stage after successful merge
///
/// This function removes or archives:
/// - Session files associated with the stage
/// - Signal files for those sessions
/// - Optionally the stage file itself
///
/// # Arguments
/// * `stage_id` - The stage ID to clean up files for
/// * `work_dir` - Path to the `.work/` directory
/// * `config` - Cleanup configuration options
///
/// # Returns
/// A `StageFileCleanupResult` describing what was cleaned up
pub fn cleanup_stage_files(
    stage_id: &str,
    work_dir: &Path,
    config: &StageFileCleanupConfig,
) -> Result<StageFileCleanupResult> {
    let mut result = StageFileCleanupResult::default();

    // Find and clean up sessions associated with this stage
    if config.cleanup_sessions {
        let sessions_result = cleanup_sessions_for_stage(stage_id, work_dir, config.verbose)?;
        result.sessions_removed = sessions_result.sessions_removed;
        result.cleaned_session_ids = sessions_result.session_ids;
        result.warnings.extend(sessions_result.warnings);

        // Clean up signals for those sessions
        if config.cleanup_signals {
            let signals_removed =
                cleanup_signals_for_sessions(&result.cleaned_session_ids, work_dir, config.verbose);
            result.signals_removed = signals_removed;
        }
    }

    // Handle stage file
    if config.archive_stage {
        if let Err(e) = archive_stage_file(stage_id, work_dir) {
            result.warnings.push(format!("Failed to archive stage file: {e}"));
        } else {
            result.stage_file_handled = true;
        }
    }

    Ok(result)
}

/// Internal result for session cleanup
struct SessionCleanupResult {
    sessions_removed: usize,
    session_ids: Vec<String>,
    warnings: Vec<String>,
}

/// Clean up session files for a specific stage
fn cleanup_sessions_for_stage(
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

    let entries = fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read sessions directory: {}", sessions_dir.display()))?;

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

/// Clean up signal files for given session IDs
fn cleanup_signals_for_sessions(session_ids: &[String], work_dir: &Path, verbose: bool) -> usize {
    let signals_dir = work_dir.join("signals");
    let mut removed = 0;

    if !signals_dir.exists() {
        return 0;
    }

    for session_id in session_ids {
        let signal_path = signals_dir.join(format!("{session_id}.md"));
        if signal_path.exists() {
            match fs::remove_file(&signal_path) {
                Ok(()) => {
                    removed += 1;
                    if verbose {
                        println!("  Removed signal file: {session_id}.md");
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "  Warning: Failed to remove signal file '{}': {e}",
                            signal_path.display()
                        );
                    }
                }
            }
        }
    }

    removed
}

/// Archive a stage file by moving it to the archive directory
fn archive_stage_file(stage_id: &str, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");
    let archive_dir = work_dir.join("archive");

    // Find the stage file
    let stage_file = find_stage_file_by_id(&stages_dir, stage_id)?;
    let Some(stage_file) = stage_file else {
        return Ok(()); // No file to archive
    };

    // Ensure archive directory exists
    fs::create_dir_all(&archive_dir)
        .with_context(|| "Failed to create archive directory")?;

    // Move to archive
    let archive_path = archive_dir.join(stage_file.file_name().unwrap_or_default());
    fs::rename(&stage_file, &archive_path)
        .with_context(|| format!("Failed to archive stage file to {}", archive_path.display()))?;

    Ok(())
}

/// Find a stage file by stage ID (handles depth prefix)
fn find_stage_file_by_id(stages_dir: &Path, stage_id: &str) -> Result<Option<std::path::PathBuf>> {
    if !stages_dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // Check for exact match (no prefix)
            if stem == stage_id {
                return Ok(Some(path));
            }

            // Check for prefixed match: XX-{stage_id}
            if let Some(suffix) = stem.strip_prefix(|c: char| c.is_ascii_digit()) {
                if let Some(suffix) = suffix.strip_prefix(|c: char| c.is_ascii_digit()) {
                    if let Some(id) = suffix.strip_prefix('-') {
                        if id == stage_id {
                            return Ok(Some(path));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
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

    let entries = fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read sessions directory: {}", sessions_dir.display()))?;

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

/// Remove a single signal file by session ID
pub fn remove_signal_file(session_id: &str, work_dir: &Path) -> Result<bool> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&signal_path)
        .with_context(|| format!("Failed to remove signal file: {}", signal_path.display()))?;

    Ok(true)
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

/// Check if any files exist for a stage that would need cleanup
pub fn stage_has_files(stage_id: &str, work_dir: &Path) -> bool {
    // Check for sessions
    if let Ok(sessions) = find_sessions_for_stage(stage_id, work_dir) {
        if !sessions.is_empty() {
            return true;
        }
    }

    // Check for stage file
    let stages_dir = work_dir.join("stages");
    if let Ok(Some(_)) = find_stage_file_by_id(&stages_dir, stage_id) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_work_dir() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create subdirectories
        fs::create_dir_all(work_dir.join("sessions")).unwrap();
        fs::create_dir_all(work_dir.join("signals")).unwrap();
        fs::create_dir_all(work_dir.join("stages")).unwrap();
        fs::create_dir_all(work_dir.join("archive")).unwrap();

        temp_dir
    }

    fn create_session_file(work_dir: &Path, session_id: &str, stage_id: &str) {
        let content = format!(
            r#"---
id: {session_id}
stage_id: {stage_id}
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session: {session_id}
"#
        );
        fs::write(
            work_dir.join("sessions").join(format!("{session_id}.md")),
            content,
        )
        .unwrap();
    }

    fn create_signal_file(work_dir: &Path, session_id: &str) {
        let content = format!("# Signal: {session_id}\n");
        fs::write(
            work_dir.join("signals").join(format!("{session_id}.md")),
            content,
        )
        .unwrap();
    }

    fn create_stage_file(work_dir: &Path, stage_id: &str) {
        let content = format!(
            r#"---
id: {stage_id}
name: Test Stage
status: Verified
---

# Stage: {stage_id}
"#
        );
        fs::write(
            work_dir.join("stages").join(format!("01-{stage_id}.md")),
            content,
        )
        .unwrap();
    }

    #[test]
    fn test_cleanup_config_default() {
        let config = StageFileCleanupConfig::default();
        assert!(config.cleanup_sessions);
        assert!(config.cleanup_signals);
        assert!(!config.archive_stage);
        assert!(config.verbose);
    }

    #[test]
    fn test_cleanup_config_quiet() {
        let config = StageFileCleanupConfig::quiet();
        assert!(!config.verbose);
    }

    #[test]
    fn test_cleanup_config_with_archive() {
        let config = StageFileCleanupConfig::with_archive();
        assert!(config.archive_stage);
    }

    #[test]
    fn test_cleanup_result_any_cleanup_done() {
        let mut result = StageFileCleanupResult::default();
        assert!(!result.any_cleanup_done());

        result.sessions_removed = 1;
        assert!(result.any_cleanup_done());
    }

    #[test]
    fn test_find_sessions_for_stage_empty() {
        let temp_dir = setup_work_dir();
        let result = find_sessions_for_stage("stage-1", temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_find_sessions_for_stage_found() {
        let temp_dir = setup_work_dir();
        create_session_file(temp_dir.path(), "session-1", "stage-1");
        create_session_file(temp_dir.path(), "session-2", "stage-1");
        create_session_file(temp_dir.path(), "session-3", "other-stage");

        let result = find_sessions_for_stage("stage-1", temp_dir.path());
        assert!(result.is_ok());
        let sessions = result.unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"session-1".to_string()));
        assert!(sessions.contains(&"session-2".to_string()));
    }

    #[test]
    fn test_remove_signal_file_exists() {
        let temp_dir = setup_work_dir();
        create_signal_file(temp_dir.path(), "session-1");

        let result = remove_signal_file("session-1", temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!temp_dir.path().join("signals/session-1.md").exists());
    }

    #[test]
    fn test_remove_signal_file_not_exists() {
        let temp_dir = setup_work_dir();

        let result = remove_signal_file("nonexistent", temp_dir.path());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_remove_session_file_exists() {
        let temp_dir = setup_work_dir();
        create_session_file(temp_dir.path(), "session-1", "stage-1");

        let result = remove_session_file("session-1", temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!temp_dir.path().join("sessions/session-1.md").exists());
    }

    #[test]
    fn test_stage_has_files_empty() {
        let temp_dir = setup_work_dir();
        assert!(!stage_has_files("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_stage_has_files_with_session() {
        let temp_dir = setup_work_dir();
        create_session_file(temp_dir.path(), "session-1", "stage-1");
        assert!(stage_has_files("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_stage_has_files_with_stage_file() {
        let temp_dir = setup_work_dir();
        create_stage_file(temp_dir.path(), "stage-1");
        assert!(stage_has_files("stage-1", temp_dir.path()));
    }

    #[test]
    fn test_cleanup_stage_files_complete() {
        let temp_dir = setup_work_dir();

        // Set up files for stage-1
        create_session_file(temp_dir.path(), "session-1", "stage-1");
        create_session_file(temp_dir.path(), "session-2", "stage-1");
        create_signal_file(temp_dir.path(), "session-1");
        create_signal_file(temp_dir.path(), "session-2");
        create_stage_file(temp_dir.path(), "stage-1");

        // Also create files for another stage (should not be cleaned)
        create_session_file(temp_dir.path(), "session-3", "other-stage");

        let config = StageFileCleanupConfig::quiet();
        let result = cleanup_stage_files("stage-1", temp_dir.path(), &config);

        assert!(result.is_ok());
        let cleanup_result = result.unwrap();
        assert_eq!(cleanup_result.sessions_removed, 2);
        assert_eq!(cleanup_result.signals_removed, 2);
        assert!(cleanup_result.any_cleanup_done());

        // Verify files are gone
        assert!(!temp_dir.path().join("sessions/session-1.md").exists());
        assert!(!temp_dir.path().join("sessions/session-2.md").exists());
        assert!(!temp_dir.path().join("signals/session-1.md").exists());
        assert!(!temp_dir.path().join("signals/session-2.md").exists());

        // Verify other stage files remain
        assert!(temp_dir.path().join("sessions/session-3.md").exists());
    }

    #[test]
    fn test_cleanup_stage_files_with_archive() {
        let temp_dir = setup_work_dir();
        create_stage_file(temp_dir.path(), "stage-1");

        let config = StageFileCleanupConfig::with_archive();
        let result = cleanup_stage_files("stage-1", temp_dir.path(), &config);

        assert!(result.is_ok());
        let cleanup_result = result.unwrap();
        assert!(cleanup_result.stage_file_handled);

        // Verify file was moved to archive
        assert!(!temp_dir.path().join("stages/01-stage-1.md").exists());
        assert!(temp_dir.path().join("archive/01-stage-1.md").exists());
    }

    #[test]
    fn test_find_stage_file_by_id_with_prefix() {
        let temp_dir = setup_work_dir();
        create_stage_file(temp_dir.path(), "my-stage");

        let result = find_stage_file_by_id(&temp_dir.path().join("stages"), "my-stage");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("01-my-stage.md"));
    }

    #[test]
    fn test_find_stage_file_by_id_not_found() {
        let temp_dir = setup_work_dir();

        let result = find_stage_file_by_id(&temp_dir.path().join("stages"), "nonexistent");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
