//! Wire types for pressure test configuration and progress.

use serde::{Deserialize, Serialize};

pub mod disabled_reason {
    pub const CODEX_MISSING: &str = "The codex CLI is not installed.";
    pub const CODEX_SKILL_MISSING: &str =
        "Codex's pressure skill is not installed under $CODEX_HOME/skills/pressure/.";
    pub const READ_ONLY: &str = "The server is in read-only mode.";
    pub const NOT_HOLDER: &str = "Another browser holds authoring.";
    pub const RUNNING: &str = "A pressure test is already running.";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureOptions {
    pub available: bool,
    pub disabled_reason: Option<String>,
    pub plans: Vec<String>,
    pub defaults: PressureSlots,
    pub claude_models: Vec<String>,
    pub claude_efforts: Vec<String>,
    pub codex_models: Vec<String>,
    pub codex_efforts: Vec<String>,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureSlots {
    pub rounds: u32,
    pub claude_model: String,
    pub claude_effort: String,
    pub codex_model: String,
    pub codex_effort: String,
    pub address_model: String,
    pub address_effort: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureStart {
    pub plan: String,
    #[serde(flatten)]
    pub slots: PressureSlots,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureStepKind {
    ClaudePressure,
    Codex,
    ClaudeAddress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Running,
    Done,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexState {
    Running,
    ReviewWritten,
    NoReview,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodexStatus {
    pub model: String,
    pub state: CodexState,
    pub report_path: Option<String>,
    pub log_tail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TokenTotals {
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureStepView {
    pub round: u32,
    pub kind: PressureStepKind,
    pub session_id: Option<String>,
    pub model: String,
    pub effort: String,
    pub state: StepState,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub tokens: Option<TokenTotals>,
    pub codex: Option<CodexStatus>,
    pub edits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEndKind {
    Recorded,
    Stopped,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureEnd {
    pub kind: RunEndKind,
    pub by: Option<String>,
    pub step: Option<String>,
    pub reason: Option<String>,
    pub line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PressureRunView {
    pub plan: String,
    pub rounds: u32,
    pub round: u32,
    pub step: PressureStepKind,
    pub started_ms: u64,
    pub tokens_total: u64,
    pub steps: Vec<PressureStepView>,
    pub ended: Option<PressureEnd>,
}
