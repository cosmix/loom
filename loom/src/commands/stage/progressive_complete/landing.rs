//! Who lands a stage, and how each outcome write steps the stage out of the
//! status it held while its branch merged.

use anyhow::{ensure, Result};

use crate::git::merge::MergeGate;
use crate::models::stage::{Stage, StageStatus};

/// Who lands the stage, which decides the status it holds on disk while the
/// branch merges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// The stage's own session completes it: the stage is `Executing` and
    /// owned by that session. The control-path and zero-commit gates apply.
    Session,
    /// An operator lands a reviewed branch (`human-review --force-complete`).
    /// The stage stays `NeedsHumanReview` on disk while the branch merges and
    /// passes through `Executing` only inside the locked write that records
    /// the outcome, so the daemon's coherence watchdog never observes an
    /// `Executing` stage with no session. Both gates are bypassed.
    Reviewed,
}

impl Landing {
    /// The merge gate this landing runs under.
    pub fn gate(self) -> MergeGate {
        match self {
            Landing::Session => MergeGate::Enforce,
            Landing::Reviewed => MergeGate::Bypass,
        }
    }

    /// Step an operator-landed stage from `NeedsHumanReview` to `Executing`,
    /// from which every merge outcome is a legal transition. Applied to the
    /// in-memory stage, and inside each outcome's locked write to the fresh
    /// on-disk stage, where it refuses a stage another writer moved out of
    /// review during the merge. A session landing is already `Executing`.
    pub(in crate::commands::stage) fn enter(self, stage: &mut Stage) -> Result<()> {
        if self == Landing::Session {
            return Ok(());
        }
        ensure!(
            stage.status == StageStatus::NeedsHumanReview,
            "stage '{}' is {} instead of awaiting review: another writer changed it \
             during the landing",
            stage.id,
            stage.status
        );
        stage.try_transition(StageStatus::Executing)?;
        stage.review_reason = None;
        Ok(())
    }
}

/// What each outcome write re-checks on the fresh on-disk stage (A-5): the
/// completion commit captured before the merge, and the landing's entry step.
pub(super) struct Claim {
    commit: Option<String>,
    landing: Landing,
}

impl Claim {
    pub(super) fn new(stage: &Stage, landing: Landing) -> Self {
        Self {
            commit: stage.completed_commit.clone(),
            landing,
        }
    }

    pub(super) fn apply(&self, stage: &mut Stage) -> Result<()> {
        ensure!(
            stage.completed_commit == self.commit,
            "completion proof changed during merge handling"
        );
        self.landing.enter(stage)
    }
}

#[cfg(test)]
thread_local! {
    static WINDOW_PROBE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `probe` once, on this thread, at the point where the next landing
/// does its slow work (the git merge, or a knowledge stage's final write).
#[cfg(test)]
pub(in crate::commands::stage) fn set_window_probe(probe: impl FnOnce() + 'static) {
    WINDOW_PROBE.with(|slot| *slot.borrow_mut() = Some(Box::new(probe)));
}

#[cfg(test)]
pub(in crate::commands::stage) fn run_window_probe() {
    if let Some(probe) = WINDOW_PROBE.with(|slot| slot.borrow_mut().take()) {
        probe();
    }
}
