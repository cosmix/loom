use std::fs;
use std::path::Path;

use anyhow::{ensure, Context};
use serde::Deserialize;

use super::locator::read_bounded_prefix;
use super::{is_safe_id, ForwardState};

const MAX_JOB_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CompanionJob {
    pub id: String,
    pub status: Option<String>,
    pub phase: Option<String>,
    #[serde(rename = "threadId")]
    pub thread_id: Option<String>,
}

pub fn read_companion_job(locator: &Path, job_id: &str) -> anyhow::Result<CompanionJob> {
    ensure!(is_safe_id(job_id), "unsafe companion job id");
    let metadata = fs::symlink_metadata(locator)
        .with_context(|| format!("reading companion job metadata for {}", locator.display()))?;
    let max_bytes = u64::try_from(MAX_JOB_RECORD_BYTES).context("job record limit overflow")?;
    ensure!(
        metadata.len() <= max_bytes,
        "companion job record exceeds the 1 MiB limit"
    );
    let input = read_bounded_prefix(locator, MAX_JOB_RECORD_BYTES)?;
    let job: CompanionJob = serde_json::from_str(&input)
        .with_context(|| format!("parsing companion job record {}", locator.display()))?;
    ensure!(job.id == job_id, "companion job record id mismatch");
    if let Some(thread_id) = job.thread_id.as_deref() {
        ensure!(is_safe_id(thread_id), "unsafe companion threadId");
    }
    Ok(job)
}

impl CompanionJob {
    pub fn state(&self) -> ForwardState {
        match self.status.as_deref() {
            Some("completed") if self.phase.as_deref() == Some("done") => ForwardState::Succeeded,
            Some("failed") => ForwardState::Failed,
            Some("cancelled" | "canceled") => ForwardState::Canceled,
            Some("queued") => ForwardState::Queued,
            Some("running" | "starting") => ForwardState::Running,
            _ => ForwardState::Unknown,
        }
    }
}

#[cfg(test)]
#[path = "job_record_tests.rs"]
mod tests;
