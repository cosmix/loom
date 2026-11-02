//! The operator's `--resolved` beside a host job of the stage.

use std::path::Path;

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::{merge_resolved, withdraw_or_refuse_host_job};
use crate::host_verify::store::{load_current, save_current};
use crate::host_verify::submit::{cancel, submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobRecord, JobState};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::save_stage;

const STAGE: &str = "s1";

/// A state directory holding the stage in `MergeConflict` with a queued check.
fn stage_with_a_queued_check(project: &Path) -> JobRecord {
    let work_dir = project.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("config.toml"), "").unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::MergeConflict,
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    let request = SubmitRequest {
        stage_id: STAGE,
        session_id: "resolver",
        request_id: "r1",
        kind: JobKind::Check,
        head: &"a".repeat(40),
        tree_digest: &"b".repeat(64),
        no_cache: false,
        criteria: None,
    };
    match submit(&work_dir, &request, Utc::now()).unwrap() {
        Submission::Queued(job) => job,
        other => panic!("expected a queued check, got {other:?}"),
    }
}

/// `merge_resolved` from `project`, as the error's text.
fn resolved_error(project: &Path) -> String {
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(project).unwrap();
    let result = merge_resolved(Some(STAGE.to_string()));
    std::env::set_current_dir(original).unwrap();
    format!("{:#}", result.unwrap_err())
}

fn job_state(project: &Path) -> JobState {
    let work_dir = project.join(".loom").join("work");
    load_current(&work_dir, STAGE).unwrap().unwrap().state
}

#[test]
#[serial]
fn a_running_daemon_refuses_and_leaves_the_job_alone() {
    let temp = TempDir::new().unwrap();
    let job = stage_with_a_queued_check(temp.path());
    let work_dir = temp.path().join(".loom").join("work");

    let error = format!(
        "{:#}",
        withdraw_or_refuse_host_job(STAGE, &work_dir, true).unwrap_err()
    );

    assert!(error.contains(&job.id), "{error}");
    assert!(error.contains("rerun --resolved"), "{error}");
    assert_eq!(job_state(temp.path()), JobState::Queued);
}

#[test]
#[serial]
fn without_a_daemon_a_queued_job_is_withdrawn_and_resolved_proceeds() {
    let temp = TempDir::new().unwrap();
    let job = stage_with_a_queued_check(temp.path());

    let error = resolved_error(temp.path());

    assert!(!error.contains(&job.id), "{error}");
    assert_eq!(job_state(temp.path()), JobState::Cancelled);
}

#[test]
#[serial]
fn without_a_daemon_a_running_job_is_withdrawn_and_resolved_asks_for_a_rerun() {
    let temp = TempDir::new().unwrap();
    let mut job = stage_with_a_queued_check(temp.path());
    let work_dir = temp.path().join(".loom").join("work");
    job.state = JobState::Running;
    save_current(&work_dir, &job).unwrap();

    let error = resolved_error(temp.path());

    assert!(error.contains(&job.id), "{error}");
    assert!(error.contains("watchdog"), "{error}");
    assert!(error.contains("Rerun --resolved"), "{error}");
    assert_eq!(job_state(temp.path()), JobState::Cancelled);
}

#[test]
#[serial]
fn a_job_that_ended_does_not_refuse_resolved() {
    let temp = TempDir::new().unwrap();
    let job = stage_with_a_queued_check(temp.path());
    let work_dir = temp.path().join(".loom").join("work");
    cancel(&work_dir, STAGE, "over").unwrap().unwrap();

    let error = resolved_error(temp.path());

    assert!(!error.contains(&job.id), "{error}");
}
