//! Applying verdict files written by adjudication sessions to stage state.

use anyhow::Result;

use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// Apply verdict files written by adjudication sessions to the
    /// stage state. See [`AdjudicatorRegistry::apply_pending_verdicts`].
    ///
    /// Retires every non-adjudication agent on the stage BEFORE the verdict
    /// applies: the agent that filed the dispute is idle and has never read
    /// the amended criteria, and left alive it would be adopted by the
    /// executor instead of a successor being spawned. A retirement that
    /// leaves survivors, or that fails outright, defers the verdict rather
    /// than re-queueing the stage onto a live writer.
    pub(crate) fn apply_pending_verdicts(&mut self) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        for (stage_id, dispute_id) in self.adjudicators.pending_verdicts(&work_dir)? {
            match self.retire_disputing_agents(&stage_id) {
                Ok(survivors) if survivors.is_empty() => {}
                Ok(survivors) => {
                    clear_status_line();
                    eprintln!(
                        "Deferring the verdict for stage '{stage_id}' dispute {dispute_id}: \
                         session(s) {} survived the retirement kill; take them down with \
                         'loom stage reset {stage_id} --kill-session'",
                        survivors.join(", ")
                    );
                    continue;
                }
                Err(error) => {
                    clear_status_line();
                    eprintln!(
                        "Deferring the verdict for stage '{stage_id}' dispute {dispute_id}: \
                         could not retire its agents: {error:#}"
                    );
                    continue;
                }
            }
            if let Err(error) = self
                .adjudicators
                .apply_verdict(&work_dir, &stage_id, dispute_id)
            {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    %error,
                    "failed to apply verdict",
                );
            }
        }
        Ok(())
    }
}
