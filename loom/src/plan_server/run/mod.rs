//! Execution handoff and run management.

pub mod abandon;
pub mod changes;
pub mod child;
pub mod commit;
pub mod dashboard;
pub mod dashboard_parse;
pub mod install;
pub mod routes;
pub mod start;
pub mod state;
pub mod strip;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::plan_server::context::PlanContext;
use crate::plan_server::routes::RouteSpec;

pub fn routes() -> Vec<RouteSpec> {
    routes::all()
}

pub fn install(ctx: &Arc<PlanContext>) {
    install::run(ctx)
}
