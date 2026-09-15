//! Routing an already-peeked request to a WebSocket upgrade, and the one
//! unauthenticated remote success: exchanging a bootstrap query token for the
//! dashboard's port cookie. Split out of `connection` to keep that file under
//! its line budget once both lanes became policy-aware.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::access::{AccessPolicy, OriginRequirement};
use super::broadcast::Broadcaster;
use super::connection::fail;
use super::http::{self, RequestHead};
use super::limits::{Lane, Limits, Slot};
use super::terminal;
use super::ws;
use super::TerminalLane;

/// Route an already-peeked upgrade request to the terminal or dashboard
/// WebSocket lane. Returns the stream back when `peeked` was not an upgrade
/// after all, so the caller can fall through to ordinary HTTP routing.
#[allow(clippy::too_many_arguments)]
pub(super) fn route_upgrade(
    stream: TcpStream,
    peeked: &RequestHead,
    broadcaster: &Broadcaster,
    base: &Path,
    running: &AtomicBool,
    limits: &Arc<Limits>,
    lane: Option<&TerminalLane>,
    policy: &AccessPolicy,
    local: SocketAddr,
) -> Option<TcpStream> {
    if peeked.upgrade_websocket && peeked.path.starts_with("/ws/terminal/") {
        terminal::handle_upgrade(stream, peeked, base, lane, running, limits, policy, local);
        return None;
    }
    if peeked.path == "/ws" && peeked.upgrade_websocket {
        handle_websocket_upgrade(stream, peeked, broadcaster, running, limits, policy, local);
        return None;
    }
    Some(stream)
}

/// Handle the one unauthenticated remote success: `GET`/`HEAD /?token=`.
/// Loopback mode reaches this too, when terminals are enabled, exactly as
/// before.
///
/// Remote mode validates `Origin` here, ahead of the token exchange: an
/// absent `Origin` is fine (this is the same navigation a bootstrap link
/// produces), but a supplied foreign one means the request did not come from
/// typing the tokenized URL into a browser, and must be refused before the
/// token is even looked at.
pub(super) fn bootstrap_terminal_token(
    stream: &mut TcpStream,
    head: &RequestHead,
    policy: &AccessPolicy,
    lane: Option<&TerminalLane>,
    local: SocketAddr,
) -> bool {
    if head.path != "/" || !matches!(head.method.as_str(), "GET" | "HEAD") {
        return false;
    }
    if policy.is_remote() && !policy.origin_allowed(local, head, OriginRequirement::Optional) {
        fail(stream, 403, "Forbidden", b"origin not allowed");
        return true;
    }
    let cookie = match policy.dashboard_auth() {
        Some(auth) => auth.bootstrap_cookie(head.query.as_deref()),
        None => lane.and_then(|lane| lane.bootstrap_cookie(head.query.as_deref())),
    };
    let Some(cookie) = cookie else {
        return false;
    };
    let Some(cookie) = cookie else {
        fail(stream, 403, "Forbidden", b"token not accepted");
        return true;
    };
    let _ = http::write_redirect(stream, "/", Some(&cookie));
    true
}

/// Upgrade an accepted `/ws` connection, or reject it if the origin check or
/// the WebSocket sub-cap turns it away. `stream`'s head was only peeked, not
/// consumed, above; on success that leaves the handshake bytes unread for
/// `tungstenite::accept` to parse itself.
fn handle_websocket_upgrade(
    mut stream: TcpStream,
    peeked: &RequestHead,
    broadcaster: &Broadcaster,
    running: &AtomicBool,
    limits: &Arc<Limits>,
    policy: &AccessPolicy,
    local: SocketAddr,
) {
    // Remote mode requires Origin on this lane; loopback keeps its existing
    // lenient rule, matching a same-origin page that never sends one.
    let requirement = if policy.is_remote() {
        OriginRequirement::Required
    } else {
        OriginRequirement::Optional
    };
    if !policy.origin_allowed(local, peeked, requirement) {
        fail(&mut stream, 403, "Forbidden", b"origin not allowed");
        return;
    }
    // Held until this subscription ends, so open tabs cannot consume the
    // connection slots ordinary requests need.
    let Some(_slot) = Slot::acquire(limits, Lane::WebSocket) else {
        fail(
            &mut stream,
            503,
            "Service Unavailable",
            b"dashboard subscription limit reached",
        );
        return;
    };
    if stream.set_read_timeout(None).is_ok() {
        ws::handle(stream, broadcaster.subscribe(), running);
    }
}
