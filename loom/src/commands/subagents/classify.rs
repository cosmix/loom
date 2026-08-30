//! Classifies a subagent's JSONL transcript into a liveness state by the
//! structural shape of its last entry -- never by `message.stop_reason`,
//! which is `null` on every current-format entry (older transcripts carry
//! `end_turn`, but structural classification is version-independent while
//! `stop_reason` is not). See `hooks/_common.sh:1010-1043` for the same
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
//! measured basis. A `.work/subagents/<stage-id>/<agentId>.json` record
//! (written by a SubagentStop hook, when one exists) is authoritative proof
//! of termination and skips the debounce entirely -- see
//! [`has_authoritative_termination`]. Do NOT corroborate completion from the
//! *parent* transcript's `tool_result` for the spawning Task/Agent call:
//! background agents get an immediate spawn-acknowledgement `tool_result`
//! there, so its presence proves spawn, never completion.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use super::{ledger, metrics, summary};

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
    /// either it has sat idle at least the debounce or a
    /// `.work/subagents/.../<agentId>.json` termination record exists: the
    /// subagent's turn ended and `final_report` holds its output.
    Done,
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
}

impl SubagentState {
    pub fn label(self) -> &'static str {
        match self {
            SubagentState::Done => "done",
            SubagentState::ToolWait => "tool-wait",
            SubagentState::Generating => "generating",
            SubagentState::Unknown => "unknown",
        }
    }
}

/// One subagent's transcript, summarized.
#[derive(Debug, Clone, Serialize)]
pub struct SubagentSummary {
    pub agent_id: String,
    pub state: SubagentState,
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
    /// Only set when `state == Done`: the concatenated text of the last
    /// entry's text blocks, i.e. the subagent's final report.
    pub final_report: Option<String>,
}

/// Read and classify one subagent transcript file. Never fails on malformed
/// content: unparseable lines are skipped, and a transcript with no
/// parseable lines at all (or an empty file) degrades to `Unknown` rather
/// than erroring. The only real failure mode is not being able to read the
/// file at all (e.g. it vanished, or a permission error).
///
/// `debounce_secs` gates the `done` state: a structurally-done last entry
/// that hasn't sat idle for at least this long is reported as `generating`
/// instead (see the module doc for why). Pass [`DEFAULT_DONE_DEBOUNCE_SECS`]
/// unless the caller has an explicit `--debounce` override to thread through.
///
/// `work_dir`, when given, is the loom `.work/` root to check for an
/// authoritative `subagents/<stage-id>/<agentId>.json` termination record
/// and optional spawn-type ledgers (see the module doc). `None` (no `.work/`
/// found, or the caller doesn't want the fast path) falls straight through
/// to the transcript rule and leaves type unknown.
pub fn analyze(
    path: &Path,
    agent_id: String,
    debounce_secs: u64,
    work_dir: Option<&Path>,
) -> Result<SubagentSummary> {
    let entries = read_entries(path)?;
    let authoritative_done = has_authoritative_termination(work_dir, &agent_id);
    let agent_type = ledger::agent_type(work_dir, &agent_id);
    let metrics = metrics::extract(&entries);
    let peak_tokens_over_ceiling = metrics
        .peak_resident_tokens
        .is_some_and(|tokens| tokens >= metrics::PEAK_TOKENS_CEILING);

    let Some(last) = entries.last() else {
        return Ok(summary::empty(
            agent_id,
            authoritative_done,
            idle_since_mtime(path),
            agent_type,
        ));
    };

    let idle_secs = entry_timestamp(last)
        .map(|ts| (Utc::now() - ts).num_seconds().max(0))
        .unwrap_or_else(|| idle_since_mtime(path));
    let state = resolve_state(last, authoritative_done, idle_secs, debounce_secs);
    let turns = entries
        .iter()
        .filter(|entry| entry_type(entry) == Some("assistant"))
        .count();
    let last_tool = last_tool_used(&entries);
    let final_report = final_report_for(state, last);

    Ok(summary::with_last(
        agent_id,
        state,
        idle_secs,
        summary::TranscriptActivity {
            turns,
            last_tool,
            final_report,
        },
        agent_type,
        metrics,
        peak_tokens_over_ceiling,
    ))
}

/// Read a transcript's bytes and decode them lossily rather than with
/// `fs::read_to_string`: the file may be mid-write, and a torn multibyte
/// UTF-8 character at the tail must degrade to replacement characters on
/// that one line -- which then simply fails to parse as JSON and is skipped
/// like any other malformed line -- rather than failing the whole read.
fn read_entries(path: &Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path)
        .with_context(|| format!("reading subagent transcript {}", path.display()))?;
    let content = String::from_utf8_lossy(&bytes);
    Ok(parse_lines(&content))
}

/// The subagent's final report: only ever `Some` when `state == Done`, and
/// even then only when the last entry actually carried a text block. An
/// authoritative termination record can force `Done` while the last flushed
/// entry is a `tool_use` or `thinking` block with no text -- in that case
/// `text_blocks` is empty and a bare `join` would produce `""`, which is not
/// a report; filtering it out here sends the caller down the same "nothing
/// harvestable" path it takes for any other reportless subagent.
fn final_report_for(state: SubagentState, last: &Value) -> Option<String> {
    (state == SubagentState::Done)
        .then(|| text_blocks(last).join("\n\n"))
        .filter(|report| !report.trim().is_empty())
}

/// Resolve the final [`SubagentState`] for the last transcript entry: an
/// authoritative termination record overrides everything (including a
/// mid-debounce or structurally-different read, since the hook fired after
/// the transcript was last flushed and the transcript itself may be
/// lagging); otherwise a structurally-`done` entry that hasn't cleared the
/// debounce is demoted to `Generating` (still being written to -- the model
/// may add a `tool_use` block any moment). `tool-wait` is untouched by the
/// debounce either way -- it is never debounced.
fn resolve_state(
    last: &Value,
    authoritative_done: bool,
    idle_secs: i64,
    debounce_secs: u64,
) -> SubagentState {
    if authoritative_done {
        return SubagentState::Done;
    }
    let structural = classify_last(last);
    if structural == SubagentState::Done && idle_secs < debounce_secs as i64 {
        SubagentState::Generating
    } else {
        structural
    }
}

/// True when a SubagentStop hook already recorded this agent's termination
/// under `.work/subagents/<stage-id>/<agentId>.json`, in any stage-id
/// subdirectory (the caller doesn't know which stage owns this agent).
/// Purely optional and best-effort: a missing `work_dir`, a missing
/// `subagents/` directory, or no record for this agent all silently return
/// `false` rather than erroring -- this command never creates the directory
/// and never depends on it existing; the transcript rule remains the source
/// of truth when this can't be corroborated.
fn has_authoritative_termination(work_dir: Option<&Path>, agent_id: &str) -> bool {
    let Some(work_dir) = work_dir else {
        return false;
    };
    let Ok(stage_dirs) = fs::read_dir(work_dir.join("subagents")) else {
        return false;
    };
    stage_dirs
        .flatten()
        .any(|entry| entry.path().join(format!("{agent_id}.json")).is_file())
}

/// Parse every non-blank line independently, discarding lines that fail to
/// parse as JSON. This is what makes a torn last line (a concurrent partial
/// write) and any other mid-file corruption non-fatal.
fn parse_lines(content: &str) -> Vec<Value> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn entry_type(entry: &Value) -> Option<&str> {
    entry.get("type").and_then(Value::as_str)
}

fn entry_timestamp(entry: &Value) -> Option<DateTime<Utc>> {
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn message_content_blocks(entry: &Value) -> Vec<&Value> {
    entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().collect())
        .unwrap_or_default()
}

fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

fn block_text(block: &Value) -> Option<&str> {
    block.get("text").and_then(Value::as_str)
}

fn block_tool_name(block: &Value) -> Option<&str> {
    block.get("name").and_then(Value::as_str)
}

fn text_blocks(entry: &Value) -> Vec<&str> {
    message_content_blocks(entry)
        .into_iter()
        .filter(|block| block_type(block) == Some("text"))
        .filter_map(block_text)
        .collect()
}

/// Classify a single entry per the frozen table above: `tool_use` takes
/// priority over `text` within the same `assistant` entry (the model may
/// narrate before calling a tool), a `thinking`-only `assistant` entry is
/// still mid-turn, a bare `user` entry means a turn just started or a
/// result just came back, and anything else is `unknown`. Never applies the
/// debounce itself -- that's `analyze`'s job, since it needs `idle_secs`.
fn classify_last(entry: &Value) -> SubagentState {
    match entry_type(entry) {
        Some("assistant") => {
            let blocks = message_content_blocks(entry);
            if blocks
                .iter()
                .any(|block| block_type(block) == Some("tool_use"))
            {
                SubagentState::ToolWait
            } else if blocks.iter().any(|block| block_type(block) == Some("text")) {
                SubagentState::Done
            } else if blocks
                .iter()
                .any(|block| block_type(block) == Some("thinking"))
            {
                SubagentState::Generating
            } else {
                SubagentState::Unknown
            }
        }
        Some("user") => SubagentState::Generating,
        _ => SubagentState::Unknown,
    }
}

/// The most recently used tool name, scanning backward through every entry
/// (not just the last) so `list` shows useful context even after the
/// subagent has moved past that tool call.
fn last_tool_used(entries: &[Value]) -> Option<String> {
    entries.iter().rev().find_map(|entry| {
        if entry_type(entry) != Some("assistant") {
            return None;
        }
        message_content_blocks(entry)
            .into_iter()
            .rev()
            .find(|block| block_type(block) == Some("tool_use"))
            .and_then(block_tool_name)
            .map(str::to_string)
    })
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
