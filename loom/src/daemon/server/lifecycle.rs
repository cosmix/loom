//! Daemon server lifecycle methods: start, stop, run.

use super::super::protocol::{read_message, write_message, Request, Response};
use super::broadcast::{spawn_log_tailer, spawn_status_broadcaster};
use super::client::{admin_token_path, handle_client_connection, USER_TOKEN_FILE};
use super::core::{DaemonServer, MAX_CONNECTIONS};
use super::orchestrator::spawn_orchestrator;
use anyhow::{bail, Context, Result};
use nix::unistd::{close, fork, pipe, setsid, ForkResult};
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Generate a 64-character hex token from 32 cryptographically-strong bytes.
///
/// Uses `OsRng` (getrandom on Linux, SecRandomCopyBytes on macOS) instead of
/// `Uuid::new_v4` so token entropy is the full 256 bits the format implies.
fn generate_token_hex() -> Result<String> {
    let mut bytes = [0u8; 32];
    let mut f = fs::File::open("/dev/urandom").context("Failed to open /dev/urandom")?;
    use std::io::Read;
    f.read_exact(&mut bytes)
        .context("Failed to read 32 random bytes")?;
    let mut s = String::with_capacity(64);
    for b in &bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

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

        // Stop is a privileged operation — only the admin token (mode 0o600,
        // host-only) authenticates this request. A container-resident agent
        // that has only the user token cannot stop the daemon. The admin
        // token lives at `$XDG_RUNTIME_DIR/loom/admin.token` (host-only —
        // never mounted into containers because containers only mount .work).
        let token_path = admin_token_path();
        let auth_token = fs::read_to_string(&token_path)
            .with_context(|| {
                format!(
                    "Failed to read admin token at {} (Stop requires admin capability)",
                    token_path.display()
                )
            })?
            .trim()
            .to_string();

        let mut stream =
            UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("Failed to set read timeout")?;

        write_message(&mut stream, &Request::Stop { auth_token })
            .context("Failed to send stop request")?;

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
            Response::AuthenticationFailed => bail!("Authentication failed - invalid token"),
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

        // Create pipe for error propagation from grandchild to original parent
        let (read_fd, write_fd) = pipe().context("Failed to create pipe")?;

        // First fork - parent exits, child continues
        match unsafe { fork() }.context("First fork failed")? {
            ForkResult::Parent { .. } => {
                // Close write end in parent
                drop(write_fd);

                // Wait for signal from grandchild
                let mut buf = [0u8; 1];
                match nix::unistd::read(&read_fd, &mut buf) {
                    Ok(1) if buf[0] == 1 => std::process::exit(0), // Success signal received
                    Ok(0) => {
                        // EOF - grandchild failed before writing success signal
                        eprintln!("Daemon failed to start");
                        std::process::exit(1);
                    }
                    _ => {
                        // Read error or unexpected data
                        eprintln!("Daemon failed to start");
                        std::process::exit(1);
                    }
                }
            }
            ForkResult::Child => {
                // Close read end in child
                drop(read_fd);
                // Child continues with daemonization (write_fd will be passed to grandchild)
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

        // Restrict PID file to owner-only access to prevent tampering
        fs::set_permissions(&self.pid_path, Permissions::from_mode(0o600))
            .context("Failed to set PID file permissions")?;

        // Generate admin + user tokens and write to separate files.
        //
        // admin.token lives at `$XDG_RUNTIME_DIR/loom/admin.token` (host-only).
        // user.token stays under `.work/` since it's mounted into containers.
        //
        // - admin.token (mode 0o600): required for privileged ops (Stop and the
        //   verification-bypass flags `--no-verify`, `--force-unsafe`,
        //   `--assume-merged`). Located in the daemon runtime directory so the
        //   container topology — which only mounts `.work/` — cannot reach it.
        // - user.token  (mode 0o644): mounted into containers RO; used for
        //   Ping / Subscribe / Unsubscribe / CompleteStageContainer.
        //
        // 32-byte / 256-bit random hex from /dev/urandom (OsRng-equivalent).
        let admin_token = generate_token_hex()?;
        let user_token = generate_token_hex()?;

        let admin_path = admin_token_path();
        if let Some(parent) = admin_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create admin token directory {}",
                    parent.display()
                )
            })?;
        }
        fs::write(&admin_path, &admin_token).with_context(|| {
            format!(
                "Failed to write admin token file at {}",
                admin_path.display()
            )
        })?;
        fs::set_permissions(&admin_path, Permissions::from_mode(0o600))
            .context("Failed to set admin token file permissions")?;

        let user_path = self.work_dir.join(USER_TOKEN_FILE);
        fs::write(&user_path, &user_token).context("Failed to write user token file")?;
        fs::set_permissions(&user_path, Permissions::from_mode(0o644))
            .context("Failed to set user token file permissions")?;

        // Redirect stdout and stderr to log file
        let log_file = File::create(&self.log_path).context("Failed to create log file")?;

        // Close stdin and redirect stdout/stderr to log file
        close(0).ok();
        // SAFETY: Using libc::dup2 directly with raw fds to avoid ownership issues.
        // fds 1 and 2 are valid open descriptors in this double-forked daemon process.
        unsafe {
            libc::dup2(log_file.as_raw_fd(), 1);
            libc::dup2(log_file.as_raw_fd(), 2);
        }

        // Signal success to original parent AFTER all initialization succeeds
        let success_signal = [1u8];
        let _ = nix::unistd::write(&write_fd, &success_signal);
        drop(write_fd); // Close pipe after writing success

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

        // Restrict PID file to owner-only access to prevent tampering
        fs::set_permissions(&self.pid_path, Permissions::from_mode(0o600))
            .context("Failed to set PID file permissions")?;

        // Remove stale socket if it exists (ignore NotFound to avoid TOCTOU race)
        if let Err(e) = fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove stale socket file");
            }
        }

        self.run_server()
    }

    /// Acquire an exclusive flock on orchestrator.lock to prevent multiple daemons.
    ///
    /// Returns the held File handle. The lock is released when the handle is dropped
    /// (including on process exit or SIGKILL).
    pub(super) fn acquire_exclusive_lock(&self) -> Result<File> {
        use std::io::{Seek, Write};

        let lock_path = self.work_dir.join("orchestrator.lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .context("Failed to open orchestrator lock file")?;

        let ret = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            // Lock held by another daemon — read its PID (file was NOT truncated)
            let existing_pid = fs::read_to_string(&lock_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());
            if let Some(pid) = existing_pid {
                bail!(
                    "Another daemon instance is already running (PID {pid}). \
                     Use 'loom stop' or 'kill {pid}' to stop it."
                );
            } else {
                bail!("Another daemon instance is already running (lock held)");
            }
        }

        // Lock acquired — truncate and write our PID
        let mut lf = &lock_file;
        lf.set_len(0).ok();
        lf.seek(std::io::SeekFrom::Start(0)).ok();
        let _ = lf.write_all(format!("{}", std::process::id()).as_bytes());
        let _ = lf.flush();

        Ok(lock_file)
    }

    /// Check if the orchestrator lock is currently held by a live process.
    ///
    /// Returns `Some(pid)` if a daemon holds the lock, `None` otherwise.
    pub fn check_lock(work_dir: &Path) -> Option<u32> {
        let lock_path = work_dir.join("orchestrator.lock");
        let lock_file = match File::open(&lock_path) {
            Ok(f) => f,
            Err(_) => return None,
        };

        let ret = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if ret == 0 {
            // We acquired the lock — no daemon running. Release immediately.
            unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_UN) };
            None
        } else {
            // Lock held by another process — read the PID
            fs::read_to_string(&lock_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
        }
    }

    /// Main server loop (listens on socket and accepts connections).
    pub(super) fn run_server(&self) -> Result<()> {
        // Acquire exclusive lock BEFORE anything else — prevents multiple daemons.
        // The _lock_guard is kept alive for the entire server lifetime; the OS
        // releases the flock when this process exits (even via SIGKILL).
        let _lock_guard = self
            .acquire_exclusive_lock()
            .context("Failed to acquire daemon lock")?;

        // Set restrictive umask before socket bind to close TOCTOU window
        // between bind() and chmod(). The socket is created with permissions
        // determined by umask, so setting 0o077 ensures it's created as 0o600.
        let old_umask = unsafe { libc::umask(0o077) };
        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;
        // Restore original umask immediately after bind
        unsafe {
            libc::umask(old_umask);
        }

        // Explicitly set permissions as defense-in-depth (umask should have handled this,
        // but being explicit is safer and documents intent)
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

        while !self.shutdown_flag.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Atomically increment connection count and check limit
                    let previous = self.connection_count.fetch_add(1, Ordering::SeqCst);
                    if previous >= MAX_CONNECTIONS {
                        // Over limit - decrement and reject
                        self.connection_count.fetch_sub(1, Ordering::SeqCst);
                        eprintln!("Connection limit reached ({MAX_CONNECTIONS}), rejecting");
                        drop(stream); // Close the connection immediately
                        continue;
                    }

                    let shutdown_flag = Arc::clone(&self.shutdown_flag);
                    let status_subscribers = Arc::clone(&self.status_subscribers);
                    let log_subscribers = Arc::clone(&self.log_subscribers);
                    let connection_count = Arc::clone(&self.connection_count);
                    let work_dir = self.work_dir.clone();

                    thread::spawn(move || {
                        let result = handle_client_connection(
                            stream,
                            shutdown_flag,
                            status_subscribers,
                            log_subscribers,
                            &work_dir,
                        );
                        // Decrement connection count when thread exits
                        connection_count.fetch_sub(1, Ordering::SeqCst);
                        if let Err(e) = result {
                            eprintln!("Client handler error: {e}");
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection available, sleep briefly but check shutdown frequently
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("Accept error: {e}");
                    break;
                }
            }
        }

        // Wait for threads to finish with timeout (5 seconds)
        let join_timeout = Duration::from_secs(5);
        let join_check_interval = Duration::from_millis(50);

        // Helper closure to wait for a thread with timeout
        let wait_with_timeout = |handle: thread::JoinHandle<()>, name: &str| {
            let start = std::time::Instant::now();
            while !handle.is_finished() && start.elapsed() < join_timeout {
                thread::sleep(join_check_interval);
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                eprintln!("Warning: {} thread did not terminate within timeout", name);
                // Thread will be abandoned but the process is exiting anyway
            }
        };

        if let Some(handle) = orchestrator_handle {
            wait_with_timeout(handle, "orchestrator");
        }
        if let Some(handle) = log_tail_handle {
            wait_with_timeout(handle, "log_tail");
        }
        wait_with_timeout(status_broadcast_handle, "status_broadcast");

        self.cleanup()?;
        Ok(())
    }

    /// Clean up socket, PID, token, and completion marker files.
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
        // Clean up token files. user.token lives in .work/; admin.token lives
        // at the host-only runtime path (NOT in .work/ — containers only mount
        // .work/, so the relocation is what prevents container-resident agents
        // from reading the admin token).
        let user_token_path = self.work_dir.join(USER_TOKEN_FILE);
        if let Err(e) = fs::remove_file(&user_token_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).context("Failed to remove user.token file");
            }
        }
        let admin_path = admin_token_path();
        if let Err(e) = fs::remove_file(&admin_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).with_context(|| {
                    format!("Failed to remove admin token at {}", admin_path.display())
                });
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
