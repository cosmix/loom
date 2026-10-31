//! Authenticated graceful daemon shutdown request.

use super::super::deadline_transport::DeadlineStream;
use super::super::protocol::{read_message, write_message, Request, Response};
use super::core::DaemonServer;
use anyhow::{bail, Context, Result};
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
        let socket_path = crate::daemon::socket_path(work_dir);
        if !Self::is_running(work_dir) {
            bail!("Daemon is not running");
        }

        let mut stream =
            DeadlineStream::connect(&socket_path, Duration::from_secs(5)).map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    anyhow::anyhow!("Daemon connection timed out; stop execution is uncertain")
                } else {
                    DaemonUnavailable.into()
                }
            })?;
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

fn read_stop_response(stream: &mut DeadlineStream) -> Result<Response> {
    match read_message(stream) {
        Ok(response) => Ok(response),
        Err(error) if is_timeout(&error) => {
            bail!("Daemon did not respond within 5 seconds; its state may be hung")
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
