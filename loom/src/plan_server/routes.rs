//! The planning server's API route table.
//!
//! Every route module (`plan_server/<name>/mod.rs`) contributes `routes()` with one line in
//! [`all_routes`], and an optional `install(&Arc<PlanContext>)` with one line in
//! [`install_all`]. `start` runs `install_all` before the first request is accepted.

use std::sync::Arc;

use super::context::PlanContext;
use super::request::{ApiRequest, ApiResponse};

/// The HTTP methods an API route answers. `HEAD` is answered by `GET` routes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// What a `POST` route accepts as its body. The gate refuses a wrong `Content-Type` (415) or
/// a `Content-Length` over the limit (413) before any body byte is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// No body; a non-zero `Content-Length` is refused.
    None,
    /// `application/json`, at most `web_server::http::MAX_BODY_BYTES` (8 KiB).
    Json,
    /// `text/plain; charset=utf-8`, at most `max` bytes.
    Text { max: usize },
}

/// Who may call a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Every browser holding the access cookie.
    Viewer,
    /// Only the authoring holder.
    Holder,
    /// The holder, or any browser when nobody holds authoring (it takes authoring).
    HolderOrTake,
}

/// One API route. `path` is exact unless it ends in `/`, which makes it a prefix.
#[derive(Clone, Copy)]
pub struct RouteSpec {
    pub method: Method,
    pub path: &'static str,
    pub body: BodyKind,
    pub role: Role,
    pub handler: fn(&ApiRequest<'_>) -> ApiResponse,
}

impl RouteSpec {
    fn matches_path(&self, path: &str) -> bool {
        if self.path.ends_with('/') {
            path.starts_with(self.path)
        } else {
            path == self.path
        }
    }
}

/// Every API route of the planning server, one line per route module.
pub fn all_routes() -> Vec<RouteSpec> {
    let mut routes = Vec::new();
    routes.extend(super::server_info::routes());
    routes
}

/// Run every route module's `install`, one line per module that has one.
///
/// The presence and authoring hub hooks come first, so `hello` is the first frame every
/// WebSocket receives and later modules' `on_open` frames follow it.
pub fn install_all(ctx: &Arc<PlanContext>) {
    PlanContext::install_hub_hooks(ctx);
}

/// The outcome of looking a request up in the route table.
pub(crate) enum Lookup<'a> {
    Found(&'a RouteSpec),
    /// The path names a route, but not for this method.
    WrongMethod,
    Missing,
}

/// Find the route for `method` and `path`; `HEAD` looks up `GET` routes.
pub(crate) fn find<'a>(routes: &'a [RouteSpec], method: &str, path: &str) -> Lookup<'a> {
    let wanted = match method {
        "GET" | "HEAD" => Some(Method::Get),
        "POST" => Some(Method::Post),
        _ => None,
    };
    let mut on_path = routes
        .iter()
        .filter(|spec| spec.matches_path(path))
        .peekable();
    if on_path.peek().is_none() {
        return Lookup::Missing;
    }
    match on_path.find(|spec| Some(spec.method) == wanted) {
        Some(spec) => Lookup::Found(spec),
        None => Lookup::WrongMethod,
    }
}
