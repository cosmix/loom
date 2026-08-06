//! Integration tests for the git-add-guard hook.
//!
//! The guard exists to stop `.work` - a symlink to shared orchestration state -
//! from being staged, and to stop blanket staging (`-A`, `--all`, `.`).
//!
//! It must do that WITHOUT blocking legitimate specific-file staging. The
//! regression these tests pin: `strip_embedded_content` only strips single-line
//! `-m` bodies, and `.` matches newlines in bash's `=~`, so an unbounded `.*`
//! let a multi-line commit MESSAGE satisfy the danger patterns. Staging named
//! files was blocked whenever the body contained "-A" (as inside
//! "Co-Authored-By") or the string ".work" - with a diagnostic naming patterns
//! the command never used.
//!
//! These tests run the hook script directly with bash - no loom invocation.

use loom::fs::permissions::constants::{HOOK_COMMON, HOOK_GIT_ADD_GUARD};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

/// Install the guard and its `_common.sh` dependency into a temp dir.
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");

    // _common.sh first - the guard sources it via dirname
    let common_path = temp.path().join("_common.sh");
    fs::write(&common_path, HOOK_COMMON).expect("write _common.sh");
    let mut perms = fs::metadata(&common_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&common_path, perms).expect("chmod _common.sh");

    let hook_path = temp.path().join("git-add-guard.sh");
    fs::write(&hook_path, HOOK_GIT_ADD_GUARD).expect("write hook");
    let mut perms = fs::metadata(&hook_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    (temp, hook_path)
}

/// Run the hook against a Bash command; return its exit code (0 allow, 2 block).
fn run_hook(hook_path: &std::path::Path, command: &str) -> i32 {
    use std::io::Write;
    use std::process::Stdio;

    let json_input = format!(
        r#"{{"tool_name": "Bash", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).expect("encode command")
    );

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

    child.wait().expect("wait hook").code().unwrap_or(-1)
}

// =============================================================================
// Blocks the genuinely dangerous patterns (exit 2)
// =============================================================================

#[test]
fn git_add_guard_blocks_stage_all_short_flag() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add -A"), 2);
}

#[test]
fn git_add_guard_blocks_stage_all_long_flag() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add --all"), 2);
}

#[test]
fn git_add_guard_blocks_stage_current_directory() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add ."), 2);
}

#[test]
fn git_add_guard_blocks_flag_after_a_path() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add src/main.rs -A"), 2);
}

#[test]
fn git_add_guard_blocks_explicit_work_dir() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add .work"), 2);
    assert_eq!(run_hook(&hook, "git add src/main.rs .work/config.toml"), 2);
}

// =============================================================================
// Allows legitimate staging (exit 0) - the false-positive regressions
// =============================================================================

#[test]
fn git_add_guard_allows_specific_files() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "git add src/main.rs src/lib.rs"), 0);
}

#[test]
fn git_add_guard_allows_multiline_message_mentioning_attribution() {
    let (_temp, hook) = setup_hook();
    // "Co-Authored-By" contains "-A". Before the fix the unbounded `.*` reached
    // it across the newline and blocked the staging.
    let cmd = "git add hooks/commit-filter.sh\n\
               git commit -q -m \"fix(hooks): tighten the guard\n\
               \n\
               Explain that a Co-Authored-By trailer must never be added.\"";
    assert_eq!(run_hook(&hook, cmd), 0);
}

#[test]
fn git_add_guard_allows_multiline_message_mentioning_the_state_dir() {
    let (_temp, hook) = setup_hook();
    // Describing .work in a commit body is not staging it.
    let cmd = "git add loom/src/fs/mod.rs\n\
               git commit -q -m \"fix(fs): correct state path handling\n\
               \n\
               The loader resolved .work/stages relative to the wrong root.\"";
    assert_eq!(run_hook(&hook, cmd), 0);
}

#[test]
fn git_add_guard_allows_paths_containing_a_hyphen_a_token() {
    let (_temp, hook) = setup_hook();
    // `-A` must be its own argument; it is a substring here, not a flag.
    assert_eq!(run_hook(&hook, "git add src/-Analysis.rs"), 0);
}

#[test]
fn git_add_guard_allows_workspace_lookalike_paths() {
    let (_temp, hook) = setup_hook();
    // .workspace / .working must not be mistaken for .work
    assert_eq!(run_hook(&hook, "git add .workspace/config.toml"), 0);
}
