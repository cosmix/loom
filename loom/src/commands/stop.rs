//! Stop command - gracefully shuts down the daemon
//!
//! Stop is a **privileged** operation that requires the admin token. The
//! ordering of side effects has been hardened:
//!
//!   1. Read `admin.token` FIRST. If unreadable, abort with a clear error.
//!   2. Send `Request::Stop` to the daemon and wait for ack.
//!
//! PID fallback (SIGTERM/SIGKILL to the daemon process) is reserved for the
//! socket-hang case and requires `--force` to opt in.

use crate::daemon::{read_admin_token, DaemonServer};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::thread;
use std::time::Duration;

/// Execute the stop command to gracefully shut down the daemon.
///
/// Order of operations:
///   1. Verify daemon is running.
///   2. Read `admin.token`. If unreadable → abort.
///   3. Send `Stop` (via [`DaemonServer::stop`]). If auth fails → abort.
///
/// PID fallback only triggers when `force` is true AND the socket is
/// unreachable; never on `AuthenticationFailed`.
pub fn execute() -> Result<()> {
    execute_with_force(false)
}

/// Variant that exposes the `--force` flag. Without `--force`, a hung daemon
/// causes us to refuse PID kill rather than risk killing the wrong process.
pub fn execute_with_force(force: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    if !DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is not running", "─".dimmed());
        return Ok(());
    }

    // Step 1: Read admin token FIRST. Stop requires Capability::Admin and we
    // refuse to perform any side effects without it.
    if read_admin_token(work_dir.root()).is_none() {
        bail!(
            "{} Cannot stop daemon: admin.token unreadable or missing.\n  \
             admin.token is created by the daemon at startup with mode 0o600 \
             and only the host user that started the daemon can read it.",
            "✗".red().bold()
        );
    }

    println!("{} Stopping daemon...", "→".cyan().bold());

    // Step 2: Send the Stop request. DaemonServer::stop reads admin.token
    // again internally and sends it on the wire.
    match DaemonServer::stop(work_dir.root()) {
        Ok(()) => {
            println!("{} Daemon stopped", "✓".green().bold());
            Ok(())
        }
        Err(e) => {
            // Distinguish auth failure from socket-hang.
            let msg = format!("{e:#}");
            if msg.contains("Authentication failed") {
                bail!(
                    "{} {} — refusing to fall back to PID kill (auth was rejected, not stuck)",
                    "✗".red().bold(),
                    msg
                );
            }

            if !force {
                bail!(
                    "{} Daemon did not respond cleanly: {e}\n  \
                     Re-run with --force to send SIGTERM/SIGKILL to the daemon PID.\n  \
                     (Only use --force if you have already verified the daemon is hung.)",
                    "✗".red().bold()
                );
            }

            // --force path: only used when the socket is unreachable AND the
            // operator has explicitly opted in.
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
