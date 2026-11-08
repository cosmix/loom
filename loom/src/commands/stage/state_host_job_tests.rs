//! `loom stage reset` cancels the stage's host job once the reset stands.

use super::loop_recovery::ResetRuntime;
use super::{reset_and_cancel, LiveAgent};
use crate::fs::work_dir::write_terminal_config;
use crate::host_verify::store::{load_current, load_job_record, load_pending, save_current};
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobRecord, JobState};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage};
use anyhow::Result;
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;

/// A runtime for a stage with no agent: nothing to kill, nothing alive.
struct NoAgents;

impl ResetRuntime for NoAgents {
    fn kill(&self, _work_dir: &Path, _agents: &[LiveAgent]) {}

    fn identity_missing(&self, _work_dir: &Path, _agent: &LiveAgent) -> bool {
        false
    }

    fn is_alive(&self, _work_dir: &Path, _agent: &LiveAgent) -> Result<bool> {
        Ok(false)
    }

    fn wait(&self) {}
}

struct ReplacingJob {
    replacement: Mutex<Option<String>>,
}

impl ResetRuntime for ReplacingJob {
    fn kill(&self, _work_dir: &Path, _agents: &[LiveAgent]) {}
    fn identity_missing(&self, _work_dir: &Path, _agent: &LiveAgent) -> bool {
        false
    }
    fn is_alive(&self, _work_dir: &Path, _agent: &LiveAgent) -> Result<bool> {
        Ok(false)
    }
    fn wait(&self) {}

    fn after_scan(&self, work_dir: &Path, stage_id: &str) {
        let request = SubmitRequest {
            stage_id,
            session_id: "later-session",
            request_id: "r2",
            kind: JobKind::Check,
            head: "cccccccccccccccccccccccccccccccccccccccc",
            tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            no_cache: false,
            criteria: None,
        };
        let Submission::Superseded { queued, .. } =
            submit(work_dir, &request, chrono::Utc::now()).unwrap()
        else {
            panic!("expected replacement job");
        };
        *self.replacement.lock().unwrap() = Some(queued.id);
    }
}

#[test]
fn reset_cancels_the_stages_host_job() {
    let temp = TempDir::new().unwrap();
    let work = temp.path();
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(work, &tmux).unwrap();
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work).unwrap();
    let request = SubmitRequest {
        stage_id: "alpha",
        session_id: "session-a",
        request_id: "r1",
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    let Submission::Queued(job) = submit(work, &request, chrono::Utc::now()).unwrap() else {
        panic!("expected a queued job");
    };

    reset_and_cancel(work, "alpha", true, true, &NoAgents).unwrap();

    let stage = load_stage("alpha", work).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    let current = load_current(work, "alpha")
        .unwrap()
        .expect("a current record");
    assert_eq!(current.id, job.id);
    assert_eq!(current.state, JobState::Cancelled);
    assert_eq!(current.detail.as_deref(), Some("stage reset"));
}

#[test]
fn reset_does_not_cancel_a_job_replaced_after_its_scan() {
    let temp = TempDir::new().unwrap();
    let work = temp.path();
    write_terminal_config(
        work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work).unwrap();
    let request = SubmitRequest {
        stage_id: "alpha",
        session_id: "old-session",
        request_id: "r1",
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    assert!(matches!(
        submit(work, &request, chrono::Utc::now()).unwrap(),
        Submission::Queued(_)
    ));
    let runtime = ReplacingJob {
        replacement: Mutex::new(None),
    };

    reset_and_cancel(work, "alpha", false, false, &runtime).unwrap();

    let replacement = runtime.replacement.lock().unwrap().clone().unwrap();
    let current = load_current(work, "alpha").unwrap().unwrap();
    assert_eq!(current.id, replacement);
    assert_eq!(current.state, JobState::Queued);
}

struct PromotingJob;

impl ResetRuntime for PromotingJob {
    fn kill(&self, _work_dir: &Path, _agents: &[LiveAgent]) {}
    fn identity_missing(&self, _work_dir: &Path, _agent: &LiveAgent) -> bool {
        false
    }
    fn is_alive(&self, _work_dir: &Path, _agent: &LiveAgent) -> Result<bool> {
        Ok(false)
    }
    fn wait(&self) {}

    fn after_scan(&self, work_dir: &Path, stage_id: &str) {
        let mut current = load_current(work_dir, stage_id).unwrap().unwrap();
        current.state = JobState::Passed;
        save_current(work_dir, &current).unwrap();
        crate::host_verify::pending::promote(work_dir, stage_id).unwrap();
    }
}

fn waiting_fixture() -> (TempDir, JobRecord, JobRecord) {
    let temp = TempDir::new().unwrap();
    let work = temp.path();
    write_terminal_config(
        work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    let mut stage = Stage::new("alpha".into(), None);
    stage.id = "alpha".into();
    stage.status = StageStatus::Executing;
    save_stage(&stage, work).unwrap();
    let request = SubmitRequest {
        stage_id: "alpha",
        session_id: "session-a",
        request_id: "r1",
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: Some(vec![1]),
    };
    let Submission::Queued(current) = submit(work, &request, chrono::Utc::now()).unwrap() else {
        panic!("expected current job");
    };
    let request = SubmitRequest {
        request_id: "r2",
        criteria: Some(vec![2]),
        ..request
    };
    let Submission::Waiting { pending, .. } = submit(work, &request, chrono::Utc::now()).unwrap()
    else {
        panic!("expected waiting job");
    };
    (temp, current, *pending)
}

fn assert_reset_cancelled(work: &Path, id: &str) {
    let record = load_job_record(work, "alpha", id).unwrap().unwrap();
    assert_eq!(record.state, JobState::Cancelled);
    assert_eq!(record.detail.as_deref(), Some("stage reset"));
    assert!(load_pending(work, "alpha").unwrap().is_none());
}

#[test]
fn reset_cancels_a_waiting_job_promoted_after_its_scan() {
    let (temp, first, waiting) = waiting_fixture();
    reset_and_cancel(temp.path(), "alpha", false, true, &PromotingJob).unwrap();
    assert_eq!(
        load_current(temp.path(), "alpha").unwrap().unwrap().id,
        waiting.id
    );
    assert_reset_cancelled(temp.path(), &waiting.id);
    assert_eq!(
        load_job_record(temp.path(), "alpha", &first.id)
            .unwrap()
            .unwrap()
            .state,
        JobState::Passed
    );
}

#[test]
fn reset_cancels_a_waiting_job_when_current_already_ended() {
    let (temp, mut first, waiting) = waiting_fixture();
    first.state = JobState::Passed;
    save_current(temp.path(), &first).unwrap();
    reset_and_cancel(temp.path(), "alpha", false, true, &NoAgents).unwrap();
    assert_reset_cancelled(temp.path(), &waiting.id);
    assert_eq!(load_current(temp.path(), "alpha").unwrap(), Some(first));
}

#[test]
fn reset_preserves_a_genuine_replacement_of_observed_current_and_waiting_jobs() {
    let (temp, _, waiting) = waiting_fixture();
    let runtime = ReplacingJob {
        replacement: Mutex::new(None),
    };
    reset_and_cancel(temp.path(), "alpha", false, true, &runtime).unwrap();
    let current = load_current(temp.path(), "alpha").unwrap().unwrap();
    assert_eq!(
        Some(current.id.clone()),
        *runtime.replacement.lock().unwrap()
    );
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.session_id, "later-session");
    let old_waiting = load_job_record(temp.path(), "alpha", &waiting.id)
        .unwrap()
        .unwrap();
    assert_eq!(old_waiting.state, JobState::Cancelled);
    assert!(old_waiting.detail.unwrap().starts_with("superseded by "));
    assert!(load_pending(temp.path(), "alpha").unwrap().is_none());
}
