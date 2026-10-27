//! The planning server's [`Router`]: `/ws` on the peeked head, then the token bootstrap, the
//! embedded app and the gated `/api/*` routes.

use std::io;
use std::net::TcpStream;
use std::sync::Arc;

use super::context::PlanContext;
use super::gate::{self, Admitted, Denied, WS_PATH};
use super::ids::{BrowserId, ConnCtx, PublicId};
use super::request::{self, ApiRequest, ApiResponse, QueryParams, REFERRER_POLICY};
use super::routes::{self, Lookup, Method, RouteSpec};
use super::wire::message::{ClientMessage, PlanMessage};
use super::wire::server::Refused;
use super::ws::{self, dispatch::dispatch, MAX_INBOUND_BYTES};
use crate::web_server::assets;
use crate::web_server::http::{self, RequestHead};
use crate::web_server::limits::{Lane, Slot};
use crate::web_server::{ConnInfo, Router};

/// The page every path outside `/api/`, `/assets/` and `/ws` serves.
const APP_PAGE: &str = "plan.html";
const NOT_EMBEDDED: &str = "planning assets are not embedded; build web/dist and rebuild loom";
const PLAIN: &str = "text/plain; charset=utf-8";

/// Routes one planning server's requests. The route table is fixed at start.
pub struct PlanRouter {
    ctx: Arc<PlanContext>,
    routes: Vec<RouteSpec>,
}

impl PlanRouter {
    pub fn new(ctx: Arc<PlanContext>, routes: Vec<RouteSpec>) -> Self {
        PlanRouter { ctx, routes }
    }

    fn port(&self) -> u16 {
        self.ctx.bind().local.port()
    }

    /// Admit and serve one `/ws` connection on the thread it arrived on.
    fn serve_websocket(&self, mut stream: TcpStream, peeked: &RequestHead, conn: &ConnInfo<'_>) {
        let admitted = match gate::admit(peeked, conn.local, conn.policy, &self.ctx, None) {
            Ok(admitted) => admitted,
            Err(denial) => return refuse(&mut stream, peeked, &denial),
        };
        if !peeked.upgrade_websocket {
            let response =
                ApiResponse::error(400, "bad_request", "a WebSocket upgrade is required");
            return response.write(&mut stream, peeked, None);
        }
        // Held for the life of the connection, so open tabs cannot use up the slots that
        // ordinary requests draw on.
        let Some(_slot) = Slot::acquire(conn.limits, Lane::WebSocket) else {
            let response = ApiResponse::error(503, "busy", "too many open planning tabs");
            return response.write(&mut stream, peeked, None);
        };
        if stream.set_read_timeout(None).is_err() {
            return;
        }
        let ctx = &self.ctx;
        let on_message = |conn: &ConnCtx, msg: ClientMessage| dispatch(ctx, conn, msg);
        let on_bad_frame = |conn: &ConnCtx, reason: &str| {
            let refused = Refused {
                command: "frame".to_owned(),
                reason: reason.to_owned(),
            };
            ctx.hub()
                .send_to_conn(conn.id, &PlanMessage::Refused(refused));
        };
        let served = ws::run_connection(
            stream,
            ctx.conn_ctx(admitted.browser),
            ctx.hub(),
            MAX_INBOUND_BYTES,
            conn.running,
            &on_message,
            &on_bad_frame,
        );
        if let Err(error) = served {
            tracing::debug!("planning WebSocket ended: {error}");
        }
    }

    /// `GET /?token=`: the right token sets both cookies and redirects to `/`; a wrong one is
    /// 403. False when the request is not a bootstrap (no `token` in the query).
    fn bootstrap(&self, stream: &mut TcpStream, head: &RequestHead, conn: &ConnInfo<'_>) -> bool {
        let Some(auth) = conn.policy.auth() else {
            return false;
        };
        let Some(exchanged) = auth.bootstrap_cookie(head.query.as_deref()) else {
            return false;
        };
        // A foreign Origin means this did not come from opening the printed link.
        let Some(access) = exchanged.filter(|_| gate::origin_matches(head, false)) else {
            let refusal = ApiResponse::text(403, PLAIN, "token not accepted");
            refusal.write(stream, head, None);
            return true;
        };
        let minted = match gate::browser_of(head, self.port()) {
            Some(_) => None,
            None => match BrowserId::mint() {
                Ok(id) => Some(gate::browser_cookie(self.port(), &id)),
                Err(error) => {
                    let message = format!("cannot mint a browser id: {error}");
                    ApiResponse::error(500, "internal", &message).write(stream, head, None);
                    return true;
                }
            },
        };
        let mut headers = vec![REFERRER_POLICY];
        if let Some(cookie) = &minted {
            headers.push(("Set-Cookie", cookie.as_str()));
        }
        if let Err(error) = http::write_redirect(stream, "/", Some(access.as_str()), &headers) {
            tracing::debug!("planning bootstrap redirect failed: {error}");
        }
        true
    }

    /// The embedded app: `/assets/<key>`, or the app page for every other path.
    fn serve_static(&self, stream: &mut TcpStream, head: &RequestHead, conn: &ConnInfo<'_>) {
        if !matches!(head.method.as_str(), "GET" | "HEAD") {
            let response = ApiResponse::text(405, PLAIN, "GET required");
            return response.write(stream, head, None);
        }
        if head.path == "/" && self.bootstrap(stream, head, conn) {
            return;
        }
        let (status, mime, body) = if head.path.starts_with("/assets/") {
            match assets::lookup(&head.path) {
                Some((body, mime)) => (200, mime, body),
                None => (404, PLAIN, b"not found".as_slice()),
            }
        } else {
            match assets::page(APP_PAGE) {
                Some(body) => (200, "text/html; charset=utf-8", body),
                None => (503, PLAIN, NOT_EMBEDDED.as_bytes()),
            }
        };
        request::write_response(stream, head, status, mime, body, None);
    }

    /// Gate, route and answer one `/api/*` request.
    fn serve_api(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        body: Vec<u8>,
        conn: &ConnInfo<'_>,
    ) {
        let lookup = routes::find(&self.routes, &head.method, &head.path);
        let spec = match lookup {
            Lookup::Found(spec) => Some(spec),
            Lookup::WrongMethod | Lookup::Missing => None,
        };
        let admitted = match gate::admit(head, conn.local, conn.policy, &self.ctx, spec) {
            Ok(admitted) => admitted,
            Err(denial) => return refuse(stream, head, &denial),
        };
        let response = match lookup {
            Lookup::Found(spec) => self.call(stream, head, body, &admitted, spec),
            Lookup::WrongMethod => {
                ApiResponse::error(405, "method_not_allowed", "wrong method for this route")
            }
            Lookup::Missing => ApiResponse::error(404, "not_found", "no such API route"),
        };
        response.write(stream, head, admitted.set_cookie.as_deref());
    }

    fn call(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        prefix: Vec<u8>,
        admitted: &Admitted,
        spec: &RouteSpec,
    ) -> ApiResponse {
        let query = match QueryParams::parse(head.query.as_deref()) {
            Ok(query) => query,
            Err(response) => return response,
        };
        let body = match read_request_body(stream, head, prefix, spec) {
            Ok(body) => body,
            Err(response) => return response,
        };
        // A minted browser id belongs to a request that presented no browser cookie; recording
        // it would add a permanent presence entry and a Viewer number per such request. Its
        // public id is a throwaway, the one every part of this response uses.
        let public_id = if admitted.minted() {
            PublicId::mint()
        } else {
            self.ctx.presence().public_id(&admitted.browser)
        };
        let request = ApiRequest {
            ctx: &self.ctx,
            head,
            browser: &admitted.browser,
            public_id: &public_id,
            query,
            body,
        };
        (spec.handler)(&request)
    }
}

/// The body of a `POST`, whose length the gate already checked; `GET` routes read none.
fn read_request_body(
    stream: &mut TcpStream,
    head: &RequestHead,
    prefix: Vec<u8>,
    spec: &RouteSpec,
) -> Result<Vec<u8>, ApiResponse> {
    if spec.method != Method::Post {
        return Ok(Vec::new());
    }
    let length = gate::content_length(head)
        .map_err(|denial| ApiResponse::error(denial.status, denial.code, &denial.message))?;
    request::read_body(stream, prefix, length)
        .map_err(|error| ApiResponse::error(400, "bad_request", &format!("{error:#}")))
}

fn refuse(stream: &mut TcpStream, head: &RequestHead, denial: &Denied) {
    ApiResponse::error(denial.status, denial.code, &denial.message).write(stream, head, None);
}

impl Router for PlanRouter {
    /// `/ws` needs its handshake bytes unread for `tungstenite`, so it is served here, on the
    /// peeked head. Everything else is handed back to be read.
    fn upgrade(
        &self,
        stream: TcpStream,
        peeked: &RequestHead,
        conn: &ConnInfo<'_>,
    ) -> Option<TcpStream> {
        if peeked.path != WS_PATH {
            return Some(stream);
        }
        self.serve_websocket(stream, peeked, conn);
        None
    }

    fn handle(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        body_prefix: Vec<u8>,
        conn: &ConnInfo<'_>,
    ) -> io::Result<()> {
        if head.path == "/api" || head.path.starts_with("/api/") {
            self.serve_api(stream, head, body_prefix, conn);
        } else {
            self.serve_static(stream, head, conn);
        }
        Ok(())
    }
}
