//! The planning server's API route table.
//!
//! Every route module (`plan_server/<name>/mod.rs`) contributes `routes()` with one line in
//! [`all_routes`], and an optional `install(&Arc<PlanContext>)` with one line in
//! [`install_all`]. The background load runs `install_all` before any route handler is called.

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
    /// A raw body of one of `types` (compared without parameters, case-insensitively), at most
    /// `max` bytes. The gate checks only the declared type and length; the handler checks the
    /// bytes.
    Bytes {
        max: usize,
        types: &'static [&'static str],
    },
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
    routes.extend(super::files::routes());
    routes.extend(super::search::routes());
    routes.extend(super::features::routes());
    routes.extend(super::concept::routes());
    routes.extend(super::guidance::routes());
    routes.extend(super::review::routes());
    routes.extend(super::summaries::routes());
    routes.extend(super::pressure::routes());
    routes.extend(super::run::routes());
    routes.extend(super::uploads::routes());
    routes.extend(super::chat::routes());
    routes.extend(super::trace::routes());
    routes
}

/// Run every route module's `install`, one line per module that has one.
///
/// The presence and authoring hub hooks come first, so `hello` is the first frame every
/// WebSocket receives and later modules' `on_open` frames follow it.
pub fn install_all(ctx: &Arc<PlanContext>) {
    PlanContext::install_hub_hooks(ctx);
    super::files::install(ctx);
    super::search::install(ctx);
    super::concept::install(ctx);
    crate::plan_server::chat::install(ctx);
    // After concept (its `ConceptState`) and chat (the session `Bridge`): both are read at install.
    super::summaries::install(ctx);
    super::guidance::install(ctx);
    super::review::install(ctx);
    super::pressure::install(ctx);
    super::run::install(ctx);
    super::uploads::install(ctx);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn handler(_: &ApiRequest<'_>) -> ApiResponse {
        ApiResponse::error(500, "unused", "never routed")
    }

    fn route(method: Method, path: &'static str) -> RouteSpec {
        RouteSpec {
            method,
            path,
            body: BodyKind::None,
            role: Role::Viewer,
            handler,
        }
    }

    /// The first `(method, path)` pair that appears twice. `find` takes the first match, so a
    /// repeated pair would leave the later route unreachable without any error.
    fn first_duplicate(routes: &[RouteSpec]) -> Option<(Method, &'static str)> {
        routes.iter().enumerate().find_map(|(index, spec)| {
            routes[..index]
                .iter()
                .any(|earlier| earlier.method == spec.method && earlier.path == spec.path)
                .then_some((spec.method, spec.path))
        })
    }

    #[test]
    fn the_route_table_holds_each_method_and_path_once() {
        assert_eq!(first_duplicate(&all_routes()), None);
    }

    #[test]
    fn a_repeated_method_and_path_is_reported_but_a_second_method_is_not() {
        let routes = [
            route(Method::Get, "/api/a"),
            route(Method::Post, "/api/a"),
            route(Method::Get, "/api/b"),
            route(Method::Get, "/api/a"),
        ];
        assert_eq!(first_duplicate(&routes), Some((Method::Get, "/api/a")));
        assert_eq!(first_duplicate(&routes[..3]), None);
    }
}
