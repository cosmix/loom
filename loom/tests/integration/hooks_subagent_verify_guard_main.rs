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
fn main_agent_of_an_integration_verify_stage_is_allowed() {
    let (code, stderr) = run_main("cargo test", write_integration_verify_stage);
    assert_eq!(
        code, 0,
        "integration-verify may run the suite; stderr={stderr}"
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
