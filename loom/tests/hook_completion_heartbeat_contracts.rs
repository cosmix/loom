//! Contract for stage hooks-and-delivery (CR-67): when
//! `loom-hooks/loom-control-complete.sh` admits the pinned `loom stage complete`
//! command, only the stage's main agent marks its heartbeat with the completion
//! activity. A positive subagent (payload `agent_type`, live
//! `LOOM_MAIN_AGENT_PID` ancestor) leaves no such mark.
//!
//! The fixture mirrors `loom-hooks/tests/loom-control-complete-heartbeat.sh`
//! (its setup and `run_pre`). Requires `bash` and `jq` in /usr/bin or /bin.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

/// `COMPLETION_ACTIVITY` in `loom/src/orchestrator/monitor/stall_line.rs`.
const ACTIVITY: &str = "Stage completion running";

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
}

struct HookRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonicalize temp dir");
        let work = root.join("repo/.loom/work");
        for sub in [root.join("bin"), root.join("repo/.worktrees/build-api")] {
            fs::create_dir_all(sub).expect("create fixture dir");
        }
        fs::create_dir_all(work.join("stages")).expect("create stages dir");
        fs::create_dir_all(work.join("heartbeat")).expect("create heartbeat dir");
        fs::write(
            work.join("stages/01-build-api.md"),
            "---\nid: build-api\nsession: session-123\n---\n# Stage\n",
        )
        .expect("write stage file");
        let stub = root.join("bin/loom");
        fs::write(
            &stub,
            "#!/usr/bin/env bash\nprintf \"LOOM_CONTROL_OUTCOME accepted\\n\"\n",
        )
        .expect("write loom stub");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod loom stub");
        Self { _dir: dir, root }
    }

    fn stub(&self) -> PathBuf {
        self.root.join("bin/loom")
    }

    fn heartbeat(&self) -> PathBuf {
        self.root.join("repo/.loom/work/heartbeat/build-api.json")
    }

    /// Runs the PreToolUse branch on the pinned command. `agent_type` goes into
    /// the payload and `main_pid` into `LOOM_MAIN_AGENT_PID` when given.
    fn run_pinned(&self, agent_type: Option<&str>, main_pid: Option<u32>) -> HookRun {
        let hook =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/loom-control-complete.sh");
        let command = format!("{} stage complete build-api", self.stub().display());
        let mut payload = json!({"tool_name": "Bash", "tool_input": {"command": command}});
        if let Some(kind) = agent_type {
            payload["agent_type"] = Value::from(kind);
        }
        let mut cmd = Command::new("bash");
        cmd.arg(&hook)
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("HOME", &self.root)
            .env("LOOM_CONTROL_TESTING", "1")
            .env("LOOM_CONTROL_TEST_BIN", self.stub())
            .env("LOOM_STAGE_ID", "build-api")
            .env("LOOM_SESSION_ID", "session-123")
            .env(
                "LOOM_WORKTREE_PATH",
                self.root.join("repo/.worktrees/build-api"),
            )
            .env("LOOM_WORK_DIR", self.root.join("repo/.loom/work"));
        if let Some(pid) = main_pid {
            cmd.env("LOOM_MAIN_AGENT_PID", pid.to_string());
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn loom-control-complete.sh");
        let mut stdin = child.stdin.take().expect("hook stdin");
        stdin
            .write_all(payload.to_string().as_bytes())
            .expect("write payload");
        drop(stdin);
        let out = child.wait_with_output().expect("wait for hook");
        HookRun {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn heartbeat_marks_completion(&self) -> bool {
        fs::read_to_string(self.heartbeat()).is_ok_and(|text| text.contains(ACTIVITY))
    }
}

#[test]
fn non_main_caller_writes_no_completion_heartbeat() {
    let subject = Fixture::new();
    let run = subject.run_pinned(Some("loom-software-engineer"), Some(std::process::id()));
    assert_eq!(run.code, Some(0), "subagent run: stderr {}", run.stderr);
    assert!(
        !run.stdout.contains("permissionDecision"),
        "the pinned command must be admitted without a decision: {}",
        run.stdout
    );
    assert!(
        !subject.heartbeat_marks_completion(),
        "a subagent's pinned completion call wrote the main agent's completion heartbeat"
    );

    let control = Fixture::new();
    let run = control.run_pinned(None, None);
    assert_eq!(run.code, Some(0), "control run: stderr {}", run.stderr);
    assert!(
        control.heartbeat_marks_completion(),
        "the main agent's pinned completion call must still mark its heartbeat"
    );
}
