//! Closing an adjudication session.
//!
//! A judge leaves the daemon in two ways — its verdict was applied, or it
//! stalled and was taken down — and both have to leave exactly the same state
//! behind, because `claim_session_slot` refuses a second judge for a stage
//! while it can still see a live one. A judge whose process was killed but
//! whose record still says `Running`, or whose signal file survives, is
//! indistinguishable from a working judge, and the stage waits on it forever.
//!
//! So the sequence lives here once rather than in each caller.

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::monitor::heartbeat::cleanup_judge_heartbeat;

use super::Orchestrator;

impl Orchestrator {
    /// Kill an adjudication session and retire every trace the daemon uses to
    /// decide whether a judge is still live.
    ///
    /// `status` is what the session record is left saying: `Completed` for a
    /// judge that did its job, `Crashed` for one that was closed without
    /// producing a verdict.
    ///
    /// Every step is best-effort and none can abort the others. A kill that
    /// fails still has to be followed by the record and signal cleanup, or a
    /// judge that is already gone keeps its stage blocked on the strength of
    /// its own leftovers.
    pub(crate) fn close_adjudication_session(&mut self, session: &Session, status: SessionStatus) {
        if let Err(error) = self.backend.kill_session(session) {
            tracing::warn!(
                target: "loom::adjudication",
                session = %session.id,
                stage = ?session.stage_id,
                %error,
                "failed to kill the adjudication session",
            );
        }
        let work_dir = self.config.work_dir.clone();
        self.monitor
            .handlers()
            .persist_session_status(session, status);
        if let Err(error) = crate::orchestrator::signals::remove_signal(&session.id, &work_dir) {
            tracing::warn!(
                target: "loom::adjudication",
                session = %session.id,
                stage = ?session.stage_id,
                %error,
                "failed to remove the adjudication session's signal file",
            );
        }
        if let Some(stage_id) = session.stage_id.as_deref() {
            cleanup_judge_heartbeat(&work_dir, stage_id);
        }
    }
}
