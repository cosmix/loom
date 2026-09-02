//! Closing a judge that stopped working.
//!
//! An adjudication session is the one agent the daemon spawns that no watchdog
//! used to cover. It has no worktree and does not own the stage it judges, so
//! neither the stage's heartbeat nor its ceiling ever spoke for it: a judge
//! stuck at a permission prompt, or on an API outage, left its stage in
//! `NeedsAdjudication` with nothing on disk changing and no message printed.
//! The attempt budget did not help either, because
//! [`MAX_ADJUDICATION_ATTEMPTS`] only bounds judges that DIED — a live one
//! blocks the next judge for its stage outright.
//!
//! The remedy is deliberately smaller than the stage-agent one. A stalled
//! stage agent's stage is handed off and re-queued; a stalled judge is only
//! CLOSED. The stage is not touched at all: it stays in `NeedsAdjudication`,
//! and the next poll either spawns a fresh judge or, once the attempts are
//! spent, escalates the stage to `NeedsHumanReview`. That path already exists
//! and already terminates, so nothing here retries, backs off, or waits.

use anyhow::Result;
use colored::Colorize;

use crate::models::session::{SessionStatus, SessionType};
use crate::orchestrator::adjudication::MAX_ADJUDICATION_ATTEMPTS;

use super::super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// Close the stalled judge named by an `AdjudicatorStalled` report.
    ///
    /// The report is a snapshot from the poll loop, so the record is re-read
    /// and re-checked here: only a session that is still on disk, still an
    /// adjudication session for this exact stage, and still `Running` may be
    /// closed. Anything else means the situation resolved itself between the
    /// observation and now, and the report is dropped without a word.
    pub(super) fn on_adjudicator_stalled(
        &mut self,
        session_id: &str,
        stage_id: &str,
        stale_duration_secs: u64,
        timeout_secs: u64,
    ) -> Result<()> {
        let loaded =
            crate::fs::session_files::load_session_exact(&self.config.work_dir, session_id);
        let session = match loaded {
            Ok(Some(session)) => session,
            Ok(None) => return Ok(()),
            Err(error) => {
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    session = %session_id,
                    %error,
                    "failed to load the stalled adjudication session; not closing it",
                );
                return Ok(());
            }
        };
        if session.session_type != SessionType::Adjudication
            || session.stage_id.as_deref() != Some(stage_id)
            || session.status != SessionStatus::Running
        {
            return Ok(());
        }

        clear_status_line();
        eprintln!(
            "{} adjudication session '{session_id}' for stage '{stage_id}' has made no tool call \
             for {stale_duration_secs}s (budget {timeout_secs}s); closing it so the dispute is \
             re-judged, until the dispute's attempt budget \
             ({MAX_ADJUDICATION_ATTEMPTS}) is spent.",
            "JUDGE STALLED:".red().bold()
        );
        self.close_adjudication_session(&session, SessionStatus::Crashed);
        Ok(())
    }
}
