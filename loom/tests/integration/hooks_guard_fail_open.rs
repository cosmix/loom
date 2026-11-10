//! `subagent-verify-guard.sh` fails open, visibly, when `jq` is missing.
//!
//! The guard only deters bare full-suite runs, so its own breakage must never
//! stop work: without `jq` it warns ("jq is not installed") and exits 1, Claude
//! Code's non-blocking hook error, so the Bash call proceeds. The `jq` check
//! runs before the guard's role check, so no process tree is needed.

use loom::fs::permissions::constants as hooks;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// The guard plus every helper it and `_common.sh` source.
fn write_guard(dir: &Path) {
    for (name, content) in [
        ("_common.sh", hooks::HOOK_COMMON),
        ("_bounded.sh", hooks::HOOK_BOUNDED),
        ("_verify_matchers.sh", hooks::HOOK_VERIFY_MATCHERS),
        ("_parser.sh", hooks::HOOK_PARSER),
        ("_parser_expand.sh", hooks::HOOK_PARSER_EXPAND),
        ("_parser_scan.sh", hooks::HOOK_PARSER_SCAN),
        ("_identity.sh", hooks::HOOK_IDENTITY),
        ("_stage_lookup.sh", hooks::HOOK_STAGE_LOOKUP),
        ("_containment.sh", hooks::HOOK_CONTAINMENT),
        ("_heartbeat.sh", hooks::HOOK_HEARTBEAT),
        (
            "subagent-verify-guard.sh",
            hooks::HOOK_SUBAGENT_VERIFY_GUARD,
        ),
    ] {
        fs::write(dir.join(name), content).unwrap_or_else(|error| panic!("write {name}: {error}"));
    }
}

/// A directory of symlinks to every file on `PATH` except `jq`; the first
/// directory to provide a name wins, as in a PATH lookup.
fn path_without_jq(dir: &Path) {
    let path = std::env::var_os("PATH").expect("PATH is set");
    let mut seen = HashSet::new();
    for entry_dir in std::env::split_paths(&path) {
        let Ok(entries) = fs::read_dir(&entry_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == "jq" || !seen.insert(name.clone()) {
                continue;
            }
            std::os::unix::fs::symlink(entry.path(), dir.join(&name))
                .unwrap_or_else(|error| panic!("link {name:?}: {error}"));
        }
    }
    assert!(!dir.join("jq").exists(), "jq must be absent from the PATH");
}

#[test]
fn a_missing_jq_lets_the_call_through_with_a_warning() {
    let hooks_dir = TempDir::new().expect("create hooks dir");
    let bin_dir = TempDir::new().expect("create PATH dir");
    write_guard(hooks_dir.path());
    path_without_jq(bin_dir.path());

    let mut child = Command::new("bash")
        .arg(hooks_dir.path().join("subagent-verify-guard.sh"))
        .env("PATH", bin_dir.path())
        .env_remove("LOOM_MAIN_AGENT_PID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the guard");
    let payload = r#"{"tool_name":"Bash","tool_input":{"command":"cargo test"}}"#;
    // The guard may exit before reading stdin; a broken pipe is not a failure.
    let _ = child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.as_bytes());
    let output = child.wait_with_output().expect("wait for the guard");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(stderr.contains("jq is not installed"), "stderr: {stderr}");
    assert!(stdout.is_empty(), "stdout: {stdout}");
}
