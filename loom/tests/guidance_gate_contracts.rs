//! Contract for stage guidance-and-gate-output: `loom-hooks/post-tool-use.sh`
//! warns a stage's main session once, on stdout as PostToolUse JSON, when its
//! own Edit/Write calls inside the worktree pass the small-change limit, and
//! never again in that session, even after the ledger trims its warned row.
//! Subagent edits and files outside the worktree never count.
//!
//! Requires `bash`, `rg` and `jq` on PATH.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

/// Rows past `_LOOM_LEDGER_MAX_ROWS` (300) in `loom-hooks/_read_ledger.sh`.
const EDITS_PAST_LEDGER_CAP: usize = 305;

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    wt: PathBuf,
}

struct HookRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("gg")
            .tempdir()
            .expect("create temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonicalize temp dir");
        for sub in ["bin", "home", "work", "wt/src", "scratch"] {
            fs::create_dir_all(root.join(sub)).expect("create fixture dir");
        }
        let stub = root.join("bin/loom");
        fs::write(&stub, "#!/bin/sh\nexit 0\n").expect("write loom stub");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod loom stub");
        let wt = root.join("wt");
        Self {
            _dir: dir,
            root,
            wt,
        }
    }

    fn run(&self, session: &str, payload: &Value) -> HookRun {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/post-tool-use.sh");
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new("bash")
            .arg(&hook)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("LOOM_STAGE_ID", "advisory-stage")
            .env("LOOM_SESSION_ID", session)
            .env("LOOM_WORK_DIR", self.root.join("work"))
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_WORKTREE_PATH", &self.wt)
            .env("LOOM_BIN", self.root.join("bin/loom"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn post-tool-use.sh");
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

    fn main_edit(&self, file: &Path, lines: usize) -> Value {
        json!({
            "tool_name": "Edit",
            "session_id": "main-1",
            "transcript_path": self.root.join("main-1.jsonl"),
            "tool_input": {"file_path": file, "old_string": "x", "new_string": text(lines)},
        })
    }

    fn main_write(&self, file: &Path, lines: usize) -> Value {
        json!({
            "tool_name": "Write",
            "session_id": "main-1",
            "transcript_path": self.root.join("main-1.jsonl"),
            "tool_input": {"file_path": file, "content": text(lines)},
        })
    }

    fn sub_edit(&self, file: &Path, lines: usize) -> Value {
        let mut payload = self.main_edit(file, lines);
        payload["agent_id"] = json!("a1");
        payload["agent_type"] = json!("loom-software-engineer");
        payload["transcript_path"] = json!(self.root.join("main-1/subagents/agent-a1.jsonl"));
        payload
    }

    fn src(&self, name: &str) -> PathBuf {
        self.wt.join("src").join(name)
    }
}

fn text(lines: usize) -> String {
    (1..=lines)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_silent(run: &HookRun, what: &str) {
    assert_eq!(
        run.code,
        Some(0),
        "{what}: exit code; stderr: {}",
        run.stderr
    );
    assert!(
        run.stdout.trim().is_empty(),
        "{what}: the size advisory must stay silent here (it fires once per session, \
         never for subagents or files outside the worktree); stdout: {}",
        run.stdout
    );
}

/// Asserts the call printed the advisory as PostToolUse JSON on stdout and
/// returns its additionalContext.
fn advisory(run: &HookRun, what: &str) -> String {
    assert_eq!(
        run.code,
        Some(0),
        "{what}: exit code; stderr: {}",
        run.stderr
    );
    let parsed: Value = serde_json::from_str(run.stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "{what}: the size advisory must print PostToolUse JSON on stdout ({e}); \
             stdout: {:?}; stderr (never shown to the model on exit 0): {:?}",
            run.stdout, run.stderr
        )
    });
    let output = &parsed["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PostToolUse", "{what}: {parsed}");
    let context = output["additionalContext"]
        .as_str()
        .unwrap_or_else(|| panic!("{what}: no additionalContext string in {parsed}"))
        .to_owned();
    assert!(context.contains("LOOM_HOOK_WARN"), "{what}: {context}");
    context
}

/// s-main: two 3-line edits stay under the limit, a third file trips it, and
/// every later edit, including those past the ledger's row cap, stays silent.
fn scenario_main(fx: &Fixture) {
    let s = "s-main";
    assert_silent(&fx.run(s, &fx.main_edit(&fx.src("a.rs"), 3)), "s-main a.rs");
    assert_silent(&fx.run(s, &fx.main_edit(&fx.src("b.rs"), 3)), "s-main b.rs");
    let context = advisory(&fx.run(s, &fx.main_edit(&fx.src("c.rs"), 1)), "s-main c.rs");
    assert!(
        context.contains("7 lines across 3 file(s)"),
        "s-main c.rs: the advisory must count 7 edited lines in 3 files: {context}"
    );
    assert_silent(&fx.run(s, &fx.main_edit(&fx.src("d.rs"), 1)), "s-main d.rs");
    repeated_edits_stay_silent(fx, s);
}

fn repeated_edits_stay_silent(fx: &Fixture, session: &str) {
    let payload = fx.main_edit(&fx.src("e.rs"), 1);
    for i in 1..=EDITS_PAST_LEDGER_CAP {
        let run = fx.run(session, &payload);
        assert_silent(&run, &format!("{session} e.rs edit {i} after the warning"));
    }
}

#[test]
fn main_session_edit_advisory_fires_once_past_the_limit() {
    let fx = Fixture::new();
    scenario_main(&fx);

    let context = advisory(
        &fx.run("s-lines", &fx.main_write(&fx.src("big.rs"), 25)),
        "s-lines big.rs",
    );
    assert!(
        context.contains("25 lines across 1 file(s)"),
        "s-lines: a 25-line Write must count 25 lines in 1 file: {context}"
    );

    for name in ["s1.rs", "s2.rs", "s3.rs"] {
        let run = fx.run("s-sub", &fx.sub_edit(&fx.src(name), 3));
        assert_silent(&run, &format!("s-sub {name} (subagent edit)"));
    }

    let outside = fx.root.join("scratch/brief.md");
    let run = fx.run("s-out", &fx.main_write(&outside, 100));
    assert_silent(&run, "s-out (Write outside the worktree)");
}
