//! Authenticate an existing Seatbelt boundary before reusing it for verification.
//! Environment variables select a claim; kernel identity and protected state
//! authorize it. Linux always installs its own bubblewrap boundary.

use std::path::Path;

#[cfg(target_os = "macos")]
mod client;
#[cfg(target_os = "macos")]
mod kernel;
#[cfg(target_os = "macos")]
mod policy;
#[cfg(target_os = "macos")]
mod transport;

pub(crate) fn available(cwd: &Path) -> bool {
    #[cfg(target_os = "macos")]
    return client::available(cwd).unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cwd;
        false
    }
}

pub(crate) fn authorize(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    cwd: &Path,
    peer_pid: Option<u32>,
) -> super::Response {
    #[cfg(target_os = "macos")]
    let result = policy::authorize(work_dir, stage_id, session_id, cwd, peer_pid);
    #[cfg(not(target_os = "macos"))]
    let result: anyhow::Result<()> = {
        let _ = (work_dir, stage_id, session_id, cwd, peer_pid);
        Err(anyhow::anyhow!(
            "inherited confinement is only supported on macOS"
        ))
    };
    match result {
        Ok(()) => super::Response::Ok,
        Err(error) => super::Response::Error {
            message: format!("inherited confinement refused: {error:#}"),
        },
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
