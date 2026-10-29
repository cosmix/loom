//! Pressure review execution and configuration.

pub mod codex;
pub mod config_guard;
pub mod driver;
pub mod install;
pub mod options;
pub mod recover;
pub mod routes;
pub mod teammates;
#[cfg(test)]
mod tests;
pub mod view;

use std::sync::Arc;

use crate::plan_server::context::PlanContext;
use crate::plan_server::routes::RouteSpec;

pub fn routes() -> Vec<RouteSpec> {
    routes::all()
}

pub fn install(ctx: &Arc<PlanContext>) {
    install::run(ctx)
}
