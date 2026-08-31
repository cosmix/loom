//! Recovering a stage whose agent stopped answering.
//!
//! A `SessionHung` report used to print a warning and nothing else, which made
//! it the second missed chance in the same failure: a session that ended its
//! turn without its stage transition landing sat `Executing` forever, and the
//! one signal that noticed — a live process that had stopped heartbeating —
//! only said so to a log nobody was reading.
//!
//! Acting on it is bounded on three sides, because killing a working agent is
//! worse than waiting for a stuck one:
//!
//! 1. the first report is still only a warning; escalation needs the silence
//!    to reach [`is_stall_escalation`]'s line, three response budgets deep,
//!    and any heartbeat in between resets the clock;
//! 2. the stage must still be `Executing` and still name this exact session,
//!    which `begin_handoff` re-checks under the stage lock; and
//! 3. a stage may be recovered this way [`MAX_STALL_RECOVERIES`] times. After
//!    that it is left where it stands for an operator — a stage that stalls
//!    every attempt is a bug in the stage, and re-queueing it forever hides it.
//!
//! The takedown itself is the ceiling backstop's, unchanged: write the
//! outgoing agent's handoff, kill every agent the stage owns, re-queue only
//! once they are confirmed gone.

use anyhow::Result;
use colored::Colorize;

use crate::fs::session_files::load_session_exact;
use crate::handoff::HandoffOrigin;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::hung_latch::is_stall_escalation;
use crate::orchestrator::monitor::parked::hung_warning;

use super::super::persistence::Persistence;
use super::super::{clear_status_line, Orchestrator};

/// How many times one stage may be recovered from a stall automatically.
///
/// Two: the first stall can be the agent's bad luck and the second its
/// successor's, but a third says the stage itself is what stalls, and an
/// operator has to look at it.
const MAX_STALL_RECOVERIES: u32 = 2;

/// One `SessionHung` report, as the event carries it.
pub(super) struct HungReport<'a> {
    pub session_id: &'a str,
    pub stage_id: Option<&'a str>,
    pub stale_duration_secs: u64,
    pub timeout_secs: u64,
    pub last_activity: Option<&'a str>,
    pub finished_without_completing: bool,
}

impl Orchestrator {
    /// Warn about a silent session, and recover its stage once the silence is
    /// long enough to be evidence rather than suspicion.
    pub(super) fn on_session_hung(&mut self, report: HungReport<'_>) -> Result<()> {
        clear_status_line();
        eprintln!(
            "{}",
            hung_warning(
                report.session_id,
                report.stage_id,
                report.stale_duration_secs,
                report.timeout_secs,
                report.last_activity,
                report.finished_without_completing,
            )
        );

        // A session naming no stage has nothing to re-queue.
        let Some(stage_id) = report.stage_id else {
            return Ok(());
        };
        if !is_stall_escalation(report.stale_duration_secs, report.timeout_secs) {
            return Ok(());
        }
        self.recover_stalled_stage(stage_id, report.session_id, report.stale_duration_secs)
    }

    /// Hand the stage off and re-queue it, or say why it was left alone.
    fn recover_stalled_stage(
        &mut self,
        stage_id: &str,
        session_id: &str,
        stale_duration_secs: u64,
    ) -> Result<()> {
        let stage = self.load_stage(stage_id)?;
        // A report about a session the stage has moved past describes nothing
        // that is still running. `begin_handoff` refuses it too; declining
        // here also keeps it from charging the recovery budget.
        if stage.session.as_deref() != Some(session_id) || stage.status != StageStatus::Executing {
            return Ok(());
        }

        if stage.stall_recoveries >= MAX_STALL_RECOVERIES {
            eprintln!(
                "{} Stage '{stage_id}' has already been recovered from a stall {} times and \
                 session '{session_id}' has now been silent for {stale_duration_secs}s. \
                 Leaving it exactly where it is: another automatic re-queue would loop. \
                 Take it over with: loom stage reset {stage_id} --kill-session",
                "STALL RECOVERY EXHAUSTED:".red().bold(),
                stage.stall_recoveries
            );
            return Ok(());
        }

        eprintln!(
            "{} Session '{session_id}' on stage '{stage_id}' has been silent for \
             {stale_duration_secs}s with its process still alive. Handing the stage off and \
             re-queueing it for a continuation session.",
            "SESSION STALLED:".red().bold()
        );

        // Same order as the ceiling backstop: latch the stage and its session
        // identity first, then write the outgoing agent's handoff from the
        // record that latch returned, then take it down.
        let Some(stage) = self.begin_handoff(stage_id, session_id)? else {
            return Ok(());
        };
        self.write_stall_handoff(&stage, session_id)?;
        self.charge_stall_recovery(stage_id)?;
        self.finish_handoff_and_requeue(stage_id, session_id, "an unrecoverable stall")
    }

    /// Write the stalled agent's handoff before the takedown takes it away, so
    /// the continuation starts from the last state the agent recorded rather
    /// than from nothing.
    ///
    /// Best-effort in one respect only: a session record that cannot be found
    /// leaves the stage with whatever handoff it already had. The takedown
    /// works off the stage and pid files, so it still proceeds — a stalled
    /// agent left running is the worse outcome.
    fn write_stall_handoff(&self, stage: &Stage, session_id: &str) -> Result<()> {
        let Some(session) = load_session_exact(&self.config.work_dir, session_id)? else {
            return Ok(());
        };
        if session.stage_id.as_deref() != Some(stage.id.as_str()) {
            return Ok(());
        }
        if let Some(path) = self.monitor.handlers().ensure_context_handoff(
            &session,
            stage,
            HandoffOrigin::Stalled,
        )? {
            eprintln!("Generated stall handoff at: {}", path.display());
        }
        Ok(())
    }

    /// Charge the recovery to the stage. Persisted rather than counted in
    /// memory so a daemon restart cannot hand the same stage an unlimited
    /// supply of re-queues.
    fn charge_stall_recovery(&mut self, stage_id: &str) -> Result<()> {
        self.update_stage(stage_id, |stage| {
            stage.stall_recoveries = stage.stall_recoveries.saturating_add(1);
            Ok(())
        })?;
        Ok(())
    }
}
