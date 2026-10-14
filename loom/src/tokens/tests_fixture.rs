//! TempDir fixtures shared by the tracker's unit tests. Lines follow the
//! samples captured in the token-flow spec (common.md section 5).

use super::model::{AgentFlow, AgentKind, StageFlow, TokenFlow, TokenUsage};
use super::tracker::scan_pass;
use crate::fs::session_files::save_session;
use crate::models::session::Session;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub(super) const SESSION_UUID: &str = "01ed9f4c-608e-4e2c-8edf-05d26adcd4d6";
pub(super) const THREAD_ID: &str = "01a1396b-2aa4-7d93-9e18-d54685ff45df";

/// `<tmp>/.loom/work` as the work root, transcripts under `<tmp>/claude`,
/// rollouts under `<tmp>/codex-sessions`.
pub(super) struct Fixture {
    pub(super) tmp: TempDir,
    pub(super) work_root: PathBuf,
}

pub(super) fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

pub(super) fn unix_now() -> i64 {
    Utc::now().timestamp()
}

fn iso(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .expect("timestamp in range")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// An assistant transcript line in the captured shape.
pub(super) fn claude_line(msg_id: &str, ts: i64, model: &str, u: &TokenUsage) -> String {
    json!({
        "type": "assistant", "timestamp": iso(ts), "isSidechain": false,
        "sessionId": SESSION_UUID,
        "message": {"id": msg_id, "model": model, "usage": {
            "input_tokens": u.input, "cache_creation_input_tokens": u.cache_write,
            "cache_read_input_tokens": u.cache_read,
            "cache_creation": {"ephemeral_5m_input_tokens": u.cache_write,
                "ephemeral_1h_input_tokens": 0},
            "output_tokens": u.output, "service_tier": "standard",
            "inference_geo": "not_available"}}
    })
    .to_string()
}

/// A rollout `token_count` line with `total_token_usage`
/// `[input, cached_input, output]`.
pub(super) fn codex_count(ts: i64, [input, cached, output]: [u64; 3]) -> String {
    let total = json!({"input_tokens": input, "cached_input_tokens": cached,
        "cache_write_input_tokens": 0, "output_tokens": output,
        "reasoning_output_tokens": 0, "total_tokens": input + output});
    json!({"timestamp": iso(ts), "type": "event_msg", "payload": {"type": "token_count",
        "info": {"total_token_usage": total, "last_token_usage": total,
            "model_context_window": 475000}}})
    .to_string()
}

/// Lines each followed by a newline.
pub(super) fn body(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

pub(super) fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open for append");
    file.write_all(text.as_bytes()).expect("append");
}

impl Fixture {
    pub(super) fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let work_root = tmp.path().join(".loom").join("work");
        fs::create_dir_all(&work_root).expect("work root");
        fs::create_dir_all(tmp.path().join("claude")).expect("claude dir");
        fs::create_dir_all(tmp.path().join("codex-sessions")).expect("codex dir");
        Self { tmp, work_root }
    }

    pub(super) fn transcript(&self, session_uuid: &str) -> PathBuf {
        self.tmp
            .path()
            .join("claude")
            .join(format!("{session_uuid}.jsonl"))
    }

    /// Writes a transcript and a session record of `stage_id` naming it.
    pub(super) fn add_session(&self, stage_id: &str, session_uuid: &str, text: &str) -> PathBuf {
        let transcript = self.transcript(session_uuid);
        fs::write(&transcript, text).expect("write transcript");
        let mut session = Session::new();
        session.stage_id = Some(stage_id.to_string());
        session.transcript_path = Some(transcript.to_string_lossy().into_owned());
        save_session(&session, &self.work_root).expect("save session");
        transcript
    }

    pub(super) fn add_subagent(&self, session_uuid: &str, file_name: &str, text: &str) -> PathBuf {
        let dir = self
            .tmp
            .path()
            .join("claude")
            .join(session_uuid)
            .join("subagents");
        fs::create_dir_all(&dir).expect("subagents dir");
        let path = dir.join(file_name);
        fs::write(&path, text).expect("subagent file");
        path
    }

    pub(super) fn codex_sessions(&self) -> PathBuf {
        self.tmp.path().join("codex-sessions")
    }

    /// `<work_root>/tokens`, mode 0700.
    pub(super) fn tokens_dir(&self) -> PathBuf {
        let dir = self.work_root.join("tokens");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .expect("tokens dir");
        dir
    }

    pub(super) fn try_scan(&self, now: i64) -> anyhow::Result<TokenFlow> {
        scan_pass(&self.work_root, Some(&self.codex_sessions()), now)
    }

    pub(super) fn scan(&self, now: i64) -> TokenFlow {
        self.try_scan(now).expect("scan_pass")
    }
}

pub(super) fn stage<'a>(flow: &'a TokenFlow, stage_id: &str) -> &'a StageFlow {
    flow.stages
        .iter()
        .find(|stage| stage.stage_id == stage_id)
        .unwrap_or_else(|| panic!("stage {stage_id} missing from {flow:?}"))
}

pub(super) fn agents_of(stage: &StageFlow, kind: AgentKind) -> Vec<&AgentFlow> {
    stage
        .agents
        .iter()
        .filter(|agent| agent.kind == kind)
        .collect()
}
