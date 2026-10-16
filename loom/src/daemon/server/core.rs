//! Core DaemonServer struct and constructors.

use super::super::protocol::DaemonConfig;
use super::super::SOCKET_FILE;
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
    /// outright — or because its sandbox masks the socket path with a
    /// non-socket (`daemon::rpc::refused_by_mask`). Distinct from
    /// `ProcessOnly`: there the connect failure (a socket nothing is bound
    /// to, or a genuinely missing socket file) is real evidence the
    /// daemon's listener is gone. Here the failure is a
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
/// recorded in `doc/loom/knowledge/mistakes/daemon-singleton.md`.
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
            socket_path: work_dir.join(SOCKET_FILE),
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
        let socket_path = crate::daemon::socket_path(work_dir);
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
                // A session whose stage is granted every Unix socket sees the
                // socket masked by `/dev/null`: refused, like a dead listener,
                // but by something that is not a socket.
                Err(e) if super::super::rpc::refused_by_mask(&e, &socket_path) => {
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
        SOCKET_FILE,
        "orchestrator.pid",
        "admin.token",
        "user.token",
        "orchestrator.complete",
    ] {
        let _ = remove_control_file(work_dir, Path::new(relative));
    }
}

#[cfg(test)]
mod tests {
    use super::super::lock::acquire_lock;
    use super::{cleanup_stale_control_files, DaemonServer, DaemonStatus};
    use crate::daemon::socket_path;
    use crate::process::sandbox_probe::{skip_unless, unix_socket_bindable};

    #[test]
    fn cleanup_preserves_completion_attestation_key() {
        let temp = tempfile::TempDir::new().unwrap();
        let key = temp.path().join(crate::handoff::ATTESTATION_KEY_FILE);
        std::fs::write(&key, "persistent-key").unwrap();
        std::fs::write(temp.path().join("admin.token"), "rotating-token").unwrap();

        cleanup_stale_control_files(temp.path());

        assert!(key.exists());
        assert!(!temp.path().join("admin.token").exists());
    }

    /// A non-socket at the socket path under a held lock is a sandbox's
    /// mask (`/dev/null` bound over the socket), never a hung daemon. A
    /// regular file and a link to `/dev/null` stand in for it. Linux only:
    /// macOS answers `ENOTSOCK` for a non-socket path.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_masked_socket_under_a_held_lock_is_unreachable() {
        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path();
        let _lock = acquire_lock(work_dir).unwrap();
        let socket = socket_path(work_dir);

        std::fs::write(&socket, b"").unwrap();
        assert_eq!(
            DaemonServer::check_status(work_dir),
            DaemonStatus::Unreachable
        );

        std::fs::remove_file(&socket).unwrap();
        std::os::unix::fs::symlink("/dev/null", &socket).unwrap();
        assert_eq!(
            DaemonServer::check_status(work_dir),
            DaemonStatus::Unreachable
        );
    }

    fn stale_socket(path: &std::path::Path) {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::FileTypeExt;

        // Bind without listening: a concurrent fork can inherit the fd, but
        // cannot keep a listener alive after this fixture drops its copy.
        // SAFETY: socket has no pointer arguments and returns an owned fd.
        let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        assert!(raw >= 0, "socket: {}", std::io::Error::last_os_error());
        // SAFETY: raw is a valid, uniquely owned socket descriptor.
        let socket = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: zero initializes every field, including the path terminator.
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        address.sun_family = libc::AF_UNIX as _;
        let bytes = path.as_os_str().as_bytes();
        assert!(bytes.len() < address.sun_path.len());
        for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
            *target = *byte as libc::c_char;
        }
        #[cfg(target_os = "macos")]
        {
            address.sun_len = std::mem::size_of_val(&address) as _;
        }
        // SAFETY: the live fd and initialized address remain valid for bind.
        let result = unsafe {
            libc::bind(
                socket.as_raw_fd(),
                (&raw const address).cast(),
                std::mem::size_of_val(&address) as _,
            )
        };
        assert_eq!(result, 0, "bind: {}", std::io::Error::last_os_error());
        drop(socket);
        assert!(std::fs::symlink_metadata(path)
            .unwrap()
            .file_type()
            .is_socket());
    }

    /// A held lock with a stale socket inode identifies a process-only daemon.
    #[test]
    fn a_stale_socket_under_a_held_lock_is_process_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path();
        if skip_unless(
            unix_socket_bindable(work_dir),
            "daemon::server::core::tests::a_stale_socket_under_a_held_lock_is_process_only",
            "this sandbox denies binding an AF_UNIX listener",
        ) {
            return;
        }
        let _lock = acquire_lock(work_dir).unwrap();
        stale_socket(&socket_path(work_dir));

        assert_eq!(
            DaemonServer::check_status(work_dir),
            DaemonStatus::ProcessOnly
        );
    }
}
