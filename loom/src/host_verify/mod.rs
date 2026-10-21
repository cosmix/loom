//! Host verification jobs: a session relays a request, the daemon runs the
//! job as a child process on the host under loom's own confinement, and the
//! session follows its progress and result through the state directory.
//!
//! The records below are the shared vocabulary: `store` persists them under
//! `<state root>/verification/<stage>/`, `submit` queues, joins and cancels
//! jobs, `runner` starts and reaps the daemon's children, `child` is the
//! `loom stage verify-host` process, `digest` fingerprints the tree a request
//! saw, and `wait` is the session side.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod child;
pub mod digest;
pub mod runner;
mod signals;
mod spawn;
pub mod store;
mod store_fs;
pub mod submit;
pub mod wait;

#[cfg(test)]
mod tests_child;
#[cfg(test)]
mod tests_digest;
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

/// Most bytes a record's `detail` keeps.
pub(crate) const DETAIL_MAX_BYTES: usize = 512;

/// What a job runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Check,
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
    /// `acceptance-<i>` (0-based) or `goal-backward`.
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
