//! Integration tests for hook commit message filtering
//!
//! Tests verify that the commit-filter hook correctly blocks commits
//! containing Claude co-authorship attribution per CLAUDE.md rule 8.
//!
//! These tests run the hook script directly with bash - no loom invocation.

use loom::fs::permissions::constants::{HOOK_COMMIT_FILTER, HOOK_COMMON};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

/// Install hook script and its dependencies to temp directory and return path
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");
    use loom::fs::permissions::constants as hooks;
    for (name, content) in [
        ("_bounded.sh", hooks::HOOK_BOUNDED),
        ("_common.sh", HOOK_COMMON),
        ("_parser.sh", hooks::HOOK_PARSER),
        ("_parser_expand.sh", hooks::HOOK_PARSER_EXPAND),
        ("_parser_scan.sh", hooks::HOOK_PARSER_SCAN),
        ("_identity.sh", hooks::HOOK_IDENTITY),
        ("_stage_lookup.sh", hooks::HOOK_STAGE_LOOKUP),
        ("_containment.sh", hooks::HOOK_CONTAINMENT),
        ("_heartbeat.sh", hooks::HOOK_HEARTBEAT),
        ("_spawn_policy.sh", hooks::HOOK_SPAWN_POLICY),
    ] {
        fs::write(temp.path().join(name), content)
            .unwrap_or_else(|error| panic!("write {name}: {error}"));
    }
    let common_path = temp.path().join("_common.sh");
    let mut perms = fs::metadata(&common_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&common_path, perms).expect("chmod _common.sh");

    // Install the hook script
    let hook_path = temp.path().join("commit-filter.sh");
    fs::write(&hook_path, HOOK_COMMIT_FILTER).expect("write hook");
    let mut perms = fs::metadata(&hook_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    (temp, hook_path)
}

/// Run hook with tool_name and command, return exit code
/// The hook reads JSON from stdin: {"tool_name": "...", "tool_input": {"command": "..."}}
fn run_hook(hook_path: &std::path::Path, tool_name: &str, command: &str) -> i32 {
    run_hook_with_env(hook_path, tool_name, command, &[])
}

/// As `run_hook`, with the stage-session variables removed first (an acceptance
/// run may itself be a stage session) and then `env` applied on top.
fn run_hook_with_env(
    hook_path: &std::path::Path,
    tool_name: &str,
    command: &str,
    env: &[(&str, &str)],
) -> i32 {
    use std::io::Write;
    use std::process::Stdio;

    // Build JSON input matching what Claude Code sends
    let json_input = format!(
        r#"{{"tool_name": "{tool_name}", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).unwrap_or_else(|_| format!("\"{command}\""))
    );

    let mut child = Command::new("bash")
        .arg(hook_path)
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_SESSION_ID")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json_input.as_bytes()).ok();
    }

    child.wait().expect("wait hook").code().unwrap_or(-1)
}

#[path = "hooks_commit_filter_session.rs"]
mod session;

#[path = "hooks_commit_filter_cases.rs"]
mod cases;
