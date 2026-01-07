//! Attach to running tmux sessions
//!
//! Usage:
//!   loom attach <stage_id|session_id>  - Attach to a specific session
//!   loom attach --all                  - Attach to all sessions (tmux overview)
//!   loom attach --all --gui            - Open each session in a GUI terminal window
//!   loom attach list                   - List attachable sessions

use anyhow::{bail, Context, Result};

use crate::orchestrator::{
    attach_by_session, attach_by_stage, attach_overview_session, create_overview_session,
    format_attachable_list, list_attachable, print_overview_instructions, spawn_gui_windows,
};

/// Attach terminal to running Claude session (via tmux)
pub fn execute(target: String) -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!("No .work/ directory found. Run 'loom init' first.");
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
        println!("(no .work/ directory - run 'loom init' first)");
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

/// Attach to all running sessions
///
/// In default mode, creates a tmux "overview" session with one window per
/// loom session. User navigates between windows with Ctrl+B N/P/W.
///
/// In GUI mode (--gui), spawns separate terminal windows for each session.
pub fn execute_all(gui_mode: bool, detach_existing: bool) -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!("No .work/ directory found. Run 'loom init' first.");
    }

    let sessions = list_attachable(&work_dir)?;

    if sessions.is_empty() {
        println!("No attachable sessions found.");
        println!("\nSessions must be in 'running' or 'paused' state with an active tmux session.");
        return Ok(());
    }

    if gui_mode {
        spawn_gui_windows(&sessions, detach_existing)
    } else {
        println!(
            "\nCreating overview session with {} loom session(s)...",
            sessions.len()
        );

        let overview_name = create_overview_session(&sessions, detach_existing)?;

        print_overview_instructions(sessions.len());

        attach_overview_session(&overview_name)
    }
}
