//! Regression test for codex-forward-guard.sh's "no stage evidence" path.
//! Split out from policy_tests.rs, which is already at its line-count ceiling.

use crate::fs::permissions::constants::{
    HOOK_CODEX_FORWARD_COMMON, HOOK_CODEX_FORWARD_GUARD, HOOK_COMMON, HOOK_LIFECYCLE,
};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

// skip_reason - stage evidence can leak in from the test process's own
// environment or from the sandbox itself, both of which would make this
// "no evidence" test lie about what it exercised. Returns why to skip rather
// than fake an absence that is not actually there.
fn skip_reason() -> Option<String> {
    for name in ["LOOM_STAGE_ID", "LOOM_SESSION_ID", "LOOM_WORK_DIR"] {
        if std::env::var_os(name).is_some() {
            return Some(format!("{name} is set in the test process environment"));
        }
    }
    if fs::read_to_string("/proc/1/comm")
        .map(|comm| comm.trim() == "bwrap")
        .unwrap_or(false)
    {
        return Some("running inside a bwrap sandbox, which is itself stage evidence".to_string());
    }
    None
}

fn install_guard(dir: &Path) -> PathBuf {
    fs::write(dir.join("_common.sh"), HOOK_COMMON).unwrap();
    fs::write(dir.join("_lifecycle.sh"), HOOK_LIFECYCLE).unwrap();
    fs::write(dir.join("_codex_forward.sh"), HOOK_CODEX_FORWARD_COMMON).unwrap();
    let script = dir.join("codex-forward-guard.sh");
    fs::write(&script, HOOK_CODEX_FORWARD_GUARD).unwrap();
    script
}

fn run_guard(script: &Path, cwd: &Path, home: &Path, payload: &Value) -> Output {
    let mut child = Command::new("bash")
        .arg(script)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// forward_guard_allows_when_no_stage_evidence_exists - with zero loom stage
// signals in the process env, its ancestry, or the sandbox, the guard has no
// policy to enforce and allows any payload, forwarder or not.
#[test]
fn forward_guard_allows_when_no_stage_evidence_exists() {
    if let Some(reason) = skip_reason() {
        eprintln!("SKIP: forward_guard_allows_when_no_stage_evidence_exists - {reason}");
        return;
    }

    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let worktree = temp.path().join("worktree");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&worktree).unwrap();
    let script = install_guard(temp.path());

    let payload = json!({
        "tool_name": "Edit",
        "agent_type": "loom-codex-forwarder",
        "tool_input": {"file_path": "README.md"}
    });
    let output = run_guard(&script, &worktree, &home, &payload);

    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(output.stderr.is_empty());
}
