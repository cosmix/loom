//! Daemon server lifecycle methods: start, stop, run.

use super::super::protocol::{read_message, write_message, Request, Response};
use super::broadcast::{spawn_log_tailer, spawn_status_broadcaster};
use super::client::handle_client_connection;
use super::core::{DaemonServer, MAX_CONNECTIONS};
use super::orchestrator::spawn_orchestrator;
use anyhow::{bail, Context, Result};
use nix::unistd::{fork, setsid, ForkResult};
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

impl DaemonServer {
    /// Stop a running daemon by sending a stop request via socket.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `Ok(())` on success, error if daemon is not running or communication fails
    pub fn stop(work_dir: &Path) -> Result<()> {
        let socket_path = work_dir.join("orchestrator.sock");

        if !Self::is_running(work_dir) {
            bail!("Daemon is not running");
        }

        let mut stream =
            UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("Failed to set read timeout")?;

        write_message(&mut stream, &Request::Stop).context("Failed to send stop request")?;

        let response: Response = match read_message(&mut stream) {
            Ok(resp) => resp,
            Err(e) => {
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == std::io::ErrorKind::WouldBlock
                        || io_err.kind() == std::io::ErrorKind::TimedOut
                    {
                        bail!(
                            "Daemon did not respond within 5 seconds. \
                             It may be frozen. Try: kill $(cat {})",
                            work_dir.join("orchestrator.pid").display()
                        );
                    }
                }
                return Err(e).context("Failed to read stop response");
            }
        };

        match response {
            Response::Ok => Ok(()),
            Response::Error { message } => bail!("Daemon returned error: {message}"),
            _ => bail!("Unexpected response from daemon"),
        }
    }

    /// Start the daemon (daemonize process).
    ///
    /// # Returns
    /// `Ok(())` on success, error if daemon fails to start
    pub fn start(&self) -> Result<()> {
        // Remove stale socket if it exists (ignore NotFound to avoid TOCTOU race)
        if let Err(e) = fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove stale socket file");
            }
        }

        // First fork - parent exits, child continues
        match unsafe { fork() }.context("First fork failed")? {
            ForkResult::Parent { .. } => {
                // Parent process exits successfully
                std::process::exit(0);
            }
            ForkResult::Child => {
                // Child continues with daemonization
            }
        }

        // Create new session (detach from controlling terminal)
        setsid().context("setsid failed")?;

        // Second fork - prevents acquiring a controlling terminal
        match unsafe { fork() }.context("Second fork failed")? {
            ForkResult::Parent { .. } => {
                // Intermediate parent exits
                std::process::exit(0);
            }
            ForkResult::Child => {
                // Grandchild continues as daemon
            }
        }

        // Write PID file
        fs::write(&self.pid_path, format!("{}", std::process::id()))
            .context("Failed to write PID file")?;

        // Redirect stdout and stderr to log file
        let log_file = File::create(&self.log_path).context("Failed to create log file")?;
        let log_fd = log_file.as_raw_fd();

        // Close stdin and redirect stdout/stderr to log file
        unsafe {
            libc::close(0);
            if libc::dup2(log_fd, 1) < 0 {
                bail!("Failed to redirect stdout");
            }
            if libc::dup2(log_fd, 2) < 0 {
                bail!("Failed to redirect stderr");
            }
        }

        // Run the server
        self.run_server()
    }

    /// Run the daemon in foreground (for testing).
    ///
    /// # Returns
    /// `Ok(())` on success, error if server fails to start
    pub fn run_foreground(&self) -> Result<()> {
        // Write PID file manually
        fs::write(&self.pid_path, format!("{}", std::process::id()))
            .context("Failed to write PID file")?;

        // Remove stale socket if it exists (ignore NotFound to avoid TOCTOU race)
        if let Err(e) = fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove stale socket file");
            }
        }

        self.run_server()
    }

    /// Main server loop (listens on socket and accepts connections).
    pub(super) fn run_server(&self) -> Result<()> {
        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;

        // Set restrictive permissions (owner read/write only) to prevent unauthorized access
        fs::set_permissions(&self.socket_path, Permissions::from_mode(0o600))
            .context("Failed to set socket permissions")?;

        // Set socket to non-blocking mode for graceful shutdown
        listener
            .set_nonblocking(true)
            .context("Failed to set socket to non-blocking")?;

        // Spawn the orchestrator thread to actually run stages
        let orchestrator_handle = spawn_orchestrator(self);

        // Spawn log tailing thread
        let log_tail_handle = spawn_log_tailer(self);

        // Spawn status broadcasting thread
        let status_broadcast_handle = spawn_status_broadcaster(self);

        while !self.shutdown_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Check connection limit before accepting
                    let current = self.connection_count.load(Ordering::Relaxed);
                    if current >= MAX_CONNECTIONS {
                        eprintln!("Connection limit reached ({MAX_CONNECTIONS}), rejecting");
                        drop(stream); // Close the connection immediately
                        continue;
                    }

                    // Increment connection count
                    self.connection_count.fetch_add(1, Ordering::Relaxed);

                    let shutdown_flag = Arc::clone(&self.shutdown_flag);
                    let status_subscribers = Arc::clone(&self.status_subscribers);
                    let log_subscribers = Arc::clone(&self.log_subscribers);
                    let connection_count = Arc::clone(&self.connection_count);

                    thread::spawn(move || {
                        let result = handle_client_connection(
                            stream,
                            shutdown_flag,
                            status_subscribers,
                            log_subscribers,
                        );
                        // Decrement connection count when thread exits
                        connection_count.fetch_sub(1, Ordering::Relaxed);
                        if let Err(e) = result {
                            eprintln!("Client handler error: {e}");
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection available, sleep briefly
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("Accept error: {e}");
                    break;
                }
            }
        }

        // Wait for threads to finish
        if let Some(handle) = orchestrator_handle {
            let _ = handle.join();
        }
        if let Some(handle) = log_tail_handle {
            let _ = handle.join();
        }
        let _ = status_broadcast_handle.join();

        self.cleanup()?;
        Ok(())
    }

    /// Clean up socket, PID, and completion marker files.
    pub(super) fn cleanup(&self) -> Result<()> {
        // Remove files directly, ignoring NotFound to avoid TOCTOU race
        if let Err(e) = fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove socket file");
            }
        }
        if let Err(e) = fs::remove_file(&self.pid_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove PID file");
            }
        }
        // Clean up completion marker file
        let completion_marker = self.work_dir.join("orchestrator.complete");
        if let Err(e) = fs::remove_file(&completion_marker) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove completion marker file");
            }
        }
        Ok(())
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
