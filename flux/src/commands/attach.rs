//! Attach to running tmux session
//! Usage: flux attach <stage_id|session_id>

use anyhow::{bail, Context, Result};

use crate::orchestrator::{
    attach_by_session, attach_by_stage, format_attachable_list, list_attachable,
};

/// Attach terminal to running Claude session (via tmux)
pub fn execute(target: String) -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!("No .work/ directory found. Run 'flux init' first.");
    }

    if target.starts_with("stage-") {
        attach_by_stage(&target, &work_dir)
    } else if target.starts_with("session-") {
        attach_by_session(&target, &work_dir)
    } else {
        attach_by_session(&target, &work_dir)
            .or_else(|_| attach_by_stage(&target, &work_dir))
            .with_context(|| format!("Could not find session or stage with identifier '{target}'"))
    }
}

/// List all attachable sessions
pub fn list() -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        println!("(no .work/ directory - run 'flux init' first)");
        return Ok(());
    }

    let sessions = list_attachable(&work_dir)?;

    if sessions.is_empty() {
        println!("No attachable sessions found.");
        println!("\nSessions must be in 'running' or 'paused' state with an active tmux session.");
        return Ok(());
    }

    print!("{}", format_attachable_list(&sessions));

    Ok(())
}
