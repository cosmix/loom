use super::*;
use crate::host_verify::store::{append_progress, save_current};
use crate::host_verify::{new_job_id, CriterionOutcome, JobRecord};
use chrono::Duration;
use tempfile::TempDir;

const STAGE: &str = "stage-1";
const SESSION: &str = "session-1";

fn record(kind: JobKind, state: JobState, now: DateTime<Utc>) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        stage_id: STAGE.to_string(),
        session_id: SESSION.to_string(),
        kind,
        head: "head".to_string(),
        tree_digest: "digest".to_string(),
        no_cache: false,
        criteria: None,
        state,
        requests: vec![],
        queued_at: now - Duration::seconds(400),
        started_at: (state != JobState::Queued).then(|| now - Duration::seconds(372)),
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

fn read(dir: &TempDir, session: Option<&str>, now: DateTime<Utc>) -> Option<ContractTesting> {
    contract_testing(dir.path(), STAGE, session, 11, now)
}

fn saved(kind: JobKind, state: JobState, now: DateTime<Utc>) -> (TempDir, JobRecord) {
    let dir = tempfile::tempdir().unwrap();
    let job = record(kind, state, now);
    save_current(dir.path(), &job).unwrap();
    (dir, job)
}

#[test]
fn running_job_counts_contract_outcomes_and_elapsed() {
    let now = Utc::now();
    let (dir, job) = saved(JobKind::Freeze, JobState::Running, now);
    for label in ["contract:a", "contract:b", "format"] {
        append_progress(dir.path(), STAGE, &job.id, &outcome(label)).unwrap();
    }
    let testing = read(&dir, Some(SESSION), now).unwrap();
    assert_eq!(
        testing,
        ContractTesting {
            done: 2,
            total: 11,
            queued: false,
            running_secs: Some(372)
        }
    );
    assert_eq!(testing.text(), "contracts written, testing 2/11 · 6m12s");
}

#[test]
fn queued_job_waits_to_test() {
    let now = Utc::now();
    let (dir, _) = saved(JobKind::Freeze, JobState::Queued, now);
    let testing = read(&dir, Some(SESSION), now).unwrap();
    assert!(testing.queued && testing.running_secs.is_none());
    assert_eq!(testing.text(), "contracts written, waiting to test");
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
        let (dir, _) = saved(JobKind::Freeze, state, now);
        assert_eq!(read(&dir, Some(SESSION), now), None, "{state:?}");
    }
}

#[test]
fn check_and_complete_jobs_give_none() {
    let now = Utc::now();
    for kind in [JobKind::Check, JobKind::Complete] {
        let (dir, _) = saved(kind, JobState::Running, now);
        assert_eq!(read(&dir, Some(SESSION), now), None, "{kind:?}");
    }
}

#[test]
fn a_different_or_missing_session_gives_none() {
    let now = Utc::now();
    let (dir, _) = saved(JobKind::Freeze, JobState::Running, now);
    assert_eq!(read(&dir, Some("session-2"), now), None);
    assert_eq!(read(&dir, None, now), None);
}

#[test]
fn missing_and_corrupt_files_give_none() {
    let now = Utc::now();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read(&dir, Some(SESSION), now), None);

    let (dir, job) = saved(JobKind::Freeze, JobState::Running, now);
    let progress = crate::host_verify::store::job_dir(dir.path(), STAGE, &job.id)
        .unwrap()
        .join("progress.jsonl");
    std::fs::write(progress, "not json\n").unwrap();
    assert_eq!(read(&dir, Some(SESSION), now), None);

    let current = crate::host_verify::store::stage_dir(dir.path(), STAGE)
        .unwrap()
        .join("current.json");
    std::fs::write(current, "{").unwrap();
    assert_eq!(read(&dir, Some(SESSION), now), None);
}

#[test]
fn a_running_job_whose_child_is_dead_gives_none() {
    let now = Utc::now();
    let dir = tempfile::tempdir().unwrap();
    let mut job = record(JobKind::Freeze, JobState::Running, now);
    job.pid = Some(999_999_999);
    save_current(dir.path(), &job).unwrap();
    assert_eq!(read(&dir, Some(SESSION), now), None);

    job.pid = Some(std::process::id());
    save_current(dir.path(), &job).unwrap();
    assert!(
        read(&dir, Some(SESSION), now).is_some(),
        "a live child tests"
    );
}
