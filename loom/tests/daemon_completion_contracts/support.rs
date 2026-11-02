//! Completion-specific contract helpers.
use chrono::Utc;
use loom::host_verify::{new_job_id, JobKind, JobRecord, JobState};
use std::fs;
#[path = "../host_job_fixture/mod.rs"]
mod host_job_fixture;
pub use host_job_fixture::*;

impl Fixture {
    /// Commit one more file on `loom/stage-a` in the worktree; returns HEAD.
    pub fn commit_change(&self, name: &str) -> String {
        fs::write(self.worktree.join(name), "change\n").expect("write change");
        git(&self.worktree, &["add", name]);
        git(&self.worktree, &["commit", "-m", name]);
        self.head()
    }
}

/// A Complete record for stage-a by session-a, with a fresh job id; a
/// terminal `state` carries start and finish times.
pub fn complete_record(state: JobState, head: &str, tree_digest: &str) -> JobRecord {
    let now = Utc::now();
    let terminal = state.is_terminal();
    JobRecord {
        id: new_job_id(),
        submission_order: 0,
        stage_id: STAGE.to_string(),
        session_id: SESSION.to_string(),
        kind: JobKind::Complete,
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
        no_cache: false,
        criteria: None,
        state,
        requests: vec!["r1".to_string()],
        queued_at: now,
        started_at: terminal.then_some(now),
        finished_at: terminal.then_some(now),
        pid: None,
        detail: None,
    }
}
