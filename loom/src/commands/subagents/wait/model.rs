use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::codex_lifecycle::CodexAuthorization;
use crate::process::ProcessIdentity;
use crate::subagent_lifecycle::WorkerIdentity;

pub const WAIT_SCHEMA_VERSION: u16 = 1;
pub const EXIT_TIMEOUT: i32 = 2;
pub const EXIT_WORKER_TERMINAL: i32 = 3;
pub const EXIT_BUSY: i32 = 4;
pub const EXIT_UNKNOWN: i32 = 5;
pub const EXIT_STALLED: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerKind {
    Claude,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkerSpec {
    pub kind: WorkerKind,
    pub id: String,
}

impl FromStr for WorkerSpec {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let Some((kind, id)) = value.split_once(':') else {
            bail!("worker must use claude:<agent-id> or codex:<unit-id>");
        };
        let kind = match kind {
            "claude" => WorkerKind::Claude,
            "codex" => WorkerKind::Codex,
            _ => bail!("worker kind must be 'claude' or 'codex'"),
        };
        if !claude_named_id_is_safe(kind, id) && !crate::models::forward_receipt::is_safe_id(id) {
            bail!("worker id is empty or unsafe");
        }
        Ok(Self {
            kind,
            id: id.to_owned(),
        })
    }
}

/// A Claude worker id may be a harness-named `<name>@session-<hex>` pair
/// (the harness gives a named agent that compound id, and its `agent_id` is
/// recorded verbatim in loom's hook-side SubagentStart ledger). `@` is never
/// a safe-id character on its own, so accepting it here can only ever mean
/// this shape; each half must still independently pass `is_safe_id`, which
/// is never loosened since it also guards file names elsewhere. A Codex id
/// never gets this treatment.
fn claude_named_id_is_safe(kind: WorkerKind, id: &str) -> bool {
    kind == WorkerKind::Claude
        && id.split_once('@').is_some_and(|(name, session)| {
            crate::models::forward_receipt::is_safe_id(name)
                && crate::models::forward_receipt::is_safe_id(session)
        })
}

/// A plain id's transcript is `agent-<id>.jsonl`. A harness-named id
/// (`<name>@session-<hex>`) instead carries an unrelated 16-hex suffix of
/// its own (`agent-a<name>-<16 hex>.jsonl`), so it is found by listing the
/// directory rather than built directly; zero or several matches is an
/// error, never a silent guess at "the" transcript.
pub fn resolve_claude_transcript(subagents_dir: &Path, id: &str) -> Result<PathBuf> {
    let Some((name, _)) = id.split_once('@') else {
        let path = subagents_dir.join(format!("agent-{id}.jsonl"));
        return std::fs::canonicalize(&path).context("canonicalizing Claude transcript");
    };
    let prefix = format!("agent-a{name}-");
    let matches: Vec<PathBuf> = std::fs::read_dir(subagents_dir)
        .context("listing subagents directory")?
        .filter_map(std::io::Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_owned();
            (file.starts_with(&prefix) && file.ends_with(".jsonl")).then(|| entry.path())
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "named Claude worker '{name}' resolved to {} transcripts",
        matches.len()
    );
    std::fs::canonicalize(&matches[0]).context("canonicalizing Claude transcript")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReference {
    pub source: String,
    pub path: PathBuf,
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexAuthority {
    pub stage_id: String,
    pub loom_session_id: String,
    pub parent_session_id: String,
    pub forwarder_agent_id: String,
    pub unit_id: String,
    pub invocation_id: String,
    pub model: String,
    pub effort: String,
    pub authorized_at: DateTime<Utc>,
    pub selected_companion: PathBuf,
    pub companion_version: String,
    pub effective_state_root: PathBuf,
    pub workspace_root: PathBuf,
    pub tool_use_id: String,
}

impl From<CodexAuthorization> for CodexAuthority {
    fn from(value: CodexAuthorization) -> Self {
        Self {
            stage_id: value.stage_id,
            loom_session_id: value.loom_session_id,
            parent_session_id: value.parent_session_id,
            forwarder_agent_id: value.forwarder_agent_id,
            unit_id: value.unit_id,
            invocation_id: value.invocation_id,
            model: value.model,
            effort: value.effort,
            authorized_at: value.authorized_at,
            selected_companion: value.selected_companion,
            companion_version: value.companion_version,
            effective_state_root: value.effective_state_root,
            workspace_root: value.workspace_root,
            tool_use_id: value.tool_use_id,
        }
    }
}

impl CodexAuthority {
    pub fn authorization(&self) -> CodexAuthorization {
        CodexAuthorization {
            stage_id: self.stage_id.clone(),
            loom_session_id: self.loom_session_id.clone(),
            parent_session_id: self.parent_session_id.clone(),
            forwarder_agent_id: self.forwarder_agent_id.clone(),
            unit_id: self.unit_id.clone(),
            invocation_id: self.invocation_id.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            authorized_at: self.authorized_at,
            selected_companion: self.selected_companion.clone(),
            companion_version: self.companion_version.clone(),
            effective_state_root: self.effective_state_root.clone(),
            workspace_root: self.workspace_root.clone(),
            tool_use_id: self.tool_use_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundWorker {
    pub worker: WorkerSpec,
    pub lifecycle_identity: WorkerIdentity,
    pub authority: Option<CodexAuthority>,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitIdentity {
    pub canonical_repo: PathBuf,
    pub canonical_worktree: PathBuf,
    pub stage_id: String,
    pub loom_session_id: String,
    pub parent_session_id: String,
    pub workers: Vec<BoundWorker>,
}

impl WaitIdentity {
    pub fn worker_specs(&self) -> Vec<WorkerSpec> {
        self.workers
            .iter()
            .map(|bound| bound.worker.clone())
            .collect()
    }

    pub fn evidence(&self) -> Vec<EvidenceReference> {
        self.workers
            .iter()
            .flat_map(|bound| bound.evidence.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootDeadline {
    pub boot_id: String,
    pub monotonic_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseOwner {
    pub pid: u32,
    pub start_time: Option<u64>,
}

impl From<ProcessIdentity> for LeaseOwner {
    fn from(value: ProcessIdentity) -> Self {
        Self {
            pid: value.pid,
            start_time: value.start_time,
        }
    }
}

impl From<LeaseOwner> for ProcessIdentity {
    fn from(value: LeaseOwner) -> Self {
        Self {
            pid: value.pid,
            start_time: value.start_time,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    Succeeded,
    TimedOut,
    Failed,
    Cancelled,
    /// A bound worker was alive but made no observable progress within its
    /// stall budget. Distinct from `TimedOut`, which only says the wait's own
    /// deadline passed and proves nothing about any worker.
    Stalled,
    Unknown,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitLease {
    pub schema_version: u16,
    pub wait_id: String,
    pub canonical_repo: PathBuf,
    pub source_revision: String,
    pub identity: WaitIdentity,
    pub deadline: BootDeadline,
    pub owner: LeaseOwner,
    pub terminal_result: Option<TerminalOutcome>,
    pub finished_unix_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Waiting,
    Succeeded,
    TimedOut,
    Failed,
    Cancelled,
    AlreadyWaiting,
    Busy,
    Stalled,
    Unknown,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaitEvent {
    pub schema_version: u16,
    pub wait_id: String,
    pub parent_session_id: String,
    pub loom_session_id: String,
    pub workers: Vec<WorkerSpec>,
    pub deadline: BootDeadline,
    pub outcome: EventOutcome,
    pub evidence: Vec<EvidenceReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn exit_code(outcome: &EventOutcome) -> i32 {
    match outcome {
        EventOutcome::Waiting | EventOutcome::Succeeded => 0,
        EventOutcome::TimedOut => EXIT_TIMEOUT,
        EventOutcome::Failed | EventOutcome::Cancelled => EXIT_WORKER_TERMINAL,
        EventOutcome::AlreadyWaiting | EventOutcome::Busy => EXIT_BUSY,
        EventOutcome::Stalled => EXIT_STALLED,
        EventOutcome::Unknown | EventOutcome::Interrupted => EXIT_UNKNOWN,
    }
}
