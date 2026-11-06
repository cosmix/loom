//! The planning trace routes: the per-feature trace, settling a document, triaging a finding
//! and finding the commit that holds a document's Settled line.
//!
//! The write routes build the new text and hand it to `files::save::save_document`, so
//! authoring, the lock, the opened-hash check, locked documents and symlinks follow the save
//! route. This is not the source-code graph (`loom map`).

pub mod changed;
mod settle_route;
mod status_route;
mod version_route;
mod view_route;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod status_tests;

use super::routes::{BodyKind, Method, Role, RouteSpec};

/// `GET /api/trace/<feature>`, `POST /api/settle`, `POST /api/findings/status` and
/// `GET /api/settled-version`.
pub fn routes() -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            method: Method::Get,
            path: view_route::PREFIX,
            body: BodyKind::None,
            role: Role::Viewer,
            handler: view_route::trace_route,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/settle",
            body: BodyKind::None,
            role: Role::HolderOrTake,
            handler: settle_route::settle_route,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/findings/status",
            body: BodyKind::Json,
            role: Role::HolderOrTake,
            handler: status_route::status_route,
        },
        RouteSpec {
            method: Method::Get,
            path: "/api/settled-version",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: version_route::version_route,
        },
    ]
}
