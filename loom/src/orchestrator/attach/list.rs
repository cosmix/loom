//! Session listing and display functionality.
//!
//! Functions to list attachable sessions and format them for display.

use std::path::Path;

use anyhow::{Context, Result};

use super::{
    format_status, is_attachable, load_session, load_stage, session_backend, AttachableSession,
};

/// List all sessions that can be attached to
///
/// - Reads .work/sessions/ for session files
/// - Filters to Running or Paused sessions with a backend (tmux_session or pid)
/// - Returns list with context health information
pub fn list_attachable(work_dir: &Path) -> Result<Vec<AttachableSession>> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut attachable = Vec::new();

    let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        match load_session(work_dir, &session_id) {
            Ok(session) => {
                if !is_attachable(&session) {
                    continue;
                }

                // Get backend from session (tmux or native)
                let backend = match session_backend(&session) {
                    Some(b) => b,
                    None => continue, // Skip sessions without a backend
                };

                let context_percent = session.context_health() as f64;

                let (stage_id, stage_name) = if let Some(ref sid) = session.stage_id {
                    match load_stage(work_dir, sid) {
                        Ok(stage) => (Some(sid.clone()), Some(stage.name)),
                        Err(_) => (Some(sid.clone()), None),
                    }
                } else {
                    (None, None)
                };

                attachable.push(AttachableSession {
                    session_id: session.id,
                    stage_id,
                    stage_name,
                    backend,
                    status: session.status,
                    context_percent,
                });
            }
            Err(_) => {
                continue;
            }
        }
    }

    attachable.sort_by(|a, b| a.session_id.cmp(&b.session_id));

    Ok(attachable)
}

/// Generate the formatted table for `loom attach list`
pub fn format_attachable_list(sessions: &[AttachableSession]) -> String {
    use super::SessionBackend;

    let mut output = String::new();

    output.push_str("SESSION          STAGE              BACKEND  STATUS      CONTEXT\n");

    for session in sessions {
        let stage_display = session
            .stage_name
            .as_ref()
            .map(|s| {
                if s.len() > 18 {
                    format!("{}...", &s[..15])
                } else {
                    s.clone()
                }
            })
            .unwrap_or_else(|| "-".to_string());

        let status_display = format_status(&session.status);

        let session_display = if session.session_id.len() > 16 {
            format!("{}...", &session.session_id[..13])
        } else {
            session.session_id.clone()
        };

        let backend_display = match &session.backend {
            SessionBackend::Tmux { .. } => "tmux",
            SessionBackend::Native { .. } => "native",
        };

        output.push_str(&format!(
            "{session_display:<16} {stage_display:<18} {backend_display:<8} {status_display:<11} {:>3.0}%\n",
            session.context_percent
        ));
    }

    output
}
