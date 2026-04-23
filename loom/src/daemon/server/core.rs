//! Core DaemonServer struct and constructors.

use super::super::protocol::DaemonConfig;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Maximum number of concurrent client connections allowed.
pub(super) const MAX_CONNECTIONS: usize = 100;

/// Daemon status indicating process and socket state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStatus {
    /// Daemon process not found
    NotRunning,
    /// Daemon process running and socket responsive
    Running,
    /// Daemon process exists but socket unreachable or unresponsive (hung state)
    ProcessOnly,
}

/// Daemon server that listens on a Unix domain socket.
pub struct DaemonServer {
    pub(super) socket_path: PathBuf,
    pub(super) pid_path: PathBuf,
    pub(super) log_path: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) config: DaemonConfig,
    pub(super) shutdown_flag: Arc<AtomicBool>,
    pub(super) connection_count: Arc<AtomicUsize>,
    pub(super) status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    pub(super) log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
}

impl DaemonServer {
    /// Create a new daemon server with default configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn new(work_dir: &Path) -> Self {
        Self::with_config(work_dir, DaemonConfig::default())
    }

    /// Create a new daemon server with custom configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    /// * `config` - The daemon configuration
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn with_config(work_dir: &Path, config: DaemonConfig) -> Self {
        Self {
            socket_path: work_dir.join("orchestrator.sock"),
            pid_path: work_dir.join("orchestrator.pid"),
            log_path: work_dir.join("orchestrator.log"),
            work_dir: work_dir.to_path_buf(),
            config,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            connection_count: Arc::new(AtomicUsize::new(0)),
            status_subscribers: Arc::new(Mutex::new(Vec::new())),
            log_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check daemon status using flock as ground truth, with socket connectivity test.
    ///
    /// The flock on `orchestrator.lock` is the authoritative indicator of whether
    /// a daemon process is alive. The socket/PID files are secondary — they can
    /// become stale if a second daemon overwrites them or if cleanup races occur.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `DaemonStatus` indicating whether the daemon is running and responsive
    pub fn check_status(work_dir: &Path) -> DaemonStatus {
        let pid_path = work_dir.join("orchestrator.pid");
        let socket_path = work_dir.join("orchestrator.sock");

        // Primary check: flock on orchestrator.lock
        // This is immune to PID-file races and socket-file deletion
        if let Some(lock_pid) = Self::check_lock(work_dir) {
            if crate::process::is_process_alive(lock_pid) {
                // A daemon holds the lock. Check socket connectivity.
                if socket_path.exists() {
                    match UnixStream::connect(&socket_path) {
                        Ok(stream) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                            return DaemonStatus::Running;
                        }
                        Err(_) => return DaemonStatus::ProcessOnly,
                    }
                }
                return DaemonStatus::ProcessOnly;
            }
        }

        // Fallback: check PID file + socket (for daemons started before the flock fix)
        if !socket_path.exists() {
            if pid_path.exists() {
                if let Some(pid) = Self::read_pid(work_dir) {
                    if crate::process::is_process_alive(pid) {
                        return DaemonStatus::ProcessOnly;
                    }
                    let _ = std::fs::remove_file(&pid_path);
                }
            }
            return DaemonStatus::NotRunning;
        }

        if let Some(pid) = Self::read_pid(work_dir) {
            if !crate::process::is_process_alive(pid) {
                let _ = std::fs::remove_file(&pid_path);
                let _ = std::fs::remove_file(&socket_path);
                return DaemonStatus::NotRunning;
            }

            match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                    DaemonStatus::Running
                }
                Err(_) => DaemonStatus::ProcessOnly,
            }
        } else {
            let _ = std::fs::remove_file(&socket_path);
            DaemonStatus::NotRunning
        }
    }

    /// Check if a daemon is already running.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `true` if a daemon is running (either responsive or hung), `false` otherwise
    pub fn is_running(work_dir: &Path) -> bool {
        matches!(
            Self::check_status(work_dir),
            DaemonStatus::Running | DaemonStatus::ProcessOnly
        )
    }

    /// Read the PID from the PID file.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `Some(pid)` if the file exists and contains a valid PID, `None` otherwise
    pub fn read_pid(work_dir: &Path) -> Option<u32> {
        let pid_path = work_dir.join("orchestrator.pid");
        std::fs::read_to_string(pid_path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    /// Request graceful shutdown of the daemon.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }
}
