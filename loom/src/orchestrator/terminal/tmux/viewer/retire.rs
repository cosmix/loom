//! Verified retirement of the overview viewer during explicit state cleanup.

use anyhow::{bail, Result};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::{fs, io};

use super::start_lock::StartLock;
use super::{socket_path_for, viewer_socket_name, OVERVIEW_SESSION};

/// Retire this repository's overview after explicit state cleanup has stopped
/// its daemon. A detached viewer remains available until that cleanup.
/// The deterministic socket name alone is insufficient authority to kill a
/// server in the per-user socket directory: verify the socket owner, type,
/// and sole tmux session before teardown. An owned socket nothing listens on is
/// what a dead server leaves, or what a starting one shows before it listens, so
/// it is removed only under tmux's start lock ([`remove_dead_socket`]).
pub(crate) fn retire_viewer(work_dir: &Path) -> Result<bool> {
    let name = viewer_socket_name(work_dir);
    let path = socket_path_for(&name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    // SAFETY: getuid() is always safe to call and cannot fail.
    let uid = unsafe { libc::getuid() };
    if !metadata.file_type().is_socket() || metadata.uid() != uid {
        bail!("viewer socket is not an owned socket: {}", path.display());
    }
    if has_no_listener(&path) {
        if let Some(removed) = remove_dead_socket(&path, &metadata)? {
            return Ok(removed);
        }
    }
    let path_arg = path.to_string_lossy();
    let output = super::super::run_tmux_control(
        &["-S", &path_arg, "list-sessions", "-F", "#{session_name}"],
        super::super::TMUX_PROBE_TIMEOUT,
        format!("tmux list-sessions ({})", path.display()),
    )?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != OVERVIEW_SESSION
    {
        bail!("viewer ownership could not be verified: {}", path.display());
    }
    ensure_same_socket(&path, &metadata)?;
    super::super::teardown_socket(&name)?;
    Ok(true)
}

/// Whether connecting to `path` is refused, the test tmux's own client applies
/// before it replaces a stale socket with a new server. Any other outcome, a
/// connection included, leaves the socket to the session check.
pub(super) fn has_no_listener(path: &Path) -> bool {
    UnixStream::connect(path).is_err_and(|error| error.kind() == io::ErrorKind::ConnectionRefused)
}

/// Remove a socket that refuses connections, probing again under tmux's start lock
/// so that a server between its bind and its listen cannot be mistaken for a dead
/// one. An error when a client holds the lock; `None` when the socket turned out to
/// have a listener, which leaves it to the session check. Whether it was removed
/// otherwise.
///
/// tmux's SIGUSR1 handler rebinds a live server's socket without this lock; a
/// retirement in that instant can still unlink it, and the signal recreates it.
fn remove_dead_socket(path: &Path, verified: &fs::Metadata) -> Result<Option<bool>> {
    let Some(_lock) = StartLock::try_take(path)? else {
        bail!(
            "a tmux server is starting on the viewer socket: {}",
            path.display()
        );
    };
    if !has_no_listener(path) {
        return Ok(None);
    }
    ensure_same_socket(path, verified)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(Some(true)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Some(false)),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn ensure_same_socket(path: &Path, verified: &fs::Metadata) -> Result<()> {
    let current = fs::symlink_metadata(path)?;
    if current.dev() != verified.dev() || current.ino() != verified.ino() {
        bail!(
            "viewer socket changed during ownership check: {}",
            path.display()
        );
    }
    Ok(())
}
