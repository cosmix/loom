//! Session directory, spool event, state machine and hook helpers shared by the bridge-core
//! contracts.
//!
//! Every tree lives in a `tempfile::Builder::new().prefix("loom-plan-bridge-")` directory under the
//! system temp dir; a session directory is `<base>/sessions/s1` with its `answers/` directory.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use loom::plan_session::guard::GuardPolicy;
use loom::plan_session::hook_cli;
use loom::plan_session::kind::SessionKind;
use loom::plan_session::model::SessionState;
use loom::plan_session::spool::SpoolEvent;
use loom::plan_session::state::{Input, StateMachine};
use serde_json::{json, Value};
use tempfile::TempDir;

/// The Claude Code session id every fake payload and record carries.
pub const SID: &str = "105e2c4a-7f1b-4c3d-9a8e-2b6d0f1e3a57";

/// A canonical temp directory.
pub fn temp() -> (TempDir, PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix("loom-plan-bridge-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let base = tmp.path().canonicalize().unwrap();
    (tmp, base)
}

/// One session directory with `answers/`, under a temp base that also holds the project root.
pub struct Session {
    _tmp: TempDir,
    pub base: PathBuf,
    pub root: PathBuf,
    pub dir: PathBuf,
}

impl Session {
    pub fn new() -> Self {
        let (tmp, base) = temp();
        let root = base.join("proj");
        let dir = base.join("runtime/sessions/s1");
        fs::create_dir_all(root.join("doc")).unwrap();
        fs::create_dir_all(dir.join("answers")).unwrap();
        fs::create_dir_all(base.join("home")).unwrap();
        Self {
            _tmp: tmp,
            base,
            root,
            dir,
        }
    }

    /// Write `guard.json` (plan 6's policy, schema 1) naming this session directory.
    pub fn write_guard(&self, kind: SessionKind) {
        let policy = GuardPolicy {
            schema: 1,
            kind,
            project_root: self.root.clone(),
            runtime_dir: self.base.join("runtime"),
            session_dir: self.dir.clone(),
            home: self.base.join("home"),
        };
        fs::write(
            self.dir.join("guard.json"),
            serde_json::to_vec(&policy).unwrap(),
        )
        .unwrap();
    }

    pub fn answers(&self) -> PathBuf {
        self.dir.join("answers")
    }

    pub fn events(&self) -> PathBuf {
        self.dir.join("events.jsonl")
    }

    /// The raw spool lines, each parsed as JSON (empty when the file does not exist).
    pub fn spool_lines(&self) -> Vec<Value> {
        let text = fs::read_to_string(self.events()).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Run `loom plan hook <event>` on a thread; the receiver yields its exit code and stdout.
    pub fn spawn_hook(&self, event: &'static str, payload: &Value) -> Receiver<(i32, Vec<u8>)> {
        let (tx, rx) = mpsc::channel();
        let dir = self.dir.clone();
        let payload = serde_json::to_vec(payload).unwrap();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let code = hook_cli::run(event, &dir, payload.as_slice(), &mut out);
            let _ = tx.send((code, out));
        });
        rx
    }

    /// Wait up to 5 s for the spool to hold a line.
    pub fn wait_for_spool_line(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let whole_line = || {
            let text = fs::read_to_string(self.events()).unwrap_or_default();
            text.contains('\n')
        };
        while !whole_line() {
            assert!(Instant::now() < deadline, "no spool line within 5 s");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Every path under `base`, for "nothing was created anywhere" checks.
pub fn tree(base: &Path) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            paths.insert(path);
        }
    }
    paths
}

/// A hook payload as Claude Code sends it on stdin.
pub fn hook_payload(event: &str, extra: Value) -> Value {
    let mut payload = json!({
        "session_id": SID,
        "transcript_path": "/home/u/.claude/projects/p/105e.jsonl",
        "cwd": "/home/u/proj",
        "hook_event_name": event,
    });
    for (key, value) in extra.as_object().unwrap() {
        payload[key.as_str()] = value.clone();
    }
    payload
}

/// The Bash permission request payload.
pub fn bash_permission_payload() -> Value {
    hook_payload(
        "PermissionRequest",
        json!({
            "permission_mode": "auto",
            "tool_name": "Bash",
            "tool_input": {"command": "rm -rf build", "description": "Remove the build directory"},
        }),
    )
}

/// The parsed one-line hook answer; asserts it is a PermissionRequest deny and returns its message.
pub fn deny_message(out: &[u8]) -> String {
    let answer: Value = serde_json::from_slice(out).expect("the hook prints one JSON answer");
    let specific = &answer["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PermissionRequest", "{answer}");
    let decision = specific["decision"].as_object().unwrap();
    let keys: Vec<&str> = decision.keys().map(String::as_str).collect();
    assert_eq!(keys, ["behavior", "message"], "{answer}");
    assert_eq!(decision["behavior"], "deny", "{answer}");
    decision["message"].as_str().unwrap().to_string()
}

/// `2026-10-27T10:00:00Z` plus `millis`.
pub fn at(millis: i64) -> DateTime<Utc> {
    let start: DateTime<Utc> = "2026-10-27T10:00:00Z".parse().unwrap();
    start + chrono::Duration::milliseconds(millis)
}

/// A spool event received at `at(millis)`; `request_id` only for a PermissionRequest.
pub fn event(name: &str, millis: i64, extra: Value) -> SpoolEvent {
    SpoolEvent {
        received_at: at(millis),
        event: name.to_string(),
        session_id: Some(SID.to_string()),
        transcript_path: Some(PathBuf::from("/home/u/.claude/projects/p/105e.jsonl")),
        request_id: None,
        payload: hook_payload(name, extra),
    }
}

pub fn permission_request(id: &str, millis: i64) -> SpoolEvent {
    let mut request = event(
        "PermissionRequest",
        millis,
        json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
    );
    request.request_id = Some(id.to_string());
    request
}

/// Apply one spool event.
pub fn feed(machine: &mut StateMachine, event: SpoolEvent) -> Option<SessionState> {
    machine.apply(Input::Spool(&event))
}

/// A machine that saw `SessionStart` startup at 0 ms: Idle.
pub fn idle() -> StateMachine {
    let mut machine = StateMachine::new();
    feed(
        &mut machine,
        event("SessionStart", 0, json!({"source": "startup"})),
    );
    assert_eq!(machine.state(), SessionState::Idle);
    machine
}

/// An idle machine that then saw `UserPromptSubmit` at 100 ms: Working.
pub fn working() -> StateMachine {
    let mut machine = idle();
    feed(
        &mut machine,
        event("UserPromptSubmit", 100, json!({"prompt": "go"})),
    );
    assert_eq!(machine.state(), SessionState::Working);
    machine
}

/// The main agent's `Stop` received at `at(millis)`.
pub fn stop(millis: i64) -> SpoolEvent {
    event("Stop", millis, json!({"stop_hook_active": false}))
}

pub fn subagent(name: &str, millis: i64) -> SpoolEvent {
    event(
        name,
        millis,
        json!({"agent_id": "a7f3", "agent_type": "Explore"}),
    )
}
