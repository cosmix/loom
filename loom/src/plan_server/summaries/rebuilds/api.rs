//! What routes, concept mode and plan 11 call: suspensions, the rebuild button, answers,
//! terminal claims, out-of-date reasons, progress, engine updates.

use std::sync::{Arc, Weak};

use super::{evaluation, RebuildReason, Rebuilds};
use crate::plan_server::summaries::scheduler::{AnswerRefused, AnswerRight, SuspendReason};
use crate::plan_server::wire::concept::Progress;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;

/// A running rebuild's progress, as concept pages show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Building {
    pub done: u32,
    pub total: u32,
}

impl From<Building> for Progress {
    fn from(building: Building) -> Self {
        Progress {
            done: building.done,
            total: building.total,
        }
    }
}

/// Why a release of terminal claims was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseRefused {
    /// The id is not a claim made from a terminal.
    NotTerminal(String),
    /// The claim files could not be changed.
    Failed(String),
}

impl std::fmt::Display for ReleaseRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseRefused::NotTerminal(id) => write!(f, "{id} is not a claim from a terminal"),
            ReleaseRefused::Failed(why) => write!(f, "cannot release the claims: {why}"),
        }
    }
}

impl std::error::Error for ReleaseRefused {}

/// Holds a suspension: nothing starts until every guard is dropped.
pub struct SuspendGuard {
    rebuilds: Weak<Rebuilds>,
    token: u64,
}

impl Drop for SuspendGuard {
    fn drop(&mut self) {
        if let Some(rebuilds) = self.rebuilds.upgrade() {
            rebuilds.resume(self.token);
        }
    }
}

impl Rebuilds {
    /// Start nothing while the guard lives (plan 11's pressure runs).
    pub fn suspend(self: &Arc<Self>, reason: SuspendReason) -> SuspendGuard {
        let token = self.run(|state, _, _| state.scheduler.suspend(reason));
        SuspendGuard {
            rebuilds: Arc::downgrade(self),
            token,
        }
    }

    fn resume(&self, token: u64) {
        self.run(|state, spawner, now| {
            let out_of_date = state.out_of_date();
            state.scheduler.resume(token, &out_of_date, now, spawner);
        });
    }

    /// Change the automatic budget and re-evaluate the pending queue immediately.
    pub fn set_auto(&self, enabled: bool) {
        self.run(|state, spawner, now| {
            state.scheduler.set_auto(enabled);
            let out_of_date = state.out_of_date();
            let events = state.scheduler.on_update(&[], &out_of_date, now, spawner);
            state.events.extend(events);
        });
    }

    /// Queue `ids`: a Button starts them without asking; another trigger counts them as a batch.
    pub fn enqueue(&self, ids: Vec<String>, trigger: Trigger) {
        self.run(|state, spawner, now| match trigger {
            Trigger::Button => state.scheduler.press(&ids, now, spawner),
            Trigger::Background | Trigger::Refinement => {
                let out_of_date = state.out_of_date();
                let events = state.scheduler.on_update(&ids, &out_of_date, now, spawner);
                state.events.extend(events);
            }
        });
    }

    /// Answer the open question with the caller's right.
    pub fn answer(
        &self,
        question: &str,
        rebuild: bool,
        right: AnswerRight,
    ) -> Result<(), AnswerRefused> {
        self.run(|state, spawner, now| {
            state
                .scheduler
                .answer(question, rebuild, right, now, spawner)
        })
    }

    /// Release claims made from a terminal (listed in the panel's `terminal_claims`).
    pub fn release_terminal_claims(&self, ids: &[String]) -> Result<(), ReleaseRefused> {
        let terminal = self.lock().scheduler.panel_state().terminal_claims;
        if let Some(id) = ids.iter().find(|id| !terminal.contains(id)) {
            return Err(ReleaseRefused::NotTerminal(id.clone()));
        }
        if let Some(released) = self.attribute(|attribution| attribution.release_terminal(ids)) {
            released.map_err(|error| ReleaseRefused::Failed(format!("{error:#}")))?;
        }
        self.run(|state, _, _| {
            let mut left = state.scheduler.panel_state().terminal_claims;
            left.retain(|id| !ids.contains(id));
            state.scheduler.set_terminal_claims(left);
        });
        Ok(())
    }

    /// Why concept mode marks `id` out of date, when the scheduler is the reason.
    pub fn out_of_date_reason(&self, id: &str) -> Option<RebuildReason> {
        self.lock().scheduler.reason(id)
    }

    /// The running rebuild's progress.
    pub fn building(&self) -> Option<Building> {
        let (done, total) = self.lock().scheduler.progress()?;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Some(Building {
            done: count(done),
            total: count(total),
        })
    }

    /// Follow one summary engine update: seals, claims and the newly invalidated summaries.
    pub fn on_update(&self, update: &EngineUpdate) {
        let eval = Arc::new(update.evaluation.clone());
        let out_of_date = evaluation::out_of_date(&eval);
        self.apply_evaluation(eval, &update.invalidated, &out_of_date);
    }
}
