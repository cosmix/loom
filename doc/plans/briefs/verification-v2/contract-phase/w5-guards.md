# contract-phase / W5 — zero-test guard, completion hook, contract lints, fixture plans

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D4 (the three rows marked stage
`contract-phase`), D9, D10. Knowledge:
`architecture/token-accounting-and-receipts.md` (the certified criterion cache section).

Pinned from W4: `crate::verify::contracts::completion::check(stage, work_dir, acceptance_dir, worktree_root) -> Result<()>`.

## Files you own

`loom/src/verify/criteria/{criterion_eval,cache_contract,runner}.rs`,
`loom/src/verify/criteria/zero_tests.rs` (new), `loom/src/verify/criteria/tests/zero_test_tests.rs`
(new), `loom/src/verify/criteria/tests/mod.rs`, `loom/src/commands/stage/complete_verification.rs`,
`loom/src/plan/schema/validation/v2_lints/mod.rs`,
`loom/src/plan/schema/validation/v2_lints/contracts.rs` (new),
`loom/src/plan/schema/tests/v2_contract_lint_tests.rs` (new), `loom/src/plan/schema/tests/mod.rs`,
`loom/tests/fixtures/plans/v2-valid.md`, `loom/tests/fixtures/plans/v1-uses-contracts.md`.

## Tasks

1. Zero-test guard (D10). `zero_tests.rs::zero_test_failure(command: &str, cwd: &Path, result: &CriterionResult) -> Option<String>`
   uses `testrun::registry::recognize` and `parse`. It applies to v2 stages only: thread the
   stage's `plan_version` into the criteria config where `run_acceptance_with_config`
   (runner.rs:23-49) already has the stage. `criterion_eval::check_criterion` (L9-24) consults it
   for both Simple and Extended criteria. Bump `CACHE_RECORD_VERSION` (cache_contract.rs:17)
   to 3 and add `tests_executed: Option<u64>` to the certified verdict, so a cached pass
   certifies the zero-test decision too.
2. Completion hook: in `complete_verification::run` (L36-49), for `stage.plan_version == 2` and
   standard stages with contracts, call `crate::verify::contracts::completion::check(..)` after
   `run_goal_checks`. Keep `run` short: add one call to a helper in this file.
3. Lints (D4 rows 11-13) in `v2_lints/contracts.rs`, registered in `v2_lints/mod.rs`'s `run`:
   - unknown `runner` → `error_in_v2`;
   - `runner` absent and detection (`skills::project::ProjectProfile::discover(repo_root)` then
     `package_details()`, owning package of `file`) gives no adapter → warning;
   - a v2 integration-verify stage with no acceptance command for which
     `registry::recognize(cmd, cwd)` is some adapter whose `is_full_run(argv)` holds →
     `error_in_v2`.
4. Re-run `loom plan verify --strict` on `loom/tests/fixtures/plans/v2-valid.md`
   (`cargo run -q --manifest-path loom/Cargo.toml -- plan verify --strict <path>`) and adjust
   both fixture plans until `v2-valid.md` passes with zero warnings and `v1-uses-contracts.md`
   still fails with ``requires `version: 2` ``.

## Named tests (binding)

- `zero_test_criterion_fails_in_v2` (`verify/criteria/tests/zero_test_tests.rs`): a v2 stage
  whose criterion prints the captured `cargo-test/no-match.stdout` (read from
  `loom/src/testrun/fixtures/`) and exits 0 → the criterion fails with
  `selected zero tests (cargo-test)`.
- `zero_test_criterion_passes_in_v1`: the same with a v1 stage → passes, as today.
- `contract_with_unknown_runner_is_rejected` (`plan/schema/tests/v2_contract_lint_tests.rs`,
  declared in `tests/mod.rs`).
- `v2_iv_without_full_test_command_is_rejected` (same file): an IV acceptance of only
  `cargo test --lib x::` → error; adding `cargo test --all-targets` clears it.

## Proof (one command, once, after W4 reports)

`cargo test --manifest-path loom/Cargo.toml --lib verify::criteria::`

## Report

Files changed; whether `complete_verification.rs` stays under 400 lines; the fixture plans'
`plan verify` output; the proof result.
