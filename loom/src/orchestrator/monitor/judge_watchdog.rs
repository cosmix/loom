//! When an adjudication session is alive and not working.
//!
//! A judge is silent in the same observable way a stage agent is, and needs a
//! different answer. It does not own the stage it works on, so there is
//! nothing to hand off and re-queue: a stalled judge is closed, and the stage
//! it left in `NeedsAdjudication` is re-judged on the next poll under the
//! dispute's own attempt budget. It is also latched once rather than twice —
//! the first report already ends the session, so there is no second line to
//! escalate to.
//!
//! # Waiting on its own command is working
//!
//! A judge must run the disputed stage's criteria, and `loom check` runs them
//! one after another under their own timeouts. A judge that sends that run to
//! the background and ends its turn makes no tool call, and so writes no
//! heartbeat, for as long as the run takes. So once its heartbeat has gone
//! quiet past the budget, every poll looks for a command of its own still
//! running ([`running_command`]); a poll that finds one counts as activity,
//! the same as a heartbeat. The idle clock therefore restarts when the
//! command ends, and the judge gets a full budget to read the result. A judge
//! with neither a heartbeat nor a command past the budget is still closed, so
//! a silent judge cannot hold its dispute.
//!
//! # The hold is capped
//!
//! A command can outlive any wait the judge has a use for: a dev server or a
//! `tail -f` sent to the background, a hung check. Its attempt budget only
//! counts judges that were closed, so an uncapped hold would leave the stage
//! in `NeedsAdjudication` for good. The one wait a judge needs is a single
//! `loom check` of the disputed stage, whose worst case is the sum of the
//! timeouts that command runs under: each acceptance criterion's (a string
//! criterion's [`DEFAULT_COMMAND_TIMEOUT`], an extended one's `timeout_secs`
//! or [`DEFAULT_CHECK_TIMEOUT`]), [`WIRING_TEST_TIMEOUT`] per wiring test, and
//! [`DEAD_CODE_TIMEOUT`] for a dead-code check. `loom check` runs no
//! `before_stage`/`after_stage` truth checks, so they add nothing. The hold
//! ceiling is that sum plus one budget, measured from the judge's last
//! heartbeat — a sighting never moves it. Past it the judge is reported
//! stalled however busy its children are.
//!
//! The probe costs nothing while a judge is making tool calls. A judge held
//! this way is announced once per wait: a watchdog that declines to act says
//! so where an operator looks.

use std::time::Duration;

use chrono::{DateTime, Utc};
use colored::Colorize;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::verify::criteria::{DEFAULT_CHECK_TIMEOUT, DEFAULT_COMMAND_TIMEOUT};
use crate::verify::goal_backward::dead_code::DEAD_CODE_TIMEOUT;
use crate::verify::goal_backward::wiring_tests::WIRING_TEST_TIMEOUT;

use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::{Heartbeat, HeartbeatWatcher};
use super::running_command::running_command;

impl Detection {
    /// A judge that is still alive and has stopped working.
    ///
    /// Measured from its own heartbeat when it has written one, and from its
    /// spawn otherwise — a judge that never reached a tool call is exactly the
    /// case this watchdog exists for, since it is the shape a permission
    /// prompt or an API outage takes — or from the last poll that found it
    /// running a command, when that is later and the judge is still inside
    /// its hold ceiling ([`hold_ceiling_secs`]).
    pub(super) fn adjudicator_stall_event(
        &mut self,
        session: &Session,
        stage_id: &str,
        stages: &[Stage],
        heartbeat_watcher: &HeartbeatWatcher,
        timeout_secs: u64,
        handlers: &Handlers,
    ) -> Option<MonitorEvent> {
        // A stage may declare `subagent_timeout_secs: 0` and nothing rejects
        // it. Zero would close every judge on its first poll, so a stage that
        // declares no real budget gets no judge watchdog — the same rule
        // `is_stall_escalation` applies to stage agents, for the same reason.
        if timeout_secs == 0 || self.reported_stalled_judges.contains(&session.id) {
            return None;
        }
        let now = heartbeat_watcher.now();
        let heard_at = judge_last_activity(session, stage_id, heartbeat_watcher);
        let silent_secs = secs_between(heard_at, now)?;
        // Heard from inside the budget: working. A judge whose process is
        // already gone is a crash or an ordinary exit, both of which
        // `detect_vanished_process` owns.
        if silent_secs <= timeout_secs
            || !matches!(handlers.check_session_alive(session), Ok(Some(true)))
        {
            return None;
        }
        let stage = stages.iter().find(|stage| stage.id == stage_id);
        let stale_duration_secs = if silent_secs <= hold_ceiling_secs(stage, timeout_secs) {
            self.idle_secs_while_held(session, stage_id, heard_at, now, timeout_secs, handlers)?
        } else {
            silent_secs
        };
        if stale_duration_secs <= timeout_secs {
            return None;
        }
        self.reported_stalled_judges.insert(session.id.clone());
        Some(MonitorEvent::AdjudicatorStalled {
            session_id: session.id.clone(),
            stage_id: stage_id.to_string(),
            stale_duration_secs,
            timeout_secs,
        })
    }

    /// Inside the hold ceiling: how long the judge has been idle, counting the
    /// last poll that found it running a command of its own as activity.
    /// `None` while such a command runs: the judge is waiting on it.
    fn idle_secs_while_held(
        &mut self,
        session: &Session,
        stage_id: &str,
        heard_at: DateTime<Utc>,
        now: DateTime<Utc>,
        timeout_secs: u64,
        handlers: &Handlers,
    ) -> Option<u64> {
        if self.judge_awaits_command(session, stage_id, heard_at, now, timeout_secs, handlers) {
            return None;
        }
        let active_at = self
            .judge_commands_seen
            .get(&session.id)
            .map_or(heard_at, |seen_at| heard_at.max(*seen_at));
        secs_between(active_at, now)
    }

    /// Whether the judge is running a command of its own, recording the
    /// sighting as its latest activity. Announced when the sighting starts a
    /// new wait: the first, or the first since the judge was last heard from.
    fn judge_awaits_command(
        &mut self,
        session: &Session,
        stage_id: &str,
        heard_at: DateTime<Utc>,
        now: DateTime<Utc>,
        timeout_secs: u64,
        handlers: &Handlers,
    ) -> bool {
        let Some(command) = running_command(handlers.work_dir(), session) else {
            return false;
        };
        let previous = self.judge_commands_seen.insert(session.id.clone(), now);
        if previous.is_none_or(|seen_at| seen_at < heard_at) {
            eprintln!(
                "{} adjudication session '{}' for stage '{stage_id}' has made no tool call for \
                 {}s (budget {timeout_secs}s) and is waiting on a command it started ({}, pid \
                 {}); it is not closed while that command runs.",
                "JUDGE WAITING:".yellow().bold(),
                session.id,
                now.signed_duration_since(heard_at).num_seconds(),
                command.name,
                command.pid,
            );
        }
        true
    }

    /// Drop the stall latch and the command sightings of every judge that is
    /// no longer running, so the next judge on the same stage is measured from
    /// scratch. Keyed by session id, so this only ever forgets sessions that
    /// have already ended.
    pub(super) fn clear_finished_judge_latches(&mut self, sessions: &[Session]) {
        let running: std::collections::HashSet<_> = sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .map(|session| session.id.as_str())
            .collect();
        self.reported_stalled_judges
            .retain(|session_id| running.contains(session_id.as_str()));
        self.judge_commands_seen
            .retain(|session_id, _| running.contains(session_id.as_str()));
    }
}

/// How long past its last heartbeat a judge may be held by a command of its
/// own: one budget plus the worst case of one `loom check` of `stage` (see
/// the module docs). A stage the poll cannot find has no check to wait for,
/// so its judge is held no longer than the budget.
fn hold_ceiling_secs(stage: Option<&Stage>, timeout_secs: u64) -> u64 {
    let check_secs = stage.map_or(0, |stage| check_run_limit(stage).as_secs());
    timeout_secs.saturating_add(check_secs)
}

/// The longest one `loom check` of `stage` can run: the sum of the timeouts
/// its acceptance criteria, wiring tests and dead-code check run under. The
/// criterion rule is the acceptance runner's (`verify::criteria::runner`).
fn check_run_limit(stage: &Stage) -> Duration {
    let acceptance = stage
        .acceptance
        .iter()
        .map(|criterion| match criterion {
            AcceptanceCriterion::Simple(_) => DEFAULT_COMMAND_TIMEOUT,
            AcceptanceCriterion::Extended(check) => check.timeout(DEFAULT_CHECK_TIMEOUT),
        })
        .fold(Duration::ZERO, Duration::saturating_add);
    let wiring_tests = u32::try_from(stage.wiring_tests.len()).map_or(Duration::MAX, |count| {
        WIRING_TEST_TIMEOUT.saturating_mul(count)
    });
    let dead_code = stage
        .dead_code_check
        .as_ref()
        .map_or(Duration::ZERO, |_| DEAD_CODE_TIMEOUT);
    acceptance
        .saturating_add(wiring_tests)
        .saturating_add(dead_code)
}

/// Whole seconds from `then` to `now`; `None` when `then` is in the future.
fn secs_between(then: DateTime<Utc>, now: DateTime<Utc>) -> Option<u64> {
    let secs = now.signed_duration_since(then).num_seconds();
    u64::try_from(secs).ok()
}

/// When a judge was last heard from.
///
/// A heartbeat naming a DIFFERENT session is a previous judge's, left on the
/// stage's adjudication key after that judge was closed. It says nothing about
/// this one, so this one is measured from its spawn instead — the same rule
/// [`HeartbeatWatcher::check_session_hung`] applies to stage agents.
fn judge_last_activity(
    session: &Session,
    stage_id: &str,
    heartbeat_watcher: &HeartbeatWatcher,
) -> DateTime<Utc> {
    heartbeat_watcher
        .judge_heartbeat(stage_id)
        .filter(|heartbeat| heartbeat.session_id == session.id)
        .map_or(session.created_at, Heartbeat::effective_progress_at)
}
