//! Fixture builders shared by the token-flow contract tests.

use chrono::{DateTime, SecondsFormat, Utc};
use loom::commands::status::data::collect_status_data;
use loom::fs::session_files::save_session;
use loom::fs::work_dir::WorkDir;
use loom::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use loom::models::session::Session;
use loom::tokens::{scan_pass, AgentFlow, AgentKind, StageFlow, TokenFlow, TokenUsage};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const MAIN_MODEL: &str = "claude-opus-5-5";
pub const THREAD_ID: &str = "01a1396b-2aa4-7d93-9e18-d54685ff45df";
pub const BAD: [char; 3] = ['\u{1b}', '\u{202e}', '\n'];

/// One fixture work dir: `<tmp>/.loom/work`, Claude transcripts under
/// `<tmp>/claude`, Codex rollouts under `<tmp>/codex-sessions`.
pub struct Fx {
    pub tmp: TempDir,
    pub work_root: PathBuf,
}

/// The current unix time.
pub fn unix_now() -> i64 {
    Utc::now().timestamp()
}

pub fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

/// Canonical UTC millisecond timestamp, the form transcripts carry.
pub fn iso(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .expect("timestamp in range")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn usage_json(u: &TokenUsage) -> Value {
    json!({
        "input_tokens": u.input,
        "cache_creation_input_tokens": u.cache_write,
        "cache_read_input_tokens": u.cache_read,
        "cache_creation": {"ephemeral_5m_input_tokens": u.cache_write, "ephemeral_1h_input_tokens": 0},
        "output_tokens": u.output,
        "service_tier": "standard",
        "inference_geo": "not_available"
    })
}

/// A main-session assistant line carrying `usage`, in the section 5 shape.
pub fn claude_line(msg_id: &str, ts: i64, u: &TokenUsage) -> String {
    json!({
        "type": "assistant",
        "timestamp": iso(ts),
        "sessionId": "01ed9f4c-608e-4e2c-8edf-05d26adcd4d6",
        "message": {"id": msg_id, "model": MAIN_MODEL, "usage": usage_json(u)}
    })
    .to_string()
}

/// A subagent assistant line with the given model.
pub fn subagent_line(agent_id: &str, msg_id: &str, ts: i64, model: &str, u: &TokenUsage) -> String {
    json!({
        "type": "assistant",
        "timestamp": iso(ts),
        "isSidechain": true,
        "agentId": agent_id,
        "sessionId": "01ed9f4c-608e-4e2c-8edf-05d26adcd4d6",
        "message": {"id": msg_id, "model": model, "usage": usage_json(u)}
    })
    .to_string()
}

/// Lines joined with a trailing newline after each.
pub fn body(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// A Claude session uuid unique per index.
pub fn uuid(n: u32) -> String {
    format!("{n:08x}-608e-4e2c-8edf-05d26adcd4d6")
}

impl Fx {
    pub fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let work_root = tmp.path().join(".loom").join("work");
        fs::create_dir_all(&work_root).expect("work root");
        fs::create_dir_all(tmp.path().join("claude")).expect("claude dir");
        fs::create_dir_all(tmp.path().join("codex-sessions")).expect("codex dir");
        Self { tmp, work_root }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    pub fn transcript(&self, session_uuid: &str) -> PathBuf {
        self.path("claude").join(format!("{session_uuid}.jsonl"))
    }

    /// Writes the stage's main transcript and a session record pointing at it.
    /// Returns the loom session id.
    pub fn add_stage(&self, stage_id: &str, session_uuid: &str, text: &str) -> String {
        let transcript = self.transcript(session_uuid);
        fs::write(&transcript, text).expect("write transcript");
        let mut session = Session::new();
        session.stage_id = Some(stage_id.to_string());
        session.transcript_path = Some(transcript.to_string_lossy().into_owned());
        save_session(&session, &self.work_root).expect("save session");
        session.id
    }

    pub fn add_subagent(&self, session_uuid: &str, agent_id: &str, text: &str) {
        let dir = self.path("claude").join(session_uuid).join("subagents");
        fs::create_dir_all(&dir).expect("subagents dir");
        fs::write(dir.join(format!("agent-{agent_id}.jsonl")), text).expect("subagent file");
    }

    /// A scoped started-agent ledger row, in the full row shape.
    pub fn add_start_row(
        &self,
        stage: &str,
        agent: &str,
        agent_type: &str,
        parent: &str,
        loom: &str,
    ) {
        let row = json!({
            "agent_id": agent, "agent_type": agent_type, "stage_id": stage,
            "parent_session_id": parent, "loom_session_id": loom, "ts": iso(unix_now()),
        });
        let dir = self.work_root.join("subagents").join(stage);
        fs::create_dir_all(&dir).expect("ledger dir");
        append(&dir.join("starts.jsonl"), &format!("{row}\n"));
    }

    /// A terminal forward receipt for `thread_id` in the stage's receipts file.
    pub fn add_forward_receipt(&self, stage: &str, parent: &str, loom: &str, thread_id: &str) {
        let identity = ForwardIdentity::new(parent, "a0c0dec0ffee", "toolu_01", stage, loom)
            .expect("forward identity");
        let observation = ForwardObservation {
            schema: FORWARD_RECEIPT_SCHEMA,
            receipt_id: identity.receipt_id(),
            parent_session_id: identity.parent_session_id.clone(),
            agent_id: identity.agent_id.clone(),
            tool_use_id: identity.tool_use_id.clone(),
            stage_id: identity.stage_id.clone(),
            loom_session_id: identity.loom_session_id.clone(),
            backend: ForwardBackend::Direct,
            backend_id: "job-1".to_string(),
            state: ForwardState::Succeeded,
            observed_at: Utc::now(),
            exit_code: Some(0),
            codex_thread_id: Some(thread_id.to_string()),
            locator: None,
            model: Some("gpt-6.1-sol".to_string()),
            effort: None,
        };
        let path = receipts_path(&self.work_root, stage).expect("receipts path");
        fs::create_dir_all(path.parent().expect("parent")).expect("receipts dir");
        let line = observation.encode_line().expect("encode receipt");
        append(&path, &format!("{line}\n"));
    }

    /// The section 5 rollout for `thread_id`, its token_count lines at `ts`.
    pub fn add_codex_rollout(&self, thread_id: &str, ts: i64) {
        let dir = self.path("codex-sessions/2026/10/14");
        fs::create_dir_all(&dir).expect("rollout dir");
        let count = |total: [u64; 3], last: [u64; 3], at: i64| {
            let usage = |[input, cached, output]: [u64; 3]| {
                json!({"input_tokens": input, "cached_input_tokens": cached,
                    "cache_write_input_tokens": 0, "output_tokens": output,
                    "reasoning_output_tokens": 0, "total_tokens": input + output})
            };
            json!({"timestamp": iso(at), "type": "event_msg", "payload": {"type": "token_count",
                "info": {"total_token_usage": usage(total), "last_token_usage": usage(last),
                    "model_context_window": 475000}}})
            .to_string()
        };
        let lines = [
            json!({"timestamp": iso(ts - 8), "type": "session_meta",
                "payload": {"id": thread_id, "cwd": "/tmp", "originator": "codex-tui"}})
            .to_string(),
            count([21255, 12160, 110], [21255, 12160, 110], ts),
            count([43274, 33280, 206], [22019, 21120, 96], ts + 10),
            json!({"type": "turn_context", "payload": {"model": "gpt-6.1-sol"}}).to_string(),
        ];
        let name = format!("rollout-2026-10-14T10-58-07-{thread_id}.jsonl");
        fs::write(dir.join(name), body(&lines)).expect("rollout file");
    }

    /// `<work_root>/quota/history/claude.jsonl` with the given rows.
    pub fn write_claude_history(&self, rows: &[Value]) {
        let dir = self.work_root.join("quota").join("history");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .expect("history dir");
        let lines: Vec<String> = rows.iter().map(Value::to_string).collect();
        fs::write(dir.join("claude.jsonl"), body(&lines)).expect("history file");
    }

    /// `<work_root>/tokens`, mode 0700.
    pub fn tokens_dir(&self) -> PathBuf {
        let dir = self.work_root.join("tokens");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .expect("tokens dir");
        dir
    }

    pub fn try_scan(&self, now: i64) -> anyhow::Result<TokenFlow> {
        scan_pass(&self.work_root, Some(&self.path("codex-sessions")), now)
    }

    pub fn scan(&self, now: i64) -> TokenFlow {
        self.try_scan(now).expect("scan_pass")
    }

    /// `StatusData.tokens` as the collector publishes it.
    pub fn status_tokens(&self) -> Option<TokenFlow> {
        let work_dir = WorkDir::new(self.tmp.path()).expect("work dir");
        collect_status_data(&work_dir).expect("status data").tokens
    }
}

/// A five-hour plus optional seven-day quota history row.
pub fn history_row(at: i64, five: (f64, i64), seven: Option<(f64, i64)>) -> Value {
    let mut windows =
        vec![json!({"kind": "five-hour", "used_percent": five.0, "resets_at": five.1})];
    if let Some((used, resets)) = seven {
        windows.push(json!({"kind": "seven-day", "used_percent": used, "resets_at": resets}));
    }
    json!({"schema_version": 1, "observed_at": at, "windows": windows, "plan": null})
}

pub fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open for append");
    file.write_all(text.as_bytes()).expect("append");
}

/// Overwrites `path` from offset 0 without truncating, keeping its inode.
pub fn overwrite_in_place(path: &Path, text: &str) {
    let mut file = OpenOptions::new().write(true).open(path).expect("open");
    file.write_all(text.as_bytes()).expect("overwrite");
}

pub fn stage<'a>(flow: &'a TokenFlow, stage_id: &str) -> &'a StageFlow {
    flow.stages
        .iter()
        .find(|stage| stage.stage_id == stage_id)
        .unwrap_or_else(|| panic!("stage {stage_id} missing from {flow:?}"))
}

pub fn agents_of(stage: &StageFlow, kind: AgentKind) -> Vec<&AgentFlow> {
    stage
        .agents
        .iter()
        .filter(|agent| agent.kind == kind)
        .collect()
}

/// The stage's single main agent.
pub fn main_agent(stage: &StageFlow) -> &AgentFlow {
    let mains = agents_of(stage, AgentKind::Main);
    assert_eq!(mains.len(), 1, "expected one main agent in {stage:?}");
    mains[0]
}

pub fn assert_clean(label: &str, value: &str) {
    assert!(
        !value.contains(BAD),
        "{label} carries ESC, U+202E or a newline: {value:?}"
    );
}
