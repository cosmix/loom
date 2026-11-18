//! `GET /api/server` (what the app shows about this server and the caller) and
//! `GET /api/share` (the tokenized link and where it works).

use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
use super::wire::server::{BindKind, ShareLink};

#[cfg(test)]
mod tests;

pub fn routes() -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            method: Method::Get,
            path: "/api/server",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: server,
        },
        RouteSpec {
            method: Method::Get,
            path: "/api/share",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: share,
        },
    ]
}

fn server(req: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(200, &req.ctx.server_info(req.browser, req.public_id))
}

/// The first bind URL and whether it is loopback or remote.
fn share(req: &ApiRequest<'_>) -> ApiResponse {
    let bind = req.ctx.bind();
    let link = ShareLink {
        url: bind.urls.first().cloned().unwrap_or_default(),
        bind: if bind.remote {
            BindKind::Remote
        } else {
            BindKind::Loopback
        },
    };
    ApiResponse::json(200, &link)
}
