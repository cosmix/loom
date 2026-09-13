//! Integration tests for the read-guard PreToolUse hook and receipt handoff.

use super::helpers::{clear_relay_env, loom_bin_path};
use loom::fs::permissions::constants::{
    HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_GUARD, HOOK_READ_LEDGER,
};
use loom::process::sandbox_probe::{process_tree_visible, skip_unless};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

const STUB_STDERR_WARNING: &str = "could not refresh the working-tree source graph";

struct HookOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).expect("write file");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn setup_hook() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("create temp dir");
    write_exec(&temp.path().join("_common.sh"), HOOK_COMMON);
    write_exec(
        &temp.path().join("_read_discipline.sh"),
        HOOK_READ_DISCIPLINE,
    );
    write_exec(&temp.path().join("_read_ledger.sh"), HOOK_READ_LEDGER);
    let hook_path = temp.path().join("read-guard.sh");
    write_exec(&hook_path, HOOK_READ_GUARD);
    (temp, hook_path)
}

fn write_file_with_lines(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "line\n".repeat(lines)).expect("write test file");
    path
}

fn write_loom_stub(dir: &Path, body: &str) {
    let script = format!(
        "#!/usr/bin/env bash\nif [[ \"$1\" == \"hook\" && \"$2\" == \"read-receipt\" ]]; then\n  if [[ -n \"${{LOOM_RECEIPT_CALLS:-}}\" ]]; then\n    printf '%s\\n' \"$*\" >>\"$LOOM_RECEIPT_CALLS\"\n    exit 0\n  fi\n  exec \"{}\" \"$@\"\nfi\ncat <<'EOF'\n{body}\nEOF\necho 'warning: {STUB_STDERR_WARNING} (stub)' >&2\n",
        loom_bin_path()
    );
    write_exec(&dir.join("loom"), &script);
}

fn covered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-covered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\n\tL14-L14\tconstant\tPOLL_INTERVAL\tconst POLL_INTERVAL: ...\n\tL40-L61\tfunction\tgather\tfn gather(\ncoverage: full",
    );
    dir
}

fn uncovered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-uncovered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\ncoverage: lexical-only - no source-graph extractor for sh",
    );
    dir
}

fn receipt_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-receipts");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(&dir, "coverage: lexical-only");
    dir
}

struct Session {
    work: TempDir,
    session_id: &'static str,
    stage_id: Option<&'static str>,
    agent_id: &'static str,
    main_agent_pid: Option<String>,
    receipt_call_log: Option<PathBuf>,
}

impl Session {
    fn new() -> Self {
        Session {
            work: TempDir::new().expect("create work dir"),
            session_id: "sess-1",
            stage_id: Some("stage-1"),
            agent_id: "agent-1",
            main_agent_pid: None,
            receipt_call_log: None,
        }
    }

    fn work_dir(&self) -> &Path {
        self.work.path()
    }

    fn enable_deny(&self) {
        fs::write(
            self.work_dir().join("config.toml"),
            "[hooks]\ndeny_enabled = true\n",
        )
        .expect("write config.toml");
    }

    fn with_live_main_agent(mut self) -> Self {
        self.main_agent_pid = Some(std::process::id().to_string());
        self
    }

    fn with_receipt_call_log(mut self, path: PathBuf) -> Self {
        self.receipt_call_log = Some(path);
        self
    }

    fn with_stage_id(mut self, stage_id: Option<&'static str>) -> Self {
        self.stage_id = stage_id;
        self
    }
}

fn configure_command_environment(
    command: &mut Command,
    session: &Session,
    stub_dir: Option<&Path>,
) {
    let path_value = match stub_dir {
        Some(dir) => format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
        None => std::env::var("PATH").unwrap_or_default(),
    };
    clear_relay_env(command)
        .env("PATH", path_value)
        .env("LOOM_WORK_DIR", session.work_dir())
        .env("LOOM_SESSION_ID", session.session_id)
        .env("LOOM_HOME", session.work_dir().join("loom-home"))
        .env("TMPDIR", session.work_dir());
    if let Some(stage_id) = session.stage_id {
        command.env("LOOM_STAGE_ID", stage_id);
    } else {
        command.env_remove("LOOM_STAGE_ID");
    }
}

fn run_payload(
    hook: &Path,
    payload: &Value,
    session: &Session,
    stub_dir: Option<&Path>,
) -> HookOutput {
    let payload_str = payload.to_string();
    let mut cmd = Command::new("bash");
    cmd.arg(hook);
    configure_command_environment(&mut cmd, session, stub_dir);
    cmd.env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(pid) = &session.main_agent_pid {
        cmd.env("LOOM_MAIN_AGENT_PID", pid);
    }
    if let Some(call_log) = &session.receipt_call_log {
        cmd.env("LOOM_RECEIPT_CALLS", call_log);
    }

    let mut child = cmd.spawn().expect("spawn hook");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload_str.as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for hook");
    HookOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_read_hook(
    hook: &Path,
    tool_input: Value,
    session: &Session,
    stub_dir: Option<&Path>,
) -> HookOutput {
    let payload = json!({
        "tool_name": "Read",
        "tool_input": tool_input,
        "agent_id": session.agent_id,
        "session_id": session.session_id,
        "cwd": session.work_dir(),
    });
    run_payload(hook, &payload, session, stub_dir)
}

fn skip_unless_gate_visible(test: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        &format!("hooks_read_guard::{test}"),
        "the guard's deny branch needs a visible process tree",
    )
}

fn warn_context(stdout: &str) -> String {
    let v: Value = serde_json::from_str(stdout.trim()).expect("parse stdout json");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext present")
        .to_string()
}

#[test]
fn unbounded_read_of_large_covered_file_denies_with_outline() {
    if skip_unless_gate_visible("unbounded_read_of_large_covered_file_denies_with_outline") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);

    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let tool_input = json!({"file_path": file.to_string_lossy()});
    let out = run_read_hook(&hook, tool_input, &session, Some(&stub_dir));

    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("500"), "stderr={}", out.stderr);
    assert!(
        out.stderr.contains("POLL_INTERVAL") && out.stderr.contains("gather"),
        "stderr={}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Read the ranges you need with offset/limit"),
        "stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains(STUB_STDERR_WARNING),
        "stub stderr leaked: {}",
        out.stderr
    );
}

#[test]
fn bounded_read_of_large_file_is_allowed() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);

    let session = Session::new();
    session.enable_deny();
    let tool_input = json!({"file_path": file.to_string_lossy(), "offset": 0, "limit": 100});
    let out = run_read_hook(&hook, tool_input, &session, Some(&stub_dir));

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

#[test]
fn line_limit_boundary_is_strictly_greater_than() {
    if skip_unless_gate_visible("line_limit_boundary_is_strictly_greater_than") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    let at_limit = write_file_with_lines(files.path(), "exactly400.rs", 400);
    let out = run_read_hook(
        &hook,
        json!({"file_path": at_limit.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);

    let over_limit = write_file_with_lines(files.path(), "over401.rs", 401);
    let out = run_read_hook(
        &hook,
        json!({"file_path": over_limit.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 2, "stderr={}", out.stderr);
}

#[test]
fn uncovered_file_warns_and_allows_even_with_deny_on() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = uncovered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.sh", 500);

    let session = Session::new();
    session.enable_deny();
    let out = run_read_hook(
        &hook,
        json!({"file_path": file.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr={}", out.stderr);
    let ctx = warn_context(&out.stdout);
    assert!(
        ctx.contains("rg -n") && ctx.contains("offset/limit"),
        "ctx={ctx}"
    );
}

#[test]
fn binary_extension_skips_repeat_rule_too() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let calls = stubs.path().join("receipt-calls");
    let file = write_file_with_lines(files.path(), "icon.png", 50);
    let session = Session::new()
        .with_live_main_agent()
        .with_receipt_call_log(calls.clone());
    session.enable_deny();

    for n in 1..=3 {
        let out = run_read_hook(
            &hook,
            json!({"file_path": file.to_string_lossy()}),
            &session,
            Some(&receipt_stub_dir(stubs.path())),
        );
        assert_eq!(out.code, 0, "read {n}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "read {n}: stdout={}",
            out.stdout
        );
    }
    assert!(!calls.exists(), "media must not call the receipt adapter");
}

#[test]
fn pdf_pages_bounded_reads_are_never_denied() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let file = write_file_with_lines(files.path(), "spec.pdf", 50);
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    for pages in ["1-20", "21-40", "41-60"] {
        let out = run_read_hook(
            &hook,
            json!({"file_path": file.to_string_lossy(), "pages": pages}),
            &session,
            None,
        );
        assert_eq!(out.code, 0, "pages={pages}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "pages={pages}: stdout={}",
            out.stdout
        );
    }
}

#[path = "hooks_read_guard_repeat.rs"]
mod repeat;

#[path = "hooks_read_guard_receipts.rs"]
mod receipts;
