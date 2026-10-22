//! Main-agent tests for `subagent-verify-guard.sh`.
//!
//! In a live loom STANDARD stage session the MAIN agent may not run an
//! unfiltered test suite (the stage gate runs once, in `loom check`); build,
//! clippy and fmt stay allowed, and nothing is blocked in an integration-verify
//! stage or outside a live loom session. Helpers come from the parent module.

use super::{
    bash_payload, install_hook, run_hook_in_tree, skip_unless_gate_visible, temp_dir_no_claude,
    write_integration_verify_stage,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const STAGE: &str = "main-guard-stage";

fn write_standard_stage(work_dir: &Path, stage_id: &str) {
    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).expect("create stages dir");
    let content = format!(
        "---\nid: {stage_id}\nname: Standard\nstatus: executing\nstage_type: standard\nplan_id: PLAN-test\nworking_dir: .\n---\n\n# Stage: Standard\n"
    );
    fs::write(stages_dir.join(format!("01-{stage_id}.md")), content).expect("write stage file");
}

/// Run `command` as the main agent of the stage written by `write_stage`.
fn run_main(command: &str, write_stage: fn(&Path, &str)) -> (i32, String) {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());
    let work_dir = dir.path().join("work");
    write_stage(&work_dir, STAGE);
    run_hook_in_tree(
        dir.path(),
        &hook_path,
        false,
        &bash_payload(command),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", STAGE),
        ],
    )
}

#[test]
fn main_agent_of_a_standard_stage_is_blocked_on_unfiltered_suites() {
    if skip_unless_gate_visible(
        "main::main_agent_of_a_standard_stage_is_blocked_on_unfiltered_suites",
    ) {
        return;
    }
    for command in [
        "cargo test",
        "bunx vitest run",
        "uv run pytest",
        "go test ./...",
    ] {
        let (code, stderr) = run_main(command, write_standard_stage);
        assert_eq!(code, 2, "{command:?} must be blocked; stderr={stderr}");
        assert!(
            stderr.contains(&format!("loom check {STAGE}")) && stderr.contains("--criterion"),
            "the refusal must point at loom check: {stderr}"
        );
    }
}

#[test]
fn main_agent_of_a_standard_stage_may_filter_lint_and_format() {
    for command in [
        "cargo test --lib a::",
        "bunx vitest run src/a.test.ts",
        "cargo clippy --all-targets",
        "cargo fmt --check",
        "go test -run TestA ./...",
    ] {
        let (code, stderr) = run_main(command, write_standard_stage);
        assert_eq!(code, 0, "{command:?} must be allowed; stderr={stderr}");
    }
}

#[test]
fn main_agent_of_a_standard_stage_is_blocked_on_every_runner_family() {
    if skip_unless_gate_visible(
        "main::main_agent_of_a_standard_stage_is_blocked_on_every_runner_family",
    ) {
        return;
    }
    for command in [
        "bun test",
        "cargo nextest run",
        "cargo nextest r",
        "jest",
        "go test",
        "rake test",
        "sbt test",
        "python -m unittest",
        "php vendor/bin/pest",
    ] {
        let (code, stderr) = run_main(command, write_standard_stage);
        assert_eq!(code, 2, "{command:?} must be blocked; stderr={stderr}");
    }
    for command in [
        "bun test src/a.test.ts",
        "cargo nextest run -E test(a)",
        "jest -t login",
        "go test ./pkg",
        "rake test TEST=test/a_test.rb",
        "sbt testOnly a.B",
        "python -m unittest tests.test_a",
        "bun build",
        "tsc -p .",
    ] {
        let (code, stderr) = run_main(command, write_standard_stage);
        assert_eq!(code, 0, "{command:?} must be allowed; stderr={stderr}");
    }
}

#[test]
fn main_agent_of_an_integration_verify_stage_is_allowed() {
    let (code, stderr) = run_main("cargo test", write_integration_verify_stage);
    assert_eq!(
        code, 0,
        "integration-verify may run the suite; stderr={stderr}"
    );
}

/// The payload Claude Code sends for a MAIN session's Bash call: no
/// `agent_type`, and a transcript named `<session_id>.jsonl` (what
/// `loom_payload_agent_verdict` reads as "main").
fn main_shaped_payload(command: &str) -> String {
    format!(
        r#"{{"tool_name": "Bash", "session_id": "main-sess", "transcript_path": "/proj/main-sess.jsonl", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).unwrap()
    )
}

/// Run an unfiltered `cargo test` through the hook, started directly (no fake
/// Claude chain), with `LOOM_MAIN_AGENT_PID` set to `main_pid`, in a standard
/// stage. Only the pid decides whether the hook sees a live loom session.
fn run_with_main_pid(main_pid: u32) -> (i32, String) {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());
    let work_dir = dir.path().join("work");
    write_standard_stage(&work_dir, STAGE);
    let mut child = Command::new("bash")
        .arg(&hook_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LOOM_MAIN_AGENT_PID", main_pid.to_string())
        .env("LOOM_WORK_DIR", &work_dir)
        .env("LOOM_STAGE_ID", STAGE)
        .spawn()
        .expect("spawn hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(main_shaped_payload("cargo test").as_bytes())
        .ok();
    let output = child.wait_with_output().expect("wait for hook");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// `LOOM_MAIN_AGENT_PID` is the only thing that scopes this globally installed
/// hook to a loom stage session. A pid that names no process (a value leaked
/// from a finished session) must leave an ordinary session unblocked.
#[test]
fn main_agent_is_allowed_when_loom_main_agent_pid_is_dead() {
    let mut gone = Command::new("true").spawn().expect("spawn true");
    gone.wait().expect("reap the short-lived process");
    let (code, stderr) = run_with_main_pid(gone.id());
    assert_eq!(
        code, 0,
        "a dead LOOM_MAIN_AGENT_PID must allow; stderr={stderr}"
    );
}

/// A live process that is not an ancestor of the hook is just as stale: only a
/// process-tree ancestor proves the caller runs inside the loom session.
#[test]
fn main_agent_is_allowed_when_loom_main_agent_pid_is_not_an_ancestor() {
    if skip_unless_gate_visible(
        "main::main_agent_is_allowed_when_loom_main_agent_pid_is_not_an_ancestor",
    ) {
        return;
    }
    let mut bystander = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let (code, stderr) = run_with_main_pid(bystander.id());
    bystander.kill().ok();
    bystander.wait().ok();
    assert_eq!(
        code, 0,
        "a live non-ancestor LOOM_MAIN_AGENT_PID must allow; stderr={stderr}"
    );
}

/// Control for the two tests above: the same standard stage, environment and
/// main-shaped payload are blocked once the pid names a live ancestor.
#[test]
fn main_shaped_payload_with_a_live_ancestor_pid_is_blocked() {
    if skip_unless_gate_visible("main::main_shaped_payload_with_a_live_ancestor_pid_is_blocked") {
        return;
    }
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());
    let work_dir = dir.path().join("work");
    write_standard_stage(&work_dir, STAGE);
    let (code, stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        false,
        &main_shaped_payload("cargo test"),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", STAGE),
        ],
    );
    assert_eq!(code, 2, "a live ancestor pid must block; stderr={stderr}");
    assert!(
        stderr.contains(&format!("loom check {STAGE}")),
        "the refusal must point at loom check: {stderr}"
    );
}

#[test]
fn no_live_loom_session_allows_everything() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());
    let work_dir = dir.path().join("work");
    write_standard_stage(&work_dir, STAGE);

    for command in ["cargo test", "bunx vitest run", "cargo build"] {
        let mut child = Command::new("bash")
            .arg(&hook_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("LOOM_MAIN_AGENT_PID")
            .env("LOOM_WORK_DIR", &work_dir)
            .env("LOOM_STAGE_ID", STAGE)
            .spawn()
            .expect("spawn hook");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(bash_payload(command).as_bytes())
            .ok();
        let output = child.wait_with_output().expect("wait for hook");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{command:?} with no live session must be allowed; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
