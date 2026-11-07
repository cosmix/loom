//! Contract: the installed spawn guard appends the reviewer brief of `loom
//! hook reviewer-brief` to a `loom-code-reviewer` spawn exactly once, after
//! the prompt and the worker brief, each separated by a blank line.

#[path = "integration/helpers.rs"]
// only clear_relay_env() and run_hook_process() are used
#[allow(dead_code)]
mod helpers;

use helpers::{clear_relay_env, run_hook_process};
use loom::fs::permissions::constants as hooks;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const WORKER_NONCE: &str = "0123456789abcdef0123456789abcdef";
const REVIEWER_NONCE: &str = "fedcba9876543210fedcba9876543210";

const STUB: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>"$LOOM_CALLS"
case "$1 $2" in
"hook worker-brief")
	cat >/dev/null
	printf '%s\n' "$WORKER_BRIEF_RESPONSE"
	;;
"hook reviewer-brief") printf '%s\n' "$REVIEWER_BRIEF_RESPONSE" ;;
"hook reviewer-model") printf 'model=opus effort=high\n' ;;
esac
exit 0
"#;

/// Installs spawn-guard.sh beside every file it sources or reads, as the
/// installer does, and returns the guard's path.
fn install_guard(dir: &Path) -> PathBuf {
    for (name, content) in [
        ("_bounded.sh", hooks::HOOK_BOUNDED),
        ("_common.sh", hooks::HOOK_COMMON),
        ("_parser.sh", hooks::HOOK_PARSER),
        ("_parser_expand.sh", hooks::HOOK_PARSER_EXPAND),
        ("_parser_scan.sh", hooks::HOOK_PARSER_SCAN),
        ("_identity.sh", hooks::HOOK_IDENTITY),
        ("_stage_lookup.sh", hooks::HOOK_STAGE_LOOKUP),
        ("_containment.sh", hooks::HOOK_CONTAINMENT),
        ("_heartbeat.sh", hooks::HOOK_HEARTBEAT),
        ("_spawn_policy.sh", hooks::HOOK_SPAWN_POLICY),
        ("_read_discipline.sh", hooks::HOOK_READ_DISCIPLINE),
        ("_read_ledger.sh", hooks::HOOK_READ_LEDGER),
        ("_subagent-preamble.txt", hooks::HOOK_SUBAGENT_PREAMBLE),
        ("spawn-guard.sh", hooks::HOOK_SPAWN_GUARD),
    ] {
        executable(&dir.join(name), content);
    }
    dir.join("spawn-guard.sh")
}

fn executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn worker_brief() -> String {
    format!("## Scoped worker brief\n\n<!-- loom-worker-brief nonce={WORKER_NONCE} -->\n\nScoped fact only.")
}

fn reviewer_brief() -> String {
    format!(
        "<!-- loom-reviewer-brief nonce={REVIEWER_NONCE} -->\n## Test-integrity events\n\n### TI-edit-src/calc_tests.rs (accepted)\n\n```text\nassert_eq!(x, 5);\n```"
    )
}

/// Runs the installed guard in `dir` on `payload` with the gate environment
/// and a stub loom answering both brief hooks with the given envelopes.
fn run_guard(dir: &Path, payload: &Value, worker: &str, reviewer: &str) -> Output {
    let (home, cwd, work) = (temp(), temp(), temp());
    let guard = install_guard(dir);
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("create stub bin");
    let stub = bin.join("loom");
    executable(&stub, STUB);
    let mut cmd = Command::new("bash");
    clear_relay_env(&mut cmd)
        .arg(&guard)
        .current_dir(cwd.path())
        .env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("LOOM_BIN", &stub)
        .env("LOOM_STAGE_ID", "test-stage")
        .env("LOOM_MAIN_AGENT_PID", std::process::id().to_string())
        .env("LOOM_WORK_DIR", work.path())
        .env("LOOM_SESSION_ID", "test-session")
        .env("LOOM_CALLS", bin.join("calls"))
        .env("WORKER_BRIEF_RESPONSE", worker)
        .env("REVIEWER_BRIEF_RESPONSE", reviewer);
    run_hook_process(&mut cmd, payload.to_string().as_bytes())
}

#[test]
fn guard_appends_the_reviewer_brief_once() {
    let dir = temp();
    let preamble_line = hooks::HOOK_SUBAGENT_PREAMBLE
        .lines()
        .next()
        .expect("preamble");
    let prompt = format!("{preamble_line}\n\nReview the stage diff.");
    let payload = json!({"tool_name": "Task", "tool_input": {
        "subagent_type": "loom-code-reviewer", "description": "review", "prompt": prompt}});
    let worker = json!({"nonce": WORKER_NONCE, "brief": worker_brief()}).to_string();
    let reviewer = json!({"nonce": REVIEWER_NONCE, "brief": reviewer_brief()}).to_string();

    let out = run_guard(dir.path(), &payload, &worker, &reviewer);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr={stderr}");
    let value: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("guard output is not JSON ({e}): {stdout} stderr={stderr}"));
    let output = &value["hookSpecificOutput"];
    assert_eq!(
        output["permissionDecision"].as_str(),
        Some("allow"),
        "{value}"
    );
    let expected = format!("{prompt}\n\n{}\n\n{}", worker_brief(), reviewer_brief());
    assert_eq!(
        output["updatedInput"]["prompt"].as_str(),
        Some(expected.as_str()),
        "calls={}",
        fs::read_to_string(dir.path().join("bin/calls")).unwrap_or_default()
    );
}

fn temp() -> TempDir {
    TempDir::new().expect("create temp dir")
}
