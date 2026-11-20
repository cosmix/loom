# Brief: guards-verify (stage confined-test-guards)

## Goal

Every test under `loom/src/verify/` that launches a confined command skips when a confined command cannot start, through one helper.

## Files you own

- `loom/src/verify/criteria/tests/**`
- `loom/src/verify/criteria/confine_host_tests.rs`, `confine_host_tool_tests.rs`, `confine_seatbelt_tests.rs`, `confine_stage_cache_tests.rs`, `confine_cargo_home_tests.rs`, `confine_git_tests.rs`
- `loom/src/verify/goal_backward/**`
- `loom/src/verify/impact_tests/runs_tests.rs`
- `loom/src/verify/completion/resolved_tests.rs`
- `loom/src/verify/contracts/format_gate_tests.rs`

Edit test code only. The stage's frozen contract files are read-only. `loom/src/verify/criteria/confine.rs` and `mod.rs` belong to another worker.

## The guard

```rust
if !crate::process::sandbox_probe::confined_commands_runnable() {
    return;
}
```

`confined_commands_runnable()` (`loom/src/process/sandbox_probe.rs:157`) prints the skip line itself and panics under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`. Another worker is giving it a real macOS probe; its signature does not change. Put the guard first in the test body, before any fixture is built.

## Which tests

A test needs the guard when it, or a helper it calls, reaches a confined launch: `run_acceptance*`, `run_single_criterion*`, `run_spec_*`, `spawn_confined`, `run_provision_with_timeout`, a spawn of a `prepare_confined(..)` result, `sandbox-exec` or `bwrap`. `CommandConfinement::default()` is `Confined` (`criteria/executor.rs:45-47`, `runner.rs:34-35`). A test that passes `CommandConfinement::Inherit`, or that only builds a command or argv without spawning it, needs none.

Inventory (counts are approximate, from a script; confirm each by reading):

| Today | Files | Tests |
| --- | --- | --- |
| guard is `host_launcher().is_some()` or `super::launcher()` | `confine_host_tests.rs` 5, `confine_host_tool_tests.rs` 4, `confine_seatbelt_tests.rs` 4 (`launcher_absent`, `:31`), `confine_stage_cache_tests.rs` 3, `confine_cargo_home_tests.rs` 3, `confine_git_tests.rs` 2, `criteria/tests/cache_git_state_tests.rs` 1, `impact_tests/runs_tests.rs` 1 (`:80`), `completion/resolved_tests.rs` 1 (`:128`) | 24 |
| no guard | `criteria/tests/`: `cache_input_runner` 10, `runner_tests` 11, `confine_tests` 7, `setup` 3, `executor` 3, `go_evidence` 3, `shared_assertion` 3, `variable` 5, `working_dir` 1, `zero_test` 2; `goal_backward/`: `dead_code` 5, `dead_code_execution_tests` 3, `truths` 9, `wiring_tests` 5; `contracts/format_gate_tests.rs` 2 (`:94`, `:126`) | about 72 |

- Where a file already has a guard helper (`launcher_absent` and the like), change the helper's condition to the probe and keep its call sites; a launcher that exists but cannot start must skip.
- Keep every existing `#[cfg(target_os = ...)]`.
- Tests that call `prepare_confined` without spawning (`criteria/tests/cache_tests.rs`) need the launcher installed and nothing else: leave them.
- Change no assertion.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib verify::criteria:: --no-run` and `... --lib verify::goal_backward:: --no-run` compile.
- Run both filters in your shell and report, per file, how many tests ran and how many printed `SKIP`. In a sandboxed shell most will skip once the probe worker's change is in; until then report what you see.
- Report every launching test you could not classify, with `file:line`.
