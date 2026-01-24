//! Session cleanup utilities
//!
//! Note: Session finding functions (find_session_for_stage, find_sessions_for_stage)
//! are now in `crate::fs::session_files`. Import from there instead.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::continuation::session_to_markdown;

/// Clean up resources associated with a completed stage
///
/// This function performs best-effort cleanup and logs warnings on failure:
/// 1. Updates session status to Completed
/// 2. Removes the signal file
pub fn cleanup_session_resources(_stage_id: &str, session_id: &str, work_dir: &Path) {
    // 1. Update session status to Completed
    if let Err(e) = update_session_status(work_dir, session_id, SessionStatus::Completed) {
        eprintln!("Warning: failed to update session status: {e}");
    }

    // 2. Remove signal file
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    match fs::remove_file(&signal_path) {
        Ok(()) => {
            println!("Removed signal file '{}'", signal_path.display());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Signal file may not exist - this is fine
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to remove signal file '{}': {e}",
                signal_path.display()
            );
        }
    }
}

/// Update a session's status in .work/sessions/
fn update_session_status(work_dir: &Path, session_id: &str, status: SessionStatus) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    let session_path = sessions_dir.join(format!("{session_id}.md"));

    if !session_path.exists() {
        bail!("Session file not found: {}", session_path.display());
    }

    let content = fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    // Parse session from markdown
    let session = session_from_markdown(&content)?;

    // Update status
    let mut session = session;
    session.status = status;
    session.last_active = chrono::Utc::now();

    // Write back
    let updated_content = session_to_markdown(&session);
    fs::write(&session_path, updated_content)
        .with_context(|| format!("Failed to write session file: {}", session_path.display()))?;

    Ok(())
}

/// Parse session from markdown with YAML frontmatter
pub fn session_from_markdown(content: &str) -> Result<Session> {
    let yaml_content = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| anyhow::anyhow!("Invalid session file format: missing frontmatter"))?;

    serde_yaml::from_str(yaml_content).context("Failed to parse session YAML")
}
