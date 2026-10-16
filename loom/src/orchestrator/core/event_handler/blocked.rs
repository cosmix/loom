//! Retiring the agents of a stage that a block, not a crash, made Blocked.
//!
//! Split out of `event_handler` to keep that file inside the size limit.

use anyhow::Result;

use crate::models::stage::StageStatus;

use super::super::{clear_status_line, persistence::Persistence, Orchestrator};

impl Orchestrator {
    /// A stage that became Blocked. Blocked with no `failure_info` means `loom stage block`
    /// (a stage agent's, an operator's) or a rejected human review, never a crash: retire any
    /// live agent so its later exit is not filed as a crash and a retry spawns a fresh session.
    ///
    /// That covers the stage agent's own `loom stage block`, an operator's block of a stage
    /// whose agent is still mid-work (that agent is retired too, with a handoff), and
    /// `loom stage human-review --reject`. A stage with `failure_info` is left alone: a crash
    /// or an infrastructure block, whose session is already gone or owned by the crash path.
    ///
    /// The status is already written when the daemon learns of the block, so retirement runs
    /// while the stage is Blocked and clears `stage.session` only if it still is: a stage
    /// retried in between keeps its new session. `close_reason` is never written here.
    pub(super) fn on_stage_blocked(&mut self, stage_id: &str) -> Result<()> {
        self.graph.mark_status(stage_id, StageStatus::Blocked)?;

        let stage = self.load_stage(stage_id)?;
        if super::super::lifecycle_snapshot::block_cause(&stage)
            != Some(super::super::lifecycle_snapshot::BlockCause::Intentional)
        {
            return Ok(());
        }
        // A daemon restart re-emits `StageBlocked` for every Blocked stage: one with
        // nothing to retire must not have its stage file rewritten.
        if stage.session.is_none() && self.retirable_agents(stage_id, &stage)?.is_empty() {
            return Ok(());
        }

        let survivors = self.retire_stage_agents(stage_id, StageStatus::Blocked)?;
        if !survivors.is_empty() {
            clear_status_line();
            eprintln!(
                "Warning: stage '{stage_id}' is blocked, but its agent(s) {} outlived the kill. \
                 Stop them with: loom stage reset {stage_id} --kill-session",
                survivors.join(", ")
            );
        }
        Ok(())
    }
}
