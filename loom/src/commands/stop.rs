//! Stop command - gracefully shuts down the daemon

use crate::daemon::DaemonServer;
use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::orchestrator::terminal::dispatcher::{BackendDispatcher, BackendNeeds};
use crate::orchestrator::terminal::BackendType;
use crate::parser::frontmatter::parse_from_markdown;
use anyhow::{Context, Result};
use colored::Colorize;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// Execute the stop command to gracefully shut down the daemon.
///
/// Before signalling the daemon, terminate every active container
/// session: those processes live in their own namespace and outlive a
/// SIGTERM to the daemon, so without explicit teardown we'd leak
/// containers and networks. Native sessions are tracked by the daemon
/// itself and exit when their parent terminal closes.
pub fn execute() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    if !DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is not running", "─".dimmed());
        return Ok(());
    }

    // Kill any active container sessions first.
    if let Err(e) = terminate_active_container_sessions(work_dir.root()) {
        eprintln!(
            "{} Failed to terminate container sessions: {e}",
            "!".yellow().bold()
        );
    }

    println!("{} Stopping daemon...", "→".cyan().bold());

    // Try graceful shutdown via socket first
    match DaemonServer::stop(work_dir.root()) {
        Ok(()) => {
            println!("{} Daemon stopped", "✓".green().bold());
            Ok(())
        }
        Err(e) => {
            // Find the daemon PID from PID file or lock file
            let pid = DaemonServer::read_pid(work_dir.root())
                .or_else(|| DaemonServer::check_lock(work_dir.root()));

            if let Some(pid) = pid {
                kill_daemon_pid(pid, work_dir.root())
            } else {
                Err(e).context("Daemon not responding (process not found)")
            }
        }
    }
}

/// Terminate every persisted container session — container sessions
/// run in their own runtime namespace and won't exit just because the
/// daemon process does. Native sessions are tied to their terminal and
/// can be left alone here.
fn terminate_active_container_sessions(work_dir: &Path) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(());
    }

    let mut sessions: Vec<Session> = Vec::new();
    let entries = std::fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read {}", sessions_dir.display()))?;
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if let Ok(session) = parse_from_markdown::<Session>(&content, "Session") {
            if session.backend == BackendType::Container {
                sessions.push(session);
            }
        }
    }

    if sessions.is_empty() {
        return Ok(());
    }

    let needs = BackendNeeds {
        native: false,
        container: true,
    };
    let dispatcher = BackendDispatcher::for_plan(BackendType::Container, needs, work_dir)
        .context("Failed to construct container dispatcher for stop")?;

    for session in &sessions {
        println!(
            "{} Terminating container session: {}",
            "→".cyan().bold(),
            session.id.dimmed()
        );
        let kill_result = dispatcher.for_session(session).kill_session(session);
        if let Err(e) = &kill_result {
            eprintln!(
                "{} Failed to kill session {}: {e}",
                "!".yellow().bold(),
                session.id
            );
        }
        if kill_result.is_ok() {
            let mut updated_session = session.clone();
            updated_session.clear_container_identity();
            if let Err(e) =
                crate::orchestrator::continuation::save_session(&updated_session, work_dir)
            {
                eprintln!(
                    "{} Failed to clear container identity for session {}: {e}",
                    "!".yellow().bold(),
                    session.id
                );
            }
        }
    }
    Ok(())
}

fn kill_daemon_pid(pid: u32, work_root: &std::path::Path) -> Result<()> {
    println!(
        "{} Daemon not responding, sending SIGTERM to PID {pid}...",
        "!".yellow().bold()
    );

    if let Err(kill_err) = kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        eprintln!("{} Failed to send SIGTERM: {}", "✗".red().bold(), kill_err);
        anyhow::bail!("Failed to stop daemon (PID {pid}): {kill_err}");
    }

    let mut attempts = 0;
    let max_attempts = 30; // 3 seconds total
    while attempts < max_attempts {
        thread::sleep(Duration::from_millis(100));
        if !crate::process::is_process_alive(pid) {
            break;
        }
        attempts += 1;
    }

    if crate::process::is_process_alive(pid) {
        println!(
            "{} Process {pid} did not exit after SIGTERM, sending SIGKILL...",
            "!".yellow().bold()
        );
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        thread::sleep(Duration::from_millis(200));
    }

    // Clean up stale files
    let _ = std::fs::remove_file(work_root.join("orchestrator.sock"));
    let _ = std::fs::remove_file(work_root.join("orchestrator.pid"));

    println!("{} Daemon terminated", "✓".green().bold());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_stop_when_daemon_not_running() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path();

        // Create a .work directory structure
        let work_dir_path = test_dir.join(".work");
        fs::create_dir(&work_dir_path).expect("Failed to create .work dir");

        // Change to test directory
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(test_dir).expect("Failed to change dir");

        // Execute stop command when daemon is not running
        let result = execute();

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Should succeed even when daemon is not running
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_stop_succeeds_when_work_dir_missing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path();

        // Change to test directory (no .work directory)
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(test_dir).expect("Failed to change dir");

        // Execute stop command when .work dir doesn't exist
        let result = execute();

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Should succeed - daemon simply reports "not running"
        // WorkDir::new succeeds even without .work, and is_running returns false
        assert!(result.is_ok());
    }
}
