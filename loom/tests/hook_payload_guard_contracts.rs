//! Contract for stage hooks-and-delivery (CR-66): a blocking PreToolUse guard
//! refuses a payload it cannot read (empty stdin, or JSON cut short before the
//! tool name), while advisory hooks keep allowing it.
//!
//! Each hook runs from a temp cwd with a cleared environment: PATH from the test
//! process, HOME a temp directory and no `LOOM_` variable.
//!
//! Requires `bash` and `jq` on PATH.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::TempDir;

const BLOCKING_GUARDS: [&str; 3] = [
    "commit-filter.sh",
    "git-add-guard.sh",
    "credential-guard.sh",
];
const ADVISORY_HOOKS: [&str; 2] = ["prefer-modern-tools.sh", "no-preexisting-failures.sh"];
const UNREADABLE_BODIES: [&str; 2] = ["", r#"{"tool_name":"Ba"#];
const CONTROL_BODY: &str = r#"{"tool_name":"NoSuchTool"}"#;

struct HookRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs `loom-hooks/<hook>` with `body` on stdin from a fresh temp cwd.
fn run_hook(hook: &str, body: &str) -> HookRun {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../loom-hooks")
        .join(hook);
    let dir = TempDir::new().expect("create temp dir");
    let home = dir.path().join("home");
    let cwd = dir.path().join("cwd");
    std::fs::create_dir_all(&home).expect("create temp home");
    std::fs::create_dir_all(&cwd).expect("create temp cwd");
    let mut child = Command::new("bash")
        .arg(&script)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", &home)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn {hook}: {err}"));
    let mut stdin = child.stdin.take().expect("hook stdin");
    stdin.write_all(body.as_bytes()).expect("write hook stdin");
    drop(stdin);
    let out = child.wait_with_output().expect("wait for hook");
    HookRun {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn assert_refused(hook: &str, body: &str) {
    let run = run_hook(hook, body);
    assert_eq!(
        run.code,
        Some(2),
        "{hook} must refuse the unreadable payload {body:?} with exit 2\nstdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(
        !run.stderr.trim().is_empty(),
        "{hook} refused {body:?} without saying why on stderr"
    );
}

fn assert_allowed(hook: &str, body: &str) {
    let run = run_hook(hook, body);
    assert_eq!(
        run.code,
        Some(0),
        "{hook} must allow the payload {body:?} with exit 0\nstdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
}

#[test]
fn blocking_guards_refuse_empty_and_truncated_input() {
    for hook in BLOCKING_GUARDS {
        for body in UNREADABLE_BODIES {
            assert_refused(hook, body);
        }
        assert_allowed(hook, CONTROL_BODY);
    }
}

#[test]
fn advisory_hook_still_allows_empty_input() {
    assert_refused("commit-filter.sh", "");
    for hook in ADVISORY_HOOKS {
        assert_allowed(hook, "");
    }
}
