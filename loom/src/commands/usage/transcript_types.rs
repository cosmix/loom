//! Record and entry type definitions for parsed Claude Code transcripts.

use std::sync::atomic::{AtomicBool, Ordering};

static COUNTER_OVERFLOW_REPORTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn saturated_counter(left: u64, right: u64) -> u64 {
    left.checked_add(right).unwrap_or_else(|| {
        if !COUNTER_OVERFLOW_REPORTED.swap(true, Ordering::Relaxed) {
            eprintln!(
                "loom usage: token totals saturated after u64 overflow; these totals are not exact"
            );
        }
        u64::MAX
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Main,
    Subagent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub ephemeral_5m: u64,
    pub ephemeral_1h: u64,
}

impl TokenUsage {
    /// Tokens the model had resident for this request.
    pub fn resident(&self) -> u64 {
        saturated_counter(
            saturated_counter(self.input, self.cache_creation),
            self.cache_read,
        )
    }
    /// Field-wise sum, for rolling totals.
    pub fn add(&mut self, other: &TokenUsage) {
        self.input = saturated_counter(self.input, other.input);
        self.cache_creation = saturated_counter(self.cache_creation, other.cache_creation);
        self.cache_read = saturated_counter(self.cache_read, other.cache_read);
        self.output = saturated_counter(self.output, other.output);
        self.ephemeral_5m = saturated_counter(self.ephemeral_5m, other.ephemeral_5m);
        self.ephemeral_1h = saturated_counter(self.ephemeral_1h, other.ephemeral_1h);
    }
}

#[derive(Debug, Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub message_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub model: String,
    pub usage: TokenUsage,
    pub tool_uses: Vec<ToolUse>,
    pub thinking_chars: usize,
    pub text_chars: usize,
    pub normalization: RequestNormalization,
}

#[derive(Debug, Clone, Default)]
pub struct RequestNormalization {
    pub usage_observed: bool,
    pub first_usage: Option<TokenUsage>,
    pub first_thinking_output_tokens: Option<u64>,
    pub usage_observations: usize,
    pub changed_usage_fields: usize,
    pub thinking_output_tokens: Option<u64>,
    pub invalid_thinking_output: bool,
    pub invalid_usage: bool,
    pub invalid_cache_relation: bool,
    pub line_ordinal: usize,
    pub first_usage_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub first_usage_ordinal: usize,
    pub first_observed_in_range: bool,
    pub true_fresh_start: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TranscriptDiagnostics {
    pub malformed_rows: usize,
    pub missing_timestamp_rows: usize,
}

/// Model marker for a synthetic API-error row that never reached the API.
/// `Transcript::requests()` includes these attempted requests.
pub(crate) use crate::provider_data::SYNTHETIC_MODEL;

#[derive(Debug, Clone)]
pub struct UserEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// `Some` when this record is a `tool_result`, naming the `tool_use` it
    /// answers; `None` when it is a message sent to the agent.
    pub tool_use_id: Option<String>,
    /// The tool result's text, or the message text.
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Entry {
    Assistant(Box<Request>),
    User(UserEntry),
}

impl Entry {
    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::Assistant(request) => request.as_ref().timestamp,
            Self::User(entry) => entry.timestamp,
        }
    }
}

/// A rendered `attachment` row: context Claude Code added to the prompt.
#[derive(Debug, Clone)]
pub struct AttachmentEntry {
    /// Non-synthetic requests retained before this row; the row first reaches
    /// the model on request `requests_before + 1`.
    pub requests_before: usize,
    pub kind: String,
    /// Characters across the row's rendered content strings.
    pub chars: usize,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub path: std::path::PathBuf,
    pub scope: Scope,
    pub project_slug: String,
    /// Raw transcript cwd retained only long enough for basename-only provider
    /// attribution. It is never serialized in a usage report.
    pub project_path: Option<std::path::PathBuf>,
    /// Main session UUID. For a subagent this is its PARENT session's UUID -
    /// that is what makes the parent/child tree the report needs.
    pub session_id: String,
    pub agent_id: Option<String>,
    /// Spawn type from loom's hook-side `starts.jsonl` ledger, else (subagents
    /// only) from the `agent-<id>.meta.json` sidecar Claude Code writes beside
    /// the transcript; see `agent_type_from_sidecar`. `None` when neither
    /// exists.
    pub agent_type: Option<String>,
    /// True when `agent_type` came from the sidecar because the ledger had no
    /// row. Ledger-coverage diagnostics must not count such a transcript.
    pub agent_type_from_sidecar: bool,
    /// The model the spawn guard recorded for this agent, joined from
    /// `spawns.jsonl`; `ambiguous` when the order join cannot decide.
    pub requested_model: Option<String>,
    /// Hook-provided lifecycle metadata. It is absent rather than inferred.
    pub stage_id: Option<String>,
    pub loom_session_id: Option<String>,
    /// Exact durable forwarding receipt metadata, when tuple attribution succeeds.
    pub forward_receipt: Option<super::forward_join::ForwardMetadata>,
    /// Whether this transcript is a forwarding execution needing attribution.
    pub forward_candidate: bool,
    /// The transcript's first user entry, captured BEFORE the `since` cutoff
    /// is applied. Spawn-prompt classification reads it, and a cutoff that
    /// dropped it would silently reclassify every long-running subagent from
    /// its real preamble to whatever mid-conversation message survived.
    pub first_user_entry: Option<UserEntry>,
    /// File order, deduplicated, entries older than the `since` cutoff dropped.
    pub entries: Vec<Entry>,
    /// Context rows the model saw that are neither requests nor user entries,
    /// older than the `since` cutoff dropped.
    pub attachments: Vec<AttachmentEntry>,
    pub diagnostics: TranscriptDiagnostics,
}

impl Transcript {
    pub fn requests(&self) -> impl Iterator<Item = &Request> {
        self.entries.iter().filter_map(|entry| match entry {
            Entry::Assistant(request) => Some(request.as_ref()),
            Entry::User(_) => None,
        })
    }

    pub fn total_usage(&self) -> TokenUsage {
        self.requests()
            .fold(TokenUsage::default(), |mut total, request| {
                total.add(&request.usage);
                total
            })
    }
}
