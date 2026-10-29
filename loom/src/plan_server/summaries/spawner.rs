//! The real `SessionSpawner` over the session bridge.

use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context};

use super::scheduler::SessionSpawner;
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_session::bridge::{Bridge, EndCause, EndMode, GuidanceFn, StartRequest};
use crate::plan_session::kind::SessionKind;
use crate::planning::guidance::{render_session_guidance, GuidanceModels, SessionGuidanceKind};
use crate::planning::summary::claims::clear_session_claims;

/// Starts summary sessions through the bridge in `ctx.extensions()`.
pub struct RealSpawner {
    ctx: Weak<PlanContext>,
}

impl RealSpawner {
    pub fn new(ctx: &Arc<PlanContext>) -> Self {
        RealSpawner {
            ctx: Arc::downgrade(ctx),
        }
    }

    fn parts(&self) -> anyhow::Result<(Arc<PlanContext>, Arc<Bridge>)> {
        let ctx = self.ctx.upgrade().context("the planning server is gone")?;
        let bridge = ctx.extensions().get::<Bridge>();
        let bridge = bridge.context("the planning server runs no session bridge")?;
        Ok((ctx, bridge))
    }
}

impl SessionSpawner for RealSpawner {
    /// A summary session has no chat: no offer, no opening origin. Plan 8 renders its guidance
    /// from a snapshot of the project and the server's runtime directory, never re-resolved
    /// (`LOOM_PLAN_STATE` can move it).
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        let (ctx, bridge) = self.parts()?;
        // The bridge's drain final-writes the sessions it ends, so a session started after the
        // first stop signal would be cut short; the panel shows this message as the start error.
        if ctx.is_stopping() {
            return Err(anyhow!(
                "loom plan is stopping; no new summary session starts"
            ));
        }
        let project = ctx.project().clone();
        let runtime = ctx.runtime().clone();
        let root = ctx.root().to_path_buf();
        let kind = SessionGuidanceKind::Summary { ids: ids.to_vec() };
        let guidance: GuidanceFn = Box::new(move |dir| {
            let models = GuidanceModels::load(&root)?;
            render_session_guidance(&kind, &project, &runtime, dir, &models).map(Into::into)
        });
        let request = StartRequest {
            kind: SessionKind::Summary,
            guidance,
            opening_origin: None,
            offer: None,
            extra_env: Vec::new(),
        };
        bridge
            .start_session(request)
            .map_err(|error| anyhow!(error))
    }

    /// Tear the session down; a refusal is printed and the session counts as running until its
    /// `Ended` arrives. A summary session queues nothing, so the returned queue is dropped.
    fn stop(&self, session: &str, release_claims: bool) {
        let (ctx, bridge) = match self.parts() {
            Ok(parts) => parts,
            Err(error) => {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot stop session {}: {why}",
                    safe(session)
                ));
                return;
            }
        };
        if let Err(error) = bridge.end(session, EndMode::Teardown, EndCause::Caller) {
            let why = safe(&error.to_string());
            line(&format!(
                "summaries: cannot stop session {}: {why}",
                safe(session)
            ));
        }
        if release_claims {
            if let Err(error) = clear_session_claims(ctx.runtime(), session) {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot release the claims of {}: {why}",
                    safe(session)
                ));
            }
        }
    }
}
