//! Tests for what a retry does to a v2 stage's contract respawn budget.

use tempfile::TempDir;

use super::*;
use crate::verify::contracts::store::{attempts_spent, spend_attempt};
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::create_stage;

#[test]
fn retry_refuses_unknown_assigned_session_without_force() {
    let temp = TempDir::new().unwrap();
    let sessions = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("session-1.md"), "malformed").unwrap();

    let error = ensure_retry_session_safe(temp.path(), "s1", "session-1", None, false).unwrap_err();
    assert!(format!("{error:#}").contains("Cannot establish whether"));
    assert!(ensure_retry_session_safe(temp.path(), "s1", "session-1", None, true).is_ok());
}

#[test]
fn retry_refuses_a_verified_live_session_without_force() {
    let temp = TempDir::new().unwrap();
    let mut session = crate::models::session::Session::new();
    session.assign_to_stage("s1".to_string());
    session.status = crate::models::session::SessionStatus::Running;
    crate::fs::session_files::save_session(&session, temp.path()).unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(
        temp.path(),
        &session,
        std::process::id(),
    )
    .unwrap();

    assert!(ensure_retry_session_safe(temp.path(), "s1", &session.id, None, false).is_err());
    assert!(ensure_retry_session_safe(temp.path(), "s1", &session.id, None, true).is_ok());
}

/// The stage `spent_contract_stage` leaves on disk, as `retry` read it.
const WRITER_BLOCKED: (&StageStatus, Option<&str>) = (&StageStatus::Blocked, Some("writer-1"));

/// The admission lock covers every launch from its write-ahead through its
/// backend's answer, so a retry judging the stage's session waits it out.
#[test]
fn retry_waits_for_the_launch_holding_the_stage_admission() {
    let temp = TempDir::new().unwrap();
    let mut stage = crate::models::stage::Stage::new("s1".to_string(), None);
    stage.id = "s1".to_string();
    stage.status = StageStatus::Blocked;
    create_stage(&stage, temp.path()).unwrap();
    let admission = crate::fs::stage_admission::lock(temp.path(), "s1").unwrap();

    let work = temp.path().to_path_buf();
    let retrying = std::thread::spawn(move || retry_in(&work, "s1".to_string(), false, None));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked,
        "the retry requeued the stage under a launch in flight"
    );

    drop(admission);
    retrying.join().unwrap().unwrap();
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
}

/// The status alone does not show a launch that finished after `retry` read
/// the stage: the session it judged is no longer the stage's.
#[test]
fn retry_refuses_a_stage_whose_session_changed_after_the_probe() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    let read_earlier = (&StageStatus::Blocked, Some("a-dead-session"));
    let error = persist_retry_delta("s1", temp.path(), &planned, read_earlier, false).unwrap_err();

    assert!(format!("{error:#}").contains("session"), "{error:#}");
    let stage = load_stage("s1", temp.path()).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.session.as_deref(), Some("writer-1"));
}

/// A blocked v2 stage whose contract writers spent the whole budget, and the
/// state `retry` plans for it.
fn spent_contract_stage(work_dir: &Path) -> crate::models::stage::Stage {
    let mut stage = contract_stage("s1", "writer-1");
    stage.status = StageStatus::Blocked;
    create_stage(&stage, work_dir).unwrap();
    for _ in 0..3 {
        spend_attempt(work_dir, "s1").unwrap();
    }
    stage.session = None;
    stage
}

#[test]
fn retry_grants_an_unfrozen_stage_a_fresh_contract_budget() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    persist_retry_delta("s1", temp.path(), &planned, WRITER_BLOCKED, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 0);
}

#[test]
fn retry_leaves_a_frozen_stage_budget_alone() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());
    write_test_freeze(temp.path(), "s1", "writer-1");

    persist_retry_delta("s1", temp.path(), &planned, WRITER_BLOCKED, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}

/// A refused transition (the on-disk status no longer matches what `retry`
/// planned against) must not reset the budget: the reset sits inside the
/// same locked closure, after the status re-validation, so its error path
/// is never reached.
#[test]
fn retry_refused_transition_leaves_budget_unspent() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    // The stage is on disk as `Blocked`, but `retry` is told to expect
    // `Executing` — the closure's own re-validation refuses the mismatch.
    let expected = (&StageStatus::Executing, Some("writer-1"));
    let result = persist_retry_delta("s1", temp.path(), &planned, expected, false);

    assert!(result.is_err());
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}

/// The silence report: what a refused retry says about a live session that
/// has stopped making progress. Budget 120 s throughout, so the worked line is
/// 360 s and the never-worked line 120 s.
mod silence_report_tests {
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;

    use super::super::silence::silence_report;
    use crate::host_verify::{new_job_id, store, JobKind, JobRecord, JobState};
    use crate::models::session::Session;
    use crate::models::stage::{Stage, StageStatus};
    use crate::orchestrator::monitor::heartbeat::Heartbeat;

    fn executing_stage() -> Stage {
        Stage {
            id: "s1".to_string(),
            status: StageStatus::Executing,
            session: Some("session-1".to_string()),
            subagent_timeout_secs: Some(120),
            ..Stage::default()
        }
    }

    fn session_created(now: DateTime<Utc>, secs_ago: i64) -> Session {
        let mut session = Session::new();
        session.id = "session-1".to_string();
        session.created_at = now - Duration::seconds(secs_ago);
        session
    }

    /// A heartbeat of `session_id` whose last progress was `silent_secs` ago.
    fn heartbeat(
        now: DateTime<Utc>,
        session_id: &str,
        silent_secs: i64,
        tool: Option<&str>,
    ) -> Heartbeat {
        let progress = now - Duration::seconds(silent_secs);
        let mut heartbeat = Heartbeat::new("s1".to_string(), session_id.to_string());
        heartbeat.timestamp = progress;
        heartbeat.progress_at = Some(progress);
        heartbeat.last_tool = tool.map(str::to_string);
        heartbeat
    }

    fn report(
        work_dir: &std::path::Path,
        stage: &Stage,
        session: Option<&Session>,
        heartbeat: Option<&Heartbeat>,
        now: DateTime<Utc>,
    ) -> Option<String> {
        silence_report(work_dir, stage, "session-1", session, heartbeat, true, now)
    }

    /// The report for `executing_stage()` whose session spawned `spawned_secs_ago` s ago.
    fn report_of(spawned_secs_ago: i64, beat: Option<&Heartbeat>, now: DateTime<Utc>) -> String {
        let session = session_created(now, spawned_secs_ago);
        let temp = TempDir::new().unwrap();
        report(temp.path(), &executing_stage(), Some(&session), beat, now).expect("a report")
    }

    #[test]
    fn a_worked_session_is_requeued_at_three_budgets_less_its_silence() {
        let now = Utc::now();
        let beat = heartbeat(now, "session-1", 100, Some("Bash"));

        let text = report_of(1_000, Some(&beat), now);

        assert!(text.contains("no progress for 100 s"), "{text}");
        assert!(text.contains("at the earliest in 260 s"), "{text}");
        assert!(text.contains("requeue the stage"), "{text}");
        assert!(text.contains("can hold it longer"), "{text}");
    }

    #[test]
    fn a_never_worked_session_is_parked_at_one_budget_less_its_silence() {
        let now = Utc::now();
        let beat = heartbeat(now, "session-1", 100, None);

        let text = report_of(1_000, Some(&beat), now);

        assert!(text.contains("at the earliest in 20 s"), "{text}");
        assert!(
            text.contains("park the stage in NeedsHumanReview"),
            "{text}"
        );
    }

    #[test]
    fn a_young_session_without_a_heartbeat_is_parked_when_its_budget_runs_out() {
        let now = Utc::now();

        let text = report_of(50, None, now);

        assert!(text.contains("no progress for 50 s"), "{text}");
        assert!(text.contains("at the earliest in 70 s"), "{text}");
        assert!(
            text.contains("park the stage in NeedsHumanReview"),
            "{text}"
        );
    }

    #[test]
    fn the_age_counts_progress_and_not_observation_polls() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let session = session_created(now, 1_000);
        let mut beat = heartbeat(now, "session-1", 100, Some("Bash"));
        beat.timestamp = now;

        let text = report(
            temp.path(),
            &executing_stage(),
            Some(&session),
            Some(&beat),
            now,
        )
        .expect("a report");

        assert!(text.contains("no progress for 100 s"), "{text}");
    }

    #[test]
    fn another_sessions_heartbeat_is_ignored_and_the_session_is_judged_from_spawn() {
        let now = Utc::now();
        let previous = heartbeat(now, "old-session", 5, Some("Bash"));

        let text = report_of(200, Some(&previous), now);

        assert!(text.contains("no progress for 200 s"), "{text}");
        assert!(
            text.contains("park the stage in NeedsHumanReview"),
            "{text}"
        );
        assert!(text.contains("eligible now"), "{text}");
    }

    #[test]
    fn a_spent_stall_budget_parks_a_worked_session() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let session = session_created(now, 1_000);
        let beat = heartbeat(now, "session-1", 100, Some("Bash"));
        let mut stage = executing_stage();
        stage.stall_recoveries = 2;

        let text = report(temp.path(), &stage, Some(&session), Some(&beat), now).expect("a report");

        assert!(
            text.contains("park the stage in NeedsHumanReview"),
            "{text}"
        );
        assert!(!text.contains("requeue"), "{text}");
    }

    #[test]
    fn a_running_host_job_of_this_session_holds_recovery() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let started = now - Duration::seconds(60);
        let record = JobRecord {
            id: new_job_id(),
            submission_order: 0,
            stage_id: "s1".to_string(),
            session_id: "session-1".to_string(),
            kind: JobKind::Complete,
            head: "a".repeat(40),
            tree_digest: "b".repeat(64),
            no_cache: false,
            criteria: None,
            contracts: false,
            state: JobState::Running,
            requests: Vec::new(),
            queued_at: started,
            started_at: Some(started),
            finished_at: None,
            pid: None,
            detail: None,
            at_base: false,
        };
        store::save_current(temp.path(), &record).unwrap();
        let session = session_created(now, 1_000);
        let beat = heartbeat(now, "session-1", 100, Some("Bash"));

        let text = report(
            temp.path(),
            &executing_stage(),
            Some(&session),
            Some(&beat),
            now,
        )
        .expect("a report");

        assert!(text.contains("a running host job holds recovery"), "{text}");
        assert!(!text.contains("at the earliest"), "{text}");
    }

    #[test]
    fn a_stage_that_is_not_executing_gets_its_reason_and_no_threshold() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let session = session_created(now, 1_000);
        let beat = heartbeat(now, "session-1", 100, Some("Bash"));
        let mut stage = executing_stage();
        stage.status = StageStatus::Blocked;

        let text = report(temp.path(), &stage, Some(&session), Some(&beat), now).expect("a report");

        assert!(text.contains("only an executing stage"), "{text}");
        assert!(!text.contains("at the earliest"), "{text}");
    }

    #[test]
    fn a_stopped_daemon_is_named_as_the_reason() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let session = session_created(now, 1_000);
        let beat = heartbeat(now, "session-1", 100, Some("Bash"));

        let text = silence_report(
            temp.path(),
            &executing_stage(),
            "session-1",
            Some(&session),
            Some(&beat),
            false,
            now,
        )
        .expect("a report");

        assert!(text.contains("the daemon is not running"), "{text}");
        assert!(!text.contains("at the earliest"), "{text}");
    }

    #[test]
    fn no_heartbeat_and_no_session_record_leave_the_refusal_unchanged() {
        let (temp, now) = (TempDir::new().unwrap(), Utc::now());
        let other = heartbeat(now, "old-session", 100, Some("Bash"));

        assert_eq!(
            report(temp.path(), &executing_stage(), None, None, now),
            None
        );
        assert_eq!(
            report(temp.path(), &executing_stage(), None, Some(&other), now),
            None
        );
    }
}
