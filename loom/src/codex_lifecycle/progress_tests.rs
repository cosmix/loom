//! Progress classification for a running companion job: process liveness first,
//! then freshness of anything the job writes.

use std::fs::File;
use std::process::Command;

use chrono::Utc;
use tempfile::TempDir;

use super::*;

const BUDGET: Duration = Duration::from_secs(600);

fn running_job(pid: Option<u32>) -> CompanionJob {
    CompanionJob {
        id: "task-abc".to_owned(),
        status: Some("running".to_owned()),
        phase: Some("editing".to_owned()),
        pid,
        started_at: Some(Utc::now()),
        ..CompanionJob::default()
    }
}

/// A pid that has exited and been reaped: its number is free, so signalling it
/// answers ESRCH exactly as a dead companion worker's would.
fn reaped_pid() -> u32 {
    let mut child = Command::new("true").spawn().expect("spawning true");
    let pid = child.id();
    child.wait().expect("reaping true");
    pid
}

#[test]
fn running_job_whose_process_is_gone_is_dead() {
    let job = running_job(Some(reaped_pid()));

    let detail = match companion_progress(&job, BUDGET, SystemTime::now()) {
        CompanionProgress::Dead(detail) => detail,
        other => panic!("expected dead, got {other:?}"),
    };
    assert!(detail.contains("task-abc"), "{detail}");
    assert!(detail.contains("is gone while its record says"), "{detail}");
    assert!(detail.contains("running/editing"), "{detail}");
}

#[test]
fn live_process_with_a_fresh_log_is_running() {
    let temp = TempDir::new().unwrap();
    let log = temp.path().join("task-abc.log");
    File::create(&log).unwrap();
    let mut job = running_job(Some(std::process::id()));
    job.log_file = Some(log);
    // Stamped long ago: the fresh log mtime, not the record, must decide.
    job.started_at = Some(Utc::now() - chrono::Duration::hours(2));

    assert_eq!(
        companion_progress(&job, BUDGET, SystemTime::now()),
        CompanionProgress::Running
    );
}

#[test]
fn live_process_with_an_old_log_is_stalled() {
    let temp = TempDir::new().unwrap();
    let log = temp.path().join("task-abc.log");
    let file = File::create(&log).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(3_600))
        .unwrap();
    let mut job = running_job(Some(std::process::id()));
    job.log_file = Some(log.clone());
    job.started_at = Some(Utc::now() - chrono::Duration::hours(2));

    let detail = match companion_progress(&job, BUDGET, SystemTime::now()) {
        CompanionProgress::Stalled(detail) => detail,
        other => panic!("expected stalled, got {other:?}"),
    };
    assert!(detail.contains("shows no progress for 3600s"), "{detail}");
    assert!(detail.contains("stall budget 600s"), "{detail}");
    assert!(detail.contains(&log.display().to_string()), "{detail}");
}

/// A queued job has no worker process yet, so an absent pid is normal.
#[test]
fn queued_job_without_a_pid_is_running() {
    let job = CompanionJob {
        id: "task-abc".to_owned(),
        status: Some("queued".to_owned()),
        phase: Some("queued".to_owned()),
        created_at: Some(Utc::now()),
        ..CompanionJob::default()
    };

    assert_eq!(
        companion_progress(&job, BUDGET, SystemTime::now()),
        CompanionProgress::Running
    );
}

/// A queued job may sit behind the companion's concurrency cap far longer than
/// the stall budget; that wait is normal, not evidence of a stall.
#[test]
fn long_queued_job_is_running() {
    let job = CompanionJob {
        id: "task-abc".to_owned(),
        status: Some("queued".to_owned()),
        phase: Some("queued".to_owned()),
        created_at: Some(Utc::now() - chrono::Duration::hours(2)),
        ..CompanionJob::default()
    };

    assert_eq!(
        companion_progress(&job, BUDGET, SystemTime::now()),
        CompanionProgress::Running
    );
}

/// A job the ledger already calls terminal is never reclassified here, however
/// old its timestamps are.
#[test]
fn terminal_job_is_left_to_the_ledger() {
    let job = CompanionJob {
        id: "task-abc".to_owned(),
        status: Some("completed".to_owned()),
        phase: Some("done".to_owned()),
        thread_id: Some("thread-1".to_owned()),
        turn_id: Some("turn-1".to_owned()),
        completed_at: Some(Utc::now() - chrono::Duration::hours(2)),
        created_at: Some(Utc::now() - chrono::Duration::hours(2)),
        result: Some(serde_json::json!({"text": "done"})),
        ..CompanionJob::default()
    };

    assert_eq!(
        companion_progress(&job, BUDGET, SystemTime::now()),
        CompanionProgress::Running
    );
}

/// With no log, no start and no creation stamp there is nothing to measure, and
/// silence is not evidence of a stall.
#[test]
fn job_without_any_freshness_signal_is_running() {
    let mut job = running_job(Some(std::process::id()));
    job.started_at = None;

    assert_eq!(
        companion_progress(&job, BUDGET, SystemTime::now()),
        CompanionProgress::Running
    );
}
