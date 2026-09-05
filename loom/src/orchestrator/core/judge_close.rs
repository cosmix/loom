//! Closing an adjudication session.
//!
//! A judge leaves the daemon in two ways — its verdict was applied, or it
//! stalled and was taken down — and both have to leave exactly the same state
//! behind, because `claim_session_slot` refuses a second judge for a stage
//! while it can still see a live one. A judge whose process was killed but
//! whose record still says `Running`, or whose signal file survives, is
//! indistinguishable from a working judge, and the stage waits on it forever.
//!
//! So the sequence lives here once rather than in each caller. The kill itself
//! only signals; because `SIGTERM` returns before the target has actually
//! exited, the close waits for confirmed death before writing that state down.

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::monitor::heartbeat::cleanup_judge_heartbeat;
use crate::orchestrator::terminal::native::cleanup_session_settings;

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
    ///
    /// The kill is confirmed through the same bounded poll `take_down_agents`
    /// uses (`confirm_session_gone`, `event_handler/stage_takedown.rs`) before
    /// the record is written: `SIGTERM` is asynchronous, so a probe taken right
    /// after `kill_session` can still see a judge that is in the process of
    /// dying, and a record written before confirmed death describes a judge
    /// that may still be running. Once this returns, callers — and the tests
    /// that assert on the judge's process — may treat it as gone.
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
        match self.confirm_session_gone(session) {
            Ok(true) => {}
            Ok(false) => tracing::warn!(
                target: "loom::adjudication",
                session = %session.id,
                stage = ?session.stage_id,
                "adjudication session survived its kill; retiring its record anyway",
            ),
            Err(error) => tracing::warn!(
                target: "loom::adjudication",
                session = %session.id,
                stage = ?session.stage_id,
                %error,
                "could not confirm the adjudication session is gone; retiring its record anyway",
            ),
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
        cleanup_session_settings(&work_dir, &session.id);
    }
}
