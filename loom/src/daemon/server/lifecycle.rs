//! Daemon server lifecycle methods: start, serve, stop, run.

use super::admission::ByteBudget;
use super::broadcast::{spawn_log_tailer, spawn_quota_poller, spawn_status_broadcaster};
use super::client::handle_client_connection;
use super::core::{
    DaemonServer, CLIENT_QUEUE_CAPACITY, CLIENT_WORKERS, MAX_IN_FLIGHT_REQUEST_BYTES,
};
use super::launch::{self, LOG_ACTIVE_BYTE, READY_BYTE};
use super::lock::{current_identity, format_identity, read_recorded_lock_identity, PID_FILE};
use super::orchestrator::spawn_orchestrator;
use super::pool::WorkerPool;
use super::storage::{
    ensure_private_control_dir, open_private_output, publish_private_file, remove_control_file,
};
use super::tokens::{publish_fresh_tokens, ADMIN_TOKEN_FILE, USER_TOKEN_FILE};
use crate::daemon::{socket_path_fits, SOCKET_FILE, SUN_PATH_MAX};
use crate::orchestrator::core::{
    abort_foreign_state, check_lock_identity, LockCheck, LockIdentity,
};

use anyhow::{Context, Result};
use nix::unistd::setsid;
use std::fs::{self, File, Permissions};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

impl DaemonServer {
    /// Start the daemon from `loom run`: re-execute the loom binary as `loom
    /// run --daemon-child` and wait until it reports ready (`launch`).
    pub fn start(&self) -> Result<()> {
        ensure_private_control_dir(&self.work_dir)?;
        launch::spawn_daemon(&self.work_dir, &self.config)
    }

    /// Serve as the daemon: the body of `loom run --daemon-child`.
    ///
    /// stdout is the readiness pipe `loom run` waits on: an error before the
    /// log redirect reaches `loom run` through it, one after it lands in
    /// `orchestrator.log`, whose tail `loom run` then quotes.
    pub(crate) fn serve(&self) -> Result<()> {
        // Leave `loom run`'s session, so neither its terminal nor its exit
        // reaches the daemon.
        setsid().context("setsid failed")?;

        // CRITICAL (A-1/O-7): Acquire the singleton flock BEFORE any destructive
        // op (socket unlink, PID overwrite, token regeneration, log truncation).
        // A losing race or a corrupt lock must NOT delete the live daemon's
        // control-plane files. `Drop`/`cleanup` are gated on `was_running`, which
        // is only set after a successful socket bind in `run_server`, so a
        // failure here leaves the winning daemon's files alone; the ready byte
        // is never written, so `loom run` reports this error.
        let lock_guard = self
            .acquire_exclusive_lock()
            .context("Failed to acquire daemon lock")?;

        // From here on we hold the singleton lock; destructive setup is safe.
        crate::orchestrator::terminal::native::record_daemon_binary();

        // Remove stale socket if it exists (ignore NotFound to avoid TOCTOU race)
        remove_control_file(&self.work_dir, Path::new(SOCKET_FILE))
            .context("Failed to remove stale socket file")?;

        let identity = current_identity();
        publish_private_file(
            &self.work_dir,
            Path::new(PID_FILE),
            format_identity(identity).as_bytes(),
        )
        .context("Failed to publish PID identity file")?;

        // Publish the search-exclusion files, then fresh admin and user
        // tokens; see `tokens::publish_fresh_tokens` for the full rationale.
        publish_fresh_tokens(&self.work_dir)?;

        // Keep the readiness pipe past the redirect. The duplicate is
        // close-on-exec, so no process the daemon starts inherits it.
        let ready = std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .context("Failed to keep the readiness pipe")?;
        self.redirect_output_to_log()?;
        let _ = nix::unistd::write(&ready, &[LOG_ACTIVE_BYTE]);

        // `run_server` writes the ready byte once the socket is bound.
        self.run_server(lock_guard, ready)
    }

    /// Rotate the previous log, then point the daemon's stdout and stderr at
    /// a fresh private `orchestrator.log`. stdin is already `/dev/null`: `loom
    /// run` starts the daemon child that way.
    fn redirect_output_to_log(&self) -> Result<()> {
        // Preserve the previous run's log first. Restarting the daemon is the
        // standard response to a stuck orchestrator, so truncating here
        // destroys the only record of *why* it got stuck at exactly the moment
        // an operator goes looking for it. Keeping one generation costs one
        // rename and bounds growth at two files.
        rotate_log(&self.work_dir);
        let log_file = open_private_output(&self.work_dir, Path::new("orchestrator.log"))
            .context("Failed to create log file")?;

        // SAFETY: Using libc::dup2 directly with raw fds to avoid ownership issues.
        // fds 1 and 2 are the readiness pipe this freshly exec'd daemon child was
        // started with, and `log_file` is open, so both calls swap valid descriptors.
        unsafe {
            libc::dup2(log_file.as_raw_fd(), 1);
            libc::dup2(log_file.as_raw_fd(), 2);
        }
        Ok(())
    }

    /// Main server loop (listens on socket and accepts connections).
    ///
    /// `lock_guard` is the held singleton flock acquired by the caller BEFORE any
    /// destructive setup (A-1/O-7). It is kept alive for the entire server
    /// lifetime; the OS releases the flock when this process exits (even via
    /// SIGKILL). `ready` is the write end of the readiness pipe `loom run`
    /// waits on; the ready byte goes to it only after the socket bind
    /// succeeds, so `loom run` reports failure if the daemon could not
    /// actually start listening.
    pub(super) fn run_server(&self, lock_guard: File, ready: OwnedFd) -> Result<()> {
        let lock_identity = LockIdentity::of_file_or_warn(&lock_guard);
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
        // SAFETY: the umask is process-wide; this freshly exec'd daemon child
        // has started no worker thread yet, and it restores the umask
        // immediately after the single bind.
        let old_umask = unsafe { libc::umask(0o077) };
        let bound = UnixListener::bind(&self.socket_path);
        // Restore original umask immediately after bind, failed or not.
        // SAFETY: paired with the `umask(0o077)` call above, still before any
        // worker thread starts.
        unsafe {
            libc::umask(old_umask);
        }
        let listener = bound.context("Failed to bind Unix socket")?;

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

        // Tell `loom run` the daemon is ready now that the socket is bound and
        // its permissions set; dropping `ready` closes the pipe's last write end.
        let _ = nix::unistd::write(&ready, &[READY_BYTE]);
        drop(ready);

        // Set socket to non-blocking mode for graceful shutdown
        listener
            .set_nonblocking(true)
            .context("Failed to set socket to non-blocking")?;

        // Spawn the orchestrator thread to actually run stages
        let orchestrator_handle = spawn_orchestrator(self, lock_identity);

        let log_tail_handle = spawn_log_tailer(self);
        let status_broadcast_handle = spawn_status_broadcaster(self);
        let quota_poller_handle = spawn_quota_poller(self);
        let client_pool = WorkerPool::new(CLIENT_WORKERS, CLIENT_QUEUE_CAPACITY);
        let byte_budget = ByteBudget::new(MAX_IN_FLIGHT_REQUEST_BYTES);

        let mut last_identity_check = Instant::now();
        while !self.shutdown_flag.load(Ordering::SeqCst) {
            watch_lock_identity(&self.work_dir, lock_identity, &mut last_identity_check);
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
        wait_with_timeout(quota_poller_handle, "quota_poller");

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
            SOCKET_FILE,
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

/// Second, coarser-grained layer of the state-identity check alongside the
/// orchestrator thread's per-tick `assert_state_identity` (see its doc
/// comment for the sub-tick window neither one closes): the accept loop's
/// other threads (`tick::record`, the quota poller) keep writing by path for
/// up to one poll interval after a state-directory swap, so this catches it
/// from here too. Throttled to once a second since it costs a `stat`.
fn watch_lock_identity(
    work_dir: &Path,
    lock_identity: Option<LockIdentity>,
    last_check: &mut Instant,
) {
    let Some(held) = lock_identity else {
        return;
    };
    if last_check.elapsed() < Duration::from_secs(1) {
        return;
    }
    *last_check = Instant::now();
    if check_lock_identity(work_dir, held) != LockCheck::Intact {
        abort_foreign_state(
            "state directory replaced under the running daemon (its .loom/work was reused for \
             a different plan); accept-loop identity check aborting",
        );
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests;
