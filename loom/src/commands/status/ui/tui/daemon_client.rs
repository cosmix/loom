//! Daemon client for TUI communication.

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::daemon::{read_auth_token, read_message, write_message, Request, Response};

/// Connection timeout for daemon socket.
pub const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Connect to the daemon serving `work_dir`. The socket and the user token are
/// both read from the resolved state root (`crate::daemon::socket_path`).
pub fn connect(work_dir: &Path) -> Result<UnixStream> {
    let socket_path = crate::daemon::socket_path(work_dir);
    let mut stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .context("Failed to set read timeout")?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .context("Failed to set write timeout")?;

    write_message(
        &mut stream,
        &Request::Ping {
            auth_token: user_token(work_dir),
        },
    )
    .context("Failed to send Ping")?;

    let response: Response = read_message(&mut stream).context("Failed to read Ping response")?;

    match response {
        Response::Pong => {}
        Response::Error { message } => {
            anyhow::bail!("Daemon returned error: {message}");
        }
        _ => {
            anyhow::bail!("Unexpected response from daemon");
        }
    }

    Ok(stream)
}

/// The user token of the daemon serving `work_dir`, read from the resolved
/// state root beside its socket; empty when unreadable.
pub(crate) fn user_token(work_dir: &Path) -> String {
    read_auth_token(
        crate::daemon::socket_path(work_dir)
            .parent()
            .unwrap_or(work_dir),
    )
    .unwrap_or_default()
}

/// Subscribe to status updates on a stream connected to the daemon of `work_dir`.
pub fn subscribe(stream: &mut UnixStream, work_dir: &Path) -> Result<()> {
    let auth_token = user_token(work_dir);
    write_message(stream, &Request::SubscribeStatus { auth_token })
        .context("Failed to send SubscribeStatus")?;

    let response: Response =
        read_message(stream).context("Failed to read subscription response")?;

    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => {
            anyhow::bail!("Subscription failed: {message}");
        }
        _ => {
            anyhow::bail!("Unexpected subscription response");
        }
    }
}

/// Check if an error indicates socket disconnection.
///
/// Returns true only for actual disconnection errors (EOF, broken pipe, etc.)
/// NOT for timeouts (WouldBlock, TimedOut) which are expected in non-blocking reads.
pub fn is_socket_disconnected(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            match io_err.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                    return false;
                }
                std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionAborted => {
                    return true;
                }
                _ => {}
            }
        }
    }

    let err_str = error.to_string().to_lowercase();

    (err_str.contains("unexpectedeof")
        || err_str.contains("connection reset")
        || err_str.contains("broken pipe")
        || err_str.contains("os error 9")
        || err_str.contains("os error 104")
        || err_str.contains("os error 32"))
        && !err_str.contains("would block")
        && !err_str.contains("timed out")
        && !err_str.contains("os error 11")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_disconnect_for_io_error(kind: std::io::ErrorKind) -> bool {
        let io_err = std::io::Error::new(kind, "test error");
        let error = anyhow::Error::new(io_err).context("Failed to read message length");
        is_socket_disconnected(&error)
    }

    #[test]
    fn test_is_socket_disconnected_timeout_not_disconnect() {
        assert!(!check_disconnect_for_io_error(
            std::io::ErrorKind::WouldBlock
        ));
        assert!(!check_disconnect_for_io_error(std::io::ErrorKind::TimedOut));
    }

    #[test]
    fn test_is_socket_disconnected_real_disconnect() {
        assert!(check_disconnect_for_io_error(
            std::io::ErrorKind::UnexpectedEof
        ));
        assert!(check_disconnect_for_io_error(
            std::io::ErrorKind::ConnectionReset
        ));
        assert!(check_disconnect_for_io_error(
            std::io::ErrorKind::BrokenPipe
        ));
        assert!(check_disconnect_for_io_error(
            std::io::ErrorKind::ConnectionAborted
        ));
    }

    #[test]
    fn subscribe_sends_the_token_of_the_given_work_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("user.token"), "tok-abc\n").unwrap();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let daemon = std::thread::spawn(move || {
            let request: Request = read_message(&mut server).unwrap();
            write_message(&mut server, &Response::Ok).unwrap();
            request
        });

        subscribe(&mut client, dir.path()).unwrap();

        match daemon.join().unwrap() {
            Request::SubscribeStatus { auth_token } => assert_eq!(auth_token, "tok-abc"),
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn test_is_socket_disconnected_other_errors() {
        assert!(!check_disconnect_for_io_error(
            std::io::ErrorKind::PermissionDenied
        ));
        assert!(!check_disconnect_for_io_error(std::io::ErrorKind::NotFound));
    }
}
