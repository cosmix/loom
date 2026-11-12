//! The planning trace routes: the per-feature trace, settling a document, triaging a finding
//! and finding the commit that holds a document's Settled line.
//!
//! The write routes build the new text and hand it to `files::save::save_document`, so
//! authoring, the lock, the opened-hash check, locked documents and symlinks follow the save
//! route; a settle then queues the commit of what it wrote (`settle_queue`). This is not the
//! source-code graph (`loom map`).

pub mod changed;
mod settle_all;
mod settle_commit;
mod settle_head;
mod settle_job;
mod settle_queue;
mod settle_route;
mod status_route;
mod version_route;
mod view_route;

#[cfg(test)]
mod commit_route_tests;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod settle_all_contract_tests;
#[cfg(test)]
mod settle_all_tests;
#[cfg(test)]
mod settle_commit_tests;
#[cfg(test)]
mod settle_head_tests;
#[cfg(test)]
mod settle_job_tests;
#[cfg(test)]
mod status_tests;

use std::sync::Arc;

use super::context::PlanContext;
use super::routes::{BodyKind, Method, Role, RouteSpec};

/// Install the settle commit worker's queue.
pub fn install(ctx: &Arc<PlanContext>) {
    settle_queue::install(ctx);
}

/// `GET /api/trace/<feature>`, `POST /api/settle`, `POST /api/settle-all`,
/// `POST /api/findings/status` and `GET /api/settled-version`.
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
            path: "/api/settle-all",
            body: BodyKind::None,
            role: Role::HolderOrTake,
            handler: settle_all::settle_all_route,
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
