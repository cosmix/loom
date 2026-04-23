//! Stop command - gracefully shuts down the daemon

use crate::daemon::DaemonServer;
use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use colored::Colorize;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::thread;
use std::time::Duration;

/// Execute the stop command to gracefully shut down the daemon
pub fn execute() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    if !DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is not running", "─".dimmed());
        return Ok(());
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
