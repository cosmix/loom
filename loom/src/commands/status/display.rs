use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::constants::DEFAULT_CONTEXT_LIMIT;
use crate::models::keys::frontmatter;
use crate::models::runner::{Runner, RunnerStatus};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::parser::markdown::MarkdownDocument;

pub fn load_runners(work_dir: &WorkDir) -> Result<(Vec<Runner>, usize)> {
    let runners_dir = work_dir.runners_dir();
    let mut runners = Vec::new();
    let mut count = 0;

    if !runners_dir.exists() {
        return Ok((runners, 0));
    }

    for entry in fs::read_dir(&runners_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            count += 1;
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(runner) = parse_runner_from_doc(&doc) {
                        runners.push(runner);
                    }
                }
            }
        }
    }

    Ok((runners, count))
}

fn parse_runner_from_doc(doc: &MarkdownDocument) -> Option<Runner> {
    let id = doc.get_frontmatter(frontmatter::ID)?.clone();
    let name = doc.get_frontmatter(frontmatter::NAME)?.clone();
    let runner_type = doc.get_frontmatter(frontmatter::RUNNER_TYPE)?.clone();

    let context_tokens = doc
        .get_frontmatter(frontmatter::CONTEXT_TOKENS)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let context_limit = doc
        .get_frontmatter(frontmatter::CONTEXT_LIMIT)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CONTEXT_LIMIT);

    Some(Runner {
        id,
        name,
        runner_type,
        status: RunnerStatus::Idle,
        assigned_track: doc.get_frontmatter(frontmatter::ASSIGNED_TRACK).cloned(),
        context_tokens,
        context_limit,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
    })
}

pub fn display_runner_health(runner: &Runner) {
    let health = runner.context_health();
    let health_str = format!("{health:.1}%");
    let context_tokens = runner.context_tokens;
    let context_limit = runner.context_limit;
    let status_str = format!("{context_tokens}/{context_limit} tokens");

    let colored_health = if health < 60.0 {
        health_str.green()
    } else if health < 75.0 {
        health_str.yellow()
    } else {
        health_str.red()
    };

    println!(
        "  {} [{}] {}",
        runner.name,
        colored_health,
        status_str.dimmed()
    );
}

pub fn count_files(dir: &std::path::Path) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }

    let count = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
        .count();

    Ok(count)
}

pub fn display_stages(work_dir: &WorkDir) -> Result<()> {
    let stages_dir = work_dir.stages_dir();
    if !stages_dir.exists() {
        return Ok(());
    }

    let mut stages = Vec::new();
    for entry in fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(stage) = parse_stage_from_doc(&doc) {
                        stages.push(stage);
                    }
                }
            }
        }
    }

    if !stages.is_empty() {
        println!("\n{}", "Active Stages".bold());
        for stage in stages {
            let status_color = match stage.status {
                StageStatus::Pending => "pending".yellow(),
                StageStatus::Ready => "ready".cyan(),
                StageStatus::Executing => "executing".blue(),
                StageStatus::WaitingForInput => "waiting-for-input".magenta(),
                StageStatus::Blocked => "blocked".red(),
                StageStatus::Completed => "completed".green(),
                StageStatus::NeedsHandoff => "needs-handoff".yellow(),
                StageStatus::Verified => "verified".green(),
            };
            println!("  {} [{}]", stage.name, status_color);
        }
    }

    Ok(())
}

fn parse_stage_from_doc(doc: &MarkdownDocument) -> Option<Stage> {
    let id = doc.get_frontmatter(frontmatter::ID)?.clone();
    let name = doc.get_frontmatter(frontmatter::NAME)?.clone();
    let status_str = doc.get_frontmatter(frontmatter::STATUS)?;
    let status = match status_str.as_str() {
        "pending" => StageStatus::Pending,
        "ready" => StageStatus::Ready,
        "executing" => StageStatus::Executing,
        "blocked" => StageStatus::Blocked,
        "completed" => StageStatus::Completed,
        "needs-handoff" => StageStatus::NeedsHandoff,
        "verified" => StageStatus::Verified,
        _ => return None,
    };

    Some(Stage {
        id,
        name,
        description: doc.get_frontmatter("description").cloned(),
        status,
        dependencies: Vec::new(),
        parallel_group: None,
        acceptance: Vec::new(),
        files: Vec::new(),
        plan_id: None,
        worktree: None,
        session: None,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        completed_at: None,
        close_reason: None,
    })
}

fn tmux_session_exists(session_name: &str) -> bool {
    std::process::Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

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
            let is_orphaned = matches!(
                session.status,
                SessionStatus::Spawning | SessionStatus::Running
            ) && session
                .tmux_session
                .as_ref()
                .map(|tmux_name| !tmux_session_exists(tmux_name))
                .unwrap_or(false);

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
        tmux_session: doc
            .get_frontmatter("tmux_session")
            .cloned()
            .filter(|s| !s.is_empty() && s != "null"),
        worktree_path: doc.get_frontmatter("worktree_path").map(|s| s.into()),
        pid: doc.get_frontmatter("pid").and_then(|s| s.parse().ok()),
        status,
        context_tokens,
        context_limit,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
    })
}
