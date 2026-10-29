//! Install execution state before starting the strip's polling thread.

use std::sync::Arc;

use crate::plan_server::context::PlanContext;

pub fn run(ctx: &Arc<PlanContext>) {
    super::child::install_jobs(ctx);
    super::commit::install(ctx);
    super::start::install(ctx);
    super::dashboard::install_dashboard(ctx);
    super::strip::spawn_strip(Arc::clone(ctx));
}
