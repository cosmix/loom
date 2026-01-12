//! Attach to running sessions
//!
//! Usage:
//!   loom attach <stage_id|session_id>  - Attach to a specific session
//!   loom attach orch                   - Attach to the orchestrator
//!   loom attach all                    - Attach to all sessions (tiled view)
//!   loom attach all --gui              - Open each session in a GUI terminal window
//!   loom attach list                   - List attachable sessions
//!   loom attach logs                   - Stream daemon logs in real-time

use anyhow::{bail, Context, Result};
use std::os::unix::net::UnixStream;

use crate::daemon::{read_message, write_message, DaemonServer, Request, Response};
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{
    attach_by_session, attach_by_stage, attach_native_all, format_attachable_list, list_attachable,
    spawn_gui_windows,
};

/// Attach terminal to running session
pub fn execute(target: String) -> Result<()> {
    // Handle orchestrator attach - redirect to logs
    if target == "orch" || target == "orchestrator" {
        println!("Daemon architecture: use 'loom attach logs' to stream daemon logs");
        println!("or 'loom status' for progress dashboard.\n");
        return execute_logs();
    }

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
        println!("\nSessions must be in 'running' or 'paused' state.");
        return Ok(());
    }

    print!("{}", format_attachable_list(&sessions));

    Ok(())
}

/// Attach to all running sessions
///
/// Default mode focuses terminal windows.
/// Use --gui to spawn separate terminal windows.
pub fn execute_all(
    gui_mode: bool,
    detach_existing: bool,
    _windows_mode: bool,
    _layout: String,
) -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!("No .work/ directory found. Run 'loom init' first.");
    }

    let sessions = list_attachable(&work_dir)?;

    if sessions.is_empty() {
        println!("No attachable sessions found.");
        println!("\nSessions must be in 'running' or 'paused' state.");
        return Ok(());
    }

    // All sessions are native backend - use window focusing
    let native_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| s.backend_type() == BackendType::Native)
        .cloned()
        .collect();

    // GUI mode: handle native sessions
    if gui_mode {
        return spawn_gui_windows(&sessions, detach_existing);
    }

    // Use native window focusing
    if !native_sessions.is_empty() {
        return attach_native_all(&native_sessions);
    }

    println!("No attachable sessions found.");
    Ok(())
}

/// Stream daemon logs in real-time
///
/// Connects to the daemon socket and subscribes to the log stream.
/// Logs are printed to stdout until the connection is closed or interrupted.
pub fn execute_logs() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    if !DaemonServer::is_running(work_dir.root()) {
        bail!("Daemon is not running. Start with 'loom run' first.");
    }

    let socket_path = work_dir.root().join("orchestrator.sock");
    let mut stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

    // Subscribe to logs
    write_message(&mut stream, &Request::SubscribeLogs)
        .context("Failed to send subscribe request")?;

    // Read response
    let response: Response =
        read_message(&mut stream).context("Failed to read subscribe response")?;
    match response {
        Response::Ok => {}
        Response::Error { message } => bail!("Subscription failed: {message}"),
        _ => bail!("Unexpected response from daemon"),
    }

    println!("Streaming daemon logs (Ctrl+C to stop)...\n");

    // Stream logs until disconnected
    loop {
        let msg_result: Result<Response> = read_message(&mut stream);
        match msg_result {
            Ok(Response::LogLine { line }) => {
                println!("{line}");
            }
            Ok(_) => continue,
            Err(_) => break, // Connection closed
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_execute_logs_daemon_not_running() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = WorkDir::new(temp_dir.path()).expect("Failed to create WorkDir");
        work_dir.initialize().expect("Failed to initialize WorkDir");

        // Change to the temp directory
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_dir.path()).expect("Failed to change dir");

        // Test should fail because daemon is not running
        let result = execute_logs();

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Should return error when daemon is not running
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not running"));
    }
}
