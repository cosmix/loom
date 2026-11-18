//! Planning guidance installed into the chat bridge and exposed as session briefs.

mod brief_route;
mod provider;

#[cfg(test)]
pub(crate) use brief_route::SessionBriefView;
pub(crate) use provider::PlanGuidance;

use std::sync::Arc;

use super::chat::guidance::GuidanceSlot;
use super::context::PlanContext;
use super::routes::RouteSpec;
use crate::fs::work_dir::PlanConfig;
use crate::planning::guidance::GuidanceModels;
use crate::user_config::UserConfig;

/// Read-only session guidance routes.
pub fn routes() -> Vec<RouteSpec> {
    vec![brief_route::route()]
}

/// Replace the chat provider after chat installation has created its guidance slot.
pub fn install(ctx: &Arc<PlanContext>) {
    let Some(slot) = ctx.extensions().get::<GuidanceSlot>() else {
        tracing::warn!("planning guidance is unavailable: chat installed no guidance slot");
        return;
    };
    let models = GuidanceModels::load(ctx.root()).unwrap_or_else(|error| {
        tracing::warn!("cannot load planning guidance models; using defaults: {error}");
        GuidanceModels::resolve(&PlanConfig::default(), &UserConfig::default())
    });
    let provider = Arc::new(PlanGuidance::new(models));
    slot.set(Arc::clone(&provider) as Arc<dyn super::chat::guidance::GuidanceProvider>);
    ctx.extensions().insert(provider);
}

#[cfg(test)]
mod tests;
