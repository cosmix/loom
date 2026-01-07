//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|ready]

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};

/// Mark a stage as complete, optionally running acceptance criteria
pub fn complete(stage_id: String, session_id: Option<String>, no_verify: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Run acceptance criteria unless --no-verify is specified
    if !no_verify && !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        for criterion in &stage.acceptance {
            println!("  → {criterion}");
            let status = Command::new("sh")
                .arg("-c")
                .arg(criterion)
                .status()
                .with_context(|| format!("Failed to run: {criterion}"))?;

            if !status.success() {
                bail!(
                    "Acceptance criterion failed: {}\nStage remains in '{}' state.",
                    criterion,
                    format!("{:?}", stage.status).to_lowercase()
                );
            }
        }
        println!("All acceptance criteria passed!");
    }

    // Update session status if session_id provided
    if let Some(ref sid) = session_id {
        if let Err(e) = update_session_status(work_dir, sid, SessionStatus::Completed) {
            eprintln!("Warning: failed to update session status: {e}");
        }
    }

    // Mark stage as complete
    stage.complete(None);
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as complete");
    Ok(())
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
fn session_from_markdown(content: &str) -> Result<Session> {
    let yaml_content = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| anyhow::anyhow!("Invalid session file format: missing frontmatter"))?;

    serde_yaml::from_str(yaml_content).context("Failed to parse session YAML")
}

/// Convert session to markdown format
fn session_to_markdown(session: &Session) -> String {
    let yaml = serde_yaml::to_string(session).unwrap_or_else(|_| String::from("{}"));

    format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Tmux**: {}\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.tmux_session.as_ref().unwrap_or(&"None".to_string()),
    )
}

/// Block a stage with a reason
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Blocked;
    stage.close_reason = Some(reason.clone());
    stage.updated_at = chrono::Utc::now();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Reset a stage to pending
pub fn reset(stage_id: String, hard: bool, kill_session: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    // Kill tmux session if requested
    if kill_session {
        let tmux_name = format!("loom-{}", stage_id);
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &tmux_name])
            .output();
    }

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Pending;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.updated_at = chrono::Utc::now();

    // Hard reset also clears session assignment
    if hard {
        stage.session = None;
    }

    save_stage(&stage, work_dir)?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as ready for execution
pub fn ready(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.mark_ready();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as ready");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently executing
    if stage.status != StageStatus::Executing {
        // Silently skip if not executing - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.mark_waiting_for_input();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently waiting for input
    if stage.status != StageStatus::WaitingForInput {
        // Silently skip if not waiting - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.mark_executing();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}
