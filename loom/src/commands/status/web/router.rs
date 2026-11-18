//! The dashboard's [`Router`]: the cookie bootstrap and the two WebSocket
//! lanes on the peeked head, then ordinary HTTP routing in `connection`.

use std::io;
use std::net::TcpStream;
use std::path::PathBuf;

use super::broadcast::Broadcaster;
use super::connection;
use super::terminal;
use super::TerminalLane;
use crate::web_server::access::OriginRequirement;
use crate::web_server::auth::Auth;
use crate::web_server::bootstrap::bootstrap_cookie;
use crate::web_server::connection::fail;
use crate::web_server::http::RequestHead;
use crate::web_server::limits::{Lane, Slot};
use crate::web_server::{ws, ConnInfo, Router};

/// Everything the dashboard's routes read, fixed for the life of one server.
pub(super) struct DashboardRouter {
    pub(super) broadcaster: Broadcaster,
    pub(super) base: PathBuf,
    pub(super) lane: Option<TerminalLane>,
    /// The terminal lane's token and cookie, for the bootstrap a loopback
    /// dashboard runs when terminals are enabled.
    lane_auth: Option<Auth>,
}

impl DashboardRouter {
    pub(super) fn new(broadcaster: Broadcaster, base: PathBuf, lane: Option<TerminalLane>) -> Self {
        let lane_auth = lane
            .as_ref()
            .map(|lane| Auth::new(lane.token.clone(), lane.cookie_name.clone()));
        Self {
            broadcaster,
            base,
            lane,
            lane_auth,
        }
    }

    /// Upgrade an accepted `/ws` connection, or reject it if the origin check
    /// or the WebSocket sub-cap turns it away. `stream`'s head was only
    /// peeked, not consumed; on success that leaves the handshake bytes unread
    /// for `tungstenite::accept` to parse itself.
    fn handle_websocket_upgrade(
        &self,
        mut stream: TcpStream,
        peeked: &RequestHead,
        conn: &ConnInfo<'_>,
    ) {
        // Remote mode requires Origin on this lane; loopback keeps its existing
        // lenient rule, matching a same-origin page that never sends one.
        let requirement = if conn.policy.is_remote() {
            OriginRequirement::Required
        } else {
            OriginRequirement::Optional
        };
        if !conn.policy.origin_allowed(conn.local, peeked, requirement) {
            fail(&mut stream, 403, "Forbidden", b"origin not allowed");
            return;
        }
        // Held until this subscription ends, so open tabs cannot consume the
        // connection slots ordinary requests need.
        let Some(_slot) = Slot::acquire(conn.limits, Lane::WebSocket) else {
            fail(
                &mut stream,
                503,
                "Service Unavailable",
                b"dashboard subscription limit reached",
            );
            return;
        };
        if stream.set_read_timeout(None).is_ok() {
            ws::handle(
                stream,
                self.broadcaster.subscribe(),
                conn.running,
                conn.limits,
            );
        }
    }
}

impl Router for DashboardRouter {
    /// Exchange a bootstrap token for the dashboard cookie, or route an
    /// upgrade request to the terminal or snapshot WebSocket lane. Hands the
    /// stream back for anything else.
    ///
    /// No dashboard cookie is checked here: viewing needs none on any bind.
    /// The routes that change something check it themselves - the terminal
    /// lane in `terminal::upgrade::admit`, `/api/config` writes in
    /// `config_api`.
    fn upgrade(
        &self,
        mut stream: TcpStream,
        peeked: &RequestHead,
        conn: &ConnInfo<'_>,
    ) -> Option<TcpStream> {
        let auth = conn.policy.auth().or(self.lane_auth.as_ref());
        if bootstrap_cookie(&mut stream, peeked, conn.policy, conn.local, auth) {
            return None;
        }
        if peeked.upgrade_websocket && peeked.path.starts_with("/ws/terminal/") {
            terminal::handle_upgrade(
                stream,
                peeked,
                &self.base,
                self.lane.as_ref(),
                conn.running,
                conn.limits,
                conn.policy,
                conn.local,
            );
            return None;
        }
        if peeked.path == "/ws" && peeked.upgrade_websocket {
            self.handle_websocket_upgrade(stream, peeked, conn);
            return None;
        }
        Some(stream)
    }

    fn handle(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        body_prefix: Vec<u8>,
        conn: &ConnInfo<'_>,
    ) -> io::Result<()> {
        connection::dispatch(self, stream, head, body_prefix, conn);
        Ok(())
    }
}
