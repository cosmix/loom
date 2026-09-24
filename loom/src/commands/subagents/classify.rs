//! Classifies a subagent's JSONL transcript into a liveness state by the
//! structural shape of its last entry -- never by `message.stop_reason`,
//! which is `null` on every current-format entry (older transcripts carry
//! `end_turn`, but structural classification is version-independent while
//! `stop_reason` is not). See `loom-hooks/_common.sh:1010-1043` for the same
//! design constraint applied to hook-side main-vs-subagent classification.
//!
//! | Last entry                                                | State        |
//! | ---------------------------------------------------------- | ------------ |
//! | `assistant`, `tool_use` block                               | `tool-wait`  |
//! | `assistant`, text block, no `tool_use`, idle >= debounce     | `done`       |
//! | `assistant`, text block, no `tool_use`, idle < debounce       | `generating` |
//! | `assistant`, `thinking` only (no text, no `tool_use`)         | `generating` |
//! | `user` (a tool result came back, or it was sent a message)    | `generating` |
//! | anything else                                                 | `unknown`    |
//!
//! `tool-wait` NEVER debounces or times out, at any idle time: a tool call
//! genuinely outstanding for 23+ minutes has been measured in this
//! codebase's own transcripts (a `Bash` call ran 603s; the overall p99 tool
//! duration is 9.1s but the max is 1,425s). `tool-wait` means the agent is
//! busy, full stop -- `harvest` must never emit for it and `watch` must
//! never call it settled, regardless of how long it has sat idle. Report the
//! elapsed time and tool name in `list` and let the human judge; do not add
//! a "tool-wait for too long" escalation.
//!
//! `unknown` is likewise never harvested and never counts as settled.
//!
//! Transcripts are appended to while being read, so the last line may be a
//! partial write, including a torn multibyte UTF-8 character: the file is
//! read as bytes and decoded lossily, turning a torn byte sequence into
//! replacement characters on that one line instead of failing the whole
//! read. Lines are then parsed independently, and an unparseable one (JSON
//! or otherwise) is skipped rather than failing the whole file, which
//! degrades a torn last line to "use the previous good entry" instead of
//! crashing or erroring.
//!
//! The `done` row needs a debounce because Claude Code flushes each content
//! block of one assistant turn as its own JSONL entry: a turn that narrates
//! before calling a tool produces a text-only assistant entry immediately
//! followed by a `tool_use` entry. Sampling in that gap would classify a
//! working agent as `done` -- see [`DEFAULT_DONE_DEBOUNCE_SECS`] for the
//! measured basis. Exact lifecycle journal evidence from a validated
//! SubagentStop hook is authoritative proof of termination and skips the
//! debounce entirely. Do NOT corroborate completion from the
//! *parent* transcript's `tool_result` for the spawning Task/Agent call:
//! background agents get an immediate spawn-acknowledgement `tool_result`
//! there, so its presence proves spawn, never completion.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use super::forward_jobs::ForwardIndex;
use super::{ledger, metrics, summary};
use crate::models::constants::DEFAULT_SUBAGENT_CEILING_TOKENS;

mod entry;
#[path = "classify_forward.rs"]
pub(super) mod forward;
pub(super) mod lifecycle;

pub(crate) use entry::{is_assistant, text_blocks};

pub(super) fn is_done_entry(entry: &Value) -> bool {
    entry::classify_last(entry) == SubagentState::Done
}

/// Minimum idle time (seconds) a structurally-`done` last entry must sit
/// unchanged before it is trusted as genuinely turn-final, rather than one
/// text block flushed mid-turn just before the next block (typically a
/// `tool_use`) lands. Measured across 1,143 real subagent transcripts /
/// 136k entry gaps: true intra-turn gaps (n=8,808) were p50 1.4s, p90
/// 12.3s, p99 53.6s, p99.9 88.9s, max 137.7s -- ZERO of the 8,808 exceeded
/// 180s. False-`done` rate by threshold: 10s -> 12.6%, 60s -> 0.65%, 120s ->
/// 0.034%, 180s -> 0%. 180s is the first round number above the observed
/// max. The cost asymmetry justifies the wait: a false `done` can make an
/// orchestrator re-dispatch onto a LIVE agent's file set -- two writers,
/// lost work -- while 3 minutes of extra latency is nothing against the
/// ~28-minute hangs this command exists to detect. Overridable via
/// `--debounce` on `list`/`harvest`/`watch`.
pub const DEFAULT_DONE_DEBOUNCE_SECS: u64 = 180;

/// A subagent's liveness, inferred from the structural shape of the last
/// entry successfully parsed from its transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubagentState {
    /// Last entry is `assistant` with a text block and no `tool_use`, AND
    /// either it has sat idle at least the debounce or exact lifecycle
    /// evidence exists: the subagent's turn ended and `final_report` holds
    /// its output.
    Done,
    /// Exact lifecycle evidence reports terminal failure.
    Failed,
    /// Exact lifecycle evidence reports terminal cancellation.
    Cancelled,
    /// Last entry is `assistant` with a `tool_use` block: waiting on a tool.
    /// Never debounced or timed out, at any idle time -- see the module doc.
    ToolWait,
    /// Last entry is `user`, OR `assistant` with only a `thinking` block, OR
    /// it structurally looks `done` but hasn't cleared the debounce yet (a
    /// text block flushed mid-turn, with the next block -- typically
    /// `tool_use` -- still to come).
    Generating,
    /// No parseable entry, or a shape the table above doesn't cover.
    Unknown,
    /// A forwarded backend job is queued or running, even if the wrapper's
    /// own transcript has stopped.
    ForwardWait,
    /// A forwarded backend job reported failure or cancellation.
    ForwardFailed,
    /// A forwarded job was expected but cannot be established exactly.
    ForwardUnknown,
}

impl SubagentState {
    pub fn label(self) -> &'static str {
        match self {
            SubagentState::Done => "done",
            SubagentState::Failed => "failed",
            SubagentState::Cancelled => "cancelled",
            SubagentState::ToolWait => "tool-wait",
            SubagentState::Generating => "generating",
            SubagentState::Unknown => "unknown",
            SubagentState::ForwardWait => "forward-wait",
            SubagentState::ForwardFailed => "forward-failed",
            SubagentState::ForwardUnknown => "forward-unknown",
        }
    }
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
    let entries = entry::read_entries(path)?;
    let lifecycle_evidence = lifecycle.map(|context| context.evidence(path, &agent_id));
    let agent_type = lifecycle_evidence
        .as_ref()
        .and_then(|evidence| evidence.agent_type.clone())
        .or_else(|| ledger::agent_type(work_dir, &agent_id));
    let metrics = metrics::extract(&entries);
    let peak_tokens_over_ceiling = peak_over_ceiling(&metrics, subagent_ceiling_tokens);

    let Some(last) = entries.last() else {
        let mut summary = summary::empty(agent_id, idle_since_mtime(path), agent_type);
        if let Some(evidence) = lifecycle_evidence {
            evidence.apply(&mut summary);
        }
        return Ok(summary);
    };

    let idle_secs = entry::timestamp(last)
        .map(|ts| (Utc::now() - ts).num_seconds().max(0))
        .unwrap_or_else(|| idle_since_mtime(path));
    let state = resolve_state(last, idle_secs, debounce_secs);
    let summary = summary::with_last(
        agent_id,
        state,
        idle_secs,
        transcript_activity(&entries, state, last),
        agent_type,
        metrics,
        peak_tokens_over_ceiling,
    );
    Ok(apply_evidence(
        summary,
        last,
        path,
        lifecycle_evidence,
        forward_index,
    ))
}

fn apply_evidence(
    mut summary: SubagentSummary,
    last: &Value,
    path: &Path,
    lifecycle_evidence: Option<lifecycle::Evidence>,
    forward_index: Option<&ForwardIndex>,
) -> SubagentSummary {
    if let Some(evidence) = lifecycle_evidence {
        evidence.apply(&mut summary);
    }
    if summary.state == SubagentState::Done && summary.final_report.is_none() {
        summary.final_report = final_report_for(summary.state, last);
    }
    if let Some(index) = forward_index {
        forward::apply(&mut summary, index, path);
    }
    summary
}

fn transcript_activity(
    entries: &[Value],
    state: SubagentState,
    last: &Value,
) -> summary::TranscriptActivity {
    summary::TranscriptActivity {
        turns: entries
            .iter()
            .filter(|entry| entry::is_assistant(entry))
            .count(),
        last_tool: entry::last_tool_used(entries),
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

/// Resolve the transcript-only state before exact lifecycle evidence is
/// applied. A structurally-`done` entry still inside the debounce remains
/// `Generating`; lifecycle evidence may authoritatively override it later.
fn resolve_state(last: &Value, idle_secs: i64, debounce_secs: u64) -> SubagentState {
    let structural = entry::classify_last(last);
    if structural == SubagentState::Done && idle_secs < debounce_secs as i64 {
        SubagentState::Generating
    } else {
        structural
    }
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
