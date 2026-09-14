use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::locator::read_bounded_prefix;
use super::{is_safe_id, parse_canonical_utc_millis, ForwardState};

const MAX_JOB_RECORD_BYTES: usize = 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_SESSION_ID_BYTES: usize = 640;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_RESULT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionJob {
    pub id: String,
    pub status: Option<String>,
    pub phase: Option<String>,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub job_class: Option<String>,
    pub write: Option<bool>,
    pub request: Option<CompanionTaskRequest>,
    pub turn_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_canonical_millis")]
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub result: Option<Value>,
    /// Companion worker process, present only while the job is queued or
    /// running: the companion rewrites it as `null` once the job is terminal,
    /// so a null must read as `None` rather than failing the record.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Append-only transcript the companion writes while the job runs; its
    /// mtime is the freshest progress signal a host can observe.
    #[serde(default)]
    pub log_file: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_canonical_millis")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "deserialize_canonical_millis")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "deserialize_canonical_millis")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionTaskRequest {
    pub cwd: PathBuf,
    pub model: String,
    pub effort: String,
    pub prompt: Option<String>,
    pub write: bool,
    pub resume_last: bool,
    pub job_id: String,
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
    let input = read_bounded_prefix(locator, MAX_JOB_RECORD_BYTES + 1)?;
    ensure!(
        input.len() <= MAX_JOB_RECORD_BYTES,
        "companion job record grew beyond the 1 MiB limit"
    );
    let job: CompanionJob = serde_json::from_str(&input)
        .with_context(|| format!("parsing companion job record {}", locator.display()))?;
    ensure!(job.id == job_id, "companion job record id mismatch");
    if let Some(thread_id) = job.thread_id.as_deref() {
        ensure!(is_safe_id(thread_id), "unsafe companion threadId");
    }
    Ok(job)
}

impl CompanionJob {
    /// Validate the fields Loom relies on in Codex companion v1.0.6.
    ///
    /// The parser remains compatible with historical abbreviated records used
    /// by forward-receipt display. Authoritative lifecycle reconciliation must
    /// call this method before trusting a job.
    pub fn validate_v1_0_6(&self) -> anyhow::Result<()> {
        ensure!(is_safe_id(&self.id), "unsafe companion job id");
        validate_bounded(self.status.as_deref(), 32, "status")?;
        validate_bounded(self.phase.as_deref(), 128, "phase")?;
        validate_bounded(
            self.session_id.as_deref(),
            MAX_SESSION_ID_BYTES,
            "sessionId",
        )?;
        validate_path(self.workspace_root.as_deref(), "workspaceRoot")?;
        validate_bounded(self.job_class.as_deref(), 32, "jobClass")?;
        let write = self.write.context("missing companion write")?;
        let request = self.request.as_ref().context("missing companion request")?;
        validate_request(request, &self.id, write)?;
        for (value, name) in [
            (self.thread_id.as_deref(), "threadId"),
            (self.turn_id.as_deref(), "turnId"),
        ] {
            if let Some(value) = value {
                ensure!(is_safe_id(value), "unsafe companion {name}");
            }
        }
        if let Some(message) = self.error_message.as_deref() {
            ensure!(
                message.len() <= MAX_TEXT_BYTES,
                "companion errorMessage exceeds cap"
            );
        }
        if let Some(result) = self.result.as_ref() {
            ensure!(
                serde_json::to_vec(result)?.len() <= MAX_RESULT_BYTES,
                "companion result exceeds cap"
            );
        }
        Ok(())
    }

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

fn validate_request(
    request: &CompanionTaskRequest,
    job_id: &str,
    write: bool,
) -> anyhow::Result<()> {
    validate_path(Some(&request.cwd), "request.cwd")?;
    validate_text(&request.model, 128, "request.model")?;
    validate_text(&request.effort, 128, "request.effort")?;
    ensure!(request.job_id == job_id, "companion request jobId mismatch");
    ensure!(request.write == write, "companion request write mismatch");
    if let Some(prompt) = request.prompt.as_deref() {
        ensure!(
            prompt.len() <= MAX_RESULT_BYTES,
            "companion request prompt exceeds cap"
        );
    }
    Ok(())
}

fn validate_path(path: Option<&Path>, name: &str) -> anyhow::Result<()> {
    let path = path.with_context(|| format!("missing companion {name}"))?;
    let text = path
        .to_str()
        .with_context(|| format!("companion {name} is not UTF-8"))?;
    ensure!(path.is_absolute(), "companion {name} is not absolute");
    validate_text(text, MAX_PATH_BYTES, name)
}

fn validate_bounded(value: Option<&str>, max: usize, name: &str) -> anyhow::Result<()> {
    let value = value.with_context(|| format!("missing companion {name}"))?;
    validate_text(value, max, name)
}

fn validate_text(value: &str, max: usize, name: &str) -> anyhow::Result<()> {
    ensure!(
        (1..=max).contains(&value.len()),
        "invalid companion {name} length"
    );
    ensure!(!value.as_bytes().contains(&0), "NUL in companion {name}");
    Ok(())
}

/// Every companion timestamp is written by one code path in the same canonical
/// UTC millisecond `Z` form, so all of them parse strictly: a drift in that
/// form is a contract break the host should see, not silently drop.
fn deserialize_canonical_millis<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed = parse_canonical_utc_millis(&value)
        .map_err(|_| serde::de::Error::custom("timestamp is not UTC millisecond Z form"))?;
    Ok(Some(parsed))
}

#[cfg(test)]
#[path = "job_record_tests.rs"]
mod tests;
