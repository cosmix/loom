//! Session management commands
//! Usage: loom sessions [list|kill <id>]

use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::parser::markdown::MarkdownDocument;

/// List all sessions
pub fn list() -> Result<()> {
    println!("Active sessions:");
    println!("─────────────────────────────────────────────────────────");

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        println!("(no .work/ directory - run 'loom init' first)");
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
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let session_file = work_dir.join("sessions").join(format!("{session_id}.md"));
    if !session_file.exists() {
        bail!("Session '{session_id}' not found");
    }

    // Read session file and extract tmux_session from frontmatter
    let content = std::fs::read_to_string(&session_file)
        .with_context(|| format!("Failed to read session file: {}", session_file.display()))?;

    let doc = MarkdownDocument::parse(&content).context("Failed to parse session file")?;

    let tmux_session = doc
        .get_frontmatter("tmux_session")
        .filter(|s| !s.is_empty() && s.as_str() != "null" && s.as_str() != "~")
        .map(|s| s.to_string());

    // Kill the tmux session if it exists
    if let Some(ref tmux_name) = tmux_session {
        println!("Killing tmux session: {tmux_name}");

        // Check if session exists before killing
        let has_session = Command::new("tmux")
            .args(["has-session", "-t", tmux_name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_session {
            let kill_result = Command::new("tmux")
                .args(["kill-session", "-t", tmux_name])
                .output()
                .context("Failed to execute tmux kill-session")?;

            if kill_result.status.success() {
                println!("  Tmux session '{tmux_name}' killed successfully");
            } else {
                let stderr = String::from_utf8_lossy(&kill_result.stderr);
                println!("  Warning: Failed to kill tmux session: {stderr}");
            }
        } else {
            println!("  Tmux session '{tmux_name}' not found (already terminated?)");
        }
    } else {
        println!("  No tmux session associated with this session");
    }

    // Remove the session file
    std::fs::remove_file(&session_file)
        .with_context(|| format!("Failed to remove session file: {}", session_file.display()))?;
    println!("  Session file removed");

    // Also remove the signal file if it exists
    let signal_file = work_dir.join("signals").join(format!("{session_id}.md"));
    if signal_file.exists() {
        std::fs::remove_file(&signal_file)
            .with_context(|| format!("Failed to remove signal file: {}", signal_file.display()))?;
        println!("  Signal file removed");
    }

    println!("\nSession '{session_id}' killed successfully");
    Ok(())
}
