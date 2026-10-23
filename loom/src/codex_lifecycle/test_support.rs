use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::models::forward_receipt::{ForwardIdentity, RECEIPTS_FILE_NAME};
use crate::models::session::{Session, SessionStatus};
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity};

use super::authorization::CodexAuthorization;
use super::jobs::workspace_state_dir;

pub(crate) const STAGE: &str = "stage-1";
pub(super) const SESSION: &str = "session-1";
pub(super) const PARENT: &str = "parent-1";
pub(super) const FORWARDER: &str = "agent-1";
pub(super) const MODEL: &str = "gpt-6-sol";
pub(super) const EFFORT: &str = "xhigh";
pub(crate) const INV_A: &str = "inv-0123456789abcdef0123456789abcdef";
pub(super) const INV_B: &str = "inv-fedcba9876543210fedcba9876543210";

/// A direct-mode forward captured from liquidmon on 2026-10-23, verbatim but
/// for shortened paths; `workspace_root` is replaced by the fixture's.
pub(crate) const LIQUIDMON_STAGE: &str = "fix-instance-lock";
pub(crate) const LIQUIDMON_SESSION: &str = "session-04b928d3-1792774508";
pub(crate) const LIQUIDMON_AUTHORIZATION: &str = r#"{"v":2,"ts":"2026-10-23T16:55:45.000Z","stage_id":"fix-instance-lock","session_id":"session-04b928d3-1792774508","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","forwarder_agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","unit_id":"L1-device-lock","invocation_id":"inv-0f4dcc11d81ad6f26cb08d4b1a0c59ba","model":"gpt-5.6-terra","effort":"xhigh","workspace_root":"/repo/.worktrees/fix-instance-lock","companion_version":"1.0.6","companion_path":"/home/u/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs","state_root":"/home/u/.codex/plugin-data/state"}"#;
pub(crate) const LIQUIDMON_RUNNING: &str = r#"{"schema":1,"receipt_id":"096464e5f3005de684f92df63d8bbdca0585c7cef4fc98b8365ada2b4c052e74","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","stage_id":"fix-instance-lock","loom_session_id":"session-04b928d3-1792774508","backend":"direct","backend_id":"01a169b0-a2fd-7793-9874-db0889fa035c","state":"running","observed_at":"2026-10-23T17:00:37.322417869Z","exit_code":null,"codex_thread_id":"01a169b0-a2fd-7793-9874-db0889fa035c","locator":null,"model":"gpt-5.6-terra","effort":"xhigh"}"#;
pub(crate) const LIQUIDMON_SUCCEEDED: &str = r#"{"schema":1,"receipt_id":"096464e5f3005de684f92df63d8bbdca0585c7cef4fc98b8365ada2b4c052e74","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","stage_id":"fix-instance-lock","loom_session_id":"session-04b928d3-1792774508","backend":"direct","backend_id":"01a169b0-a2fd-7793-9874-db0889fa035c","state":"succeeded","observed_at":"2026-10-23T17:00:37.322417869Z","exit_code":0,"codex_thread_id":"01a169b0-a2fd-7793-9874-db0889fa035c","locator":null,"model":"gpt-5.6-terra","effort":"xhigh"}"#;

pub(crate) struct Fixture {
    _temp: TempDir,
    pub stage: String,
    pub work_dir: PathBuf,
    pub workspace: PathBuf,
    pub state_root: PathBuf,
    pub companion: PathBuf,
    pub session: Session,
}

impl Fixture {
    pub fn new() -> Result<Self> {
        Self::bound(STAGE, SESSION)
    }

    /// A fixture whose stage file binds `stage` to the running `session`.
    pub fn bound(stage: &str, session_id: &str) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let work_dir = root.join(".loom/work");
        let workspace = root.join("workspace");
        let state_root = root.join("home/.codex/plugin-data/state");
        let companion = root.join("plugin/codex-companion.mjs");
        fs::create_dir_all(work_dir.join("stages"))?;
        fs::create_dir_all(&workspace)?;
        fs::create_dir_all(&state_root)?;
        fs::create_dir_all(companion.parent().context("companion parent")?)?;
        fs::write(&companion, "// companion fixture\n")?;
        fs::write(
            work_dir.join("stages").join(format!("{stage}.md")),
            format!("---\nid: {stage}\nsession: {session_id}\n---\n"),
        )?;
        let mut session = Session::new();
        session.id = session_id.into();
        session.stage_id = Some(stage.into());
        session.worktree_path = Some(workspace.clone());
        session.status = SessionStatus::Running;
        Ok(Self {
            _temp: temp,
            stage: stage.into(),
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
        let mut text = String::new();
        for row in rows {
            text.push_str(&serde_json::to_string(row)?);
            text.push('\n');
        }
        self.write_stage_file("codex.jsonl", &text)
    }

    /// Write raw forward receipt lines, each newline-terminated.
    pub fn write_receipts<S: AsRef<str>>(&self, lines: &[S]) -> Result<()> {
        let text: String = lines
            .iter()
            .map(|line| format!("{}\n", line.as_ref()))
            .collect();
        self.write_stage_file(RECEIPTS_FILE_NAME, &text)
    }

    pub fn stage_path(&self, name: &str) -> PathBuf {
        self.work_dir.join("subagents").join(&self.stage).join(name)
    }

    fn write_stage_file(&self, name: &str, text: &str) -> Result<()> {
        let path = self.stage_path(name);
        fs::create_dir_all(path.parent().context("stage directory")?)?;
        fs::write(path, text)?;
        Ok(())
    }

    /// The liquidmon authorization row bound to this fixture's workspace.
    pub fn liquidmon_authorization_value(&self) -> Result<Value> {
        let mut value: Value = serde_json::from_str(LIQUIDMON_AUTHORIZATION)?;
        value["workspace_root"] = json!(&self.workspace);
        Ok(value)
    }

    pub fn append_authorization_text(&self, text: &str) -> Result<()> {
        use std::io::Write;
        let path = self.stage_path("codex.jsonl");
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

/// The liquidmon direct forward's stage, session, and authorization row; no
/// receipts are written.
pub(crate) fn liquidmon_direct_fixture() -> Result<Fixture> {
    let fixture = Fixture::bound(LIQUIDMON_STAGE, LIQUIDMON_SESSION)?;
    fixture.write_authorizations(&[fixture.liquidmon_authorization_value()?])?;
    Ok(fixture)
}

/// `base` receipt line after `edit`, with its receipt id recomputed from the
/// edited identity fields so the line still decodes.
pub(crate) fn receipt_line(base: &str, edit: impl FnOnce(&mut Value)) -> Result<String> {
    let mut value: Value = serde_json::from_str(base)?;
    edit(&mut value);
    let field = |name: &str| value[name].as_str().unwrap_or_default().to_owned();
    let identity = ForwardIdentity {
        parent_session_id: field("parent_session_id"),
        agent_id: field("agent_id"),
        tool_use_id: field("tool_use_id"),
        stage_id: field("stage_id"),
        loom_session_id: field("loom_session_id"),
    };
    value["receipt_id"] = json!(identity.receipt_id());
    Ok(serde_json::to_string(&value)?)
}

pub(super) fn identity(authorization: &CodexAuthorization, job_id: &str) -> WorkerIdentity {
    super::codex_worker_identity(
        authorization,
        CodexExecution::Companion {
            job_id: job_id.into(),
        },
    )
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
