//! Guard two common textual forms of direct loom binary spawning.
//!
//! The binary these tests spawn is not built with `cfg(test)`, so it reads
//! its `LOOM_*` session variables straight from the real process environment.
//! Each file listed in [`SANCTIONED_SPAWNERS`] scrubs those before every
//! spawn (see its doc comment); a raw call to `Command::new` around
//! `env!("CARGO_BIN_EXE_loom")` elsewhere skips that scrub and would inherit
//! whatever `LOOM_*` identity the `cargo test` process itself happens to be
//! running under — including this very suite's own loom session. Aliases and
//! other spellings are outside this textual guard's coverage.

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
/// `hook_false_positive_contracts::ledger`'s `blocks_report`, and
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
];

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
