use super::*;
use crate::host_verify::store::{append_progress, save_current};
use crate::host_verify::{new_job_id, CriterionOutcome, JobRecord};
use crate::models::stage::Stage;
use chrono::Duration;
use tempfile::TempDir;

const STAGE: &str = "stage-1";

fn record(kind: JobKind, state: JobState, now: DateTime<Utc>) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        submission_order: 0,
        stage_id: STAGE.to_string(),
        session_id: "session-1".to_string(),
        kind,
        head: "head".to_string(),
        tree_digest: "digest".to_string(),
        no_cache: false,
        criteria: None,
        state,
        requests: vec![],
        queued_at: now - Duration::seconds(400),
        started_at: (state != JobState::Queued).then(|| now - Duration::seconds(192)),
        finished_at: None,
        pid: None,
        detail: None,
    }
}

fn outcome(label: &str) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed: true,
        summary: String::new(),
        log: None,
    }
}

fn saved(kind: JobKind, state: JobState, now: DateTime<Utc>) -> (TempDir, JobRecord) {
    let dir = tempfile::tempdir().unwrap();
    let job = record(kind, state, now);
    save_current(dir.path(), &job).unwrap();
    (dir, job)
}

fn read(dir: &TempDir, now: DateTime<Utc>) -> Option<VerdictCompleting> {
    verdict_completing(dir.path(), STAGE, 14, now)
}

#[test]
fn running_job_counts_acceptance_outcomes_and_elapsed() {
    let now = Utc::now();
    let (dir, job) = saved(JobKind::VerdictComplete, JobState::Running, now);
    for label in ["acceptance-0", "acceptance-1", "impact-tests"] {
        append_progress(dir.path(), STAGE, &job.id, &outcome(label)).unwrap();
    }
    let completing = read(&dir, now).unwrap();
    assert_eq!(
        completing,
        VerdictCompleting {
            done: 2,
            total: 14,
            queued: false,
            running_secs: Some(192)
        }
    );
    assert_eq!(
        completing.text(),
        "disputes ruled, daemon verifying 2/14 · 3m12s"
    );
}

#[test]
fn queued_job_waits_to_verify() {
    let now = Utc::now();
    let (dir, _) = saved(JobKind::VerdictComplete, JobState::Queued, now);
    let completing = read(&dir, now).unwrap();
    assert!(completing.queued && completing.running_secs.is_none());
    assert_eq!(completing.text(), "disputes ruled, waiting to verify");
}

#[test]
fn other_job_kinds_give_none() {
    let now = Utc::now();
    for kind in [JobKind::Freeze, JobKind::Check, JobKind::Complete] {
        let (dir, _) = saved(kind, JobState::Running, now);
        assert_eq!(read(&dir, now), None, "{kind:?}");
    }
}

#[test]
fn terminal_job_gives_none() {
    let now = Utc::now();
    for state in [
        JobState::Passed,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Interrupted,
    ] {
        let (dir, _) = saved(JobKind::VerdictComplete, state, now);
        assert_eq!(read(&dir, now), None, "{state:?}");
    }
}

#[test]
fn missing_and_corrupt_files_give_none() {
    let now = Utc::now();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read(&dir, now), None);

    let (dir, job) = saved(JobKind::VerdictComplete, JobState::Running, now);
    let progress = crate::host_verify::store::job_dir(dir.path(), STAGE, &job.id)
        .unwrap()
        .join("progress.jsonl");
    std::fs::write(progress, "not json\n").unwrap();
    assert_eq!(read(&dir, now), None);
}

#[test]
fn a_running_job_whose_child_is_dead_gives_none() {
    let now = Utc::now();
    let dir = tempfile::tempdir().unwrap();
    let mut job = record(JobKind::VerdictComplete, JobState::Running, now);
    job.pid = Some(999_999_999);
    save_current(dir.path(), &job).unwrap();
    assert_eq!(read(&dir, now), None);

    job.pid = Some(std::process::id());
    save_current(dir.path(), &job).unwrap();
    assert!(read(&dir, now).is_some(), "a live child verifies");
}

#[test]
fn only_a_stage_awaiting_adjudication_is_read() {
    let now = Utc::now();
    let (dir, _) = saved(JobKind::VerdictComplete, JobState::Running, now);
    let mut stage = Stage::new("stage".to_string(), None);
    stage.id = STAGE.to_string();
    stage.status = StageStatus::Executing;
    assert_eq!(verdict_completing_for(&stage, dir.path(), now), None);

    stage.status = StageStatus::NeedsAdjudication;
    let completing = verdict_completing_for(&stage, dir.path(), now).unwrap();
    assert_eq!((completing.done, completing.total), (0, 0));
}
