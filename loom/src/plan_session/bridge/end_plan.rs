//! The end plan of a session: what makes its end due and how a later request hardens it.

use std::time::{Duration, Instant};

use super::session::{Closing, EndPlan, Phase, Session};
use super::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::DenyReason;
use crate::plan_session::state::Input;

impl Session {
    /// Ready to close, a teardown asked for, the final-write bound passed (the end's own bound
    /// when it set one, else `bound`), never launched, its opening never delivered (no turn ran,
    /// so there is no task to finish and no closing message to paste), or the server stopping:
    /// the outcome and the reason pending requests are denied with.
    pub(super) fn end_due(
        &mut self,
        stopping: bool,
        bound: Duration,
    ) -> Option<(EndOutcome, DenyReason)> {
        if !matches!(self.phase, Phase::Live | Phase::Spawning(_)) {
            return None;
        }
        let plan = self.flow.end.as_ref();
        let reason = match stopping {
            true => DenyReason::ServerStopping,
            false => plan.map_or(DenyReason::SessionEnded, |plan| plan.reason),
        };
        if let Some(outcome) = self.flow.close_ready.take() {
            return Some((outcome, reason));
        }
        let unlaunched = matches!(self.phase, Phase::Spawning(_));
        let unopened = self.flow.opening.is_some();
        let due = match plan {
            Some(plan) => {
                let late = plan.requested_at.elapsed() >= plan.bound.unwrap_or(bound);
                plan.mode == EndMode::Teardown || unlaunched || unopened || late
            }
            None => stopping,
        };
        due.then_some((EndOutcome::TornDown, reason))
    }

    /// The first end request sets the plan; a later one may only harden it.
    /// Agent ends use `FinalWrite` with `Skip` to retain the close gate and its timeout bound.
    pub(super) fn plan_end(
        &mut self,
        mut mode: EndMode,
        cause: EndCause,
        deny_new: Option<(DenyReason, &'static str)>,
    ) {
        let reason = deny_new.map_or(DenyReason::SessionEnded, |(reason, _)| reason);
        if let Some(plan) = &mut self.flow.end {
            if mode == EndMode::Teardown {
                plan.mode = mode;
            }
            if deny_new.is_some() {
                (plan.deny_new, plan.reason) = (deny_new, reason);
            }
            return;
        }
        self.machine.apply(Input::EndRequested);
        // The closing write needs something to write: a session the user never wrote in
        // (and that was not recovered, whose transcript is not read back yet) is torn down.
        let writable = self.flow.recovered.is_some() || self.user_wrote();
        let closing = match self.kind {
            SessionKind::Authoring if cause == EndCause::Agent => Closing::Skip,
            SessionKind::Authoring if writable => Closing::NotSent,
            SessionKind::Authoring => {
                mode = EndMode::Teardown;
                Closing::Skip
            }
            _ => Closing::Skip,
        };
        self.flow.end = Some(EndPlan {
            mode,
            requested_at: Instant::now(),
            closing,
            deny_new,
            reason,
            bound: None,
            cause,
        });
    }

    /// Lower the end's own bound to `bound`; a later call never raises it.
    pub(super) fn bound_end(&mut self, bound: Duration) {
        if let Some(plan) = &mut self.flow.end {
            plan.bound = Some(plan.bound.map_or(bound, |existing| existing.min(bound)));
        }
    }
}
