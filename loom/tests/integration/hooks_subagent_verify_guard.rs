//! Integration tests for the subagent-verify-guard PreToolUse:Bash hook.
//!
//! The hook blocks a SUBAGENT (a Claude Code process running under a main
//! agent, per the same detector `commit-filter.sh` uses) from running
//! project-wide verification (`cargo test`, `cargo clippy`, etc). Main
//! agents are unaffected; scoped runs are still allowed; subagents running
//! inside an `integration-verify` stage are exempted (they are supposed to
//! run the full suite); git commits are never blocked by this hook even
//! when their message body mentions a verification command.
//!
//! GOTCHA 1 (temp dir): do NOT use the default temp directory
//! (`TempDir::new()` / `std::env::temp_dir()`) for the process trees below.
//! In this harness `$TMPDIR` is `/tmp/claude-1000`, and the detector matches
//! "claude" case-insensitively anywhere in a process's cmdline (it only
//! excludes `.claude/hooks`). A temp dir under `/tmp/claude-1000/...` makes
//! EVERY script's cmdline contain "claude" - including the hook script
//! itself - which collapses the "claude"-named vs plain-named distinction
//! the process-tree simulation below depends on. Tests here build their temp
//! directory under `CARGO_MANIFEST_DIR/target/` instead, which is free of
//! the substring "claude" - and assert that at runtime (see
//! `temp_dir_no_claude`), since a checkout path containing "claude" would
//! silently reproduce the same collapse.
//!
//! GOTCHA 2 (env leak): `run_hook_in_tree` spawns real child processes via
//! `Command::new("bash")`, which inherits the *entire* environment of the
//! test runner - including `LOOM_STAGE_ID` / `LOOM_WORK_DIR` if this very
//! `cargo test` process happens to run inside a real loom stage session
//! (e.g. this suite's own integration-verify stage). If left inherited,
//! those leaked values point at a REAL stage file, the integration-verify
//! carve-out spuriously fires, and every "must be blocked" case in this file
//! silently turns into a false pass. `run_hook_in_tree` explicitly clears
//! both vars before applying any `extra_env` the caller supplies, so only
//! `subagent_in_integration_verify_stage_allowed` (which sets them back
//! deliberately) ever exercises the carve-out.

use loom::fs::permissions::constants::{HOOK_COMMON, HOOK_SUBAGENT_VERIFY_GUARD};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Create a temp dir under `loom/target/` (never under `$TMPDIR`) so the
/// path never contains the substring "claude" - see module doc comment,
/// GOTCHA 1. Asserts the invariant instead of silently trusting it, since a
/// checkout path containing "claude" would otherwise fail these tests with a
/// baffling "main agent was blocked" message far from the real cause.
fn temp_dir_no_claude() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    assert!(
        !base.to_string_lossy().to_lowercase().contains("claude"),
        "CARGO_MANIFEST_DIR ({}) contains \"claude\" - the claude-name-based \
         process-tree simulation in this file would misclassify every \
         process under it (including the hook script itself) as a \
         claude-named ancestor, collapsing the main-agent vs subagent \
         distinction these tests depend on. Move the checkout to a path \
         without \"claude\" in it.",
        base.display()
    );
    fs::create_dir_all(&base).expect("create target dir");
    tempfile::Builder::new()
        .prefix("subagent-verify-guard-test-")
        .tempdir_in(&base)
        .expect("create temp dir")
}

fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// Install the hook script and its `_common.sh` dependency into `dir`.
/// Mirrors `hooks_commit_filter.rs::setup_hook()` (knowledge mistakes.md,
/// "Hook Integration Tests Need _common.sh").
fn install_hook(dir: &Path) -> PathBuf {
    write_exec(&dir.join("_common.sh"), HOOK_COMMON);
    let hook_path = dir.join("subagent-verify-guard.sh");
    write_exec(&hook_path, HOOK_SUBAGENT_VERIFY_GUARD);
    hook_path
}

/// Build a fake Claude-Code process tree and run the hook at its leaf.
///
/// `subagent == false` builds a 2-level "claude"-named chain
/// (claude-outer.sh -> claude-mid.sh -> hook), which the detector treats as
/// the MAIN agent (LOOM_MAIN_AGENT_PID is the nearest claude-matching
/// ancestor). `subagent == true` builds a 3-level chain (adds
/// claude-inner.sh), which the detector treats as a SUBAGENT (one
/// claude-matching process sits between the hook and LOOM_MAIN_AGENT_PID).
/// Empirically verified against the (unmodified) detector logic in
/// commit-filter.sh before writing these tests - see loom memory.
///
/// stdin receives `json_input`; it is inherited down the process chain.
/// `LOOM_STAGE_ID`/`LOOM_WORK_DIR` are cleared from the inherited
/// environment before `extra_env` is applied - see module doc, GOTCHA 2.
/// Returns (exit_code, stderr).
fn run_hook_in_tree(
    dir: &Path,
    hook_path: &Path,
    subagent: bool,
    json_input: &str,
    extra_env: &[(&str, &str)],
) -> (i32, String) {
    let hook_display = hook_path.display();

    if subagent {
        write_exec(
            &dir.join("claude-inner.sh"),
            &format!("#!/usr/bin/env bash\nbash \"{hook_display}\"\n"),
        );
        write_exec(
            &dir.join("claude-mid.sh"),
            &format!(
                "#!/usr/bin/env bash\nbash \"{}\"\n",
                dir.join("claude-inner.sh").display()
            ),
        );
    } else {
        write_exec(
            &dir.join("claude-mid.sh"),
            &format!("#!/usr/bin/env bash\nbash \"{hook_display}\"\n"),
        );
    }

    write_exec(
        &dir.join("claude-outer.sh"),
        &format!(
            "#!/usr/bin/env bash\nexport LOOM_MAIN_AGENT_PID=$$\nbash \"{}\"\n",
            dir.join("claude-mid.sh").display()
        ),
    );

    let mut cmd = Command::new("bash");
    cmd.arg(dir.join("claude-outer.sh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // GOTCHA 2: clear ambient loom env before layering on whatever this
        // specific test wants. Must happen before the extra_env loop below.
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_WORK_DIR");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    let mut child = cmd.spawn().expect("spawn process tree");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json_input.as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for process tree");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Build the JSON payload Claude Code sends for a Bash tool call.
fn bash_payload(command: &str) -> String {
    format!(
        r#"{{"tool_name": "Bash", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).unwrap()
    )
}

/// Write a minimal stage file that satisfies the integration-verify
/// carve-out's glob (`${{LOOM_WORK_DIR}}/stages/*-${{LOOM_STAGE_ID}}.md`,
/// the same one session-end.sh uses). Frontmatter shape mirrors a real
/// file under `.work/stages/` (e.g. `02-integration-verify.md`).
fn write_integration_verify_stage(work_dir: &Path, stage_id: &str) {
    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).expect("create stages dir");
    let content = format!(
        "---\nid: {stage_id}\nname: Integration Verification\nstatus: executing\nstage_type: integration-verify\nplan_id: PLAN-test\nworking_dir: .\n---\n\n# Stage: Integration Verification\n"
    );
    fs::write(stages_dir.join(format!("01-{stage_id}.md")), content).expect("write stage file");
}

const BLOCK_MESSAGE_SNIPPET: &str = "AT MOST ONE narrowly-scoped check";

/// Run `command` as a SUBAGENT (no carve-out env set) and return
/// `Some(description)` describing the mismatch, or `None` if it matched
/// `expected_exit` (and, for blocks, contained the guidance snippet).
fn check_subagent_case(
    dir: &Path,
    hook_path: &Path,
    command: &str,
    expected_exit: i32,
    label: &str,
) -> Option<String> {
    let (code, stderr) = run_hook_in_tree(dir, hook_path, true, &bash_payload(command), &[]);
    if code != expected_exit {
        return Some(format!(
            "[{label}] command={command:?} expected exit {expected_exit}, got {code}; stderr={stderr}"
        ));
    }
    if expected_exit == 2 && !stderr.contains(BLOCK_MESSAGE_SNIPPET) {
        return Some(format!(
            "[{label}] command={command:?} blocked but stderr missing {BLOCK_MESSAGE_SNIPPET:?}: {stderr}"
        ));
    }
    None
}

// =============================================================================
// Table-driven: project-wide runners are blocked, scoped runs are allowed.
//
// The cases themselves live in `hooks_subagent_verify_guard_cases.rs` (read its
// module docs before editing one). Failures collect into a single combined
// panic instead of stopping at the first mismatch, so one run shows the whole
// pass/fail picture rather than one case at a time.
// =============================================================================

#[path = "hooks_subagent_verify_guard_cases.rs"]
mod cases;

#[test]
fn subagent_block_and_allow_table() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    // Tables live in a companion file; see its module docs before editing.
    let block_cases = cases::BLOCK_CASES;
    let allow_cases = cases::ALLOW_CASES;

    let mut failures = Vec::new();
    for (command, label) in block_cases {
        if let Some(msg) = check_subagent_case(dir.path(), &hook_path, command, 2, label) {
            failures.push(msg);
        }
    }
    for (command, label) in allow_cases {
        if let Some(msg) = check_subagent_case(dir.path(), &hook_path, command, 0, label) {
            failures.push(msg);
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} table cases failed:\n{}",
        failures.len(),
        block_cases.len() + allow_cases.len(),
        failures.join("\n")
    );
}

// =============================================================================
// Main agent is unaffected (critical property) - paired with the same
// commands blocked for a subagent above, so the suite is non-vacuous: each
// command must exit 0 as main agent and 2 as subagent.
// =============================================================================

#[test]
fn main_agent_project_wide_commands_allowed() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    let mut failures = Vec::new();
    for command in ["cargo test", "cargo build", "cargo clippy", "cargo fmt"] {
        let (code, stderr) =
            run_hook_in_tree(dir.path(), &hook_path, false, &bash_payload(command), &[]);
        if code != 0 {
            failures.push(format!(
                "main agent running {command:?} must never be blocked, got exit {code}; stderr={stderr}"
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// =============================================================================
// Subagent inside an integration-verify stage is unaffected
// =============================================================================

#[test]
fn subagent_in_integration_verify_stage_allowed() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    let work_dir = dir.path().join("work");
    let stage_id = "iv-test-stage";
    write_integration_verify_stage(&work_dir, stage_id);

    let (code, stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        true,
        &bash_payload("cargo test"),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", stage_id),
        ],
    );

    assert_eq!(
        code, 0,
        "a subagent inside an integration-verify stage must not be blocked; stderr={stderr}"
    );
}

// =============================================================================
// Non-vacuous message-stripping check: the "&&" lives INSIDE the quoted -m
// message, so this only passes if strip_embedded_content actually removes
// the message body before tokenization. Without stripping, the quoted "&&"
// re-splits into a real separator token, exposing "cargo build" as a fresh
// command and blocking it - so this genuinely depends on the fix.
// =============================================================================

#[test]
fn subagent_git_commit_message_with_internal_separator_allowed() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    let command = r#"git commit -m "x && cargo build -v""#;
    let (code, stderr) =
        run_hook_in_tree(dir.path(), &hook_path, true, &bash_payload(command), &[]);

    assert_eq!(
        code, 0,
        "a git commit whose message body happens to contain '&& cargo build -v' \
         must not be blocked (message body is stripped before pattern matching); \
         stderr={stderr}"
    );
}
