//! The contract-testing state of a stage whose writer has frozen its contracts.
//!
//! `loom stage contracts freeze` asks the daemon to run every contract as a
//! host job that is a child of the daemon, not of the writer's terminal, and
//! waits for that job and for the daemon's application of it. The writer is
//! blocked on the job for minutes and shows nothing, so the status surfaces
//! read the job's progress to say so.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::host_verify::store::{load_current, read_progress};
use crate::host_verify::{JobKind, JobState};
use crate::utils::format_elapsed;

/// Progress of the freeze job a contract writer is waiting on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractTesting {
    /// Contracts whose run has finished.
    pub done: u32,
    /// Contracts the stage declares.
    pub total: u32,
    /// The job has not started running yet.
    pub queued: bool,
    /// Seconds since the job started; `None` while it is queued.
    pub running_secs: Option<i64>,
}

impl ContractTesting {
    /// `contracts written, testing 9/11 · 6m12s`, or
    /// `contracts written, waiting to test` while queued.
    pub fn text(&self) -> String {
        if self.queued {
            return "contracts written, waiting to test".to_owned();
        }
        let progress = format!("contracts written, testing {}/{}", self.done, self.total);
        match self.running_secs {
            Some(secs) => format!("{progress} · {}", format_elapsed(secs)),
            None => progress,
        }
    }
}

/// The freeze job the stage's current session is waiting on, if any. Any
/// read failure answers `None`: status never fails or logs because of this.
pub(super) fn contract_testing(
    work_dir: &Path,
    stage_id: &str,
    session: Option<&str>,
    total: usize,
    now: DateTime<Utc>,
) -> Option<ContractTesting> {
    let job = load_current(work_dir, stage_id).ok()??;
    let running = match job.state {
        JobState::Queued => false,
        JobState::Running => true,
        _ => return None,
    };
    // A daemon killed mid-freeze leaves the job `Running` for good; a child
    // that is gone ended with it.
    if running
        && job
            .pid
            .is_some_and(|pid| !crate::process::is_process_alive(pid))
    {
        return None;
    }
    if job.kind != JobKind::Freeze || session != Some(job.session_id.as_str()) {
        return None;
    }
    let done = read_progress(work_dir, stage_id, &job.id)
        .ok()?
        .iter()
        .filter(|outcome| outcome.label.starts_with("contract:"))
        .count();
    Some(ContractTesting {
        done: u32::try_from(done).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        queued: !running,
        running_secs: job
            .started_at
            .filter(|_| running)
            .map(|started| (now - started).num_seconds().max(0)),
    })
}

#[cfg(test)]
#[path = "contract_testing_tests.rs"]
mod tests;
