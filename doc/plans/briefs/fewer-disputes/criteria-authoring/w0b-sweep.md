# criteria-authoring W0b: test literal sweep

Tier: haiku. Wave 1, with W0, W4 and W5. Read `common.md` first (sections "Ownership" and "Tests" only).

## What changed

`ContractSpec.test` is now `Option<String>` and `FrozenContract` has a new field `test: Option<String>`
(W0 writes both types). Every test file that builds either with a struct literal, or reads `.test`, needs a
mechanical edit. You change nothing else.

## Files you own and the edit in each

Edit A = in a `ContractSpec { .. }` literal, wrap the `test` value: `test: "x".into()` becomes `test:
Some("x".into())`, `test: name.to_string()` becomes `test: Some(name.to_string())`. A helper that takes a
`test: &str` parameter keeps its parameter and wraps where it builds the literal.
Edit B = in a `FrozenContract { .. }` literal that does not end in `..FrozenContract::from(..)`, add `test:
None,` after the last field.
Edit C = an assertion or comparison that reads a spec's `.test` as a `String` (`assert_eq!(spec.test,
"x")`) becomes `Some("x".to_string())` or `.as_deref()` against `Some("x")`.

Find the sites with `rg -n 'ContractSpec \{|FrozenContract \{|\.test\b' <file>` in each file:

| File | Edits |
| --- | --- |
| `tests/host_verification_e2e.rs` | A |
| `tests/refusal_adjudication_contracts.rs` | A |
| `tests/gate_dedup_contracts/support.rs` | A, B |
| `tests/adjudication_e2e/kinds.rs` | A |
| `src/verify/contracts/run_dir_build_tests.rs` | A (two literals; helper `spec(adapter, file, test)`) |
| `src/verify/contracts/store_tests.rs` | A, B (four `FrozenContract` literals) |
| `src/verify/contracts/completion_share_tests.rs` | A (two) |
| `src/verify/contracts/completion_tests.rs` | B (four) |
| `src/verify/contracts/completion_adapter_tests.rs` | B |
| `src/verify/contracts/collection/tests.rs` | A |
| `src/verify/contracts/test_support.rs` | A (two: `contract()` and `liquidmon_contract()`) |
| `src/verify/contracts/run_dir_tests.rs` | A |
| `src/plan/schema/validation/v2_lints/contracts_tests.rs` | A |
| `src/plan/schema/tests/v2_contract_lint_tests.rs` | A |
| `src/plan/schema/tests/v2_tests.rs` | A |
| `src/plan/schema/tests/mod.rs` | A |
| `src/models/stage/methods_v2_tests.rs` | A |
| `src/host_verify/tests_freeze_memo.rs` | A (four) |
| `src/commands/stage/contracts/freeze_run_dir_tests.rs` | A, B (two) |
| `src/commands/stage/contracts/freeze_tests.rs` | A |
| `src/orchestrator/adjudication/apply_contract_tests.rs` | A (two), B |
| `src/orchestrator/adjudication/prompt/contract_tests.rs` | B |
| `src/stage_service/dispute_race_tests.rs` | B |
| `src/fs/tests_stage_loading_round_trip.rs` | A, plus the new test below |
| `src/testrun/adapters/bun_test.rs` | A (one literal inside its test module) |

## Two small tests to add (the only additions)

- In `src/fs/tests_stage_loading_round_trip.rs`: a stage with `consumer_deferred: Some("the consumer is
  planned for a later plan".into())` survives the definition round trip the file already tests for other
  fields, and a stage file without the key loads with `None`. Follow the file's existing fixture helpers.
- In `src/verify/contracts/store_tests.rs`: `refreeze_set` with a `RefrozenContract` whose `test` is
  `Some("a::b")` leaves the entry's `test` as `Some("a::b")`, for an existing entry and a new one. Follow the
  neighbouring refreeze tests; `RefrozenContract` has the new field `test: Option<String>`.

## Rules

- Do not change a test's meaning, delete an assertion or rename anything. Do not touch a file not in the table.
- `src/verify/contracts/mod.rs`, `src/plan/schema/types_v2.rs` and `src/stage_service/contracts.rs` hold
  literals too but belong to W0 and W1b; `src/orchestrator/signals/contract_tests.rs` belongs to W3, which
  rewrites the rule text it pins and wraps its `test` values itself.
- Report every assertion line you changed (file:line) so the orchestrator can file the integrity dispute.
- Run no check: the crate cannot compile until wave 2.
