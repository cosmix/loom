//! Session management commands
//! Usage: flux sessions [list|kill <id>]

use anyhow::{bail, Result};

/// List all sessions
pub fn list() -> Result<()> {
    println!("Active sessions:");
    println!("─────────────────────────────────────────────────────────");

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        println!("(no .work/ directory - run 'flux init' first)");
        return Ok(());
    }

    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        println!("(no sessions directory)");
        return Ok(());
    }

    // List session files
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        let mut found = false;
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "md") {
                let name = entry.file_name();
                println!("  {}", name.to_string_lossy().trim_end_matches(".md"));
                found = true;
            }
        }
        if !found {
            println!("(no active sessions)");
        }
    }

    Ok(())
}

/// Kill a session by ID
pub fn kill(session_id: String) -> Result<()> {
    println!("Killing session: {session_id}");

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'flux init' first.");
    }

    let session_file = work_dir.join("sessions").join(format!("{session_id}.md"));
    if !session_file.exists() {
        bail!("Session '{session_id}' not found");
    }

    // Would kill tmux session here
    let tmux_session = format!("flux-{session_id}");
    println!("Would kill tmux session: {tmux_session}");
    println!("\nNote: Full kill requires Phase 5 (orchestrator module)");

    Ok(())
}
