//! Contract tests for the native worker outcome mapping (common.md section 14).

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::native::outcome_for;
use crate::native_worker::model::{NativeManifest, NativeUnitState, Outcome, Ownership, Phase};
use crate::subagent_lifecycle::WorkerOutcome;

const BUDGET: Duration = Duration::from_secs(600);

fn state(
    phase: Phase,
    outcome: Option<(Outcome, &str)>,
    ownership: Ownership,
    last_progress_at: DateTime<Utc>,
) -> NativeUnitState {
    NativeUnitState {
        manifest: NativeManifest {
            v: 1,
            stage_id: "native-consumers".to_string(),
            unit_id: "c1".to_string(),
            invocation_id: "inv-0123456789abcdef0123456789abcdef".to_string(),
            owner_session_id: "session-1".to_string(),
            workspace_root: PathBuf::from("/work"),
            brief_path: PathBuf::from("/work/brief.md"),
            brief_sha256: "0".repeat(64),
            model: "gpt-6.1-terra".to_string(),
            effort: "xhigh".to_string(),
            deadline_secs: 1500,
            created_at: last_progress_at,
        },
        phase,
        outcome: outcome.map(|(outcome, _)| outcome),
        outcome_reason: outcome.map(|(_, reason)| reason.to_string()),
        ownership,
        thread_id: Some("thr-1".to_string()),
        cancel_requested: false,
        last_progress_at,
        report: None,
        messages: Vec::new(),
    }
}

fn terminal(outcome: Outcome, reason: &str, ownership: Ownership) -> NativeUnitState {
    state(
        Phase::Terminal,
        Some((outcome, reason)),
        ownership,
        Utc::now(),
    )
}

#[test]
fn unverified_ownership_is_never_success() {
    let now = Utc::now();
    let unverified = terminal(Outcome::Succeeded, "turn completed", Ownership::Unverified);
    let outcome = outcome_for(&unverified, now, BUDGET);
    assert!(
        matches!(outcome, WorkerOutcome::Unknown(_)),
        "unverified ownership must not report success: {outcome:?}"
    );

    let held = terminal(Outcome::Succeeded, "turn completed", Ownership::Held);
    let outcome = outcome_for(&held, now, BUDGET);
    assert_eq!(
        outcome,
        WorkerOutcome::Active,
        "held ownership is still running"
    );
}

#[test]
fn quiet_running_worker_is_stalled() {
    let now = Utc::now();
    let quiet = state(
        Phase::Running,
        None,
        Ownership::Held,
        now - chrono::Duration::seconds(700),
    );
    let outcome = outcome_for(&quiet, now, BUDGET);
    assert!(
        matches!(outcome, WorkerOutcome::Stalled(_)),
        "700 s without progress over a 600 s budget is a stall: {outcome:?}"
    );

    let busy = state(
        Phase::Running,
        None,
        Ownership::Held,
        now - chrono::Duration::seconds(100),
    );
    let outcome = outcome_for(&busy, now, BUDGET);
    assert_eq!(outcome, WorkerOutcome::Active, "recent progress is active");
}

#[test]
fn timed_out_worker_is_failed() {
    let now = Utc::now();
    let timed_out = terminal(Outcome::TimedOut, "deadline", Ownership::Released);
    match outcome_for(&timed_out, now, BUDGET) {
        WorkerOutcome::Failed(reason) => {
            assert!(reason.contains("deadline"), "carries the reason: {reason}");
        }
        other => panic!("TimedOut must map to Failed, got {other:?}"),
    }

    let cancelled = terminal(Outcome::Cancelled, "operator cancel", Ownership::Released);
    let outcome = outcome_for(&cancelled, now, BUDGET);
    assert!(
        matches!(outcome, WorkerOutcome::Cancelled(_)),
        "Cancelled maps to Cancelled: {outcome:?}"
    );

    let unknown = terminal(Outcome::Unknown, "lost the stream", Ownership::Released);
    let outcome = outcome_for(&unknown, now, BUDGET);
    assert!(
        matches!(outcome, WorkerOutcome::Unknown(_)),
        "Unknown maps to Unknown: {outcome:?}"
    );
}
