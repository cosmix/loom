//! Waiting for a spawned wrapper script's process to become discoverable.

use anyhow::Result;
use std::path::Path;
use std::thread;
use std::time::Duration;

use super::pid_tracking;

const CLAUDE_STARTUP_DELAY_MS: u64 = 1000;
const PID_DISCOVERY_TIMEOUT_SECS: u64 = 5;
const PID_FILE_RETRY_DELAY_MS: u64 = 200;
const PID_FILE_MAX_RETRIES: u32 = 10; // Total 2s of retries

fn finalize_pid_identity(work_dir: &Path, pid_key: &str, pid: u32) -> Result<u32> {
    let Some(start_time) = crate::process::process_start_time(pid) else {
        // Unsupported platforms retain PID-only evidence. Informational
        // liveness remains conservative, while destructive PID signaling
        // continues to fail closed.
        return Ok(pid);
    };
    let identity = crate::process::ProcessIdentity {
        pid,
        start_time: Some(start_time),
    };
    anyhow::ensure!(
        crate::process::verify_process_identity(identity)
            == crate::process::IdentityStatus::VerifiedAlive,
        "process {pid} exited or was replaced while recording its identity"
    );
    pid_tracking::write_pid_entry(work_dir, pid_key, identity)?;
    anyhow::ensure!(
        crate::process::verify_process_identity(identity)
            == crate::process::IdentityStatus::VerifiedAlive,
        "process {pid} exited or was replaced after recording its identity"
    );
    Ok(pid)
}

/// Wait for the process a wrapper script `exec`s into to become discoverable,
/// and return its PID.
///
/// The wait is layered: PID-file poll, then `/proc`-constrained discovery of
/// the process carrying this session's `LOOM_SESSION_ID` marker.
///
/// # Arguments
/// * `work_dir` - The `.loom/work` directory for PID-file tracking
/// * `pid_key` - The per-session PID-file key (tracking_key + session.id)
/// * `workdir` - The spawned process's working directory (used to constrain
///   `/proc` discovery to this session)
/// * `session_id` - The `LOOM_SESSION_ID` marker the wrapper script exports
///
/// # Returns
/// The first live PID found by the layers above, else `Err` naming `pid_key`.
pub(crate) fn await_session_pid(
    work_dir: &Path,
    pid_key: &str,
    workdir: &Path,
    session_id: &str,
) -> Result<u32> {
    // Wait initial delay for the process to start.
    thread::sleep(Duration::from_millis(CLAUDE_STARTUP_DELAY_MS));

    // Try to read the PID from the PID file with retries.
    // The wrapper script may take time to execute and write the PID.
    for retry in 0..PID_FILE_MAX_RETRIES {
        if let Some(identity) = pid_tracking::read_pid_entry(work_dir, pid_key) {
            match crate::process::verify_process_identity(identity) {
                crate::process::IdentityStatus::VerifiedAlive
                | crate::process::IdentityStatus::Unverifiable => {
                    return finalize_pid_identity(work_dir, pid_key, identity.pid);
                }
                crate::process::IdentityStatus::Dead => {}
            }
        }

        if retry < PID_FILE_MAX_RETRIES - 1 {
            thread::sleep(Duration::from_millis(PID_FILE_RETRY_DELAY_MS));
        }
    }

    // Fallback: discover the process constrained by this session's
    // LOOM_SESSION_ID marker, so we never latch onto a user's interactive
    // claude that shares the spawn directory (e.g. the repo root).
    if let Some(claude_pid) = pid_tracking::discover_claude_pid(
        workdir,
        session_id,
        Duration::from_secs(PID_DISCOVERY_TIMEOUT_SECS),
    ) {
        return finalize_pid_identity(work_dir, pid_key, claude_pid);
    }

    Err(anyhow::anyhow!(
        "No PID discovered for session (pid_key={pid_key})"
    ))
}
