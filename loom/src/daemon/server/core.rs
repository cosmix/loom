//! Core DaemonServer struct and constructors.

use super::super::protocol::DaemonConfig;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Daemon server that listens on a Unix domain socket.
pub struct DaemonServer {
    pub(super) socket_path: PathBuf,
    pub(super) pid_path: PathBuf,
    pub(super) log_path: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) config: DaemonConfig,
    pub(super) shutdown_flag: Arc<AtomicBool>,
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
            status_subscribers: Arc::new(Mutex::new(Vec::new())),
            log_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if a daemon is already running.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `true` if a daemon is running, `false` otherwise
    pub fn is_running(work_dir: &Path) -> bool {
        let pid_path = work_dir.join("orchestrator.pid");
        let socket_path = work_dir.join("orchestrator.sock");

        // Both PID file and socket must exist for daemon to be considered running
        if !socket_path.exists() {
            // No socket means daemon is not ready to accept connections
            // Clean up stale PID file if it exists
            if pid_path.exists() {
                if let Some(pid) = Self::read_pid(work_dir) {
                    let pid_exists = unsafe { libc::kill(pid as i32, 0) == 0 };
                    if !pid_exists {
                        let _ = std::fs::remove_file(&pid_path);
                    }
                }
            }
            return false;
        }

        if let Some(pid) = Self::read_pid(work_dir) {
            // Check if process exists by sending signal 0
            let pid_exists = unsafe { libc::kill(pid as i32, 0) == 0 };
            if !pid_exists {
                // PID file exists but process is dead, clean up stale files
                let _ = std::fs::remove_file(&pid_path);
                let _ = std::fs::remove_file(&socket_path);
                return false;
            }
            true
        } else {
            // Socket exists but no PID file - clean up stale socket
            let _ = std::fs::remove_file(&socket_path);
            false
        }
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
