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
// environment, its ancestry, or the sandbox itself, all of which would make
// this "no evidence" test lie about what it exercised. Returns why to skip
// rather than fake an absence that is not actually there.
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
    if let Some(reason) = ancestor_stage_evidence_reason() {
        return Some(reason);
    }
    None
}

// ancestor_stage_evidence_reason - mirrors _loom_ancestor_stage_evidence in
// loom-hooks/_codex_forward.sh: the guard walks up to 12 ancestor pids
// starting at its own process, and any ancestor whose /proc/<pid>/environ
// carries LOOM_STAGE_ID/LOOM_SESSION_ID/LOOM_WORK_DIR counts as stage
// evidence, even when the guard's own (and this test's) direct environment
// was scrubbed. `loom stage complete`'s acceptance runner clears LOOM_* only
// on the test process itself, so under it that evidence sits on an ancestor,
// which the direct env check above cannot see.
fn ancestor_stage_evidence_reason() -> Option<String> {
    let mut pid = std::process::id();
    for _ in 0..12 {
        if pid_environ_has_stage_var(pid) {
            return Some(format!(
                "ancestor pid {pid} carries a LOOM_* stage variable"
            ));
        }
        pid = match parent_pid(pid) {
            Some(p) if p > 1 => p,
            _ => break,
        };
    }
    None
}

// pid_environ_has_stage_var - reads /proc/<pid>/environ (NUL-separated) for a
// LOOM_STAGE_ID=, LOOM_SESSION_ID=, or LOOM_WORK_DIR= entry with a non-empty
// value. Missing or unreadable (no /proc on macOS, or a pid owned by another
// user) means "no evidence from this pid", never a panic.
fn pid_environ_has_stage_var(pid: u32) -> bool {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/environ")) else {
        return false;
    };
    bytes.split(|&b| b == 0).any(|entry| {
        ["LOOM_STAGE_ID=", "LOOM_SESSION_ID=", "LOOM_WORK_DIR="]
            .iter()
            .any(|prefix| entry.len() > prefix.len() && entry.starts_with(prefix.as_bytes()))
    })
}

// parent_pid - reads the ppid field from /proc/<pid>/stat. Parsing resumes
// after the last ')' because the comm field it precedes can itself contain
// spaces and parentheses; the field right after that is state, and the one
// after state is ppid. Returns None when unreadable or unparsable.
fn parent_pid(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(1)?.parse().ok()
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
