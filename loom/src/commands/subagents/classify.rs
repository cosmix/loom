//! Reads a subagent's JSONL transcript and classifies it into a liveness
//! state by the structural shape of its last entry. The classification table,
//! the done debounce and the `stop_reason` rationale live with the rules in
//! [`crate::agent_state::progress`].
//!
//! Transcripts are appended to while being read, so the last line may be a
//! partial write, including a torn multibyte UTF-8 character: the file is
//! read as bytes and decoded lossily, turning a torn byte sequence into
//! replacement characters on that one line instead of failing the whole
//! read. Lines are then parsed independently, and an unparseable one (JSON
//! or otherwise) is skipped rather than failing the whole file, which
//! degrades a torn last line to "use the previous good entry" instead of
//! crashing or erroring.

use std::fs::{self, File};
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use super::forward_jobs::ForwardIndex;
use super::{ledger, metrics, summary};
use crate::agent_state::progress::{classify_last, resolve_state, AgentProgress};
use crate::models::constants::DEFAULT_SUBAGENT_CEILING_TOKENS;

pub use crate::agent_state::progress::{SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};

mod entry;
#[cfg(test)]
pub(super) use entry::read_count;
#[path = "classify_forward.rs"]
pub(super) mod forward;
pub(super) mod lifecycle;

pub(crate) use entry::{is_assistant, text_blocks};

/// A tool call a transcript's last turn left without a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenToolCall {
    /// The tool's name, when the `tool_use` block carries one.
    pub name: Option<String>,
}

/// The tool call open at the end of an opened transcript, from a bounded tail
/// read: the last conversation turn is an assistant `tool_use` -- the
/// [`SubagentState::ToolWait`] shape. Bookkeeping rows after it do not close
/// the call. An unreadable transcript reports nothing open.
pub(crate) fn open_tool_call(file: &mut File) -> Option<OpenToolCall> {
    let entries = entry::read_tail_entries(file).ok()?;
    let last = entries.iter().rev().find(|row| entry::is_message(row))?;
    (classify_last(last) == SubagentState::ToolWait).then(|| OpenToolCall {
        name: entry::last_tool_used(std::slice::from_ref(last)),
    })
}

pub(super) fn is_done_entry(entry: &Value) -> bool {
    classify_last(entry) == SubagentState::Done
}

/// Evidence that allowed a summary to enter [`SubagentState::Done`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoneEvidence {
    Lifecycle,
    LegacyTranscript,
}

/// One subagent's transcript, summarized.
#[derive(Debug, Clone, Serialize)]
pub struct SubagentSummary {
    pub agent_id: String,
    pub state: SubagentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_evidence: Option<DoneEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    pub idle_secs: i64,
    pub turns: usize,
    pub last_tool: Option<String>,
    /// Spawn type from loom's optional hook-side ledgers, when they can
    /// identify this agent without inferring it from a conflicting spawn.
    pub agent_type: Option<String>,
    /// Raw model from the first assistant transcript row; the table narrows
    /// Claude model names for display while JSON preserves this source value.
    pub model: Option<String>,
    /// Distinct non-null top-level `requestId` values in the transcript.
    /// `None` means no parseable entry was found at all -- a real zero (some
    /// entries parsed, none carried a `requestId`) stays `Some(0)`.
    pub request_count: Option<usize>,
    /// Largest resident context carried by one assistant request, excluding
    /// output tokens. `None` means no assistant row exposed usage data.
    pub peak_resident_tokens: Option<u64>,
    /// Whether [`peak_resident_tokens`](Self::peak_resident_tokens) reached
    /// the context safety ceiling used to mark the table cell.
    pub peak_tokens_over_ceiling: bool,
    /// Read-only evidence for a forwarded backend job. Omitted for ordinary
    /// subagents so their JSON output shape remains unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) forward: Option<super::forward_jobs::ForwardMetadata>,
    /// A validated forward may block settlement without changing an ordinary
    /// worker's human-facing list row.
    #[serde(skip)]
    pub(crate) display_state: Option<SubagentState>,
    /// The last entry's text blocks when ordinary transcript evidence says a
    /// report is safe to harvest; this remains separate from forward status.
    pub final_report: Option<String>,
}

/// Read and classify one transcript. Malformed content degrades to `Unknown`;
/// only unreadable files return an error.
///
/// `debounce_secs` keeps a recent structurally-done entry `generating`.
///
/// This test-only entry point deliberately omits lifecycle and forward-job
/// evidence so ambient Loom stage variables cannot affect unit tests.
#[cfg(test)]
pub fn analyze(
    path: &Path,
    agent_id: String,
    debounce_secs: u64,
    work_dir: Option<&Path>,
) -> Result<SubagentSummary> {
    analyze_with_evidence_at_ceiling(
        path,
        agent_id,
        debounce_secs,
        work_dir,
        resolve_subagent_ceiling(work_dir),
        None,
        None,
    )
}

/// Resolve the plan-wide subagent ceiling, falling back to the Rust default.
pub(super) fn resolve_subagent_ceiling(work_dir: Option<&Path>) -> u64 {
    work_dir
        .and_then(|path| crate::fs::work_dir::read_context_config(path).ok())
        .map_or(u64::from(DEFAULT_SUBAGENT_CEILING_TOKENS), |config| {
            u64::from(config.subagent_ceiling_tokens)
        })
}

pub(super) fn analyze_with_evidence_at_ceiling(
    path: &Path,
    agent_id: String,
    debounce_secs: u64,
    work_dir: Option<&Path>,
    subagent_ceiling_tokens: u64,
    lifecycle: Option<&lifecycle::Context>,
    forward_index: Option<&ForwardIndex>,
) -> Result<SubagentSummary> {
    let (entries, bytes, trusted) = entry::read_entries(path)?;
    let forwarding =
        forward_index.map(|_| super::forward_jobs_transcript::from_bytes(path, &bytes, trusted));
    let lifecycle_evidence = lifecycle.map(|context| context.evidence(path, &agent_id));
    let agent_type = resolve_agent_type(lifecycle_evidence.as_ref(), work_dir, &agent_id);
    let metrics = metrics::extract(&entries);
    let peak_tokens_over_ceiling = peak_over_ceiling(&metrics, subagent_ceiling_tokens);
    let mut progress = AgentProgress::default();
    entries.iter().for_each(|row| progress.observe(row));

    let Some(last) = progress.last() else {
        return Ok(empty_summary(
            path,
            agent_id,
            agent_type,
            lifecycle_evidence,
        ));
    };

    let now = Utc::now();
    let idle_secs = progress
        .idle_secs(now)
        .unwrap_or_else(|| idle_since_mtime(path));
    let state = resolve_state(last, idle_secs, debounce_secs);
    let summary = summary::with_last(
        agent_id,
        state,
        idle_secs,
        transcript_activity(&progress, state, last),
        agent_type,
        metrics,
        peak_tokens_over_ceiling,
    );
    Ok(apply_evidence(
        summary,
        last,
        forwarding.as_ref(),
        lifecycle_evidence,
        forward_index,
    ))
}

/// Lifecycle evidence names the agent type first; the ledger is the fallback.
fn resolve_agent_type(
    evidence: Option<&lifecycle::Evidence>,
    work_dir: Option<&Path>,
    agent_id: &str,
) -> Option<String> {
    evidence
        .and_then(|evidence| evidence.agent_type.clone())
        .or_else(|| ledger::agent_type(work_dir, agent_id))
}

/// Summary for a transcript with no parsable entries, idle since its mtime.
fn empty_summary(
    path: &Path,
    agent_id: String,
    agent_type: Option<String>,
    lifecycle_evidence: Option<lifecycle::Evidence>,
) -> SubagentSummary {
    let mut summary = summary::empty(agent_id, idle_since_mtime(path), agent_type);
    if let Some(evidence) = lifecycle_evidence {
        evidence.apply(&mut summary);
    }
    summary
}

fn apply_evidence(
    mut summary: SubagentSummary,
    last: &Value,
    forwarding: Option<&super::forward_jobs_transcript::ForwardTranscript>,
    lifecycle_evidence: Option<lifecycle::Evidence>,
    forward_index: Option<&ForwardIndex>,
) -> SubagentSummary {
    if let Some(evidence) = lifecycle_evidence {
        evidence.apply(&mut summary);
    }
    if summary.state == SubagentState::Done && summary.final_report.is_none() {
        summary.final_report = final_report_for(summary.state, last);
    }
    if let (Some(index), Some(transcript)) = (forward_index, forwarding) {
        forward::apply(&mut summary, index, transcript);
    }
    summary
}

fn transcript_activity(
    progress: &AgentProgress,
    state: SubagentState,
    last: &Value,
) -> summary::TranscriptActivity {
    summary::TranscriptActivity {
        turns: progress.assistant_entries(),
        last_tool: progress.last_tool().map(str::to_owned),
        final_report: final_report_for(state, last),
    }
}

fn peak_over_ceiling(metrics: &metrics::TranscriptMetrics, ceiling: u64) -> bool {
    ceiling > 0
        && metrics
            .peak_resident_tokens
            .is_some_and(|tokens| tokens >= ceiling)
}

/// Return non-empty final text only for `Done` entries.
fn final_report_for(state: SubagentState, last: &Value) -> Option<String> {
    (state == SubagentState::Done)
        .then(|| entry::text_blocks(last).join("\n\n"))
        .filter(|report| !report.trim().is_empty())
}

/// Fallback idle time for entries with no parseable timestamp (or files
/// with no parseable entry at all): time since the transcript file's own
/// mtime, or 0 if even that can't be read.
fn idle_since_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "classify_recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "classify_ceiling_tests.rs"]
mod ceiling_tests;

#[cfg(test)]
#[path = "classify_forward_tests.rs"]
mod forward_tests;

#[cfg(test)]
#[path = "classify_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "classify_progress_tests.rs"]
mod progress_tests;
