//! The hold a stage's running host job puts on its sessions' escalation.

use chrono::{Duration as ChronoDuration, Utc};
use tempfile::TempDir;

use super::*;
use crate::host_verify::{new_job_id, store, JobKind, JobState};

/// Stage-1, owned by session-1.
fn stage() -> Stage {
    Stage {
        id: "stage-1".to_string(),
        session: Some("session-1".to_string()),
        ..Stage::default()
    }
}

/// Save stage-1's current job in `state`, started `started_secs_ago`.
fn save_job(work_dir: &Path, state: JobState, started_secs_ago: i64) {
    let started = Utc::now() - ChronoDuration::seconds(started_secs_ago);
    let record = JobRecord {
        id: new_job_id(),
        stage_id: "stage-1".to_string(),
        session_id: "session-1".to_string(),
        kind: JobKind::Complete,
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
        no_cache: false,
        state,
        requests: Vec::new(),
        queued_at: started,
        started_at: Some(started),
        finished_at: None,
        pid: None,
        detail: None,
    };
    store::save_current(work_dir, &record).unwrap();
}

#[test]
fn a_running_job_within_its_deadline_holds_the_escalation() {
    let temp = TempDir::new().unwrap();
    save_job(temp.path(), JobState::Running, 60);

    assert!(host_job_holds_escalation(temp.path(), &stage(), Utc::now()));

    let mut detection = Detection::new();
    detection.record_hung_report("session-1", 310, 300, Some(HELD_LINE));
    assert!(!detection.hung_report_due("session-1", 5_000, 300, Some(HELD_LINE)));
    detection.record_hung_report("session-1", 5_000, 300, Some(HELD_LINE));
    let (reported, escalated) = detection.hung_latches();
    assert!(
        reported.contains("session-1"),
        "the warning is still reported"
    );
    assert!(
        !escalated.contains("session-1"),
        "the escalation is not spent"
    );
}

#[test]
fn a_finished_or_overdue_job_releases_the_escalation() {
    let temp = TempDir::new().unwrap();
    assert!(!host_job_holds_escalation(
        temp.path(),
        &stage(),
        Utc::now()
    ));

    save_job(temp.path(), JobState::Passed, 60);
    assert!(!host_job_holds_escalation(
        temp.path(),
        &stage(),
        Utc::now()
    ));

    let past_deadline = i64::try_from(job_deadline_secs(&stage())).unwrap() + 1;
    save_job(temp.path(), JobState::Running, past_deadline);
    assert!(!host_job_holds_escalation(
        temp.path(),
        &stage(),
        Utc::now()
    ));

    let mut detection = Detection::new();
    detection.record_hung_report("session-1", 310, 300, Some(HELD_LINE));
    assert!(
        detection.hung_report_due("session-1", 900, 300, None),
        "once released, the held escalation is reported"
    );
}

#[test]
fn a_job_relayed_by_another_session_holds_nothing() {
    let temp = TempDir::new().unwrap();
    save_job(temp.path(), JobState::Running, 60);
    let now = Utc::now();
    let successor = Stage {
        session: Some("session-2".to_string()),
        ..stage()
    };

    assert!(host_job_holds_session(
        temp.path(),
        &stage(),
        "session-1",
        now
    ));
    assert!(
        !host_job_holds_session(temp.path(), &successor, "session-2", now),
        "a crashed session's job held its successor's escalation"
    );
    assert!(!host_job_holds_escalation(temp.path(), &successor, now));
    let unowned = Stage {
        session: None,
        ..stage()
    };
    assert!(!host_job_holds_escalation(temp.path(), &unowned, now));
}
