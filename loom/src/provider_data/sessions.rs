use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::parser::frontmatter::parse_from_markdown;
use anyhow::{Context, Result};
use std::fs;

/// Load all sessions from the state directory's sessions/ directory
pub fn load_all_sessions(work_dir: &WorkDir) -> Result<Vec<Session>> {
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_session_from_file(&path) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load session from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(sessions)
}

/// Load a single session from a markdown file
fn load_session_from_file(path: &std::path::Path) -> Result<Session> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;

    parse_from_markdown(&content, "Session")
}
