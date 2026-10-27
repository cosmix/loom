//! Ordinary HTTP routing for one dashboard request whose head was read.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;

use super::broadcast::{self, Broadcaster};
use super::config_api;
use super::router::DashboardRouter;
use super::terminal::{serve_history, HISTORY_PREFIX};
use crate::web_server::access::{AccessPolicy, OriginRequirement};
use crate::web_server::assets;
use crate::web_server::connection::{fail, respond};
use crate::web_server::http::RequestHead;
use crate::web_server::ConnInfo;

/// Body returned when the work directory cannot produce a snapshot. The
/// underlying error names absolute work-directory paths, so it is logged
/// rather than served.
const SNAPSHOT_UNAVAILABLE: &[u8] = b"status snapshot unavailable";

/// The non-WebSocket target selected from a request path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    Api,
    /// The read/write config surface — the one route that accepts `POST`.
    Config,
    Asset {
        body: &'static [u8],
        mime: &'static str,
    },
    Missing,
    Spa,
}

/// Answer one request whose head was read, by method and route.
pub(super) fn dispatch(
    router: &DashboardRouter,
    stream: &mut TcpStream,
    head: &RequestHead,
    body_prefix: Vec<u8>,
    conn: &ConnInfo<'_>,
) {
    let broadcaster = &router.broadcaster;
    let (base, policy, local) = (router.base.as_path(), conn.policy, conn.local);
    // Routed before the method gate so `POST` opens for `/api/config` and
    // nothing else: every other path answers a `POST` with the same 405 it
    // always has.
    let route = route(&head.path);
    // Gated like the terminal lane, which `handle_route` knows nothing of.
    if head.path.starts_with(HISTORY_PREFIX) && matches!(head.method.as_str(), "GET" | "HEAD") {
        return serve_history(stream, head, base, router.lane.as_ref(), policy, local);
    }
    match head.method.as_str() {
        "GET" | "HEAD" => handle_route(stream, head, route, broadcaster, base, policy, local),
        "POST" if route == Route::Config => {
            config_api::handle_post(stream, head, body_prefix, base, policy, local)
        }
        _ => fail(stream, 405, "Method Not Allowed", b"GET required"),
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_route(
    stream: &mut TcpStream,
    head: &RequestHead,
    route: Route,
    broadcaster: &Broadcaster,
    base: &Path,
    policy: &AccessPolicy,
    local: SocketAddr,
) {
    match route {
        Route::Api => serve_api(stream, head, broadcaster, base, policy, local),
        Route::Config => config_api::serve_get(stream, head, base, policy, local),
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

fn serve_api(
    stream: &mut TcpStream,
    head: &RequestHead,
    broadcaster: &Broadcaster,
    base: &Path,
    policy: &AccessPolicy,
    local: SocketAddr,
) {
    if !policy.origin_allowed(local, head, OriginRequirement::Optional) {
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
        .unwrap_or_else(|| broadcast::fresh_file_snapshot(base, broadcaster.terminals()))
    {
        Ok(frame) => respond(
            stream,
            head,
            200,
            "OK",
            "application/json; charset=utf-8",
            frame.as_bytes(),
        ),
        Err(error) => {
            tracing::warn!("dashboard could not collect a status snapshot: {error}");
            respond(
                stream,
                head,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                SNAPSHOT_UNAVAILABLE,
            );
        }
    }
}

/// The response for the SPA entry page: the embedded page, or a 503 naming
/// what to build when the bundle is absent.
pub(super) fn index_response(
    page: Option<&'static [u8]>,
) -> (u16, &'static str, &'static str, &'static [u8]) {
    match page {
        Some(body) => (200, "OK", "text/html; charset=utf-8", body),
        None => (
            503,
            "Service Unavailable",
            "text/plain; charset=utf-8",
            b"dashboard assets are not embedded; build web/dist and rebuild loom",
        ),
    }
}

fn serve_index(stream: &mut TcpStream, head: &RequestHead) {
    let (status, reason, content_type, body) = index_response(assets::index_html());
    respond(stream, head, status, reason, content_type, body);
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
    } else if path == "/api/config" {
        Route::Config
    } else if path.starts_with("/assets/") || path.starts_with("/api/") || path.starts_with("/ws/")
    {
        Route::Missing
    } else {
        Route::Spa
    }
}
