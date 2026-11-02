//! Authenticated graceful daemon shutdown request.

use super::super::deadline_transport::DeadlineStream;
use super::super::protocol::{read_message, write_message, Request, Response};
use super::core::DaemonServer;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::time::Duration;

/// The connect failed, so no request reached the daemon. A stop that was sent
/// and went unanswered is a different, uncertain error.
#[derive(Debug)]
pub(crate) struct DaemonUnavailable {
    timed_out: bool,
}

impl std::fmt::Display for DaemonUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(if self.timed_out {
            "daemon connection timed out; no stop request was sent"
        } else {
            "daemon control socket is unavailable"
        })
    }
}

impl std::error::Error for DaemonUnavailable {}

impl DaemonServer {
    /// Send an externally authorized stop request to the running daemon.
    pub fn stop(work_dir: &Path, operator_proof: &str) -> Result<()> {
        let socket_path = crate::daemon::socket_path(work_dir);
        if !Self::is_running(work_dir) {
            bail!("Daemon is not running");
        }

        let mut stream =
            DeadlineStream::connect(&socket_path, Duration::from_secs(5)).map_err(|error| {
                DaemonUnavailable {
                    timed_out: error.kind() == std::io::ErrorKind::TimedOut,
                }
            })?;
        if let Err(error) = write_message(
            &mut stream,
            &Request::Stop {
                auth_token: operator_proof.to_string(),
            },
        ) {
            if is_timeout(&error) {
                bail!("Daemon stop request timed out; stop execution is uncertain");
            }
            return Err(error).context("Failed to send stop request");
        }

        match read_stop_response(&mut stream)? {
            Response::Ok => Ok(()),
            Response::AuthenticationFailed => bail!("Operator proof was rejected"),
            Response::Error { message } => bail!("Daemon returned error: {message}"),
            _ => bail!("Unexpected response from daemon"),
        }
    }
}

fn read_stop_response(stream: &mut DeadlineStream) -> Result<Response> {
    match read_message(stream) {
        Ok(response) => Ok(response),
        Err(error) if is_timeout(&error) => {
            bail!("Daemon did not respond within 5 seconds; stop execution is uncertain")
        }
        Err(error) => Err(error).context("Failed to read stop response"),
    }
}

fn is_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::time::Instant;

    #[test]
    fn a_silent_stop_peer_reports_uncertain_execution_at_the_deadline() {
        let (stream, _silent_peer) = UnixStream::pair().unwrap();
        let mut transport =
            DeadlineStream::connected_for_test(stream, Duration::from_millis(50)).unwrap();

        let started = Instant::now();
        let error = read_stop_response(&mut transport).unwrap_err();

        assert!(error.to_string().contains("stop execution is uncertain"));
        assert!(
            error.downcast_ref::<DaemonUnavailable>().is_none(),
            "a sent request must never reach the --force SIGTERM fallback"
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    /// A daemon that stopped accepting (SIGSTOP, a wedged accept loop) leaves
    /// connections parked in its backlog. A status probe and a stop must each
    /// come back regardless. Linux only: there a blocking connect to a full
    /// backlog waits forever, and a non-blocking one answers `EAGAIN`.
    #[cfg(target_os = "linux")]
    #[test]
    fn status_and_stop_are_bounded_when_the_daemon_never_accepts() {
        use super::super::lock::acquire_lock;
        use crate::daemon::DaemonStatus;
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixListener;

        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path().to_path_buf();
        if !crate::process::sandbox_probe::unix_socket_bindable(&work_dir) {
            return;
        }
        let _lock = acquire_lock(&work_dir).unwrap();
        let socket = crate::daemon::socket_path(&work_dir);
        let listener = UnixListener::bind(&socket).unwrap();
        // SAFETY: listener owns a valid socket descriptor for this call.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let mut parked = Vec::new();
        while let Ok(stream) = DeadlineStream::connect(&socket, Duration::from_millis(50)) {
            parked.push(stream);
            assert!(parked.len() < 64, "the backlog never filled");
        }

        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let status = DaemonServer::check_status(&work_dir);
            let stopped = DaemonServer::stop(&work_dir, "proof").unwrap_err();
            let unavailable = stopped.downcast_ref::<DaemonUnavailable>().is_some();
            let _ = send.send((status, stopped.to_string(), unavailable));
        });

        let (status, stopped, unavailable) = receive
            .recv_timeout(Duration::from_secs(20))
            .expect("the status probe or the stop request blocked past every bound");
        assert_eq!(status, DaemonStatus::ProcessOnly);
        assert!(
            stopped.contains("connection timed out"),
            "a stop that never connected must say so"
        );
        assert!(
            unavailable,
            "a connect that delivered nothing must reach the --force SIGTERM fallback"
        );
    }

    /// `loom stop --force` falls back to SIGTERM only on `DaemonUnavailable`,
    /// so a connect that fails outright must be that error too.
    #[test]
    fn a_stop_with_no_socket_to_connect_to_is_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let _lock = super::super::lock::acquire_lock(temp.path()).unwrap();

        let error = DaemonServer::stop(temp.path(), "proof").unwrap_err();

        assert!(
            error.downcast_ref::<DaemonUnavailable>().is_some(),
            "{error:#}"
        );
        assert_eq!(error.to_string(), "daemon control socket is unavailable");
    }
}
