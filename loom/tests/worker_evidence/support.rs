use super::setup::{
    generated_commands, install_hooks, install_shims, prepare_layout, transcript_row, write_stage,
};
use loom::hooks::HookEvent;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use tempfile::{Builder, NamedTempFile, TempDir};

pub const STAGE: &str = "worker-evidence";
pub const LOOM_SESSION: &str = "loom-session-a";
pub const SUCCESSOR_SESSION: &str = "loom-session-b";
pub const PARENT_UUID: &str = "11111111-2222-4333-8444-555555555555";
pub const AGENT_ID: &str = "worker-a";
pub const AGENT_TYPE: &str = "loom-software-engineer";
const START_AT: &str = "2026-09-13T10:00:00.000Z";
const STOP_AT: &str = "2026-09-13T10:00:01.000Z";
const LATER_AT: &str = "2026-09-13T10:00:02.000Z";
pub struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub work: PathBuf,
    pub parent: PathBuf,
    pub worker: PathBuf,
    commands: HashMap<HookEvent, String>,
    bin: PathBuf,
}

impl Fixture {
    pub fn new(label: &str) -> Self {
        let temp = Builder::new()
            .prefix(&format!("worker-evidence-{label}-"))
            .tempdir()
            .expect("create fixture under TMPDIR");
        let root = fs::canonicalize(temp.path()).expect("canonical fixture root");
        let work = root.join(".loom/work");
        let hooks = root.join("hooks");
        let bin = root.join("bin");
        let (parent, worker) = prepare_layout(&root, &work);
        install_hooks(&hooks);
        install_shims(&bin);
        let commands = generated_commands(&hooks, &work);
        let fixture = Self {
            _temp: temp,
            root,
            work,
            parent,
            worker,
            commands,
            bin,
        };
        let output = fixture.run_hook(
            HookEvent::SubagentStart,
            &fixture.start_payload(),
            STAGE,
            LOOM_SESSION,
            START_AT,
        );
        assert_exit(&output, 0);
        fixture.assert_start_identity();
        fixture
    }

    pub fn stop_payload(&self) -> Value {
        json!({
            "hook_event_name": "SubagentStop",
            "session_id": PARENT_UUID,
            "transcript_path": self.parent,
            "agent_id": AGENT_ID,
            "agent_type": AGENT_TYPE,
            "agent_transcript_path": self.worker,
        })
    }

    pub fn idle_payload(&self) -> Value {
        json!({
            "hook_event_name": "TeammateIdle",
            "session_id": PARENT_UUID,
            "transcript_path": self.parent,
            "team_name": "review-team",
            "teammate_name": AGENT_ID,
        })
    }

    pub fn run_stop(&self, payload: &Value) -> Output {
        self.run_hook(
            HookEvent::SubagentStop,
            payload,
            STAGE,
            LOOM_SESSION,
            STOP_AT,
        )
    }

    pub fn run_stop_as(&self, payload: &Value, stage: &str, session: &str) -> Output {
        self.run_hook(HookEvent::SubagentStop, payload, stage, session, STOP_AT)
    }

    pub fn run_idle(&self, payload: &Value) -> Output {
        self.run_hook(
            HookEvent::TeammateIdle,
            payload,
            STAGE,
            LOOM_SESSION,
            LATER_AT,
        )
    }

    pub fn run_malformed_stop(&self) -> Output {
        self.run_hook_raw(
            HookEvent::SubagentStop,
            "{malformed",
            STAGE,
            LOOM_SESSION,
            STOP_AT,
        )
    }

    pub fn list_json(&self) -> Value {
        let mut command = self.cli_command();
        command
            .args(["subagents", "list", "--dir"])
            .arg(self.worker.parent().expect("worker directory"))
            .args(["--json", "--debounce", "100000"]);
        let output = command.output().expect("run subagents list");
        assert_exit(&output, 0);
        serde_json::from_slice(&output.stdout).expect("parse list JSON")
    }

    pub fn only_summary(&self) -> Value {
        let list = self.list_json();
        let rows = list.as_array().expect("list output array");
        assert_eq!(rows.len(), 1, "one transcript must produce one worker");
        rows[0].clone()
    }

    pub fn watch(&self) -> Output {
        let mut command = self.cli_command();
        command
            .args(["subagents", "watch", "--worker"])
            .arg(format!("claude:{AGENT_ID}"))
            .args(["--timeout", "3", "--json"]);
        command.output().expect("run subagents watch")
    }

    pub fn journal_values(&self) -> Vec<Value> {
        let path = self.journal_path();
        let Ok(content) = fs::read_to_string(&path) else {
            return Vec::new();
        };
        assert!(
            content.ends_with('\n'),
            "journal must be newline terminated"
        );
        content
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid lifecycle JSON line"))
            .collect()
    }

    pub fn assert_no_lifecycle_evidence(&self) {
        let mut lines = 0;
        for entry in fs::read_dir(self.work.join("subagents")).expect("read subagents root") {
            let path = entry
                .expect("read subagents entry")
                .path()
                .join("lifecycle.jsonl");
            if let Ok(content) = fs::read_to_string(path) {
                lines += content.lines().count();
            }
        }
        assert_eq!(lines, 0, "invalid evidence must not reach any journal");
    }

    pub fn append_worker_record(&self, text: &str) {
        let row = transcript_row(text);
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.worker)
            .expect("open worker transcript");
        writeln!(file, "{row}").expect("grow worker transcript");
    }

    pub fn append_journal_value(&self, value: &Value) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.journal_path())
            .expect("open lifecycle journal");
        writeln!(file, "{value}").expect("append lifecycle conflict");
    }

    pub fn rebind_stage(&self, session: &str) {
        write_stage(&self.work, session);
    }

    pub fn heartbeat_path(&self) -> PathBuf {
        self.work.join("heartbeat").join(format!("{STAGE}.json"))
    }

    pub fn seed_heartbeat(&self, bytes: &[u8]) {
        fs::create_dir_all(self.work.join("heartbeat")).expect("create heartbeat directory");
        fs::write(self.heartbeat_path(), bytes).expect("seed heartbeat");
    }

    pub fn create_worker(&self, agent_id: &str) -> PathBuf {
        let path = self
            .worker
            .parent()
            .expect("worker directory")
            .join(format!("agent-{agent_id}.jsonl"));
        fs::write(&path, format!("{}\n", transcript_row("other worker")))
            .expect("write alternate worker");
        path
    }

    pub fn create_other_parent(&self) -> PathBuf {
        let directory = self.root.join("other-claude-project");
        fs::create_dir_all(&directory).expect("create alternate project");
        let path = directory.join(format!("{PARENT_UUID}.jsonl"));
        fs::write(&path, "{\"type\":\"parent\"}\n").expect("write alternate parent");
        path
    }

    fn start_payload(&self) -> Value {
        json!({
            "hook_event_name": "SubagentStart",
            "session_id": PARENT_UUID,
            "transcript_path": self.worker,
            "agent_id": AGENT_ID,
            "agent_type": AGENT_TYPE,
        })
    }

    fn assert_start_identity(&self) {
        let path = self.work.join("subagents").join(STAGE).join("starts.jsonl");
        let content = fs::read_to_string(path).expect("read start ledger");
        let rows: Vec<Value> = content
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid start row"))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["parent_session_id"], PARENT_UUID);
        assert_eq!(rows[0]["loom_session_id"], LOOM_SESSION);
        assert_eq!(rows[0]["ts"], START_AT);
    }

    fn journal_path(&self) -> PathBuf {
        self.work
            .join("subagents")
            .join(STAGE)
            .join("lifecycle.jsonl")
    }

    fn run_hook(
        &self,
        event: HookEvent,
        payload: &Value,
        stage: &str,
        session: &str,
        now: &str,
    ) -> Output {
        self.run_hook_raw(event, &payload.to_string(), stage, session, now)
    }

    fn run_hook_raw(
        &self,
        event: HookEvent,
        payload: &str,
        stage: &str,
        session: &str,
        now: &str,
    ) -> Output {
        let mut input = NamedTempFile::new_in(&self.root).expect("create hook stdin");
        input
            .write_all(payload.as_bytes())
            .expect("write hook stdin");
        input.flush().expect("flush hook stdin");
        let stdin = File::open(input.path()).expect("reopen hook stdin");
        let mut command = Command::new(self.commands.get(&event).expect("generated hook command"));
        command.current_dir(&self.root).stdin(Stdio::from(stdin));
        self.configure(&mut command, stage, session, now);
        command.output().expect("run generated hook command")
    }

    fn cli_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        command.current_dir(&self.root);
        self.configure(&mut command, STAGE, LOOM_SESSION, STOP_AT);
        command
    }

    fn configure(&self, command: &mut Command, stage: &str, session: &str, now: &str) {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        let mut path = OsString::from(self.bin.as_os_str());
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        command
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("FIXTURE_NOW", now)
            .env("LOOM_STAGE_ID", stage)
            .env("LOOM_SESSION_ID", session)
            .env("LOOM_WORK_DIR", &self.work)
            .env("LOOM_WORKTREE_PATH", &self.root);
    }
}

pub fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_watch_rejected(output: &Output) {
    assert!(
        matches!(output.status.code(), Some(1 | 5)),
        "owned watch unexpectedly settled or timed out\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
