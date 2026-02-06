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

    /// Check daemon status with socket connectivity test.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `DaemonStatus` indicating whether the daemon is running and responsive
    pub fn check_status(work_dir: &Path) -> DaemonStatus {
        let pid_path = work_dir.join("orchestrator.pid");
        let socket_path = work_dir.join("orchestrator.sock");

        // Check if socket file exists
        if !socket_path.exists() {
            // No socket means daemon is not ready to accept connections
            // Clean up stale PID file if it exists
            if pid_path.exists() {
                if let Some(pid) = Self::read_pid(work_dir) {
                    let pid_exists = crate::process::is_process_alive(pid);
                    if !pid_exists {
                        let _ = std::fs::remove_file(&pid_path);
                    }
                }
            }
            return DaemonStatus::NotRunning;
        }

        // Check if PID file exists and process is alive
        if let Some(pid) = Self::read_pid(work_dir) {
            let pid_exists = crate::process::is_process_alive(pid);

            if !pid_exists {
                // PID file exists but process is dead, clean up stale files
                let _ = std::fs::remove_file(&pid_path);
                let _ = std::fs::remove_file(&socket_path);
                return DaemonStatus::NotRunning;
            }

            // Process is alive, now test socket connectivity
            match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    // Set a short timeout for the connectivity test
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                    // Connection succeeded - daemon is responsive
                    DaemonStatus::Running
                }
                Err(_) => {
                    // Process alive but socket unreachable/unresponsive
                    DaemonStatus::ProcessOnly
                }
            }
        } else {
            // Socket exists but no PID file - clean up stale socket
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
        self.shutdown_flag.store(true, Ordering::Relaxed);
    }
}
