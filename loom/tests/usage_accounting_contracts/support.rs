//! Fixture builders for the usage-accounting contracts: Claude transcripts,
//! Codex rollouts, forward receipts and session records, all synthetic.

use chrono::{DateTime, Utc};
use loom::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

use crate::helpers;

pub const DAY: &str = "2026-10-14";
pub const UNTIL: &str = "2026-10-15T00:00:00Z";

/// A scratch root holding HOME/LOOM_HOME config, the Claude root `C`, the
/// Codex root `X` and project directories.
pub struct Scratch {
    pub dir: TempDir,
}

impl Scratch {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create scratch root");
        fs::write(dir.path().join("config.toml"), "[update]\ncheck = false\n").unwrap();
        Self { dir }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// A project directory holding `.loom/work/`.
    pub fn project(&self, name: &str) -> PathBuf {
        let project = self.path(name);
        fs::create_dir_all(project.join(".loom/work")).unwrap();
        project
    }

    /// Runs `loom usage --json <args>` and returns the decoded stdout.
    pub fn usage(&self, args: &[&str]) -> Value {
        let output = self.run_usage(args);
        assert!(
            output.status.success(),
            "loom usage {args:?} failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "usage stdout must be JSON: {error}; stderr={}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }

    fn run_usage(&self, args: &[&str]) -> Output {
        let mut command = helpers::loom_cmd();
        helpers::clear_relay_env(&mut command);
        command.arg("usage").args(args).arg("--json");
        command
            .env("HOME", self.dir.path())
            .env("LOOM_HOME", self.dir.path())
            .current_dir(self.dir.path());
        command.output().expect("execute loom usage")
    }
}

/// The Claude project slug: the canonical path with '/' and '.' as '-'.
pub fn slug(project: &Path) -> String {
    fs::canonicalize(project)
        .unwrap()
        .to_string_lossy()
        .replace(['/', '.'], "-")
}

pub fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

/// Writes `lines`, each followed by a newline, creating parent directories.
pub fn write_lines(path: &Path, lines: &[String]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    fs::write(path, body).unwrap();
}

pub fn usage_json(input: u64, output: u64) -> Value {
    json!({
        "input_tokens": input,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
        "output_tokens": output
    })
}

/// A main-session assistant row with usage and optional content.
pub fn assistant(session: &str, id: &str, ts: &str, output: u64, content: Value) -> String {
    json!({
        "type": "assistant",
        "timestamp": ts,
        "sessionId": session,
        "message": {
            "id": id,
            "model": "claude-opus-5-5",
            "content": content,
            "usage": usage_json(10, output)
        }
    })
    .to_string()
}

/// A subagent assistant row with usage and optional content.
pub fn subagent(
    session: &str,
    agent: &str,
    id: &str,
    ts: &str,
    output: u64,
    content: Value,
) -> String {
    json!({
        "type": "assistant",
        "timestamp": ts,
        "isSidechain": true,
        "agentId": agent,
        "sessionId": session,
        "message": {
            "id": id,
            "model": "claude-sonnet-5-5",
            "content": content,
            "usage": usage_json(10, output)
        }
    })
    .to_string()
}

pub fn bash_use(id: &str, command: &str) -> Value {
    json!([{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": command}}])
}

pub fn tool_result(session: &str, ts: &str, id: &str, text: &str) -> String {
    json!({
        "type": "user",
        "timestamp": ts,
        "sessionId": session,
        "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": text}]}
    })
    .to_string()
}

/// `<P>/.loom/work/sessions/<id>.md` naming `transcript`.
pub fn write_session_record(project: &Path, id: &str, stage: &str, transcript: &Path) {
    let dir = project.join(".loom/work/sessions");
    fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "---\nid: {id}\nstage_id: {stage}\nstatus: completed\ncontext_tokens: 0\ntranscript_path: {}\ncreated_at: \"2026-10-14T06:00:00Z\"\nlast_active: \"2026-10-14T07:00:00Z\"\nsession_type: stage\n---\n\n# Session: {id}\n",
        transcript.display()
    );
    fs::write(dir.join(format!("{id}.md")), body).unwrap();
}

/// A rollout under `<X>/sessions/2026/10/14/` with one token_usage_record.
pub fn write_rollout(codex: &Path, thread: &str, ts: &str, output: u64) {
    let meta = json!({
        "timestamp": ts,
        "type": "session_meta",
        "payload": {"id": thread, "cwd": format!("/clones/{thread}")}
    });
    let record = json!({
        "timestamp": ts,
        "type": "token_usage_record",
        "payload": {
            "response_id": format!("response-{thread}"),
            "model": "gpt-5.6-terra",
            "usage": {
                "input_tokens": 100,
                "cached_input_tokens": 40,
                "output_tokens": output,
                "reasoning_output_tokens": 0,
                "total_tokens": 100 + output
            }
        }
    });
    let path = codex.join(format!("sessions/2026/10/14/rollout-{thread}.jsonl"));
    write_lines(&path, &[meta.to_string(), record.to_string()]);
}

/// The three-rollout store of the Codex scope contracts; outputs total 41.
pub fn three_rollout_store(codex: &Path) {
    write_rollout(codex, "thread-a", "2026-10-14T10:00:00Z", 11);
    write_rollout(codex, "thread-b", "2026-10-14T10:00:30Z", 13);
    write_rollout(codex, "thread-x", "2026-10-14T10:01:00Z", 17);
}

fn observation(
    identity: &ForwardIdentity,
    thread: &str,
    state: ForwardState,
    exit_code: Option<i32>,
    observed_at: &str,
) -> ForwardObservation {
    ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id.clone(),
        agent_id: identity.agent_id.clone(),
        tool_use_id: identity.tool_use_id.clone(),
        stage_id: identity.stage_id.clone(),
        loom_session_id: identity.loom_session_id.clone(),
        backend: ForwardBackend::Direct,
        backend_id: thread.to_string(),
        state,
        observed_at: observed_at.parse::<DateTime<Utc>>().unwrap(),
        exit_code,
        codex_thread_id: Some(thread.to_string()),
        locator: None,
        model: Some("gpt-5.6-terra".to_string()),
        effort: Some("xhigh".to_string()),
    }
}

/// Appends a Running then Succeeded receipt naming each thread to
/// `<P>/.loom/work/subagents/stage-a/forward-receipts.jsonl`; returns its path.
pub fn write_receipts(project: &Path, threads: &[&str]) -> PathBuf {
    let path = receipts_path(&project.join(".loom/work"), "stage-a").unwrap();
    let mut lines = Vec::new();
    for (index, thread) in threads.iter().enumerate() {
        let identity = ForwardIdentity::new(
            "parent-1",
            format!("fwd-{index}"),
            format!("toolu_{index}"),
            "stage-a",
            "session-a",
        )
        .unwrap();
        for (state, exit, at) in [
            (ForwardState::Running, None, "2026-10-14T10:02:00.000Z"),
            (ForwardState::Succeeded, Some(0), "2026-10-14T10:05:00.000Z"),
        ] {
            let line = observation(&identity, thread, state, exit, at);
            lines.push(line.encode_line().unwrap().trim_end().to_string());
        }
    }
    write_lines(&path, &lines);
    path
}

/// The provider entry of `.provider_ledger.providers[]` named `provider`.
pub fn provider<'a>(report: &'a Value, provider: &str) -> &'a Value {
    report["provider_ledger"]["providers"]
        .as_array()
        .unwrap_or_else(|| panic!("no provider_ledger.providers in {report}"))
        .iter()
        .find(|entry| entry["provider"] == provider)
        .unwrap_or_else(|| panic!("no {provider} provider in {report}"))
}

pub fn output_total(entry: &Value) -> Option<u64> {
    entry["totals"]["output"]["value"].as_u64()
}

pub fn state(entry: &Value) -> &str {
    entry["completeness"]["state"]
        .as_str()
        .unwrap_or("<absent>")
}

pub fn has_reason(entry: &Value, reason: &str) -> bool {
    entry["completeness"]["reasons"]
        .as_array()
        .is_some_and(|reasons| reasons.iter().any(|value| value == reason))
}

pub fn diagnostic(entry: &Value, name: &str) -> Option<u64> {
    entry["diagnostics"][name].as_u64()
}
