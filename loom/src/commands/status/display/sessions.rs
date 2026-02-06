use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::BackendType;
use crate::parser::frontmatter::parse_from_markdown;
use crate::process::is_process_alive;

pub fn display_sessions(work_dir: &WorkDir) -> Result<()> {
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Ok(());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(session) = parse_from_markdown::<Session>(&content, "Session") {
                    sessions.push(session);
                }
            }
        }
    }

    if !sessions.is_empty() {
        println!("\n{}", "Active Sessions".bold());
        for session in sessions {
            let is_orphaned = is_session_orphaned(&session);

            let status_color = if is_orphaned {
                "orphaned".red()
            } else {
                match session.status {
                    SessionStatus::Spawning => "spawning".yellow(),
                    SessionStatus::Running => "running".green(),
                    SessionStatus::Paused => "paused".yellow(),
                    SessionStatus::Completed => "completed".dimmed(),
                    SessionStatus::Crashed => "crashed".red(),
                    SessionStatus::ContextExhausted => "context-exhausted".red(),
                }
            };

            let stage_info = session
                .stage_id
                .as_ref()
                .map(|s| format!(" (stage: {s})"))
                .unwrap_or_default();

            println!("  {}{} [{}]", session.id, stage_info, status_color);
        }
    }

    Ok(())
}

pub fn is_session_orphaned(session: &Session) -> bool {
    if !matches!(
        session.status,
        SessionStatus::Spawning | SessionStatus::Running
    ) {
        return false;
    }

    let backend_type = if session.pid.is_some() {
        Some(BackendType::Native)
    } else {
        None
    };

    match backend_type {
        Some(BackendType::Native) => {
            if let Some(pid) = session.pid {
                !is_process_alive(pid)
            } else {
                false
            }
        }
        None => false,
    }
}
