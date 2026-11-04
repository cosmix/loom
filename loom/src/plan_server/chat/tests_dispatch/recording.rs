//! A guidance provider that records every start it is asked to render.

use std::sync::{Arc, Mutex};

use super::super::guidance::{FixedGuidance, GuidanceProvider, GuidanceSlot};
use super::super::tests::Fixture;
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::SessionDir;

type GuidanceCall = (Option<String>, Option<String>, Option<String>);

#[derive(Default)]
pub(super) struct RecordingGuidance {
    pub(super) calls: Mutex<Vec<GuidanceCall>>,
}

impl GuidanceProvider for RecordingGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> anyhow::Result<LaunchGuidance> {
        self.calls.lock().unwrap().push((
            offer.map(str::to_owned),
            first_message.map(str::to_owned),
            feature.map(str::to_owned),
        ));
        FixedGuidance.authoring(ctx, dir, offer, first_message, feature)
    }

    fn closing_message(&self) -> String {
        FixedGuidance.closing_message()
    }
}

pub(super) fn record_guidance(f: &Fixture) -> Arc<RecordingGuidance> {
    let provider = Arc::new(RecordingGuidance::default());
    f.ctx
        .extensions()
        .get::<GuidanceSlot>()
        .unwrap()
        .set(provider.clone());
    provider
}
