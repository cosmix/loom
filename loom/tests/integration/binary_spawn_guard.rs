//! Guard two common textual forms of direct loom binary spawning.
//!
//! The binary these tests spawn is not built with `cfg(test)`, so it reads
//! its `LOOM_*` session variables straight from the real process environment.
//! Each file listed in [`SANCTIONED_SPAWNERS`] scrubs those before every
//! spawn (see its doc comment); a raw call to `Command::new` around
//! `env!("CARGO_BIN_EXE_loom")` elsewhere skips that scrub and would inherit
//! whatever `LOOM_*` identity the `cargo test` process itself happens to be
//! running under — including this very suite's own loom session. A file that
//! only aliases the binary handle is caught by the second check: every file
//! naming the handle must be sanctioned or listed in [`NON_SPAWNING_REFERENCES`]
//! with the reason it does not spawn unscrubbed.

use std::fs;
use std::path::{Path, PathBuf};

/// Substrings (whitespace stripped, see [`strip_whitespace`]) that spawn the
/// loom binary directly, bypassing `helpers::loom_cmd()`'s environment scrub.
/// Passing the binary's path as an env var for a hook script to read later
/// (e.g. naming the binary's own env-var handle in a `.env(...)` call) is not
/// a direct spawn and does not match either pattern.
///
/// Built from parts at runtime rather than as string literals: a literal
/// copy of either needle would make this very file — which the test below
/// also scans — match itself.
fn forbidden_patterns() -> Vec<String> {
    let cargo_bin_exe_loom = ["CARGO_BIN_", "EXE_loom"].concat();
    vec![
        ["Command::new(env!(\"", &cargo_bin_exe_loom, "\"))"].concat(),
        ["cargo", "_bin("].concat(),
    ]
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Paths (relative to `loom/tests/`, using `/` separators) exempted from the
/// two textual patterns above. Each one owns a spawn helper
/// that removes every `LOOM_*` variable from the child's environment before
/// every spawn: `helpers::loom_cmd()`, `worker_evidence::support`'s
/// `cli_command`/`configure`, and `codex_evidence::fixture_runtime`'s
/// `cli_command`/`configure_child`, and `subagent_owned_wait::support`'s
/// `cli_command`, `merge_off_checkout_cli`'s `merge_resolved`,
/// `planning_check_cli_contracts`'s `check`, `plan_server_cli`'s `command`,
/// `plan_server_contracts`'s `spawn_loom_plan`, `file_mode_contracts`'s
/// `verify_text_agrees_with_verify_json`, `planning_summary_support`'s
/// `loom_summary`, `planning_guidance_contracts`'s
/// `template_skeleton_carries_implements_and_verifies`,
/// `pressure_pipeline_contracts::support`'s `run_loom`,
/// `plan_handoff_operator`'s `loom` closure,
/// `plan_handoff_operator::common`'s `spawn_loom_plan`,
/// `memory_spool_query`'s `memory_cli`,
/// `hook_false_positive_contracts::ledger`'s `blocks_report`,
/// `web_host`'s `loom_cmd` (scrubs through `test_support::clear_loom_env`), and
/// `plan_owner_watch_session`, which has no helper and scrubs inline in its
/// test body before it spawns the `OwnerWatch` hook. A new test target that
/// spawns the binary directly must scrub the same way and be added to this
/// list.
const SANCTIONED_SPAWNERS: &[&str] = &[
    "integration/helpers.rs",
    "worker_evidence/support.rs",
    "codex_evidence/fixture_runtime.rs",
    "subagent_owned_wait/support.rs",
    "merge_off_checkout_cli.rs",
    "planning_check_cli_contracts.rs",
    "plan_owner_watch_session.rs",
    "plan_server_cli.rs",
    "plan_server_contracts.rs",
    "file_mode_contracts.rs",
    "planning_summary_support/mod.rs",
    "planning_guidance_contracts.rs",
    "pressure_pipeline_contracts/support.rs",
    "plan_handoff_operator.rs",
    "plan_handoff_operator/common.rs",
    "memory_spool_query.rs",
    "hook_false_positive_contracts/ledger.rs",
    "web_host.rs",
];

/// Files (relative to `loom/tests/`) that name the binary handle without a
/// direct spawn that skips the `LOOM_*` scrub, each with the reason.
const NON_SPAWNING_REFERENCES: &[(&str, &str)] = &[
    (
        "plan_session_sandbox_contracts/support.rs",
        "alias; spawns only through `loom_cmd`, which scrubs every `LOOM_*`",
    ),
    (
        "plan_session_confinement/fixture.rs",
        "alias; the scrub is done by `Confined::run`",
    ),
    (
        "worktree_cleanup_guidance.rs",
        "let binding; its `run` scrubs `LOOM_*`",
    ),
    (
        "planning_summary_seatbelt.rs",
        "scrubs `LOOM_*` before its spawn",
    ),
    (
        "plan_redesign_contracts.rs",
        "path handed to library code, no direct spawn",
    ),
    (
        "plan_session_bridge_e2e.rs",
        "path handed to library code, no direct spawn",
    ),
    (
        "host_verification_contracts/support.rs",
        "path handed to the host-verify runner, which removes session variables; \
         its own spawns go through `helpers::loom_cmd`",
    ),
    (
        "integration/hooks_skill_trigger_fixture.rs",
        "`LOOM_BIN` hand-off for a hook script, no spawn",
    ),
    (
        "integration/hooks_skill_project.rs",
        "`LOOM_BIN` hand-off for a hook script, no spawn",
    ),
];

/// Whether `content` names the binary's env-var handle in any spelling,
/// including an alias that a spawn pattern would not match. Built from parts,
/// like [`forbidden_patterns`], so this file never holds the literal.
fn mentions_binary_handle(content: &str) -> bool {
    content.contains(&["CARGO_BIN_", "EXE_loom"].concat())
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_sanctioned_spawners_spawn_the_loom_binary_directly() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    rust_files_under(&tests_dir, &mut files);

    let forbidden: Vec<String> = forbidden_patterns()
        .iter()
        .map(|pattern| strip_whitespace(pattern))
        .collect();

    let offenders: Vec<String> = files
        .into_iter()
        .filter(|path| {
            let relative = path
                .strip_prefix(&tests_dir)
                .expect("file under tests_dir")
                .to_str()
                .expect("test file path is valid UTF-8");
            !SANCTIONED_SPAWNERS.contains(&relative)
        })
        .filter_map(|path| {
            let content = fs::read_to_string(&path).ok()?;
            let normalized = strip_whitespace(&content);
            forbidden
                .iter()
                .any(|pattern| normalized.contains(pattern.as_str()))
                .then(|| path.display().to_string())
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "these files spawn the loom binary directly instead of going through \
         a sanctioned spawn helper (see SANCTIONED_SPAWNERS), so a real \
         LOOM_* session variable would leak into the child process \
         unfiltered: {offenders:?}"
    );
}

#[test]
fn every_use_of_the_binary_handle_is_accounted_for() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    rust_files_under(&tests_dir, &mut files);

    let unaccounted: Vec<String> = files
        .iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(&tests_dir)
                .expect("file under tests_dir")
                .to_str()
                .expect("test file path is valid UTF-8");
            let listed = SANCTIONED_SPAWNERS.contains(&relative)
                || NON_SPAWNING_REFERENCES
                    .iter()
                    .any(|(listed, _)| *listed == relative);
            let own_file = relative == "integration/binary_spawn_guard.rs";
            let content = fs::read_to_string(path).ok()?;
            (!own_file && !listed && mentions_binary_handle(&content)).then(|| relative.to_owned())
        })
        .collect();
    assert!(
        unaccounted.is_empty(),
        "these files name the loom binary handle but are in neither \
         SANCTIONED_SPAWNERS nor NON_SPAWNING_REFERENCES; scrub `LOOM_*` before \
         spawning and list them: {unaccounted:?}"
    );

    for (relative, reason) in NON_SPAWNING_REFERENCES {
        let content = fs::read_to_string(tests_dir.join(relative))
            .unwrap_or_else(|e| panic!("{relative} ({reason}) is listed but unreadable: {e}"));
        assert!(
            mentions_binary_handle(&content),
            "{relative} no longer names the binary handle; drop it from NON_SPAWNING_REFERENCES"
        );
    }
}

#[test]
fn an_aliased_handle_is_a_mention() {
    let handle = ["CARGO_BIN_", "EXE_loom"].concat();
    let aliased = format!("const LOOM: &str = env!(\"{handle}\");");
    assert!(mentions_binary_handle(&aliased));
    assert!(!mentions_binary_handle("const LOOM: &str = \"loom\";"));
}
