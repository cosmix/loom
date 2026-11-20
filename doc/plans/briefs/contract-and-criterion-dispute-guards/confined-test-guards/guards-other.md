# Brief: guards-other (stage confined-test-guards)

## Goal

The confined-launch tests outside `loom/src/verify/` skip when a confined command cannot start.

## Files you own

- `loom/src/commands/stage/contracts/freeze_tests.rs`
- `loom/src/commands/stage/contracts/freeze_run_dir_tests.rs`
- `loom/tests/e2e/context_variables.rs`
- `loom/tests/token_optimization_contracts.rs`
- `loom/tests/completion_gates_contracts.rs`
- `loom/tests/gate_dedup_contracts.rs`
- `loom/tests/integration/confined_loom_runtime.rs`

Edit test code only. The stage's frozen contract files are read-only.

## The guard

Inside the crate (`loom/src/**`):

```rust
if !crate::process::sandbox_probe::confined_commands_runnable() {
    return;
}
```

In integration tests (`loom/tests/**`) the path is `loom::process::sandbox_probe::confined_commands_runnable()`; confirm the module is public from the crate root (`rg -n 'pub mod process' loom/src/lib.rs`, `rg -n 'pub mod sandbox_probe' loom/src/process/mod.rs`) and that an existing integration test already calls it (`loom/tests/host_verification_contracts.rs:179`). The helper prints the skip line and panics under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`. Put the guard first in the test body.

## Which tests

A test needs the guard when it reaches a confined launch: `run_acceptance*`, `run_single_criterion*`, `run_spec_*`, a freeze or completion run that executes a command (for example `cargo fmt --check` through the format gate), a spawned `loom check`, `sandbox-exec` or `bwrap`. Reported locations (confirm each by reading):

| File | Tests |
| --- | --- |
| `commands/stage/contracts/freeze_tests.rs` | `:126-186` |
| `commands/stage/contracts/freeze_run_dir_tests.rs` | `:113` |
| `tests/e2e/context_variables.rs` | 6 |
| `tests/token_optimization_contracts.rs` | 3 |
| `tests/completion_gates_contracts.rs` | `:361` |
| `tests/gate_dedup_contracts.rs` | `:61` (spawns `loom check`) |
| `tests/integration/confined_loom_runtime.rs` | `:13` |

Change no assertion. A test that passes `CommandConfinement::Inherit` needs no guard.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --all-targets --no-run` compiles.
- Report, per file, the tests you guarded and any launching test you found in these files that the table lacks.
