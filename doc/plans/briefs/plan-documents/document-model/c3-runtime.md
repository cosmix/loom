# C3: runtime paths

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first.

## Files owned

`loom/src/planning/runtime.rs`, `loom/src/planning/tests/runtime_tests.rs` (seeded; declared).

## Files to read

`doc/plans/briefs/plan-documents/document-model/w1-discovery.md` section 5 (the rules, including the LOOM_PLAN_STATE provenance checks), common.md "Runtime paths", `loom/src/user_config/mod.rs` around line 154 (`config_path`), `loom/src/context/source_graph/mod.rs` line 127 (`body_hash`).

## Entry points

`project_key`, `RuntimePaths` (fields planning_root, dir, lock, drafts, claims, summaries_lock, summary_cost, sessions), `runtime_paths_under`, `runtime_paths_from(state, session, loom_dir, root)`, `runtime_paths(root)`, `server_runtime_paths(root)`.

## Steps

1. Write `runtime.rs` (pure path computation; it creates nothing). The LOOM_PLAN_STATE checks use `std::fs::symlink_metadata`, `std::os::unix::fs::MetadataExt::uid`, `nix::unistd::getuid` and `nix::unistd::access(parent, nix::unistd::AccessFlags::W_OK)`; every refusal is an anyhow error whose message names LOOM_PLAN_STATE and the failed rule.
2. Write `runtime_tests.rs`: key sanitising and the 12-hex hash, two directories with one name giving two keys, every acceptance and refusal case of the frozen contract runtime_dir_from_session_state (read-only parent via chmod 0o555 with std::os::unix::fs::PermissionsExt, writable parent, missing session, relative path, symlink, wrong key), and `server_runtime_paths` ignoring the variable (use `serial_test::serial` and restore the environment).
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::runtime_tests::` and it passes.

## Traps

Restore chmod on test directories before TempDir drops them, or cleanup fails. Never read the real home directory in a test: pass loom_dir explicitly to runtime_paths_under and runtime_paths_from.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines
  (cargo fmt grows them; the hard limits are 400 and 50).
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment (the gate runs cargo doc with warnings as errors).
- Touch no file outside the Files owned list. Library code returns anyhow::Result and never unwraps
  outside tests.
