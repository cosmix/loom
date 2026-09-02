//! Daemon server lifecycle methods: start, stop, run.

mod socket_limit;

use super::admission::ByteBudget;
use super::broadcast::{spawn_log_tailer, spawn_status_broadcaster};
use super::client::handle_client_connection;
use super::core::{
    DaemonServer, CLIENT_QUEUE_CAPACITY, CLIENT_WORKERS, MAX_IN_FLIGHT_REQUEST_BYTES,
};
use super::environment::DaemonEnvironment;
use super::lock::{current_identity, format_identity, read_recorded_lock_identity, PID_FILE};
use super::orchestrator::spawn_orchestrator;
use super::pool::WorkerPool;
use super::storage::{
    ensure_private_control_dir, open_private_output, publish_private_file, remove_control_file,
};
use super::tokens::{ADMIN_TOKEN_FILE, USER_TOKEN_FILE};
use socket_limit::{socket_path_fits, SUN_PATH_MAX};

use anyhow::{Context, Result};
use nix::unistd::{close, fork, pipe, setsid, ForkResult};
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixListener;
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
    /// Start the daemon (daemonize process).
    ///
    /// # Returns
    /// `Ok(())` on success, error if daemon fails to start
    pub fn start(&self) -> Result<()> {
        ensure_private_control_dir(&self.work_dir)?;
        let daemon_environment = DaemonEnvironment::capture();

        // Create pipe for error propagation from grandchild to original parent.
        // The success byte is written by `run_server` only AFTER the socket is
        // bound (see A-1/O-7), so the parent's `loom run` exits 0 only when the
        // daemon is genuinely listening.
        let (read_fd, write_fd) = pipe().context("Failed to create pipe")?;

        // First fork - parent exits, child continues.
        // SAFETY: daemonization occurs before Loom starts worker threads; both
        // branches immediately follow the constrained parent/child path below.
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

        // Second fork - prevents acquiring a controlling terminal.
        // SAFETY: this is still the single-threaded daemonization path, and the
        // intermediate parent exits without returning to shared application state.
        match unsafe { fork() }.context("Second fork failed")? {
            ForkResult::Parent { .. } => {
                // Intermediate parent exits
                std::process::exit(0);
            }
            ForkResult::Child => {
                // Grandchild continues as daemon
            }
        }

        daemon_environment.apply();

        // CRITICAL (A-1/O-7): Acquire the singleton flock BEFORE any destructive
        // op (socket unlink, PID overwrite, token regeneration, log truncation).
        // A losing race or a corrupt lock must NOT delete the live daemon's
        // control-plane files. `Drop`/`cleanup` are gated on `was_running`, which
        // is only set after a successful socket bind in `run_server`. If lock
        // acquisition fails here, we return Err before touching anything; the
        // success byte is never written so the parent reports failure.
        let lock_guard = match self.acquire_exclusive_lock() {
            Ok(guard) => guard,
            Err(e) => {
                // We do NOT hold the lock — `was_running` is false, so the Drop
                // cleanup is a no-op and the winning daemon's files survive.
                return Err(e).context("Failed to acquire daemon lock");
            }
        };

        // From here on we hold the singleton lock; destructive setup is safe.

        // Remove stale socket if it exists (ignore NotFound to avoid TOCTOU race)
        remove_control_file(&self.work_dir, Path::new("orchestrator.sock"))
            .context("Failed to remove stale socket file")?;

        let identity = current_identity();
        publish_private_file(
            &self.work_dir,
            Path::new(PID_FILE),
            format_identity(identity).as_bytes(),
        )
        .context("Failed to publish PID identity file")?;

        // Generate admin + user tokens and write to separate files. Both live
        // under the per-project `.loom/work/` directory.
        //
        // - admin.token (mode 0o600): required for privileged ops (Stop and the
        //   verification-bypass flags `--no-verify`, `--force-unsafe`,
        //   `--assume-merged`). Owner-only so a stage-confined agent cannot
        //   read it.
        // - user.token  (mode 0o600): used for Ping / Subscribe / Unsubscribe /
        //   DisputeCriteria. Owner-only so another local user cannot read it
        //   and exercise User-capability RPCs (S-8a).
        //
        // 32-byte / 256-bit random hex from /dev/urandom (OsRng-equivalent).
        let admin_token = generate_token_hex()?;
        let user_token = generate_token_hex()?;

        publish_private_file(
            &self.work_dir,
            Path::new(ADMIN_TOKEN_FILE),
            admin_token.as_bytes(),
        )
        .context("Failed to publish admin token file")?;
        publish_private_file(
            &self.work_dir,
            Path::new(USER_TOKEN_FILE),
            user_token.as_bytes(),
        )
        .context("Failed to publish user token file")?;

        // Redirect stdout and stderr to log file.
        //
        // Preserve the previous run's log first. Restarting the daemon is the
        // standard response to a stuck orchestrator, so truncating here
        // destroys the only record of *why* it got stuck at exactly the moment
        // an operator goes looking for it. Keeping one generation costs one
        // rename and bounds growth at two files.
        rotate_log(&self.work_dir);
        let log_file = open_private_output(&self.work_dir, Path::new("orchestrator.log"))
            .context("Failed to create log file")?;

        // Close stdin and redirect stdout/stderr to log file
        close(0).ok();
        // SAFETY: Using libc::dup2 directly with raw fds to avoid ownership issues.
        // fds 1 and 2 are valid open descriptors in this double-forked daemon process.
        unsafe {
            libc::dup2(log_file.as_raw_fd(), 1);
            libc::dup2(log_file.as_raw_fd(), 2);
        }

        // Run the server. The success byte is signaled to the original parent
        // from inside `run_server`, immediately after the socket bind succeeds.
        self.run_server(lock_guard, Some(write_fd))
    }

    /// Main server loop (listens on socket and accepts connections).
    ///
    /// `lock_guard` is the held singleton flock acquired by the caller BEFORE any
    /// destructive setup (A-1/O-7). It is kept alive for the entire server
    /// lifetime; the OS releases the flock when this process exits (even via
    /// SIGKILL). `success_pipe`, when present, is the write end of the start
    /// pipe — the success byte is written to it only after the socket bind
    /// succeeds, so the parent `loom run` reports failure if the daemon could
    /// not actually start listening.
    pub(super) fn run_server(
        &self,
        lock_guard: File,
        success_pipe: Option<std::os::fd::OwnedFd>,
    ) -> Result<()> {
        // The guard is owned for the full server lifetime.
        let _lock_guard = lock_guard;

        // Before the umask twiddling below, so a bail here leaves it untouched.
        if !socket_path_fits(&self.socket_path) {
            anyhow::bail!(
                "socket path '{}' ({} bytes) exceeds the {SUN_PATH_MAX}-byte sun_path limit",
                self.socket_path.display(),
                self.socket_path.as_os_str().len()
            );
        }

        // Set restrictive umask before socket bind to close TOCTOU window
        // between bind() and chmod(). The socket is created with permissions
        // determined by umask, so setting 0o077 ensures it's created as 0o600.
        // SAFETY: this daemon grandchild has not started worker threads, and it
        // restores the process-wide umask immediately after the single bind.
        let old_umask = unsafe { libc::umask(0o077) };
        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;
        // Restore original umask immediately after bind.
        // SAFETY: paired with the single-threaded `umask(0o077)` call above.
        unsafe {
            libc::umask(old_umask);
        }

        // Explicitly set permissions as defense-in-depth (umask should have handled this,
        // but being explicit is safer and documents intent)
        fs::set_permissions(&self.socket_path, Permissions::from_mode(0o600))
            .context("Failed to set socket permissions")?;

        // We now hold the singleton lock AND own a bound socket: this process is
        // the live daemon. Mark `was_running` so Drop cleanup is allowed to remove
        // OUR control-plane files on exit. (A-1/O-7) Anything that failed before
        // this point leaves `was_running` false, so a losing-race or pre-bind
        // failure never deletes the winning daemon's files.
        self.was_running.store(true, Ordering::SeqCst);

        // Signal success to the original parent now that the socket is bound and
        // permissions are set. Closing the pipe afterwards lets the parent's read
        // return. Only the daemonized `start()` path supplies a pipe.
        if let Some(write_fd) = success_pipe {
            let success_signal = [1u8];
            let _ = nix::unistd::write(&write_fd, &success_signal);
            drop(write_fd);
        }

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
        let client_pool = WorkerPool::new(CLIENT_WORKERS, CLIENT_QUEUE_CAPACITY);
        let byte_budget = ByteBudget::new(MAX_IN_FLIGHT_REQUEST_BYTES);

        while !self.shutdown_flag.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let shutdown_flag = Arc::clone(&self.shutdown_flag);
                    let status_subscribers = Arc::clone(&self.status_subscribers);
                    let log_subscribers = Arc::clone(&self.log_subscribers);
                    let work_dir = self.work_dir.clone();
                    let request_budget = Arc::clone(&byte_budget);
                    if !client_pool.try_execute(move || {
                        let result = handle_client_connection(
                            stream,
                            shutdown_flag,
                            status_subscribers,
                            log_subscribers,
                            &work_dir,
                            request_budget,
                        );
                        if let Err(e) = result {
                            eprintln!("Client handler error: {e}");
                        }
                    }) {
                        eprintln!("Daemon client capacity is exhausted; rejecting connection");
                    }
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

        drop(client_pool);

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
    ///
    /// CRITICAL (A-1/O-7): This only removes files when THIS process was the live
    /// daemon — i.e. it acquired the singleton lock and bound the socket
    /// (`was_running == true`). A `DaemonServer` that lost the singleton race or
    /// failed before binding must NEVER delete the winning daemon's
    /// socket/PID/admin.token/user.token/log. As defense-in-depth we also verify
    /// the lock file still names our PID before deleting.
    pub(super) fn cleanup(&self) -> Result<()> {
        if !self.was_running.load(Ordering::SeqCst) {
            // We never became the live daemon — touch nothing.
            return Ok(());
        }
        if read_recorded_lock_identity(&self.work_dir).map(|identity| identity.pid)
            != Some(std::process::id())
        {
            return Ok(());
        }

        for relative in [
            "orchestrator.sock",
            PID_FILE,
            USER_TOKEN_FILE,
            ADMIN_TOKEN_FILE,
            "orchestrator.complete",
        ] {
            remove_control_file(&self.work_dir, Path::new(relative))
                .with_context(|| format!("Failed to remove daemon control file {relative}"))?;
        }
        Ok(())
    }
}

/// Move the existing daemon log aside to `<log>.prev`, keeping exactly one
/// previous generation.
///
/// Best-effort: if the rename fails the caller still truncates and starts a
/// fresh log, which is the pre-existing behaviour. Losing history is a
/// diagnostic regression, not a reason to refuse to start the daemon.
fn rotate_log(work_dir: &Path) {
    let Ok(directory) = crate::fs::safe_fs::safe_open_dirfd(work_dir) else {
        return;
    };
    let _ = crate::fs::safe_fs::safe_rename_in_workdir(
        directory.as_raw_fd(),
        Path::new("orchestrator.log"),
        Path::new("orchestrator.log.prev"),
    );
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests;
