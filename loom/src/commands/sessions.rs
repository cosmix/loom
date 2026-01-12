//! Session management commands
//! Usage: loom sessions [list|kill <id>...]

use anyhow::{bail, Context, Result};

use crate::fs::session_files::find_session_file;
use crate::fs::worktree_files::find_sessions_for_stage;
use crate::models::session::Session;
use crate::orchestrator::terminal::{create_backend, BackendType};

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

/// Kill one or more sessions by ID/prefix, or all sessions for a stage
pub fn kill(session_ids: Vec<String>, stage: Option<String>) -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

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
    let session =
        parse_session_from_markdown(&content).context("Failed to parse session from markdown")?;

    // Detect backend type from session metadata
    let backend_type = detect_backend_type(&session);

    // Kill the session using the appropriate backend
    if let Some(backend_type) = backend_type {
        println!("  Detected backend: {backend_type}");
        let backend = create_backend(backend_type, work_dir)
            .with_context(|| format!("Failed to create {backend_type} backend"))?;

        // Check if session is alive
        if backend.is_session_alive(&session)? {
            println!("  Killing session using {backend_type} backend...");
            backend.kill_session(&session)?;
            println!("  Session killed successfully");
        } else {
            println!("  Session already terminated");
        }
    } else {
        println!("  No backend information found (session may not have been spawned)");
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

/// Parse session from markdown with YAML frontmatter
fn parse_session_from_markdown(content: &str) -> Result<Session> {
    let yaml_content = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| anyhow::anyhow!("Invalid session file format: missing frontmatter"))?;

    serde_yaml::from_str(yaml_content).context("Failed to parse session YAML")
}

/// Detect backend type from session metadata
///
/// Returns Native if pid is set, otherwise None.
fn detect_backend_type(session: &Session) -> Option<BackendType> {
    if session.pid.is_some() {
        Some(BackendType::Native)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backend_type_native() {
        let mut session = Session::new();
        session.pid = Some(12345);

        assert_eq!(detect_backend_type(&session), Some(BackendType::Native));
    }

    #[test]
    fn test_detect_backend_type_none() {
        let session = Session::new();

        assert_eq!(detect_backend_type(&session), None);
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

        let session = parse_session_from_markdown(content).unwrap();
        assert_eq!(session.id, "session-1");
        assert_eq!(session.stage_id, Some("stage-1".to_string()));
        assert_eq!(session.pid, Some(12345));
    }

    #[test]
    fn test_parse_session_from_markdown_invalid() {
        let content = "Invalid content without frontmatter";

        let result = parse_session_from_markdown(content);
        assert!(result.is_err());
    }
}
