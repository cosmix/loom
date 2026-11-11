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
//! Diagnostic classification keeps `tool-wait` at any idle time and never
//! harvests it. Watch applies a separate stall heuristic (see
//! [`super::stall`]). A stall is weaker evidence than exact lifecycle
//! termination.
//!
//! `unknown` is likewise never harvested and never counts as settled.
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

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

/// Minimum idle time (seconds) a structurally-`done` last entry must sit
/// unchanged before it is trusted as genuinely turn-final, rather than one
/// text block flushed mid-turn just before the next block (typically a
/// `tool_use`) lands. Observed intra-turn gaps reached 137.7 seconds; 180
/// seconds adds headroom because false completion risks concurrent writers.
/// Exact lifecycle evidence bypasses this delay. Overridable via `--debounce`
/// on `list` and `harvest`.
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
    /// Diagnostic classification does not time it out; watch applies its own stall policy.
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

/// What a transcript has shown so far, folded one parsed row at a time in file
/// order. A whole-file reader and a line-at-a-time reader reach the same
/// state, idle time, assistant-entry count and last tool.
#[derive(Debug, Clone, Default)]
pub struct AgentProgress {
    last: Option<Value>,
    last_at: Option<DateTime<Utc>>,
    assistant_entries: usize,
    last_tool: Option<String>,
}

impl AgentProgress {
    /// Fold in the next parsed row, of any type. `last` is the latest row
    /// observed, a bookkeeping row included, and `last_at` is that row's own
    /// timestamp (`None` when it has none, never an earlier row's).
    pub fn observe(&mut self, entry: &Value) {
        self.last_at = timestamp(entry);
        self.last = Some(entry.clone());
        if entry_type(entry) == Some("assistant") {
            self.assistant_entries += 1;
            if let Some(name) = assistant_tool_name(entry) {
                self.last_tool = Some(name.to_owned());
            }
        }
    }

    /// The transcript-only state: `Unknown` before any row was observed, else
    /// the structural state of the last row with the done debounce applied.
    pub fn state(&self, now: DateTime<Utc>, debounce_secs: u64) -> SubagentState {
        match &self.last {
            Some(last) => resolve_state(last, self.idle_secs(now).unwrap_or(0), debounce_secs),
            None => SubagentState::Unknown,
        }
    }

    /// Whole seconds from the last row's timestamp to `now`, at least 0;
    /// `None` when that row carries no timestamp.
    pub fn idle_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        self.last_at.map(|at| (now - at).num_seconds().max(0))
    }

    pub fn last(&self) -> Option<&Value> {
        self.last.as_ref()
    }

    pub fn last_at(&self) -> Option<DateTime<Utc>> {
        self.last_at
    }

    pub fn assistant_entries(&self) -> usize {
        self.assistant_entries
    }

    /// The most recent tool call, even when a later user result exists.
    pub fn last_tool(&self) -> Option<&str> {
        self.last_tool.as_deref()
    }
}

/// Classify one entry using the frozen table in the module doc. This emits
/// only transcript states: lifecycle-only `Failed` and `Cancelled` are
/// applied after exact identity lookup, and the done debounce is applied by
/// [`resolve_state`].
pub fn classify_last(entry: &Value) -> SubagentState {
    match entry_type(entry) {
        Some("assistant") => classify_assistant(message_content_blocks(entry)),
        Some("user") => SubagentState::Generating,
        _ => SubagentState::Unknown,
    }
}

fn classify_assistant(blocks: Vec<&Value>) -> SubagentState {
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

/// Resolve the transcript-only state before exact lifecycle evidence is
/// applied. A structurally-`done` entry still inside the debounce remains
/// `Generating`; lifecycle evidence may authoritatively override it later.
pub fn resolve_state(last: &Value, idle_secs: i64, debounce_secs: u64) -> SubagentState {
    let structural = classify_last(last);
    if structural == SubagentState::Done && idle_secs < debounce_secs as i64 {
        SubagentState::Generating
    } else {
        structural
    }
}

/// The name of an assistant entry's last named `tool_use` block, if it has one.
pub(crate) fn assistant_tool_name(entry: &Value) -> Option<&str> {
    (entry_type(entry) == Some("assistant"))
        .then(|| message_content_blocks(entry))?
        .into_iter()
        .rev()
        .find(|block| block_type(block) == Some("tool_use"))
        .and_then(block_tool_name)
}

pub(crate) fn timestamp(entry: &Value) -> Option<DateTime<Utc>> {
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

pub(crate) fn entry_type(entry: &Value) -> Option<&str> {
    entry.get("type").and_then(Value::as_str)
}

pub(crate) fn message_content_blocks(entry: &Value) -> Vec<&Value> {
    entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().collect())
        .unwrap_or_default()
}

pub(crate) fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

pub(crate) fn block_text(block: &Value) -> Option<&str> {
    block.get("text").and_then(Value::as_str)
}

fn block_tool_name(block: &Value) -> Option<&str> {
    block.get("name").and_then(Value::as_str)
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod progress_tests;
