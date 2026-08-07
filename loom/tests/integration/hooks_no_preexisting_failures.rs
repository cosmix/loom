//! Integration tests for the no-preexisting-failures hook.
//!
//! CLAUDE.md rule 15: "Nothing is 'pre-existing' - every warning and failure
//! you see is your responsibility." The hook nudges an agent that is about to
//! record a red gate as somebody else's problem.
//!
//! It is ADVISORY: it must always exit 0. The phrase has legitimate uses
//! (prevention notes, quoting the rule, naming the anti-pattern in review), so
//! blocking would be worse than the excuse. These tests pin both the detection
//! and the never-block guarantee.

use loom::fs::permissions::constants::{HOOK_COMMON, HOOK_NO_PREEXISTING_FAILURES};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");

    let common_path = temp.path().join("_common.sh");
    fs::write(&common_path, HOOK_COMMON).expect("write _common.sh");
    let mut perms = fs::metadata(&common_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&common_path, perms).expect("chmod _common.sh");

    let hook_path = temp.path().join("no-preexisting-failures.sh");
    fs::write(&hook_path, HOOK_NO_PREEXISTING_FAILURES).expect("write hook");
    let mut perms = fs::metadata(&hook_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    (temp, hook_path)
}

/// Run the hook with a raw JSON payload; return (exit_code, stdout).
fn run_hook_json(hook_path: &std::path::Path, json_input: &str) -> (i32, String) {
    use std::io::Write;

    let mut child = Command::new("bash")
        .arg(hook_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json_input.as_bytes()).ok();
    }

    let out = child.wait_with_output().expect("wait hook");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn run_bash(hook_path: &std::path::Path, command: &str) -> (i32, String) {
    let json = format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{}}}}}"#,
        serde_json::to_string(command).unwrap()
    );
    run_hook_json(hook_path, &json)
}

fn run_write(hook_path: &std::path::Path, content: &str) -> (i32, String) {
    let json = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"/tmp/x.md","content":{}}}}}"#,
        serde_json::to_string(content).unwrap()
    );
    run_hook_json(hook_path, &json)
}

fn warned(stdout: &str) -> bool {
    stdout.contains("LOOM_HOOK_WARN")
}

// =============================================================================
// Always advisory - the hook must never block
// =============================================================================

#[test]
fn preexisting_failures_hook_never_blocks() {
    let (_t, hook) = setup_hook();
    // The most flagrant phrasing still exits 0.
    let (code, out) = run_bash(
        &hook,
        "loom memory note \"2 pre-existing failures, not mine\"",
    );
    assert_eq!(code, 0, "hook must be advisory, never blocking");
    assert!(warned(&out));
}

// =============================================================================
// Detects the excuse in its common forms
// =============================================================================

#[test]
fn preexisting_failures_hook_flags_the_canonical_phrase() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_bash(&hook, "git commit -m \"skip the pre-existing failures\"");
    assert!(warned(&out), "should flag 'pre-existing failures'");
}

#[test]
fn preexisting_failures_hook_flags_hyphenless_spelling() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_write(&hook, "These are preexisting failures in the suite.");
    assert!(warned(&out), "should flag 'preexisting failures'");
}

#[test]
fn preexisting_failures_hook_flags_already_broken_on_main() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_write(&hook, "That suite was already broken on main, moving on.");
    assert!(warned(&out), "should flag 'already broken on main'");
}

#[test]
fn preexisting_failures_hook_flags_blaming_main() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_write(&hook, "The test fails on main too, so it is out of scope.");
    assert!(warned(&out), "should flag a failure attributed to main");
}

#[test]
fn preexisting_failures_hook_flags_environmental_and_flaky() {
    let (_t, hook) = setup_hook();
    let (_c, e) = run_write(&hook, "This is an environmental failure, ignoring.");
    assert!(warned(&e), "should flag 'environmental failure'");
    let (_c2, f) = run_write(&hook, "Just a flaky failure, rerunning is enough.");
    assert!(warned(&f), "should flag 'flaky failure'");
}

#[test]
fn preexisting_failures_hook_flags_unrelated_to_my_change() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_write(&hook, "Unrelated to my change, so leaving it red.");
    assert!(warned(&out), "should flag the out-of-scope disclaimer");
}

#[test]
fn preexisting_failures_hook_scans_edit_new_string() {
    let (_t, hook) = setup_hook();
    let json = r#"{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.md","old_string":"a","new_string":"known failure, leaving as is"}}"#;
    let (code, out) = run_hook_json(&hook, json);
    assert_eq!(code, 0);
    assert!(warned(&out), "should scan Edit new_string");
}

// =============================================================================
// Does NOT fire on legitimate prose - false positives would train agents to
// ignore the warning, which is worse than not having it
// =============================================================================

#[test]
fn preexisting_failures_hook_ignores_unrelated_text() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_bash(&hook, "cargo test --manifest-path loom/Cargo.toml");
    assert!(!warned(&out), "plain test run must not warn");
}

#[test]
fn preexisting_failures_hook_ignores_preexisting_non_failure_nouns() {
    let (_t, hook) = setup_hook();
    // "pre-existing" is ordinary English about code, not an excuse.
    let (_c, a) = run_write(&hook, "Reuse the pre-existing helper instead of a new one.");
    assert!(!warned(&a), "'pre-existing helper' must not warn");
    let (_c2, b) = run_write(&hook, "Match the pre-existing convention in this module.");
    assert!(!warned(&b), "'pre-existing convention' must not warn");
}

#[test]
fn preexisting_failures_hook_ignores_a_genuine_fix_description() {
    let (_t, hook) = setup_hook();
    let (_c, out) = run_bash(
        &hook,
        "git commit -m \"fix(fs): retry a racing O_CREAT open\n\nDiagnosed to a root cause and fixed.\"",
    );
    assert!(!warned(&out), "describing an actual fix must not warn");
}
