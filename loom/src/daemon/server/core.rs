//! Core DaemonServer struct and constructors.

use super::super::protocol::DaemonConfig;
use super::lock::{inspect_lock, read_persisted_identity, LockState};
use super::storage::remove_control_file;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) const CLIENT_WORKERS: usize = 8;
pub(super) const CLIENT_QUEUE_CAPACITY: usize = 16;
pub(super) const MAX_IN_FLIGHT_REQUEST_BYTES: usize = 512 * 1024;

/// Daemon status indicating process and socket state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStatus {
    /// Daemon process not found
    NotRunning,
    /// Daemon process running and socket responsive
    Running,
    /// Daemon process exists but socket unreachable or unresponsive (hung state)
    ProcessOnly,
    /// The singleton flock proves a daemon owns this `.loom/work/`, but this
    /// process could not `connect()` to its socket because AF_UNIX socket
    /// syscalls are denied to it — e.g. Claude Code's Bash tool sandbox,
    /// which permits `stat`/`ls` on the socket file but rejects `socket()`
    /// outright. Distinct from `ProcessOnly`: there the connect failure
    /// (connection refused, or a genuinely missing socket file) is real
    /// evidence the daemon's listener is gone. Here the failure is a
    /// property of the CALLER's sandbox, not the daemon, so it must never
    /// drive "restart the daemon" advice (`commands/repair.rs`) or a
    /// "socket missing" message (`commands/status.rs`).
    Unreachable,
}

/// Whether `status` should be treated as "a daemon owns this `.loom/work/`" for
/// the purposes of refusing a second `loom run` to start.
///
/// `Unreachable` counts as running: by construction (see `check_status`) it
/// is only ever produced from `LockState::Held`, i.e. the flock already
/// proved a live daemon owns the state before the connect attempt even ran —
/// the failed `connect()` is a property of the CALLER, not evidence the
/// daemon died. Treating it as "not running" would let a second daemon start
/// against the same `.loom/work/`, which is exactly the singleton failure
/// recorded in `doc/loom/knowledge/concerns/daemon-singleton.md`.
pub(super) fn daemon_running_from_status(status: DaemonStatus) -> bool {
    matches!(
        status,
        DaemonStatus::Running | DaemonStatus::ProcessOnly | DaemonStatus::Unreachable
    )
}

/// Daemon server that listens on a Unix domain socket.
pub struct DaemonServer {
    pub(super) socket_path: PathBuf,
    pub(super) log_path: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) config: DaemonConfig,
    pub(super) shutdown_flag: Arc<AtomicBool>,
    pub(super) status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    pub(super) log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    /// Set to `true` only after this process has acquired the singleton flock
    /// AND bound the socket. `Drop::cleanup()` is gated on this so a daemon that
    /// loses the singleton race (or fails before bind) never deletes the live
    /// daemon's socket/PID/admin.token/log. See A-1/O-7.
    pub(super) was_running: Arc<AtomicBool>,
}

impl DaemonServer {
    /// Create a new daemon server with default configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .loom/work/ directory path
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn new(work_dir: &Path) -> Self {
        Self::with_config(work_dir, DaemonConfig::default())
    }

    /// Create a new daemon server with custom configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .loom/work/ directory path
    /// * `config` - The daemon configuration
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn with_config(work_dir: &Path, config: DaemonConfig) -> Self {
        Self {
            socket_path: work_dir.join("orchestrator.sock"),
            log_path: work_dir.join("orchestrator.log"),
            work_dir: work_dir.to_path_buf(),
            config,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            status_subscribers: Arc::new(Mutex::new(Vec::new())),
            log_subscribers: Arc::new(Mutex::new(Vec::new())),
            was_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check daemon status using flock as ground truth, with socket connectivity test.
    ///
    /// The flock on `orchestrator.lock` is the authoritative indicator of whether
    /// a daemon process is alive. The socket/PID files are secondary — they can
    /// become stale if a second daemon overwrites them or if cleanup races occur.
    ///
    /// # Arguments
    /// * `work_dir` - The .loom/work/ directory path
    ///
    /// # Returns
    /// `DaemonStatus` indicating whether the daemon is running and responsive
    pub fn check_status(work_dir: &Path) -> DaemonStatus {
        let socket_path = work_dir.join("orchestrator.sock");
        match inspect_lock(work_dir) {
            LockState::Held(_) => match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                    DaemonStatus::Running
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    // Sandboxed callers (e.g. this process running inside
                    // Claude Code's Bash tool) have AF_UNIX socket syscalls
                    // denied outright, so `UnixStream::connect` fails either
                    // at socket() creation or at connect() itself — POSIX
                    // doesn't guarantee which, but both surface as
                    // `PermissionDenied` here and mean the identical thing:
                    // the daemon is fine, this process just cannot reach it.
                    // The flock already proved a live daemon owns this state
                    // (we are in the `LockState::Held` arm), so trust that
                    // over the failed connect.
                    //
                    // Deliberately NOT corroborating with
                    // `socket_path.exists()`: a sandbox that denies
                    // `connect()` may or may not also deny `stat`/`lstat` on
                    // the same path, so a failed `exists()` here would prove
                    // nothing (the socket can still be there) and a
                    // successful one adds no confidence that `connect` would
                    // ever succeed from this process. Classify by error kind
                    // alone and never let this downgrade back to
                    // `ProcessOnly`.
                    DaemonStatus::Unreachable
                }
                Err(_) => DaemonStatus::ProcessOnly,
            },
            LockState::Free(guard) => {
                if let Some(guard) = guard {
                    cleanup_stale_control_files(work_dir);
                    drop(guard);
                }
                DaemonStatus::NotRunning
            }
            LockState::Indeterminate => DaemonStatus::ProcessOnly,
        }
    }

    /// Check if a daemon is already running.
    ///
    /// # Arguments
    /// * `work_dir` - The .loom/work/ directory path
    ///
    /// # Returns
    /// `true` if a daemon is running (either responsive or hung), `false` otherwise
    pub fn is_running(work_dir: &Path) -> bool {
        daemon_running_from_status(Self::check_status(work_dir))
    }

    /// Read the PID from the PID file.
    ///
    /// # Arguments
    /// * `work_dir` - The .loom/work/ directory path
    ///
    /// # Returns
    /// `Some(pid)` if the file exists and contains a valid PID, `None` otherwise
    pub fn read_pid(work_dir: &Path) -> Option<u32> {
        Self::read_identity(work_dir).map(|identity| identity.pid)
    }

    /// Read the persisted PID and process start-time evidence.
    pub fn read_identity(work_dir: &Path) -> Option<crate::process::ProcessIdentity> {
        read_persisted_identity(work_dir)
    }

    /// Request graceful shutdown of the daemon.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }
}

fn cleanup_stale_control_files(work_dir: &Path) {
    for relative in [
        "orchestrator.sock",
        "orchestrator.pid",
        "admin.token",
        "user.token",
        "orchestrator.complete",
    ] {
        let _ = remove_control_file(work_dir, Path::new(relative));
    }
}
