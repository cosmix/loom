//! Applying verdict files written by adjudication sessions to stage state.

use anyhow::Result;
use std::path::Path;

use crate::models::session::{Session, SessionStatus, SessionType};

use super::{clear_status_line, Orchestrator};

#[cfg(test)]
#[path = "verdict_apply_tests.rs"]
mod verdict_apply_tests;

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
            // Read who wrote the verdict before applying it — the record is
            // read either way, so there is no reason to delay this past the
            // point where a failed apply would otherwise lose it.
            let judge = self
                .adjudicators
                .verdict_session_id(&work_dir, &stage_id, dispute_id);
            match self
                .adjudicators
                .apply_verdict(&work_dir, &stage_id, dispute_id)
            {
                Ok(()) => self.retire_adjudicator(&stage_id, dispute_id, judge.as_deref()),
                Err(error) => {
                    tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        dispute = dispute_id,
                        %error,
                        "failed to apply verdict",
                    );
                }
            }
        }
        Ok(())
    }

    /// Close the judge whose verdict was just applied. A judge's exit is
    /// ordinary, but a Claude Code session does not exit when its turn ends,
    /// and an idle judge blocks the next dispute on the stage:
    /// `claim_session_slot` refuses a second live adjudication session.
    fn retire_adjudicator(&mut self, stage_id: &str, dispute_id: u32, judge: Option<&str>) {
        let work_dir = self.config.work_dir.clone();
        for session in self.sessions_to_retire(&work_dir, stage_id, dispute_id, judge) {
            // Shared with the stalled-judge watchdog: both paths have to leave
            // identical state behind, or a stage stays blocked on a judge that
            // only looks live. See `super::judge_close`.
            self.close_adjudication_session(&session, SessionStatus::Completed);
            clear_status_line();
            eprintln!(
                "Closed adjudication session '{}' for stage '{stage_id}' dispute {dispute_id}",
                session.id
            );
        }
    }

    /// Which session(s) to close for a just-applied verdict.
    ///
    /// With a recorded id, only that exact session is a candidate — and only
    /// once it is confirmed to be this stage's adjudicator, so a verdict
    /// naming the wrong session can never take another one down.
    fn sessions_to_retire(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        judge: Option<&str>,
    ) -> Vec<Session> {
        let Some(id) = judge else {
            return self.idle_judges_if_no_dispute_remains(work_dir, stage_id, dispute_id);
        };
        match crate::fs::session_files::load_session_exact(work_dir, id) {
            Ok(Some(session))
                if session.session_type == SessionType::Adjudication
                    && session.stage_id.as_deref() == Some(stage_id) =>
            {
                vec![session]
            }
            Ok(_) => {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    session = %id,
                    "verdict names a session that is not this stage's live adjudicator; \
                     not closing it",
                );
                Vec::new()
            }
            Err(error) => {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    session = %id,
                    %error,
                    "failed to load the recorded adjudication session; not closing it",
                );
                Vec::new()
            }
        }
    }

    /// Fallback for a verdict recorded before `session_id` existed: close any
    /// live adjudication session for the stage, but only once no dispute is
    /// left unanswered — a live judge may still be working one of those.
    fn idle_judges_if_no_dispute_remains(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
    ) -> Vec<Session> {
        match self.adjudicators.unanswered_disputes(work_dir, stage_id) {
            Ok(0) => crate::orchestrator::session_registry::live_sessions_for_stage_of_type(
                work_dir,
                stage_id,
                SessionType::Adjudication,
            )
            .unwrap_or_default(),
            Ok(_) => Vec::new(),
            Err(error) => {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    %error,
                    "failed to check remaining disputes; leaving any live judge alone",
                );
                Vec::new()
            }
        }
    }
}
