//! Session I/O operations for continuation.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::models::session::Session;

/// Save session to .work/sessions/{id}.md
pub fn save_session(session: &Session, work_dir: &Path) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    fs::create_dir_all(&sessions_dir).context("Failed to create sessions directory")?;

    let session_file = sessions_dir.join(format!("{}.md", session.id));
    let content = session_to_markdown(session);

    fs::write(&session_file, content)
        .with_context(|| format!("Failed to write session file: {}", session_file.display()))?;

    Ok(())
}

/// Convert session to markdown format
pub fn session_to_markdown(session: &Session) -> String {
    let yaml = serde_yaml::to_string(session).unwrap_or_else(|_| String::from("{}"));

    format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Tmux**: {}\n- **Context**: {:.1}%\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.tmux_session.as_ref().unwrap_or(&"None".to_string()),
        session.context_health()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_to_markdown() {
        let mut session = Session::new();
        session.id = "test-session-123".to_string();
        session.assign_to_stage("stage-1".to_string());
        session.set_tmux_session("loom-stage-1".to_string());

        let markdown = session_to_markdown(&session);

        assert!(markdown.contains("---"));
        assert!(markdown.contains("# Session: test-session-123"));
        assert!(markdown.contains("## Details"));
        assert!(markdown.contains("**Stage**: stage-1"));
        assert!(markdown.contains("**Tmux**: loom-stage-1"));
    }
}
