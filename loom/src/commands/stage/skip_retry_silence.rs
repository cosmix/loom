//! What `loom stage retry` says about a live session that has stopped making
//! progress: how long it has been silent and when the daemon's monitor could
//! recover it on its own.
//!
//! The monitor owns the verdicts. This module asks its functions
//! (`never_worked`, `is_escalation`, `host_job_holds_session`) and only words
//! the answer; it never restates their thresholds.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::fs::session_files::load_session_exact;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::MAX_STALL_RECOVERIES;
use crate::orchestrator::monitor::heartbeat::{heartbeat_path, read_heartbeat, Heartbeat};
use crate::orchestrator::monitor::hung_latch::host_job_holds_session;
use crate::orchestrator::monitor::never_worked::{is_escalation, never_worked};

/// Widest multiple of the budget [`escalation_line`] looks for.
const MAX_LINE_MULTIPLE: u64 = 16;

/// The note `retry` prints under a live-session refusal, read from the files
/// the CLI can see. `None` when nothing is known about the session's silence.
pub(super) fn live_session_note(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
) -> Option<String> {
    // An unreadable record is treated as missing, as the daemon's own hung
    // handling does: it only withholds the never-worked verdict.
    let session = load_session_exact(work_dir, session_id).ok().flatten();
    let heartbeat = read_heartbeat(&heartbeat_path(work_dir, &stage.id)).ok();
    silence_report(
        work_dir,
        stage,
        session_id,
        session.as_ref(),
        heartbeat.as_ref(),
        crate::fs::daemon_state::is_running(work_dir),
        Utc::now(),
    )
}

/// The silence report for `session_id`, or `None` when there is neither a
/// session record nor a heartbeat of its own to judge it by.
pub(super) fn silence_report(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    session: Option<&Session>,
    heartbeat: Option<&Heartbeat>,
    daemon_running: bool,
    now: DateTime<Utc>,
) -> Option<String> {
    // A heartbeat naming another session is a previous attempt's.
    let heartbeat = heartbeat.filter(|heartbeat| heartbeat.session_id == session_id);
    let since = match (heartbeat, session) {
        (Some(heartbeat), _) => heartbeat.effective_progress_at(),
        (None, Some(session)) => session.created_at,
        (None, None) => return None,
    };
    let age = u64::try_from(now.signed_duration_since(since).num_seconds()).unwrap_or(0);
    let budget = stage.effective_subagent_timeout_secs();
    let never_started = session.is_some_and(|s| {
        let judged_at = match heartbeat {
            Some(_) => now,
            // The monitor judges a heartbeat-less session once its silence
            // reaches the budget, so it is judged at that point, not now.
            None => spawn_line(s, budget).map_or(now, |line| now.max(line)),
        };
        never_worked(s, heartbeat, judged_at, budget)
    });
    let verdict = match hold_reason(work_dir, stage, session_id, daemon_running, budget, now) {
        Some(reason) => format!("Automatic recovery is not pending: {reason}."),
        None => eligibility(stage, age, budget, never_started),
    };
    Some(format!(
        "  The session has made no progress for {age} s (response budget {budget} s).\n  {verdict}"
    ))
}

/// The instant a session spawned at `created_at` has been silent for `budget`.
fn spawn_line(session: &Session, budget: u64) -> Option<DateTime<Utc>> {
    let budget = chrono::Duration::try_seconds(i64::try_from(budget).ok()?)?;
    session.created_at.checked_add_signed(budget)
}

/// Why the daemon will not recover this session at all, when it will not.
fn hold_reason(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    daemon_running: bool,
    budget: u64,
    now: DateTime<Utc>,
) -> Option<String> {
    if stage.status != StageStatus::Executing {
        return Some(format!(
            "the stage is {} and the daemon recovers only an executing stage",
            stage.status
        ));
    }
    if stage.session.as_deref() != Some(session_id) {
        return Some("the stage names another session".to_string());
    }
    if !daemon_running {
        return Some("the daemon is not running, so no recovery runs".to_string());
    }
    if budget == 0 {
        return Some("the response budget is 0, so the session never escalates".to_string());
    }
    host_job_holds_session(work_dir, stage, session_id, now).then(|| {
        "a running host job holds recovery until it ends or reaches its deadline".to_string()
    })
}

/// When the daemon could act on a silence of `age` s, and what it would do.
fn eligibility(stage: &Stage, age: u64, budget: u64, never_started: bool) -> String {
    let action = if never_started || stage.stall_recoveries >= MAX_STALL_RECOVERIES {
        "park the stage in NeedsHumanReview"
    } else {
        "requeue the stage"
    };
    let when = if is_escalation(age, budget, never_started, None) {
        "now, at the daemon's next check".to_string()
    } else {
        match escalation_line(budget, never_started) {
            Some(line) => format!("at the earliest in {} s", line.saturating_sub(age)),
            None => "once the silence reaches the monitor's escalation line".to_string(),
        }
    };
    format!(
        "Automatic recovery is eligible {when}, when the daemon would {action}. \
         A running foreground command, codex run or completion blocker can hold it longer."
    )
}

/// The silence at which [`is_escalation`] first accepts, found by asking it:
/// a never-worked session escalates at its budget, any other at a whole
/// multiple of it.
fn escalation_line(budget: u64, never_started: bool) -> Option<u64> {
    (1..=MAX_LINE_MULTIPLE)
        .map(|multiple| budget.saturating_mul(multiple))
        .find(|line| is_escalation(*line, budget, never_started, None))
}
