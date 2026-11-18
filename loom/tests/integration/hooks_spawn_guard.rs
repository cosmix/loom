//! Spawn-guard integration coverage for rewrites, denials, warnings, and its ledger.

use super::helpers::{clear_relay_env, run_hook_process};
use loom::fs::permissions::constants::{
    HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_LEDGER, HOOK_SPAWN_GUARD, HOOK_SUBAGENT_PREAMBLE,
};
use loom::process::sandbox_probe::{process_tree_visible, skip_unless};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const PREAMBLE_LINE: &str = "CLAUDE.md is already in your context; the rules below are the ones that bind you as a subagent. The knowledge you need for this task is quoted in this brief - do not open doc/loom/knowledge/ unless the brief says a pull came back empty.";

const UNTYPED_MSG_LEAD: &str = "Untyped spawn inherits the model of the spawning session.";

const SPAWN_KEYS: &[&str] = &[
    "\"ts\"",
    "\"stage_id\"",
    "\"session_id\"",
    "\"caller\"",
    "\"agent_type\"",
    "\"model\"",
    "\"model_source\"",
    "\"description\"",
];

/// Installs the hook beside every file it sources or reads, as the installer does.
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    use loom::fs::permissions::constants as hooks;
    let temp = TempDir::new().expect("create temp dir");
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
        ("_read_discipline.sh", HOOK_READ_DISCIPLINE),
        ("_read_ledger.sh", HOOK_READ_LEDGER),
        ("_subagent-preamble.txt", HOOK_SUBAGENT_PREAMBLE),
        ("spawn-guard.sh", HOOK_SPAWN_GUARD),
    ] {
        let path = temp.path().join(name);
        fs::write(&path, content).unwrap_or_else(|e| panic!("write {name}: {e}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let hook_path = temp.path().join("spawn-guard.sh");
    (temp, hook_path)
}

/// `prompt` as the hook rewrites it: the preamble file, a blank line, then
/// the prompt byte for byte.
fn with_preamble(prompt: &str) -> String {
    format!(
        "{}\n\n{prompt}",
        HOOK_SUBAGENT_PREAMBLE.trim_end_matches('\n')
    )
}

fn temp() -> TempDir {
    TempDir::new().expect("create temp dir")
}

fn skip_unless_gate_visible(test: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        &format!("hooks_spawn_guard::{test}"),
        "the enforcement gate needs a visible process tree",
    )
}

fn write_agent_def(base: &Path, agent_type: &str, model: &str) {
    let dir = base.join(".claude/agents");
    fs::create_dir_all(&dir).expect("create agents dir");
    let content = format!("---\nname: {agent_type}\nmodel: {model}\n---\n\nBody.\n");
    fs::write(dir.join(format!("{agent_type}.md")), content).expect("write agent def");
}

struct HookOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_hook(
    hook: &Path,
    tool_name: &str,
    tool_input: Value,
    cwd: &Path,
    home: &Path,
    extra_env: &[(&str, &str)],
) -> HookOutput {
    let payload = json!({"tool_name": tool_name, "tool_input": tool_input}).to_string();

    let mut cmd = Command::new("bash");
    clear_relay_env(&mut cmd)
        .arg(hook)
        .current_dir(cwd)
        // Never let the developer's shell leak `loom_debug` output onto stderr.
        .env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .env("HOME", home)
        // Out of a stage the session ledgers live under TMPDIR: keep them per test.
        .env("TMPDIR", home)
        .env("PATH", "/usr/bin:/bin");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    let output = run_hook_process(&mut cmd, payload.as_bytes());
    HookOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn gated_task(hook: &Path, tool_input: Value, cwd: &Path, home: &Path, work: &Path) -> HookOutput {
    let pid = std::process::id().to_string();
    let work_str = work.to_string_lossy().into_owned();
    run_hook(
        hook,
        "Task",
        tool_input,
        cwd,
        home,
        &[
            ("LOOM_STAGE_ID", "test-stage"),
            ("LOOM_MAIN_AGENT_PID", pid.as_str()),
            ("LOOM_WORK_DIR", work_str.as_str()),
            ("LOOM_SESSION_ID", "test-session"),
        ],
    )
}

fn get_str<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_str()
}

fn read_spawn_line(work_dir: &Path) -> String {
    let path = work_dir.join("subagents/test-stage/spawns.jsonl");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected one spawn record in {}: {content:?}",
        path.display()
    );
    lines[0].to_string()
}

fn assert_spawn_key_order(line: &str) {
    let mut last = 0usize;
    for key in SPAWN_KEYS {
        let pos = line[last..]
            .find(key)
            .unwrap_or_else(|| panic!("{key} not found in order after offset {last}: {line}"));
        last += pos + key.len();
    }
}

#[path = "hooks_spawn_guard_codex.rs"]
mod codex;

#[path = "hooks_spawn_guard_gate.rs"]
mod gate;

#[path = "hooks_spawn_guard_cases.rs"]
mod cases;
