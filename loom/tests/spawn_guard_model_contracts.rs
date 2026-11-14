//! Contract: outside a loom stage the installed spawn guard pins an untyped
//! spawn that names no model to sonnet, and leaves an explicit model alone.

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
use std::process::Command;
use tempfile::TempDir;

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
        let path = dir.join(name);
        fs::write(&path, content).unwrap_or_else(|e| panic!("write {name}: {e}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    dir.join("spawn-guard.sh")
}

/// Runs the guard on a `Task` spawn with no loom stage in the environment,
/// so the enforcement gate does not pass, and parses its JSON output.
fn run_ungated(tool_input: Value) -> Value {
    let (hooks_dir, home, cwd) = (temp(), temp(), temp());
    let guard = install_guard(hooks_dir.path());
    let payload = json!({"tool_name": "Task", "tool_input": tool_input}).to_string();
    let mut cmd = Command::new("bash");
    clear_relay_env(&mut cmd)
        .arg(&guard)
        .current_dir(cwd.path())
        .env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("PATH", "/usr/bin:/bin");
    let output = run_hook_process(&mut cmd, payload.as_bytes());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse {stdout:?}: {e}"))
}

fn temp() -> TempDir {
    TempDir::new().expect("create temp dir")
}

#[test]
fn untyped_spawn_outside_stage_is_pinned_to_sonnet_and_explicit_model_kept() {
    let out = run_ungated(json!({"subagent_type": "general-purpose", "prompt": "x"}));
    let hso = &out["hookSpecificOutput"];
    assert_eq!(hso["permissionDecision"], "allow", "output={out}");
    assert_eq!(hso["updatedInput"]["model"], "sonnet", "output={out}");
    assert_eq!(hso["updatedInput"]["prompt"], "x", "output={out}");
    assert_eq!(
        hso["updatedInput"]["subagent_type"], "general-purpose",
        "updatedInput replaces the whole tool input: output={out}"
    );

    let explicit = run_ungated(json!({
        "subagent_type": "general-purpose",
        "prompt": "x",
        "model": "haiku"
    }));
    assert!(
        explicit["hookSpecificOutput"].get("updatedInput").is_none(),
        "an explicit model must not be rewritten: output={explicit}"
    );
}
