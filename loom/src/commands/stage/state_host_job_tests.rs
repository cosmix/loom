//! `loom stage reset` cancels the stage's host job once the reset stands.

use super::loop_recovery::ResetRuntime;
use super::{reset_and_cancel, LiveAgent};
use crate::fs::work_dir::write_terminal_config;
use crate::host_verify::store::load_current;
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobState};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage};
use anyhow::Result;
use std::path::Path;
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
