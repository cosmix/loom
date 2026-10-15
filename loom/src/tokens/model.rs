//! Wire and file model of live token flow: `StatusData.tokens` and
//! `.loom/work/tokens/flow.json`.

use crate::fs::safe_read::read_bounded;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const BUCKET_SECS: i64 = 30;
pub const STAGE_SERIES_LEN: usize = 120;
pub const PLAN_SERIES_LEN: usize = 60;
pub const RATE_BUCKETS: usize = 10;
pub const ACTIVE_BUCKETS: usize = 4;
pub const TOKENS_DIR: &str = "tokens";
pub const FLOW_FILE: &str = "flow.json";
pub const MAX_FLOW_BYTES: u64 = 512 * 1024;
pub const FLOW_STALE_SECS: i64 = 600;
/// Distinct (model, effort) keys one agent keeps; usage for a further key goes to `OTHER_MODEL`.
pub const MAX_MODEL_KEYS: usize = 8;
/// Entries of `TokenFlow.models`; the rest are merged into one `OTHER_MODEL` entry.
pub const MAX_MODEL_ROWS: usize = 24;
/// The model of usage read before any line named one.
pub const UNKNOWN_MODEL: &str = "unknown";
/// The key that absorbs usage past `MAX_MODEL_KEYS` or `MAX_MODEL_ROWS`.
pub const OTHER_MODEL: &str = "other";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl TokenUsage {
    /// input + cache_write + output, saturating.
    pub fn fresh(&self) -> u64 {
        self.input
            .saturating_add(self.cache_write)
            .saturating_add(self.output)
    }

    /// Field-wise saturating add.
    pub fn add(&mut self, other: &TokenUsage) {
        self.input = self.input.saturating_add(other.input);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.output = self.output.saturating_add(other.output);
    }
}

/// A token count a source line may leave out: an absent field and a JSON
/// `null` are both 0, any other value that is not a non-negative integer is
/// `None` (the line is unreadable).
pub(super) fn optional_count(value: Option<&Value>) -> Option<u64> {
    match value {
        None | Some(Value::Null) => Some(0),
        Some(value) => value.as_u64(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    Main,
    Subagent,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentFlow {
    pub id: String,
    pub kind: AgentKind,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub active: bool,
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
}

/// Tokens used with one model at one reasoning effort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub effort: Option<String>,
    pub total: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageFlow {
    pub stage_id: String,
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
    /// Cache hit percent per bucket, aligned with `series`; `None` for a bucket
    /// without input-side tokens.
    pub hit_series: Vec<Option<u8>>,
    pub agents: Vec<AgentFlow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanFlow {
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuotaProvider {
    Claude,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaLeft {
    pub provider: QuotaProvider,
    pub minutes_left: Option<u32>,
    pub lasts_to_reset: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenFlow {
    pub observed_at: i64,
    pub bucket_secs: i64,
    pub plan: PlanFlow,
    pub stages: Vec<StageFlow>,
    /// Plan-wide totals per (model, effort), largest fresh total first, at most
    /// `MAX_MODEL_ROWS` entries.
    pub models: Vec<ModelUsage>,
    pub quota_left: Vec<QuotaLeft>,
    pub unreadable_lines: u64,
}

/// `<work_root>/tokens/flow.json`.
pub fn flow_path(work_root: &Path) -> PathBuf {
    work_root.join(TOKENS_DIR).join(FLOW_FILE)
}

/// The last written flow, or `None` for missing, symlinked (the file or the
/// `tokens` directory), non-regular, oversized, unreadable, or invalid files.
/// `read_bounded` opens no-follow and non-blocking and checks the type and size
/// of the opened file, so a FIFO put in its place never blocks the caller.
/// Never panics or logs.
pub fn read_flow(work_root: &Path) -> Option<TokenFlow> {
    let bytes = read_bounded(
        &work_root.join(TOKENS_DIR),
        Path::new(FLOW_FILE),
        MAX_FLOW_BYTES as usize,
    )
    .ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
