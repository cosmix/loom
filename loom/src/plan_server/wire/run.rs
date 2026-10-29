//! Wire types for run handoff and progress.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceState {
    Absent,
    Initialized,
    Running,
    Stopped,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOffer {
    Start,
    StartInitialized,
    NoneRunning,
    Resume,
    StartNext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlanChoice {
    pub path: String,
    pub slug: String,
    pub listed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangedPath {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UnmergedBranch {
    pub branch: String,
    pub ahead: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AbandonView {
    pub plan: String,
    pub merged_stages: Vec<String>,
    pub unmerged_stages: Vec<String>,
    pub unmerged_branches: Vec<UnmergedBranch>,
    pub base_branch: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Tmux,
    Native,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunJob {
    Commit,
    Start,
    Abandon,
    FinishAbandon,
    Dashboard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunStateView {
    pub workspace: WorkspaceState,
    pub active_plan: Option<String>,
    pub offer: RunOffer,
    pub plans: Vec<PlanChoice>,
    pub preselected: Option<String>,
    pub branch: String,
    pub doc_changes: Vec<ChangedPath>,
    pub blocking_changes: Vec<ChangedPath>,
    pub backends: Vec<Backend>,
    pub default_backend: Backend,
    pub clean_notice: bool,
    pub abandon: Option<AbandonView>,
    pub finish_abandon: Option<String>,
    pub dashboard_url: Option<String>,
    pub busy: Option<RunJob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommitRequest {
    pub paths: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunStartRequest {
    pub plan: String,
    pub backend: Backend,
    pub clean: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AbandonRequest {
    pub confirmed_unmerged: Vec<UnmergedBranch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunOutput {
    pub job: RunJob,
    pub step: String,
    pub stream: Option<OutputStream>,
    pub line: Option<String>,
    pub exit_code: Option<i32>,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunStrip {
    pub plan: Option<String>,
    pub daemon_running: bool,
    pub merged: u32,
    pub total: u32,
    pub attention: u32,
    pub dashboard_url: Option<String>,
}
