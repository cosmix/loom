use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::constants::DEFAULT_CONTEXT_LIMIT;
use crate::models::keys::frontmatter;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::check_pid_alive;
use crate::orchestrator::terminal::BackendType;
use crate::parser::markdown::MarkdownDocument;

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
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(session) = parse_session_from_doc(&doc) {
                        sessions.push(session);
                    }
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

fn parse_session_from_doc(doc: &MarkdownDocument) -> Option<Session> {
    let id = doc.get_frontmatter(frontmatter::ID)?.clone();
    let status_str = doc.get_frontmatter(frontmatter::STATUS)?;
    let status = match status_str.as_str() {
        "spawning" => SessionStatus::Spawning,
        "running" => SessionStatus::Running,
        "paused" => SessionStatus::Paused,
        "completed" => SessionStatus::Completed,
        "crashed" => SessionStatus::Crashed,
        "context-exhausted" => SessionStatus::ContextExhausted,
        _ => return None,
    };

    let context_tokens = doc
        .get_frontmatter("context_tokens")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let context_limit = doc
        .get_frontmatter("context_limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CONTEXT_LIMIT);

    Some(Session {
        id,
        stage_id: doc.get_frontmatter("stage_id").cloned(),
        worktree_path: doc.get_frontmatter("worktree_path").map(|s| s.into()),
        pid: doc.get_frontmatter("pid").and_then(|s| s.parse().ok()),
        status,
        context_tokens,
        context_limit,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        session_type: crate::models::session::SessionType::default(),
        merge_source_branch: None,
        merge_target_branch: None,
    })
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
                !check_pid_alive(pid)
            } else {
                false
            }
        }
        None => false,
    }
}
