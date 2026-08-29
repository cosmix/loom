//! Integration tests for the read-guard PreToolUse:Read hook.
//!
//! read-guard.sh enforces CLAUDE.md's read discipline (rule 14: query before you read, read
//! ranges not files; rule 17: 400-line file ceiling) through the shared core in
//! hooks/_read_discipline.sh - the same rules poll-guard.sh applies to Bash-side cat/head/tail/sed
//! reads, so the two hooks can never drift apart. Every deny branch there is gated by the
//! `[hooks] deny_enabled` switch in `_common.sh`'s `loom_deny_enabled` - OFF by default, in which
//! case a would-be deny is a `LOOM_HOOK_WARN` warning at exit 0 instead. These tests exercise the
//! rules with the switch BOTH ways: a hook that only denies with the switch on must never deny
//! with it off, and a rule that should never deny (tier-1 knowledge, repeated range reads) must
//! stay a warning even with the switch on.
//!
//! Runs the hook script directly with bash - no loom invocation. `loom map --outline` is stubbed
//! on PATH so rule 1's covered/uncovered split never depends on the developer's installed loom
//! binary or its source graph.
//!
//! The repeat-read escalation cases, the non-Read/no-path no-op check, and the reads-ledger
//! row-shape check live in `hooks_read_guard_repeat.rs` (see its module docs) - split out purely
//! for size, sharing this file's harness via `use super::*`.

use loom::fs::permissions::constants::{
    HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_GUARD, HOOK_READ_LEDGER,
};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Text the stub `loom` binary writes to stderr - must never reach a hook
/// message, since `_loom_outline_covered_rows` discards the real command's
/// stderr on purpose.
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

/// Install read-guard.sh plus its three sourced dependencies into a temp dir.
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

/// Write a file with exactly `lines` lines (by `wc -l`) under `dir`.
fn write_file_with_lines(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "line\n".repeat(lines)).expect("write test file");
    path
}

/// Write a fake `loom` executable at `<dir>/loom` whose `map --outline`
/// prints `body` to stdout and a source-graph refresh warning to stderr.
fn write_loom_stub(dir: &Path, body: &str) {
    let script = format!(
        "#!/usr/bin/env bash\ncat <<'EOF'\n{body}\nEOF\necho 'warning: {STUB_STDERR_WARNING} (stub)' >&2\n"
    );
    write_exec(&dir.join("loom"), &script);
}

/// A "covered" `loom map --outline` stub: two symbol rows plus
/// `coverage: full`, so rule 1 treats the file as graph-covered (deny).
fn covered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-covered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\n\tL14-L14\tconstant\tPOLL_INTERVAL\tconst POLL_INTERVAL: ...\n\tL40-L61\tfunction\tgather\tfn gather(\ncoverage: full",
    );
    dir
}

/// An "uncovered" stub: header + `coverage: lexical-only`, no symbol rows -
/// rule 1 treats the file as NOT graph-covered (warn only).
fn uncovered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-uncovered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\ncoverage: lexical-only - no source-graph extractor for sh",
    );
    dir
}

/// One test's isolated loom session: its own `.work`-shaped dir (so every
/// ledger and config.toml lands inside a TempDir, never the developer's real
/// `/tmp` or `~/.claude`), with fixed session/stage/agent ids.
struct Session {
    work: TempDir,
    session_id: &'static str,
    stage_id: &'static str,
    agent_id: &'static str,
    /// `LOOM_MAIN_AGENT_PID` to export, if any - `loom_hook_deny_or_warn`
    /// (`_read_discipline.sh`) only ever denies when this is set AND a live
    /// ancestor of the hook's bash process. `None` (the default) leaves a
    /// would-be deny as a warning, matching an ordinary session with no loom
    /// orchestrator above it.
    main_agent_pid: Option<String>,
}

impl Session {
    fn new() -> Self {
        Session {
            work: TempDir::new().expect("create work dir"),
            session_id: "sess-1",
            stage_id: "stage-1",
            agent_id: "agent-1",
            main_agent_pid: None,
        }
    }

    fn work_dir(&self) -> &Path {
        self.work.path()
    }

    /// Turn on the `[hooks] deny_enabled = true` switch for this session.
    fn enable_deny(&self) {
        fs::write(
            self.work_dir().join("config.toml"),
            "[hooks]\ndeny_enabled = true\n",
        )
        .expect("write config.toml");
    }

    /// Mark this session as running under a LIVE loom main-agent process:
    /// this test binary's own pid, a direct ancestor of the hook's bash
    /// child. Required, together with `enable_deny`, for a deny branch to
    /// actually deny rather than degrade to a warning.
    fn with_live_main_agent(mut self) -> Self {
        self.main_agent_pid = Some(std::process::id().to_string());
        self
    }

    /// Mark this session with an explicit (possibly non-ancestor)
    /// `LOOM_MAIN_AGENT_PID`, e.g. `"1"` - proves the liveness check, not
    /// just presence of the variable, gates a deny.
    fn with_main_agent_pid(mut self, pid: &str) -> Self {
        self.main_agent_pid = Some(pid.to_string());
        self
    }
}

/// Run read-guard.sh against a raw `payload`, with `stub_dir` (if any)
/// placed first on PATH so `loom map --outline` resolves to the fake binary.
fn run_payload(
    hook: &Path,
    payload: &Value,
    session: &Session,
    stub_dir: Option<&Path>,
) -> HookOutput {
    let payload_str = payload.to_string();
    let path_value = match stub_dir {
        Some(dir) => format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
        None => std::env::var("PATH").unwrap_or_default(),
    };

    let mut cmd = Command::new("bash");
    cmd.arg(hook)
        .env("PATH", path_value)
        .env("LOOM_WORK_DIR", session.work_dir())
        .env("LOOM_SESSION_ID", session.session_id)
        .env("LOOM_STAGE_ID", session.stage_id)
        // Never let the developer's own shell env leak `loom_debug` output
        // onto stderr - it would fail assertions for reasons unrelated to
        // the hook's actual decision.
        .env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(pid) = &session.main_agent_pid {
        cmd.env("LOOM_MAIN_AGENT_PID", pid);
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

/// Convenience wrapper over `run_payload` for a `Read` tool call.
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
    });
    run_payload(hook, &payload, session, stub_dir)
}

/// Extract the single `LOOM_HOOK_WARN: ...` additionalContext string from a
/// warn response's stdout JSON.
fn warn_context(stdout: &str) -> String {
    let v: Value = serde_json::from_str(stdout.trim()).expect("parse stdout json");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext present")
        .to_string()
}

// 1. Unbounded read of a >400-line, graph-covered file: denied (switch on)
//    with the outline inline, and the stub's own stderr warning never leaks.
#[test]
fn unbounded_read_of_large_covered_file_denies_with_outline() {
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

// 2. The same read bounded by offset/limit is allowed outright - a range
//    read is exactly what the guard steers toward.
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

// 3. The 400-line ceiling is a `>` comparison: exactly 400 lines is allowed
//    unbounded, 401 lines is caught by rule 1.
#[test]
fn line_limit_boundary_is_strictly_greater_than() {
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

// 4. When the source graph does not cover the file, rule 1 warns and
//    allows - it must never deny, even with the switch on.
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

// 5, 6, 7 and 8. Binary/image extension exemptions (rule 1 and repeat-rule),
// repeat-read escalation (full and range reads), the PDF `pages` bounded-
// range case, the tier-1 knowledge override, and the live-session deny gate
// all live in `hooks_read_guard_repeat.rs`, split out purely for size - see
// its module docs before editing one. It shares this file's harness via
// `use super::*`.
#[path = "hooks_read_guard_repeat.rs"]
mod repeat;
