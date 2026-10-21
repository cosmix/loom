# W3: Plan Lints for Every Runner

Tier: sonnet. Files owned: `src/plan/schema/validation_suite.rs` and `src/plan/schema/tests/validation_suite_tests.rs`. Read `common.md` first.

## Current state (`1b5a0c91`)

- `plan/schema/validation_suite.rs` detects only an unfiltered `cargo test` or `cargo nextest run`.
  - `is_full_suite_run` (:106) is a hand tokenizer built on `SCOPING_OPTIONS`, `VALUE_TAKING_OPTIONS` (:15) and `post_dash_is_scoped` (:54).
  - `warn_full_suite_outside_integration_verify` (:148) returns early for `StageType::IntegrationVerify`. Otherwise it warns "Acceptance criterion #{idx+1} runs the whole test suite ... leave the unfiltered suite to the integration-verify stage".
  - Its caller is the last statement of `warn_ungrantable_acceptance` in `validation.rs`, routed there to keep a ledgered function line-neutral. Do not move the call.
- Every adapter already answers `recognizes(argv)` and `is_full_run(argv)` (`testrun/mod.rs:92,96`). The registry is `testrun/registry.rs`, and prefixes are stripped in `testrun/recognize.rs`.

## Steps

1. Replace the cargo-only tokenizer in `is_full_suite_run`:
   - Split the criterion into words the way `testrun::registry::recognize` expects.
   - Find the first adapter that recognises it.
   - Return that adapter's `is_full_run`.

   Keep the existing `--no-run` exclusion if an adapter does not already cover it. Delete the tokenizer constants the new path orphans, keeping any the existing tests still need through the public function. Change the warning text only by adding the adapter name: `... runs the whole test suite (<adapter>) ...`.
2. Add `warn_subsumed_filtered_runs` in the same file, called from `warn_full_suite_outside_integration_verify`'s module entry so `validation.rs` stays untouched. For an integration-verify stage:
   - when one criterion is a full run of adapter A;
   - and another criterion is recognised by A and is not a full run;
   - and both run in the same `working_dir` (criteria share the stage's `working_dir`, so this means the same stage);

   then warn: "Acceptance criterion #<n> is a filtered <A> run the full-suite criterion #<m> already covers; drop it, or move the proof into stdout_contains on #<m>". Use the 1-based numbering the existing warning uses.
3. Tests in `validation_suite_tests.rs`:
   - full and filtered runs for at least cargo test, vitest (`bunx vitest run` vs `bunx vitest run src/a.test.ts`), pytest (`uv run pytest` vs `uv run pytest tests/test_a.py`), jest, and go test (`go test ./...` vs `go test ./pkg/a`);
   - the subsumed warning fires for an integration-verify stage with `cargo test` plus `cargo test --lib foo::`;
   - the subsumed warning does not fire for a standard stage, or for two different runners.
   Keep the existing five tests passing; adjust only the assertion text the adapter-name addition changes.

## Contract you must turn green

`plan-verify-flags-full-vitest-suite-in-standard-stage`.

## Proof

`cargo test --lib plan::schema::`, `cargo test --test gate_dedup_contracts plan_verify`.
