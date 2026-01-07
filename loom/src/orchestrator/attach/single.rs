//! Single session attachment functionality.
//!
//! Functions to attach to individual tmux sessions by stage ID or session ID.

use std::path::Path;

use anyhow::{anyhow, Result};

#[cfg(not(unix))]
use anyhow::{bail, Context};

use super::{find_session_for_stage, format_manual_mode_error, load_session};

/// Attach to a tmux session by stage ID
///
/// - Looks up the session for the stage
/// - Prints helpful detach instructions first
/// - Executes: `tmux attach -t {session_name}`
/// - This will replace the current process (exec)
pub fn attach_by_stage(stage_id: &str, work_dir: &Path) -> Result<()> {
    let session = find_session_for_stage(work_dir, stage_id)?
        .ok_or_else(|| anyhow!("No active session found for stage '{stage_id}'"))?;

    let tmux_session = match session.tmux_session {
        Some(ref s) => s.clone(),
        None => {
            return Err(format_manual_mode_error(
                &session.id,
                session.worktree_path.as_ref(),
                work_dir,
            ));
        }
    };

    print_attach_instructions(&tmux_session);

    exec_tmux_attach(&tmux_session)
}

/// Attach to a tmux session directly by session ID or tmux session name
pub fn attach_by_session(session_id: &str, work_dir: &Path) -> Result<()> {
    let session = load_session(work_dir, session_id)?;

    let tmux_session = match session.tmux_session {
        Some(ref s) => s.clone(),
        None => {
            return Err(format_manual_mode_error(
                session_id,
                session.worktree_path.as_ref(),
                work_dir,
            ));
        }
    };

    print_attach_instructions(&tmux_session);

    exec_tmux_attach(&tmux_session)
}

/// Print the pre-attach instructions message
///
/// Shows helpful info about detaching and scrolling
pub fn print_attach_instructions(session_name: &str) {
    // Truncate session name if too long to fit in the box
    let display_name = if session_name.len() > 32 {
        format!("{}...", &session_name[..29])
    } else {
        session_name.to_string()
    };

    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│  Attaching to session {display_name:<32}│");
    println!("│                                                         │");
    println!("│  To detach (return to loom): Press Ctrl+B then D        │");
    println!("│  To scroll: Ctrl+B then [ (exit scroll: q)              │");
    println!("└─────────────────────────────────────────────────────────┘\n");
}

/// Execute tmux attach, replacing the current process on Unix
fn exec_tmux_attach(tmux_session: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(tmux_session)
            .exec();
        Err(anyhow!("Failed to exec tmux: {error}"))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg(tmux_session)
            .status()
            .context("Failed to execute tmux command")?;

        if !status.success() {
            bail!("tmux attach failed with status: {}", status);
        }
        Ok(())
    }
}
