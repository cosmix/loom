//! Host verification jobs: a session relays a request, the daemon runs the
//! job as a child process on the host under loom's own confinement, and the
//! session follows its progress and result through the state directory.
//!
//! The records below are the shared vocabulary: `store` persists them under
//! `<state root>/verification/<stage>/`, `submit` queues, joins and cancels
//! jobs, `runner` starts and reaps the daemon's children, `child` is the
//! `loom stage verify-host` process, `digest` fingerprints the tree a request
//! saw, `wait` is the session side, and `apply` is what the daemon does with a
//! finished job: complete the stage, record a checkpoint, or freeze contracts
//! (`freeze`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod apply;
mod apply_rejection;
pub mod child;
mod child_complete;
pub mod digest;
pub mod freeze;
mod freeze_refuse;
pub mod runner;
mod signals;
mod spawn;
pub mod store;
mod store_fs;
pub mod submit;
pub mod wait;
mod wait_report;
mod wait_state;

#[cfg(test)]
mod tests_apply;
#[cfg(test)]
mod tests_apply_recovery;
#[cfg(test)]
mod tests_child;
#[cfg(test)]
mod tests_child_complete;
#[cfg(test)]
mod tests_child_impact;
#[cfg(test)]
mod tests_child_reuse;
#[cfg(test)]
mod tests_complete_support;
#[cfg(test)]
mod tests_digest;
#[cfg(test)]
mod tests_digest_pinned;
#[cfg(test)]
mod tests_freeze;
#[cfg(test)]
mod tests_freeze_refusal;
#[cfg(test)]
mod tests_runner;
#[cfg(test)]
mod tests_runner_recovery;
#[cfg(test)]
mod tests_store;
#[cfg(test)]
mod tests_submit;
#[cfg(test)]
mod tests_wait;
#[cfg(test)]
mod tests_wait_criteria;
#[cfg(test)]
mod tests_wait_refusal;

/// Most bytes a record's `detail` keeps.
pub(crate) const DETAIL_MAX_BYTES: usize = 512;

/// What a job runs: the stage's gate (`loom check`), the completion run
/// whose pass lets the daemon complete the stage, or the contract run that
/// freezes a stage's contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Check,
    Complete,
    Freeze,
}

impl JobKind {
    /// The kind as it is serialized: `check`, `complete` or `freeze`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Complete => "complete",
            Self::Freeze => "freeze",
        }
    }
}

/// Where a job is in its life. `Queued` and `Running` are the only states a
/// job leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
    Interrupted,
}

impl JobState {
    /// Whether the job has ended and will never change state again.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Passed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

/// A fresh job id: a v4 uuid in its simple form, 32 lowercase hex digits.
pub fn new_job_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// One job of one stage; `verification/<stage>/current.json` holds the
/// stage's current one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: String,
    pub stage_id: String,
    /// The session whose request created the job.
    pub session_id: String,
    pub kind: JobKind,
    /// The HEAD the request named.
    pub head: String,
    /// Digest of the tree the request saw (`digest::tree_digest`).
    pub tree_digest: String,
    pub no_cache: bool,
    /// The 0-based acceptance criteria a check runs (`loom check
    /// --criterion`); `None` runs every one. Only a check carries one.
    pub criteria: Option<Vec<usize>>,
    pub state: JobState,
    /// Relay request ids joined to this job, the last 16 kept.
    pub requests: Vec<String>,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub pid: Option<u32>,
    /// Why the job was cancelled, interrupted or failed without a result;
    /// at most 512 bytes, cut on a char boundary.
    pub detail: Option<String>,
}

/// The outcome of one criterion or check of a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriterionOutcome {
    /// `acceptance-<i>` (0-based), `goal-backward`, or a completion check
    /// (`contracts`, `impact-tests`, `reachable`, `after-stage`, `unwired`,
    /// `duplicates`, `aggregated-wiring`); a freeze job's `contract:<id>`
    /// and `format`.
    pub label: String,
    pub passed: bool,
    /// The printed block for this criterion, at most 8 KiB.
    pub summary: String,
    /// File name, inside the job directory, of the criterion's full output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

/// What the child verified; `<job dir>/result.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobResult {
    pub job_id: String,
    /// The HEAD the child verified.
    pub head: String,
    /// False whenever `error` is set.
    pub passed: bool,
    pub criteria: Vec<CriterionOutcome>,
    /// A pipeline error that stopped the run, at most 2 KiB.
    pub error: Option<String>,
    /// Set by a completion job; `None` for a check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
}

/// `text` cut to [`DETAIL_MAX_BYTES`] on a char boundary.
pub(crate) fn bounded_detail(text: &str) -> String {
    crate::models::dispute::truncate_to_byte_limit(text, DETAIL_MAX_BYTES)
}
