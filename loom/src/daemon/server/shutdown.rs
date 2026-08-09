//! Authenticated graceful daemon shutdown request.

use super::super::protocol::{read_message, write_message, Request, Response};
use super::core::DaemonServer;
use anyhow::{bail, Context, Result};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct DaemonUnavailable;

impl std::fmt::Display for DaemonUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("daemon control socket is unavailable")
    }
}

impl std::error::Error for DaemonUnavailable {}

impl DaemonServer {
    /// Send an externally authorized stop request to the running daemon.
    pub fn stop(work_dir: &Path, operator_proof: &str) -> Result<()> {
        let socket_path = work_dir.join("orchestrator.sock");
        if !Self::is_running(work_dir) {
            bail!("Daemon is not running");
        }

        let mut stream = UnixStream::connect(socket_path).map_err(|_| DaemonUnavailable)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("Failed to set read timeout")?;
        write_message(
            &mut stream,
            &Request::Stop {
                auth_token: operator_proof.to_string(),
            },
        )
        .context("Failed to send stop request")?;

        match read_stop_response(&mut stream)? {
            Response::Ok => Ok(()),
            Response::AuthenticationFailed => bail!("Operator proof was rejected"),
            Response::Error { message } => bail!("Daemon returned error: {message}"),
            _ => bail!("Unexpected response from daemon"),
        }
    }
}

fn read_stop_response(stream: &mut UnixStream) -> Result<Response> {
    match read_message(stream) {
        Ok(response) => Ok(response),
        Err(error) if is_timeout(&error) => {
            bail!("Daemon did not respond within 5 seconds; its state may be hung")
        }
        Err(error) => Err(error).context("Failed to read stop response"),
    }
}

fn is_timeout(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )
    })
}
