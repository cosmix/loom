//! Wire and file model of live token flow: `StatusData.tokens` and
//! `.loom/work/tokens/flow.json`.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageFlow {
    pub stage_id: String,
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
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
    pub quota_left: Vec<QuotaLeft>,
    pub unreadable_lines: u64,
}

/// `<work_root>/tokens/flow.json`.
pub fn flow_path(work_root: &Path) -> PathBuf {
    work_root.join(TOKENS_DIR).join(FLOW_FILE)
}

/// The last written flow, or `None` for missing, symlinked, non-regular,
/// oversized, unreadable, or invalid files. Checks file type before opening
/// so a FIFO does not block the caller. Never panics or logs.
pub fn read_flow(work_root: &Path) -> Option<TokenFlow> {
    let path = flow_path(work_root);
    crate::quota::cache::reject_symlink(&path).ok()?;
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_FLOW_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .ok()?
        .take(MAX_FLOW_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FLOW_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
