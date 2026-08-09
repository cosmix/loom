//! Stop command - gracefully shuts down the daemon
//!
//! Stop is a **privileged** operation that requires an action-bound one-time
//! operator proof. The target command never reads the admin token itself.
//!
//!   1. Take the proof from `LOOM_ADMIN_PROOF` and remove it from the environment.
//!   2. Send `Request::Stop` to the daemon and wait for acknowledgement.
//!
//! Verified SIGTERM fallback is reserved for an unreachable daemon socket and
//! requires `--force`; raw-PID SIGKILL is never attempted.

use crate::commands::stage::admin_proof::{
    take_admin_proof_from_env, verify_and_consume_admin_proof, AdminProofRequest,
};
use crate::daemon::{DaemonServer, DaemonStatus, DaemonUnavailable};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Result};
use colored::Colorize;
use std::thread;
use std::time::Duration;

/// Execute the stop command to gracefully shut down the daemon.
///
/// Order of operations:
///   1. Verify daemon is running.
///   2. Consume the external action-bound proof.
///   3. Send `Stop` (via [`DaemonServer::stop`]). If proof verification fails → abort.
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

    if DaemonServer::check_status(work_dir.root()) == DaemonStatus::NotRunning {
        println!("{} Daemon is not running", "─".dimmed());
        return Ok(());
    }

    let operator_proof = take_admin_proof_from_env().map_err(|_| {
        anyhow::anyhow!(
            "daemon stop requires an action-bound one-time operator proof in LOOM_ADMIN_PROOF; mint one with `LOOM_ADMIN_TOKEN=<daemon-admin-token> loom stage admin-proof --daemon-stop`, then run `LOOM_ADMIN_PROOF=<printed-proof> loom stop`"
        )
    })?;

    println!("{} Stopping daemon...", "→".cyan().bold());

    match DaemonServer::stop(work_dir.root(), &operator_proof) {
        Ok(()) => {
            println!("{} Daemon stopped", "✓".green().bold());
            Ok(())
        }
        Err(e) => {
            if e.downcast_ref::<DaemonUnavailable>().is_none() {
                bail!(
                    "{} Daemon refused or did not acknowledge stop: {e}",
                    "✗".red().bold()
                );
            }
            if !force {
                bail!(
                    "{} Daemon did not respond cleanly: {e}\n  \
                     Re-run with --force and a fresh one-time operator proof to request \
                     verified SIGTERM of the recorded daemon identity.",
                    "✗".red().bold()
                );
            }
            verify_and_consume_admin_proof(
                work_dir.root(),
                AdminProofRequest::daemon_stop(),
                Some(&operator_proof),
            )?;
            terminate_daemon_identity(work_dir.root())
        }
    }
}

fn terminate_daemon_identity(work_root: &std::path::Path) -> Result<()> {
    let identity = DaemonServer::read_identity(work_root)
        .ok_or_else(|| anyhow::anyhow!("daemon identity file is missing or invalid"))?;
    let locked_identity = DaemonServer::held_identity(work_root)?
        .ok_or_else(|| anyhow::anyhow!("daemon singleton lock is free; refusing to signal"))?;
    if identity != locked_identity {
        bail!("daemon lock and PID identity evidence do not match; refusing to signal");
    }
    println!(
        "{} Daemon not responding, sending verified SIGTERM to PID {}...",
        "!".yellow().bold(),
        identity.pid
    );
    crate::process::terminate_verified(identity)?;

    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        match crate::process::verify_process_identity(identity) {
            crate::process::IdentityStatus::Dead => {
                let _ = DaemonServer::check_status(work_root);
                println!("{} Daemon terminated", "✓".green().bold());
                return Ok(());
            }
            crate::process::IdentityStatus::VerifiedAlive => {}
            crate::process::IdentityStatus::Unverifiable => {
                bail!("daemon identity became unverifiable after SIGTERM")
            }
        }
    }
    bail!(
        "verified daemon PID {} did not exit after SIGTERM; refusing an unverified SIGKILL",
        identity.pid
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::os::fd::AsRawFd;
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

    #[test]
    #[serial]
    fn readable_admin_token_cannot_self_authorize_stop() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir(&work_dir).unwrap();
        fs::write(work_dir.join("admin.token"), "readable-secret").unwrap();
        fs::write(
            work_dir.join("orchestrator.lock"),
            format!("{} -\n", std::process::id()),
        )
        .unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(work_dir.join("orchestrator.lock"))
            .unwrap();
        assert_eq!(
            // SAFETY: `lock` owns a live descriptor and the flags form a valid
            // non-blocking exclusive flock used only by this serial test.
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let original_dir = std::env::current_dir().unwrap();
        let original_proof = std::env::var_os("LOOM_ADMIN_PROOF");
        std::env::remove_var("LOOM_ADMIN_PROOF");
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let error = execute().unwrap_err();

        std::env::set_current_dir(original_dir).unwrap();
        if let Some(proof) = original_proof {
            std::env::set_var("LOOM_ADMIN_PROOF", proof);
        }
        assert!(error.to_string().contains("one-time operator proof"));
    }
}
