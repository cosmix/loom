//! Bounded request-body acquisition.

use anyhow::{bail, Context, Result};
use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::MAX_BODY_BYTES;

/// Consume exactly `length` body bytes, counting the `prefix` that already
/// came off the socket with the head.
///
/// `length` is the request's `Content-Length` and is refused outright past
/// [`MAX_BODY_BYTES`]; nothing beyond it is ever read, so a client whose body
/// is longer than it declared gets the declared prefix parsed and the rest left
/// on the socket for the caller's drain, never buffered here. Each individual
/// body must finish within five seconds and is cancelled when the server stops.
pub fn read_body(
    stream: &mut TcpStream,
    prefix: Vec<u8>,
    length: usize,
    running: &AtomicBool,
) -> Result<Vec<u8>> {
    read_body_until(
        stream,
        prefix,
        length,
        running,
        Instant::now() + Duration::from_secs(5),
    )
}

#[cfg(test)]
#[path = "../http_body_tests.rs"]
mod body_tests;

fn read_body_until(
    stream: &mut TcpStream,
    prefix: Vec<u8>,
    length: usize,
    running: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>> {
    if length > MAX_BODY_BYTES {
        bail!("request body exceeds {MAX_BODY_BYTES} bytes");
    }
    let mut body = prefix;
    body.truncate(length);
    let mut chunk = [0_u8; 1024];
    while body.len() < length {
        if !running.load(Ordering::SeqCst) {
            bail!("server stopping");
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("request body timed out");
        }
        stream.set_read_timeout(Some(remaining.min(Duration::from_millis(100))))?;
        let wanted = (length - body.len()).min(chunk.len());
        let read = match stream.read(&mut chunk[..wanted]) {
            Ok(read) => read,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => return Err(error).context("failed to read request body"),
        };
        if read == 0 {
            bail!("client closed connection before completing request body");
        }
        body.extend_from_slice(&chunk[..read]);
    }
    if !running.load(Ordering::SeqCst) {
        bail!("server stopping");
    }
    if Instant::now() >= deadline {
        bail!("request body timed out");
    }
    Ok(body)
}
