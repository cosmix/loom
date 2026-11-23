# verification-evidence / w1-freeze-red: a frozen red needs a failing test or a compiler rejection

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/verification.md`, section E1.

## Findings covered

- E1: `judge_run` accepts a build that stopped before the compiler (toolchain or boundary failure) as a `build_failed` red, and accepts a run with no exit code (killed by a signal), so a contract can freeze red without proving anything about the code.

## Files

Own (write):
- `loom/src/verify/contracts/freeze.rs`
- `loom/src/verify/contracts/freeze/prepared.rs`
- `loom/src/verify/contracts/freeze_environment_tests.rs`
- `loom/src/commands/stage/contracts/freeze_tests.rs`
- `loom/src/testrun/adapters/bun_test.rs` (one test call only)
- `loom/src/host_verify/freeze_memo.rs` (module doc comment only)

Read only: `loom/src/testrun/adapters/cargo_test.rs` (`parse`, `compile_rejected`), `loom/src/testrun/adapters/cargo_nextest.rs` (`compile_rejected`), `loom/src/testrun/mod.rs` (`TestRunnerAdapter::compile_rejected`, default = `build_failed`), `loom/src/testrun/outcome.rs` (`classify`), `loom/src/verify/contracts/refusal.rs` (`run_output_line`), `loom/src/models/dispute.rs` (`tail_to_byte_limit`), `loom/src/testrun/recognize.rs` (`combined_output`).

Frozen (contract tests, never edit): `loom/src/verify/contracts/freeze_red_contract_tests.rs`.

## Changes

### 1. `judge_run` takes the compiler-rejection verdict (`loom/src/verify/contracts/freeze.rs`, `judge_run`, about lines 82-130)

Today: `Some(RunOutcome::BuildFailed) => BUILD_FAILED` is accepted whatever stopped the build, and `Some(RunOutcome::Unparsed) | None if exit != Some(0)` accepts `exit == None`.

New signature (pinned; the contract session and other callers rely on it):

```rust
pub(crate) fn judge_run(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    summary: RunSummary,
    exit: Option<i32>,
    streams: Option<(&str, &str)>,
    compile_rejected: bool,
) -> Result<ContractRunReport, String>
```

Behaviour, in this order:
1. The existing `match` stays as it is for `Failed`, `Passed`, `NotSelected` and the two `Unparsed | None` arms.
2. The `BuildFailed` arm: when `compile_rejected` is false, return `Err(stopped_build_refusal(id, streams))`; when true, the outcome is `BUILD_FAILED` as today.
3. After the `match`, for every outcome it accepted: when `exit.is_none()`, return
   `Err(format!("contract `{id}` ended without an exit code (a signal stopped it), so its red proves nothing about the implementation: run the freeze again"))`.
   This covers a counted failure, a compiler rejection and the unparsed case alike.
4. `refuse_environment_red` stays as it is (skipped for `BUILD_FAILED`).

Add a private helper in `freeze.rs`:

```rust
fn stopped_build_refusal(id: &str, streams: Option<(&str, &str)>) -> String
```

Text: `contract `{id}`: the build stopped before the compiler judged the code (a toolchain or boundary failure), so its red proves nothing about the implementation; its output ends:\n{quoted}`. `{quoted}` is `run_output_line(&tail_to_byte_limit(&combined_output(&RunOutput { stdout, stderr, exit_code: None }), STOPPED_BUILD_TAIL_BYTES))` built from `streams`, or the word `(no output)` when `streams` is `None`. Move the constant `STOPPED_BUILD_TAIL_BYTES` (2048) from `prepared.rs` into `freeze.rs`. Keep `judge_run` at 50 lines or fewer. Update the doc comment of `judge_run` to state the new rule: a red is a failing test, a compiler rejection, or a non-zero exit the runner could not read; a stopped build and a missing exit code are refused.

### 2. `judge_prepared_result` computes the verdict first (`loom/src/verify/contracts/freeze/prepared.rs`, lines 94-190)

- Make it `pub(crate) fn judge_prepared_result(contract: &ContractSpec, adapter: Option<&dyn TestRunnerAdapter>, run: &CriterionResult, emit: fn(&str)) -> JudgedRun` (signature otherwise unchanged) and re-export it from `freeze.rs` next to the existing line: `pub(crate) use prepared::{judge_prepared_result, prepare_contract, run_prepared, JudgedRun, PreparedContract};`. The contract tests call it at `crate::verify::contracts::freeze::judge_prepared_result`.
- Compute `let compile_failed = adapter.is_some_and(|adapter| adapter.compile_rejected(&output));` before `judge_run` and pass it as the new last argument. With no adapter it is `false`; `classify` never runs then, so `BuildFailed` cannot occur.
- Delete `warn_if_build_stopped` and the `STOPPED_BUILD_TAIL_BYTES` constant here; drop the now unused imports (`tail_to_byte_limit`, `combined_output`, `BUILD_FAILED` if unused). `warn_if_unverified` stays.

### 3. Callers of `judge_run` (pass the new argument)

- `prepared.rs`, as above.
- `loom/src/verify/contracts/freeze_environment_tests.rs`, helper `judge` (line 28): pass `summary.build_failed` as `compile_rejected` (read it before moving `summary`). No assertion changes.
- `loom/src/commands/stage/contracts/freeze_tests.rs` lines 58, 64, 70, 75, 77: pass `false`. No assertion changes.
- `loom/src/testrun/adapters/bun_test.rs` line 233 (test `bun_selects_only_the_exact_nested_name_when_available`): pass `false`.
- `rg -n 'judge_run\(' loom/src loom/tests` must show no other caller.

### 4. Stale comment (`loom/src/host_verify/freeze_memo.rs`, module doc lines 4-9)

It says a transient failure "reds its own contract only". After this change such a failure is refused. Rewrite that sentence: only a compile rejection is shared; a build the toolchain or the boundary stopped is refused by the freeze (`judge_run`), so it never reaches the memo. Change nothing else in the file.

## Interfaces shared with other workers

None. The pinned `judge_run` and `judge_prepared_result` signatures above are used by this stage's contract file only.

## Tests

- `loom/src/verify/contracts/freeze/prepared.rs`, `mod tests`, helper `judged_lines(stderr)` (line 236, cargo-test adapter, exit 101):
  - Replace `a_build_stopped_before_the_compiler_is_warned_with_its_output` with `a_build_stopped_before_the_compiler_is_refused_with_its_output`: same stderr (`error: failed to run `rustc` to learn about target-specific information`); assert `judged.judged` is `Err` containing `before the compiler judged the code` and `failed to run `rustc` to learn about target-specific`, that the quoted output is one line (the message has exactly two lines), `!judged.compile_failed`, and that `lines` is empty. Fails today: the outcome is `BUILD_FAILED`.
  - Keep `a_compile_rejection_is_not_warned` unchanged; add `assert_eq!(judged.judged.unwrap().outcome, BUILD_FAILED)` to it.
  - Add `a_counted_failure_without_an_exit_code_is_refused`: build a `CriterionResult::new("cargo test", false, <libtest output with `running 1 test` and `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out`>, "", None, Duration::ZERO, false)` and call `judge_prepared_result`; assert the error contains `without an exit code`. Fails today: the outcome is `failed`.
- `loom/src/commands/stage/contracts/freeze_tests.rs`: add `an_unparsed_run_without_an_exit_code_is_refused`: `judge_run(&spec(), None, RunSummary::default(), None, None, false)` is `Err` containing `without an exit code`. Fails today (`UNVERIFIED`).
- Contracts this worker turns green: `stopped-build-is-not-a-red`, `compiler-rejection-stays-a-red`, `signal-killed-run-is-not-a-red`.

## Proof

From `loom/`:

```bash
cargo check --all-targets
cargo test --lib verify::contracts::freeze
cargo test --lib commands::stage::contracts::freeze
cargo test --lib testrun::adapters::bun_test
cargo test --lib host_verify::tests_freeze
```

## Traps

- Do not refuse every `BuildFailed`: a real compiler rejection (`could not compile`) is the most common legitimate red for a new contract; `compiler-rejection-stays-a-red` guards it.
- Do not decide "stopped build" inside `judge_run` by matching text: the adapter's `compile_rejected` owns that reading (cargo-test and cargo-nextest override it; every other adapter defaults to `build_failed`, so their behaviour does not change).
- The missing-exit check must cover the `Failed` arm too, not only the unparsed one.
- `freeze_environment_tests.rs` must keep `a_build_failure_stays_an_accepted_red_even_with_a_marker_in_its_output` passing: its helper passes `summary.build_failed` as `compile_rejected`.
- Report the replaced test name to the orchestrator: it is an integrity event.
