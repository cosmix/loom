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
//! 1. the first report about a session that has worked is still only a
//!    warning; escalation needs the silence to reach [`is_escalation`]'s line,
//!    three response budgets deep, and any heartbeat in between resets the
//!    clock;
//! 2. the stage must still be `Executing` and still name this exact session,
//!    which `begin_handoff` re-checks under the stage lock; and
//! 3. a stage may be recovered this way [`MAX_STALL_RECOVERIES`] times. The
//!    next stall parks it in `NeedsHumanReview`, its review reason naming the
//!    silence and the last pane lines: a stage that stalls every attempt is a
//!    bug in the stage, and re-queueing it forever hides it.
//!
//! A session that never started work ([`never_worked`]) is parked at its first
//! report instead. With no tool call and no tokens there is nothing waiting
//! can bring back, and a re-queue would repeat whatever stopped it. When
//! `claude auth status` in the stage environment answers "not logged in", the
//! review reason names the login remedy.
//!
//! The takedown is the ceiling backstop's: write the outgoing agent's handoff
//! (a session that never worked has none), kill every agent the stage owns,
//! and re-queue or park only once they are confirmed gone. A parked stage is
//! announced by the needs-human-review notice; approving the review re-queues
//! it.

use anyhow::Result;
use chrono::Utc;
use colored::Colorize;

use crate::claude::auth::{stage_auth_status, AuthProbe};
use crate::fs::session_files::load_session_exact;
use crate::handoff::{
    current_blocker, expected_stage_commit, load_trusted_session_checkpoint, CompletionCheckpoint,
    HandoffOrigin,
};
use crate::models::session::{Session, SessionExitReason};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::heartbeat::{heartbeat_path, read_heartbeat};
use crate::orchestrator::monitor::never_worked::{is_escalation, never_worked};
use crate::orchestrator::monitor::parked::hung_warning;
use crate::orchestrator::terminal::session_tail;

use super::super::persistence::Persistence;
use super::super::{clear_status_line, Orchestrator};
use super::loop_recovery::{
    exhausted_reason, never_worked_reason, not_logged_in_reason, with_pane_notes,
};

/// How many times one stage may be recovered from a stall automatically.
///
/// Two: the first stall can be the agent's bad luck and the second its
/// successor's, but a third says the stage itself is what stalls, and an
/// operator has to look at it.
const MAX_STALL_RECOVERIES: u32 = 2;

/// How many pane lines a park keeps below its review reason.
const PANE_TAIL_LINES: usize = 40;

pub(super) fn checkpoint_has_current_blocker(
    checkpoint: &CompletionCheckpoint,
    stage: &Stage,
    current_commit: Option<&str>,
) -> bool {
    current_blocker(checkpoint, stage, current_commit).is_some()
}

/// One `SessionHung` report, as the event carries it.
pub(super) struct HungReport<'a> {
    pub session_id: &'a str,
    pub stage_id: Option<&'a str>,
    pub stale_duration_secs: u64,
    pub timeout_secs: u64,
    pub last_activity: Option<&'a str>,
    pub finished_without_completing: bool,
}

/// The two outside facts a park reads, supplied by the caller so a test never
/// runs the real `claude` or tmux. `on_session_hung` builds the production
/// pair; each runs at most once per park, never per poll.
pub(super) struct StallProbes<'a> {
    /// `claude auth status` in the stage environment; `None` when no `claude`
    /// binary is found.
    pub login: &'a dyn Fn() -> Option<AuthProbe>,
    /// The last lines of the session's pane or stderr log.
    pub tail: &'a dyn Fn(&Session) -> Option<String>,
}

impl Orchestrator {
    /// Warn about a silent session, and recover its stage once the silence is
    /// long enough to be evidence rather than suspicion.
    pub(super) fn on_session_hung(&mut self, report: HungReport<'_>) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        let login = || {
            crate::claude::find_claude_path()
                .ok()
                .map(|p| stage_auth_status(&p))
        };
        let tail = |s: &Session| session_tail(s, &work_dir, PANE_TAIL_LINES);
        self.on_session_hung_with(
            report,
            &StallProbes {
                login: &login,
                tail: &tail,
            },
        )
    }

    /// `on_session_hung` with its outside probes supplied.
    pub(super) fn on_session_hung_with(
        &mut self,
        report: HungReport<'_>,
        probes: &StallProbes<'_>,
    ) -> Result<()> {
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
        let session = self.hung_session(report.session_id);
        let heartbeat = read_heartbeat(&heartbeat_path(&self.config.work_dir, stage_id)).ok();
        let never_started = session.as_ref().is_some_and(|session| {
            never_worked(session, heartbeat.as_ref(), Utc::now(), report.timeout_secs)
        });
        if !is_escalation(
            report.stale_duration_secs,
            report.timeout_secs,
            never_started,
        ) {
            return Ok(());
        }
        self.recover_stalled_stage(stage_id, &report, never_started, session.as_ref(), probes)
    }

    /// The hung session's record. One that cannot be read is treated as
    /// missing, which only ever withholds the never-worked verdict.
    fn hung_session(&self, session_id: &str) -> Option<Session> {
        load_session_exact(&self.config.work_dir, session_id).unwrap_or_else(|error| {
            tracing::warn!(
                target: "loom::recovery",
                session = %session_id,
                %error,
                "could not read the hung session's record; judging the stall without it",
            );
            None
        })
    }

    /// Park the stage, re-queue it, or say why it was left alone.
    fn recover_stalled_stage(
        &mut self,
        stage_id: &str,
        report: &HungReport<'_>,
        never_started: bool,
        session: Option<&Session>,
        probes: &StallProbes<'_>,
    ) -> Result<()> {
        let session_id = report.session_id;
        let stage = self.load_stage(stage_id)?;
        // A report about a session the stage has moved past describes nothing
        // that is still running. `begin_handoff` refuses it too; declining
        // here also keeps it from charging the recovery budget.
        if stage.session.as_deref() != Some(session_id) || stage.status != StageStatus::Executing {
            return Ok(());
        }

        if self.completion_blocker_owns_stage(&stage, session_id) {
            return Ok(());
        }

        if never_started {
            return self.park_never_worked(stage_id, report, session, probes);
        }
        if stage.stall_recoveries >= MAX_STALL_RECOVERIES {
            let recoveries = stage.stall_recoveries;
            return self.park_exhausted(stage_id, report, recoveries, session, probes);
        }
        self.requeue_stalled_stage(stage_id, session_id, report.stale_duration_secs)
    }

    /// Hand the stage off, charge the recovery, and re-queue it.
    fn requeue_stalled_stage(
        &mut self,
        stage_id: &str,
        session_id: &str,
        stale_duration_secs: u64,
    ) -> Result<()> {
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
        self.finish_handoff_and_requeue(
            stage_id,
            session_id,
            "an unrecoverable stall",
            SessionExitReason::Stalled,
        )
    }

    /// Park a stage whose session never started work. Nothing was done, so
    /// there is no handoff to write and no recovery to charge. The pane is
    /// read before the takedown takes it away.
    fn park_never_worked(
        &mut self,
        stage_id: &str,
        report: &HungReport<'_>,
        session: Option<&Session>,
        probes: &StallProbes<'_>,
    ) -> Result<()> {
        let tail = session.and_then(probes.tail);
        // Only a definite "not logged in" names the login remedy: a logged-in
        // or unreadable answer, or no claude binary, says nothing about it.
        let not_logged_in = matches!((probes.login)(), Some(AuthProbe::NotLoggedIn));
        let reason = if not_logged_in {
            not_logged_in_reason(report.session_id, stage_id)
        } else {
            never_worked_reason(report, tail.as_deref())
        };
        eprintln!(
            "{} Session '{}' on stage '{stage_id}' never started work in {}s{}. Taking it down \
             and parking the stage for human review.",
            "SESSION STALLED:".red().bold(),
            report.session_id,
            report.stale_duration_secs,
            if not_logged_in {
                " and is not logged in to claude"
            } else {
                ""
            },
        );
        if self.begin_handoff(stage_id, report.session_id)?.is_none() {
            return Ok(());
        }
        let review_reason = with_pane_notes(reason, tail.as_deref());
        self.finish_handoff_and_park(
            stage_id,
            report.session_id,
            review_reason,
            SessionExitReason::Stalled,
        )
    }

    /// Park a stage that has used its automatic recoveries: one more re-queue
    /// would loop. The stalled agent's handoff is written as for a re-queue, so
    /// whoever takes the stage on starts from its last recorded state.
    fn park_exhausted(
        &mut self,
        stage_id: &str,
        report: &HungReport<'_>,
        recoveries: u32,
        session: Option<&Session>,
        probes: &StallProbes<'_>,
    ) -> Result<()> {
        let tail = session.and_then(probes.tail);
        eprintln!(
            "{} Session '{}' on stage '{stage_id}' has been silent for {}s after {recoveries} \
             automatic recoveries. Taking it down and parking the stage for human review.",
            "SESSION STALLED:".red().bold(),
            report.session_id,
            report.stale_duration_secs,
        );
        let Some(stage) = self.begin_handoff(stage_id, report.session_id)? else {
            return Ok(());
        };
        self.write_stall_handoff(&stage, report.session_id)?;
        let reason = exhausted_reason(report, recoveries, tail.as_deref());
        self.finish_handoff_and_park(
            stage_id,
            report.session_id,
            with_pane_notes(reason, tail.as_deref()),
            SessionExitReason::Stalled,
        )
    }

    fn completion_blocker_owns_stage(&self, stage: &Stage, session_id: &str) -> bool {
        let checkpoint =
            match load_trusted_session_checkpoint(&stage.id, session_id, &self.config.work_dir) {
                Ok(Some(checkpoint)) => checkpoint,
                Ok(None) => return false,
                Err(error) => {
                    tracing::warn!(
                        target: "loom::recovery",
                        stage = %stage.id,
                        session = %session_id,
                        %error,
                        "could not read completion checkpoint; continuing generic stall recovery",
                    );
                    return false;
                }
            };
        let commit = expected_stage_commit(stage, &self.config.repo_root).ok();
        let owns_stage = checkpoint_has_current_blocker(&checkpoint, stage, commit.as_deref());
        if owns_stage {
            tracing::info!(
                target: "loom::recovery",
                stage = %stage.id,
                session = %session_id,
                "current trusted completion blocker owns stage; skipping generic stall recovery",
            );
        }
        owns_stage
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
