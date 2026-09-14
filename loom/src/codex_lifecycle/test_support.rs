use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::models::session::{Session, SessionStatus};
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity};

use super::authorization::CodexAuthorization;
use super::jobs::workspace_state_dir;

pub(super) const STAGE: &str = "stage-1";
pub(super) const SESSION: &str = "session-1";
pub(super) const PARENT: &str = "parent-1";
pub(super) const FORWARDER: &str = "agent-1";
pub(super) const MODEL: &str = "gpt-5.6-sol";
pub(super) const EFFORT: &str = "xhigh";
pub(super) const INV_A: &str = "inv-0123456789abcdef0123456789abcdef";
pub(super) const INV_B: &str = "inv-fedcba9876543210fedcba9876543210";

pub(super) struct Fixture {
    _temp: TempDir,
    pub work_dir: PathBuf,
    pub workspace: PathBuf,
    pub state_root: PathBuf,
    pub companion: PathBuf,
    pub session: Session,
}

impl Fixture {
    pub fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let work_dir = temp.path().join(".loom/work");
        let workspace = temp.path().join("workspace");
        let state_root = temp.path().join("home/.codex/plugin-data/state");
        let companion = temp.path().join("plugin/codex-companion.mjs");
        fs::create_dir_all(work_dir.join("stages"))?;
        fs::create_dir_all(&workspace)?;
        fs::create_dir_all(&state_root)?;
        fs::create_dir_all(companion.parent().context("companion parent")?)?;
        fs::write(&companion, "// companion fixture\n")?;
        let workspace = fs::canonicalize(workspace)?;
        let state_root = fs::canonicalize(state_root)?;
        let companion = fs::canonicalize(companion)?;
        fs::write(
            work_dir.join("stages").join(format!("{STAGE}.md")),
            format!("---\nid: {STAGE}\nsession: {SESSION}\n---\n"),
        )?;
        let mut session = Session::new();
        session.id = SESSION.into();
        session.stage_id = Some(STAGE.into());
        session.worktree_path = Some(workspace.clone());
        session.status = SessionStatus::Running;
        Ok(Self {
            _temp: temp,
            work_dir,
            workspace,
            state_root,
            companion,
            session,
        })
    }

    pub fn authorization_value(&self, unit: &str, invocation: &str) -> Value {
        json!({
            "v": 2,
            "ts": "2026-09-14T10:00:00.000Z",
            "stage_id": STAGE,
            "session_id": SESSION,
            "parent_session_id": PARENT,
            "forwarder_agent_id": FORWARDER,
            "tool_use_id": format!("tool-{unit}"),
            "unit_id": unit,
            "invocation_id": invocation,
            "model": MODEL,
            "effort": EFFORT,
            "workspace_root": &self.workspace,
            "companion_version": "1.0.6",
            "companion_path": &self.companion,
            "state_root": &self.state_root
        })
    }

    pub fn authorization(&self, unit: &str, invocation: &str) -> Result<CodexAuthorization> {
        CodexAuthorization::from_v2_value(&self.authorization_value(unit, invocation))
    }

    pub fn write_authorizations(&self, rows: &[Value]) -> Result<()> {
        let directory = self.work_dir.join("subagents").join(STAGE);
        fs::create_dir_all(&directory)?;
        let mut text = String::new();
        for row in rows {
            text.push_str(&serde_json::to_string(row)?);
            text.push('\n');
        }
        fs::write(directory.join("codex.jsonl"), text)?;
        Ok(())
    }

    pub fn append_authorization_text(&self, text: &str) -> Result<()> {
        use std::io::Write;
        let path = self
            .work_dir
            .join("subagents")
            .join(STAGE)
            .join("codex.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        file.write_all(text.as_bytes())?;
        Ok(())
    }

    pub fn write_job(
        &self,
        authorization: &CodexAuthorization,
        job_id: &str,
        status: &str,
    ) -> Result<PathBuf> {
        self.write_custom_job(
            authorization,
            job_id,
            status,
            &authorization.encoded_session_id(),
            MODEL,
            &self.workspace,
        )
    }

    pub fn write_custom_job(
        &self,
        authorization: &CodexAuthorization,
        job_id: &str,
        status: &str,
        session_id: &str,
        model: &str,
        workspace_root: &Path,
    ) -> Result<PathBuf> {
        let jobs = workspace_state_dir(&self.state_root, &self.workspace)?.join("jobs");
        fs::create_dir_all(&jobs)?;
        let path = jobs.join(format!("{job_id}.json"));
        fs::write(
            &path,
            serde_json::to_vec(&job_value(
                authorization,
                job_id,
                status,
                session_id,
                model,
                workspace_root,
            ))?,
        )?;
        Ok(path)
    }

    pub fn write_malformed_job(&self, job_id: &str) -> Result<()> {
        let jobs = workspace_state_dir(&self.state_root, &self.workspace)?.join("jobs");
        fs::create_dir_all(&jobs)?;
        fs::write(jobs.join(format!("{job_id}.json")), "{malformed")?;
        Ok(())
    }
}

pub(super) fn identity(authorization: &CodexAuthorization, job_id: &str) -> WorkerIdentity {
    WorkerIdentity::Codex {
        stage_id: authorization.stage_id.clone(),
        loom_session_id: authorization.loom_session_id.clone(),
        parent_session_id: authorization.parent_session_id.clone(),
        forwarder_agent_id: authorization.forwarder_agent_id.clone(),
        unit_id: authorization.unit_id.clone(),
        invocation_id: authorization.invocation_id.clone(),
        workspace_root: authorization.workspace_root.clone(),
        execution: CodexExecution::Companion {
            job_id: job_id.into(),
        },
    }
}

fn job_value(
    authorization: &CodexAuthorization,
    job_id: &str,
    status: &str,
    session_id: &str,
    model: &str,
    workspace_root: &Path,
) -> Value {
    let terminal = matches!(status, "completed" | "failed" | "cancelled");
    json!({
        "id": job_id,
        "workspaceRoot": workspace_root,
        "jobClass": "task",
        "write": true,
        "sessionId": session_id,
        "status": status,
        "phase": job_phase(status),
        "threadId": terminal.then_some("thread-1"),
        "turnId": terminal.then_some("turn-1"),
        "completedAt": terminal.then_some("2026-09-14T10:01:00.000Z"),
        "errorMessage": job_error(status),
        "result": terminal.then_some(json!({"text": "terminal"})),
        "request": {
            "cwd": &authorization.workspace_root,
            "model": model,
            "effort": EFFORT,
            "prompt": "fixture",
            "write": true,
            "resumeLast": false,
            "jobId": job_id
        }
    })
}

fn job_phase(status: &str) -> &str {
    match status {
        "completed" => "done",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "queued" => "queued",
        _ => "starting",
    }
}

fn job_error(status: &str) -> Option<&str> {
    match status {
        "failed" => Some("companion failed"),
        "cancelled" => Some("companion cancelled"),
        _ => None,
    }
}
