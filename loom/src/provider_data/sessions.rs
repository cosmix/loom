use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::parser::frontmatter::parse_from_markdown;
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs::read_dir;
use std::path::Path;

const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;

/// Load all sessions from the state directory's sessions/ directory
pub fn load_all_sessions(work_dir: &WorkDir) -> Result<Vec<Session>> {
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_session_from_file(&sessions_dir, &entry.file_name()) {
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
fn load_session_from_file(sessions_dir: &Path, name: &OsStr) -> Result<Session> {
    let content = crate::fs::safe_read::read_to_string_bounded(
        sessions_dir,
        Path::new(name),
        MAX_SESSION_FILE_BYTES,
    )
    .with_context(|| {
        format!(
            "Failed to read session file: {}",
            sessions_dir.join(name).display()
        )
    })?;

    parse_from_markdown(&content, "Session")
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
