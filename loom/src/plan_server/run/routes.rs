//! Execution handoff routes and their access requirements.

use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};

pub fn all() -> Vec<RouteSpec> {
    let mut routes = vec![
        RouteSpec {
            method: Method::Get,
            path: "/api/run/state",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: super::state::get_state,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/run/commit",
            body: BodyKind::Json,
            role: Role::Holder,
            handler: super::commit::post_commit,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/run/start",
            body: BodyKind::Json,
            role: Role::Holder,
            handler: super::start::post_start,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/run/dashboard",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: super::dashboard::post_dashboard,
        },
    ];
    routes.extend(super::abandon::routes());
    routes
}
