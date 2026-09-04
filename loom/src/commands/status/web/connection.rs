//! Routing for one dashboard HTTP or WebSocket connection.

use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::assets;
use super::broadcast::{self, Broadcaster};
use super::http::{self, RequestHead, MAX_HEAD_BYTES};
use super::ws;

/// How long a single peek may block, bounding how long a connection thread
/// ignores a shutdown request.
const PEEK_TIMEOUT: Duration = Duration::from_millis(250);

/// Total budget for a client to deliver a complete request head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Bytes drained from an unfinished request before an error response.
const MAX_DRAIN_BYTES: usize = 64 * 1024;

/// The non-WebSocket target selected from a request path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    Api,
    Asset {
        body: &'static [u8],
        mime: &'static str,
    },
    Missing,
    Spa,
}

/// Handle one accepted connection to completion, or until `running` clears.
pub fn handle(mut stream: TcpStream, broadcaster: &Broadcaster, base: &Path, running: &AtomicBool) {
    if stream.set_read_timeout(Some(PEEK_TIMEOUT)).is_err() {
        return;
    }
    let Some(peeked) = complete_head(&mut stream, running) else {
        return;
    };
    if peeked.path == "/ws" && peeked.upgrade_websocket {
        handle_websocket_upgrade(stream, &peeked, broadcaster, running);
        return;
    }

    let Ok(head) = http::read_head(&mut stream) else {
        fail(&mut stream, 400, "Bad Request", b"bad request");
        return;
    };
    if !matches!(head.method.as_str(), "GET" | "HEAD") {
        fail(&mut stream, 405, "Method Not Allowed", b"GET required");
        return;
    }
    handle_route(&mut stream, &head, broadcaster, base);
}

/// Upgrade an accepted `/ws` connection, or reject it if the origin check
/// fails. `stream`'s head was only peeked, not consumed, above; on success
/// that leaves the handshake bytes unread for `tungstenite::accept` to
/// parse itself.
fn handle_websocket_upgrade(
    mut stream: TcpStream,
    peeked: &RequestHead,
    broadcaster: &Broadcaster,
    running: &AtomicBool,
) {
    if !http::origin_allowed(peeked.origin.as_deref()) {
        fail(&mut stream, 403, "Forbidden", b"origin not allowed");
        return;
    }
    if stream.set_read_timeout(None).is_ok() {
        ws::handle(stream, broadcaster.subscribe(), running);
    }
}

/// Read and discard whatever the client has already sent, up to
/// [`MAX_DRAIN_BYTES`].
///
/// Every caller reaches this with request bytes deliberately left unconsumed:
/// the head was peeked rather than read, or the head parsed but a body
/// followed it. Closing a socket with unread bytes still in the receive buffer
/// makes Linux answer with RST instead of FIN, which discards the response we
/// just wrote before it reaches the client. Draining first makes the close a
/// clean FIN, so the error status actually arrives.
fn drain_pending(stream: &mut TcpStream) {
    if stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .is_err()
    {
        return;
    }
    let mut chunk = [0_u8; 4096];
    let mut drained = 0;
    while drained < MAX_DRAIN_BYTES {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => drained += read,
        }
    }
}

/// Drain the unread request bytes, then write a plain-text error response.
fn fail(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) {
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

/// Outcome of one peek-and-parse attempt for a request head.
enum HeadAttempt {
    /// The peeked bytes hold a complete head.
    Ready(RequestHead),
    /// The head is still partial; peek again after a short sleep.
    Retry,
    /// The head can never complete (too large, timed out, or malformed); an
    /// HTTP error response has already been written to `stream`.
    Failed,
}

/// Write a plain-text HTTP error response and report the head as failed.
fn fail_head(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) -> HeadAttempt {
    fail(stream, status, reason, body);
    HeadAttempt::Failed
}

/// Peek up to `MAX_HEAD_BYTES` without consuming them, so `/ws` can later hand
/// the socket to `tungstenite::accept` with the handshake bytes still unread,
/// and classify the result as complete, retryable, or a terminal failure.
fn peek_head(stream: &mut TcpStream, buffer: &mut [u8], started: Instant) -> HeadAttempt {
    match stream.peek(buffer) {
        Ok(0) => {
            tracing::debug!("dashboard client closed before sending a request head");
            HeadAttempt::Failed
        }
        Ok(read) => match http::parse_head(&buffer[..read]) {
            Ok(Some(head)) => HeadAttempt::Ready(head),
            Ok(None) if read >= MAX_HEAD_BYTES => fail_head(
                stream,
                431,
                "Request Header Fields Too Large",
                b"request head too large",
            ),
            Ok(None) if started.elapsed() >= HEAD_TIMEOUT => {
                fail_head(stream, 408, "Request Timeout", b"request head timed out")
            }
            Ok(None) => HeadAttempt::Retry,
            Err(error) => {
                tracing::debug!("dashboard request head was invalid: {error}");
                fail_head(stream, 400, "Bad Request", b"bad request")
            }
        },
        // A client that connects and then says nothing times out every peek;
        // once the budget is spent it gets the 408 this branch advertises.
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            if started.elapsed() < HEAD_TIMEOUT {
                HeadAttempt::Retry
            } else {
                fail_head(stream, 408, "Request Timeout", b"request head timed out")
            }
        }
        Err(error) => {
            tracing::debug!("dashboard request peek failed: {error}");
            HeadAttempt::Failed
        }
    }
}

/// Peek-retry until the request head is complete, times out, overflows the
/// head budget, or fails to parse; on any terminal outcome besides success,
/// the caller has already had an HTTP error response written for it.
fn complete_head(stream: &mut TcpStream, running: &AtomicBool) -> Option<RequestHead> {
    let started = Instant::now();
    let mut buffer = [0_u8; MAX_HEAD_BYTES];
    while running.load(Ordering::SeqCst) {
        match peek_head(stream, &mut buffer, started) {
            HeadAttempt::Ready(head) => return Some(head),
            HeadAttempt::Retry => std::thread::sleep(Duration::from_millis(5)),
            HeadAttempt::Failed => return None,
        }
    }
    None
}

fn handle_route(
    stream: &mut TcpStream,
    head: &RequestHead,
    broadcaster: &Broadcaster,
    base: &Path,
) {
    match route(&head.path) {
        Route::Api => serve_api(stream, head, broadcaster, base),
        Route::Asset { body, mime } => respond(stream, head, 200, "OK", mime, body),
        Route::Missing => respond(
            stream,
            head,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"not found",
        ),
        Route::Spa => serve_index(stream, head),
    }
}

fn serve_api(stream: &mut TcpStream, head: &RequestHead, broadcaster: &Broadcaster, base: &Path) {
    if !http::origin_allowed(head.origin.as_deref()) {
        respond(
            stream,
            head,
            403,
            "Forbidden",
            "text/plain; charset=utf-8",
            b"origin not allowed",
        );
        return;
    }
    match broadcaster
        .latest()
        .map(|frame| (*frame).clone())
        .map(Ok)
        .unwrap_or_else(|| broadcast::fresh_file_snapshot(base))
    {
        Ok(frame) => respond(
            stream,
            head,
            200,
            "OK",
            "application/json; charset=utf-8",
            frame.as_bytes(),
        ),
        Err(error) => respond(
            stream,
            head,
            500,
            "Internal Server Error",
            "text/plain; charset=utf-8",
            error.to_string().as_bytes(),
        ),
    }
}

fn serve_index(stream: &mut TcpStream, head: &RequestHead) {
    match assets::index_html() {
        Some(body) => respond(stream, head, 200, "OK", "text/html; charset=utf-8", body),
        None => respond(
            stream,
            head,
            503,
            "Service Unavailable",
            "text/plain; charset=utf-8",
            b"dashboard assets are not embedded; build web/dist and rebuild loom",
        ),
    }
}

/// Answer a routed request, omitting the body when the client sent HEAD.
fn respond(
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

/// Resolve `path` to an embedded asset, leaving `/` to the SPA branch.
fn asset_for(path: &str) -> Option<(&'static [u8], &'static str)> {
    if path == "/" {
        return None;
    }
    assets::lookup(path)
}

/// Select an ordinary HTTP route, with assets taking precedence over SPA fallback.
pub(super) fn route(path: &str) -> Route {
    if path.split('/').any(|segment| segment == "..") {
        // Nothing leaks - asset lookup never touches the filesystem - but
        // answering a traversal probe with the SPA's 200 is a scanner flag.
        Route::Missing
    } else if let Some((body, mime)) = asset_for(path) {
        Route::Asset { body, mime }
    } else if path == "/api/status" {
        Route::Api
    } else if path.starts_with("/assets/") || path.starts_with("/api/") {
        Route::Missing
    } else {
        Route::Spa
    }
}
