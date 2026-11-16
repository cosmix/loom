//! Native Codex worker invocations of one stage, as the dashboards list them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::sanitize::{valid_stage_id, MAX_SAFE_INTEGER};
use crate::context::untrusted::inline_safe;
use crate::fs::work_dir::WorkDir;
use crate::native_worker::index::{jobs_in, load_stage, NativeJob, StageRecords};
use crate::native_worker::model::{Outcome, Phase};

/// One native Codex invocation of a stage. A unit that failed before any Codex
/// thread started has `thread_id: None` and no token agent, so this is the
/// only place it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexJob {
    pub unit_id: String,
    pub invocation_id: String,
    pub model: String,
    pub effort: String,
    pub phase: Phase,
    pub outcome: Option<Outcome>,
    pub outcome_reason: Option<String>,
    pub thread_id: Option<String>,
    pub started_at: DateTime<Utc>,
    /// The terminal event's time; `None` while the unit runs.
    pub ended_at: Option<DateTime<Utc>>,
    pub deadline_secs: u64,
}

impl From<NativeJob> for CodexJob {
    fn from(job: NativeJob) -> Self {
        Self {
            unit_id: job.unit_id,
            invocation_id: job.invocation_id,
            model: job.model,
            effort: job.effort,
            phase: job.phase,
            outcome: job.outcome,
            outcome_reason: job.outcome_reason,
            thread_id: job.thread_id,
            started_at: job.started_at,
            ended_at: job.ended_at,
            deadline_secs: job.deadline_secs,
        }
    }
}

impl CodexJob {
    /// Flattens the strings the dashboards print: `outcome_reason` is worker
    /// error text and the rest comes from the worker's manifest.
    pub(super) fn flatten(&mut self) {
        for value in [
            &mut self.unit_id,
            &mut self.invocation_id,
            &mut self.model,
            &mut self.effort,
        ] {
            *value = inline_safe(value);
        }
        for value in [&mut self.outcome_reason, &mut self.thread_id] {
            value.iter_mut().for_each(|text| *text = inline_safe(text));
        }
        // Past 2^53 the dashboard schema rejects the number and drops the status frame.
        self.deadline_secs = self.deadline_secs.min(MAX_SAFE_INTEGER);
    }
}

/// The stage's native worker records, read once for every projection of a snapshot. The
/// work root is resolved the way `execution_models_for_stage` resolves it, so a linked
/// worktree's symlinked `.loom/work` reads the same records.
pub(super) fn native_records(work_dir: &WorkDir, stage_id: &str) -> StageRecords {
    match std::fs::canonicalize(work_dir.root()) {
        Ok(root) if valid_stage_id(stage_id) => load_stage(&root, stage_id),
        _ => StageRecords::default(),
    }
}

/// The stage's native worker invocations in `records`, oldest first.
pub(super) fn codex_jobs_in(records: &StageRecords) -> Vec<CodexJob> {
    jobs_in(records).into_iter().map(CodexJob::from).collect()
}

/// The stage's native worker invocations, oldest first.
pub fn codex_jobs_for_stage(work_dir: &WorkDir, stage_id: &str) -> Vec<CodexJob> {
    codex_jobs_in(&native_records(work_dir, stage_id))
}

#[cfg(test)]
#[path = "codex_jobs_tests.rs"]
pub(super) mod tests;
