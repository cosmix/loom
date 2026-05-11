//! Session management commands
//! Usage: loom sessions [list|kill <id>...]

use anyhow::{bail, Context, Result};

use crate::commands::common::find_work_dir;
use crate::fs::session_files::find_session_file;
use crate::fs::worktree_files::find_sessions_for_stage;
use crate::models::session::Session;
use crate::orchestrator::terminal::dispatcher::{BackendDispatcher, BackendNeeds};
use crate::parser::frontmatter::parse_from_markdown;

/// List all sessions
pub fn list() -> Result<()> {
    println!("Active sessions:");
    println!("─────────────────────────────────────────────────────────");

    let work_dir = match find_work_dir() {
        Ok(dir) => dir,
        Err(_) => {
            println!("(no .work/ directory - run 'loom init' first)");
            return Ok(());
        }
    };

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

/// Kill one or more sessions by ID/prefix, or all sessions for a stage
pub fn kill(session_ids: Vec<String>, stage: Option<String>) -> Result<()> {
    let work_dir = find_work_dir()?;

    // Collect all session IDs to kill
    let mut ids_to_kill = session_ids;

    // If --stage is provided, find all sessions for that stage
    if let Some(stage_id) = &stage {
        let stage_sessions = find_sessions_for_stage(stage_id, &work_dir)
            .with_context(|| format!("Failed to find sessions for stage '{stage_id}'"))?;

        if stage_sessions.is_empty() {
            println!("No sessions found for stage '{stage_id}'");
            return Ok(());
        }

        println!(
            "Found {} session(s) for stage '{stage_id}'",
            stage_sessions.len()
        );
        ids_to_kill.extend(stage_sessions);
    }

    if ids_to_kill.is_empty() {
        bail!("No sessions specified. Provide session IDs or use --stage <stage-id>");
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    for session_id in &ids_to_kill {
        match kill_single_session(&work_dir, session_id) {
            Ok(()) => success_count += 1,
            Err(e) => {
                eprintln!("Failed to kill session '{session_id}': {e}");
                failure_count += 1;
            }
        }
    }

    // Report summary
    println!();
    if failure_count == 0 {
        println!("Successfully killed {success_count} session(s)");
    } else {
        println!("Killed {success_count} session(s), {failure_count} failed");
    }

    if failure_count > 0 {
        bail!("{failure_count} session(s) failed to kill");
    }

    Ok(())
}

/// Kill a single session by ID or prefix
fn kill_single_session(work_dir: &std::path::Path, session_id: &str) -> Result<()> {
    let session_file = match find_session_file(work_dir, session_id)? {
        Some(path) => path,
        None => bail!("Session '{session_id}' not found"),
    };

    // Extract the actual session ID from the found file
    let actual_session_id = session_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(session_id);

    println!("Killing session: {actual_session_id}");

    // Read session file and parse it
    let content = std::fs::read_to_string(&session_file)
        .with_context(|| format!("Failed to read session file: {}", session_file.display()))?;

    // Parse session from markdown YAML frontmatter
    let session: Session = parse_from_markdown(&content, "Session")
        .context("Failed to parse session from markdown")?;

    // Route via the session's persisted backend metadata. This is
    // correct even after restart: every session writes its `backend`
    // field at spawn time and on disk we never lose it.
    let backend_type = session.backend;
    println!("  Backend: {backend_type}");

    // Build a single-backend dispatcher and let `for_session` pick.
    // We use a single-backend dispatcher rather than reading the
    // project default because the session's own metadata is the ground
    // truth — even if the project has since been re-provisioned with a
    // different backend, the still-running session belongs to whatever
    // backend it was spawned under.
    let needs = BackendNeeds::from_project_and_overrides(backend_type, &[]);
    let dispatcher = BackendDispatcher::for_plan(backend_type, needs, work_dir)
        .with_context(|| format!("Failed to construct {backend_type} backend dispatcher"))?;
    let backend = dispatcher.for_session(&session);

    if backend.is_session_alive(&session)? {
        println!("  Killing session using {backend_type} backend...");
        backend.kill_session(&session)?;
        println!("  Session killed successfully");
    } else {
        println!("  Session already terminated");
    }

    // Remove the session file
    std::fs::remove_file(&session_file)
        .with_context(|| format!("Failed to remove session file: {}", session_file.display()))?;
    println!("  Session file removed");

    // Also remove the signal file if it exists
    let signal_file = work_dir
        .join("signals")
        .join(format!("{actual_session_id}.md"));
    if signal_file.exists() {
        std::fs::remove_file(&signal_file)
            .with_context(|| format!("Failed to remove signal file: {}", signal_file.display()))?;
        println!("  Signal file removed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::BackendType;

    #[test]
    fn test_session_backend_defaults_to_native() {
        let session = Session::new();
        assert_eq!(session.backend, BackendType::Native);
    }

    #[test]
    fn test_parse_session_from_markdown_valid() {
        let content = r#"---
id: session-1
stage_id: stage-1
pid: 12345
status: running
context_tokens: 0
context_limit: 200000
created_at: 2024-01-01T00:00:00Z
last_active: 2024-01-01T00:00:00Z
---

# Session: session-1
"#;

        let session: Session = parse_from_markdown(content, "Session").unwrap();
        assert_eq!(session.id, "session-1");
        assert_eq!(session.stage_id, Some("stage-1".to_string()));
        assert_eq!(session.pid, Some(12345));
    }

    #[test]
    fn test_parse_session_from_markdown_invalid() {
        let content = "Invalid content without frontmatter";

        let result: Result<Session> = parse_from_markdown(content, "Session");
        assert!(result.is_err());
    }
}
