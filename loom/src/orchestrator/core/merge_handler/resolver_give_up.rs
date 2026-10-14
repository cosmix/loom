//! A merge resolver that reports, through `loom stage block`, that it cannot
//! finish its stage's merge. A resolver is not the stage's worker, so the
//! block is a give-up: every resolver of the stage is stopped and the stage is
//! routed to human review with the resolver's reason and the manual steps.

use crate::context::untrusted::inline_safe;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::signals::find_live_merge_session_for_stage;

use super::resolver_stop::ResolverStop;
use super::review_route::{awaits_merge, ReviewRoute};

impl Orchestrator {
    /// Hold `stage_id`'s merge for the operator after resolver `session`
    /// reported that it cannot finish: stop every resolver of the stage, then
    /// route it to `NeedsHumanReview` with the resolver's `reason` and the
    /// manual steps. `Ok` is what the resolver's request settles with; `Err`
    /// is the refusal. Only the stage's live resolver may give up
    /// ([`Self::require_giveable`]).
    pub(in crate::orchestrator::core) fn hold_merge_for_resolver(
        &mut self,
        stage_id: &str,
        session: &Session,
        reason: &str,
    ) -> Result<String, String> {
        self.require_giveable(stage_id, session)?;
        // Agent text: flattened and bounded before it is stored or printed.
        let reason = inline_safe(reason);
        let stop = self.stop_resolvers_proven(stage_id);
        let followup = match &stop {
            Some(stopped) => stopped.note.clone(),
            None => format!(
                "To finish it by hand, {}",
                self.manual_merge_steps(stage_id)
            ),
        };
        let review = format!(
            "merge resolver {} could not finish the merge: {reason}. {followup}",
            session.id
        );
        match self.route_merge_stage_to_review(stage_id, review, None) {
            ReviewRoute::Routed => {
                self.clear_merge_resolver_attempts(stage_id);
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' needs human review: merge resolver {} reported that it \
                     cannot finish the merge: {reason}",
                    inline_safe(&session.id)
                );
                Ok(held_outcome(stop.as_ref()))
            }
            ReviewRoute::StageMovedOn => Err(
                "the stage left its merge state meanwhile; it was not routed to human review"
                    .to_string(),
            ),
            ReviewRoute::NotSaved => {
                Err("the stage could not be saved; it was not routed to human review".to_string())
            }
        }
    }

    /// `session` may give up `stage_id`'s merge: the stage waits for its
    /// merge, no typed block has loom retrying it, and `session` is the
    /// stage's live resolver.
    fn require_giveable(&self, stage_id: &str, session: &Session) -> Result<(), String> {
        let stage = self.load_stage(stage_id).map_err(|e| format!("{e:#}"))?;
        if !awaits_merge(&stage.status) {
            return Err(format!(
                "stage '{stage_id}' is {}, not MergeConflict or MergeBlocked",
                stage.status
            ));
        }
        // A typed block is not a conflict, and loom retries it every tick: a
        // give-up would cancel the retry of a resolution it already accepted.
        if let (StageStatus::MergeBlocked, Some(block)) = (&stage.status, &stage.merge.block) {
            return Err(format!(
                "stage '{stage_id}' is blocked ({block}), not in conflict: loom retries the \
                 merge itself once that changes, so there is nothing to give up"
            ));
        }
        self.require_live_resolver(stage_id, session)
    }

    /// `session` is running and is the resolver loom tracks for `stage_id`, or
    /// the one its live merge signal names. A resolver that exited after it
    /// relayed its block, while a successor spawned, is neither.
    fn require_live_resolver(&self, stage_id: &str, session: &Session) -> Result<(), String> {
        if session.status != SessionStatus::Running {
            return Err(format!("session '{}' is no longer running", session.id));
        }
        let tracked = self
            .active_sessions
            .get(stage_id)
            .is_some_and(|live| live.id == session.id);
        if tracked {
            return Ok(());
        }
        match find_live_merge_session_for_stage(stage_id, &self.config.work_dir) {
            Ok(Some(id)) if id == session.id => Ok(()),
            Ok(_) => Err(format!(
                "session '{}' is not the live merge resolver of stage '{stage_id}'",
                session.id
            )),
            Err(error) => Err(format!(
                "loom could not tell whether session '{}' is the live merge resolver of stage \
                 '{stage_id}': {error:#}",
                session.id
            )),
        }
    }
}

/// What a held give-up reports to the resolver's ledger entry: the stop as
/// `stop` proved it.
fn held_outcome(stop: Option<&ResolverStop>) -> String {
    let stopped = match stop {
        Some(stopped) if stopped.proven => "every merge resolver of the stage was stopped",
        Some(_) => {
            "loom could not confirm every merge resolver stopped; stop any still running by hand"
        }
        None => "no running merge resolver was found to stop",
    };
    format!("the stage is held in needs-human-review; {stopped}")
}

#[cfg(test)]
#[path = "resolver_give_up_tests.rs"]
mod tests;
