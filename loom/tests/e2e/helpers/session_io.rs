//! Session file I/O helpers for tests

use anyhow::{Context, Result};
use loom::models::session::Session;
use std::path::Path;

use super::yaml::extract_yaml_frontmatter;

/// Writes a session to .work/sessions/{session.id}.md
pub fn create_session_file(work_dir: &Path, session: &Session) -> Result<()> {
    let sessions_dir = work_dir.join(".work").join("sessions");
    std::fs::create_dir_all(&sessions_dir).context("Failed to create sessions directory")?;

    let session_path = sessions_dir.join(format!("{}.md", session.id));

    let yaml = serde_yaml::to_string(session).context("Failed to serialize session to YAML")?;

    let content = format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Context**: {:.1}%\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.context_usage_percent()
    );

    std::fs::write(&session_path, content)
        .with_context(|| format!("Failed to write session file: {}", session_path.display()))?;

    Ok(())
}

/// Reads a session from .work/sessions/{session_id}.md
pub fn read_session_file(work_dir: &Path, session_id: &str) -> Result<Session> {
    let session_path = work_dir
        .join(".work")
        .join("sessions")
        .join(format!("{session_id}.md"));

    if !session_path.exists() {
        anyhow::bail!("Session file not found: {}", session_path.display());
    }

    let content = std::fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    parse_session_from_markdown(&content)
        .with_context(|| format!("Failed to parse session from: {}", session_path.display()))
}

/// Parse a Session from markdown with YAML frontmatter
fn parse_session_from_markdown(content: &str) -> Result<Session> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let session: Session = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Session from YAML frontmatter")?;

    Ok(session)
}
