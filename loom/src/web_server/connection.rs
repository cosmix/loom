//! Socket timeouts and the plain-text answers every connection may need:
//! refusals, error statuses, and routed responses that honour `HEAD`.

use std::io::{ErrorKind, Read};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use super::http::{self, RequestHead};

/// How long a single peek may block, bounding how long a connection thread
/// ignores a shutdown request.
pub(crate) const PEEK_TIMEOUT: Duration = Duration::from_millis(250);

/// How long a single blocked `write` syscall may hold the connection thread.
///
/// `set_write_timeout` bounds one syscall, not a whole response, so this is
/// not a budget for serving `index.js`: a client that acknowledges a few bytes
/// just inside the timeout keeps `write_all` looping for far longer. What it
/// does rule out is the thread parking in `write_all` for good once a client
/// that stopped reading altogether fills its receive window — the same hazard
/// the WebSocket lane guards against. The whole-response bound comes from the
/// connection cap instead: a slow reader occupies one of [`MAX_CONNECTIONS`]
/// slots and no more. The budget is looser than the WebSocket lane's because
/// this one writes whole bundle assets rather than one small snapshot frame.
///
/// [`MAX_CONNECTIONS`]: super::limits::MAX_CONNECTIONS
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Bytes drained from an unfinished request before an error response.
const MAX_DRAIN_BYTES: usize = 64 * 1024;

/// Wall-clock bound on [`drain_pending`].
///
/// The byte cap alone bounds nothing in time: a client trickling one byte per
/// read timeout satisfies every read, so the loop can run for as many
/// iterations as [`MAX_DRAIN_BYTES`] allows. The drain therefore gives up on
/// whichever bound it reaches first. It blocks, so only a connection thread
/// may call it; the accept loop uses [`reject_unavailable`] instead.
const DRAIN_DEADLINE: Duration = Duration::from_millis(300);

/// Read and discard whatever the client has already sent, up to
/// [`MAX_DRAIN_BYTES`].
///
/// Every caller reaches this with request bytes deliberately left unconsumed:
/// the head was peeked rather than read, or the head parsed but a body
/// followed it. Closing a socket with unread bytes still in the receive buffer
/// makes Linux answer with RST instead of FIN, which discards the response we
/// just wrote before it reaches the client. Draining first makes the close a
/// clean FIN, so the error status actually arrives.
pub(crate) fn drain_pending(stream: &mut TcpStream) {
    if stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .is_err()
    {
        return;
    }
    let mut chunk = [0_u8; 4096];
    let mut drained = 0;
    let deadline = Instant::now() + DRAIN_DEADLINE;
    while drained < MAX_DRAIN_BYTES && Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => drained += read,
        }
    }
}

/// How long the 503 written by [`reject_unavailable`] may block the accept loop.
const REJECT_WRITE_TIMEOUT: Duration = Duration::from_millis(250);

/// Turn a connection away with a 503 carrying `body`, without occupying a
/// connection slot or a thread.
///
/// Runs on the accept loop, so it never waits on the client: it reads only the
/// bytes that have already arrived (non-blocking, no timeout, no sleep), then
/// writes the answer under [`REJECT_WRITE_TIMEOUT`]. A blocking drain here let
/// one client that trickles bytes stall every other accept.
///
/// Trade-off: a request that arrives after the single read pass leaves unread
/// bytes, which turns the close into an RST that may discard the 503. The
/// message is advisory, and the connection is refused either way.
pub(crate) fn reject_unavailable(stream: &mut TcpStream, body: &[u8]) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    read_available(stream);
    if stream.set_nonblocking(false).is_err()
        || stream
            .set_write_timeout(Some(REJECT_WRITE_TIMEOUT))
            .is_err()
    {
        return;
    }
    let written = http::write_response(
        stream,
        503,
        "Service Unavailable",
        "text/plain; charset=utf-8",
        body,
        true,
    );
    if written.is_ok() {
        let _ = stream.shutdown(Shutdown::Write);
    }
}

/// Discard what a non-blocking `stream` already holds, up to
/// [`MAX_DRAIN_BYTES`], stopping at `WouldBlock`, end of stream or an error.
fn read_available(stream: &mut TcpStream) {
    let mut chunk = [0_u8; 4096];
    let mut drained = 0;
    while drained < MAX_DRAIN_BYTES {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => drained += read,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// Drain the unread request bytes, then write a plain-text error response.
pub(crate) fn fail(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) {
    drain_pending(stream);
    let _ = http::write_response(
        stream,
        status,
        reason,
        "text/plain; charset=utf-8",
        body,
        true,
    );
}

/// Answer a routed request, omitting the body when the client sent HEAD.
pub(crate) fn respond(
    stream: &mut TcpStream,
    head: &RequestHead,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) {
    let _ = http::write_response(
        stream,
        status,
        reason,
        content_type,
        body,
        head.method != "HEAD",
    );
}
