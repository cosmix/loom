# criteria-authoring W0: schema, record and Stage types (foundation)

Tier: sonnet. Wave 1, with W0b, W4 and W5. Read `common.md` first. Frozen contracts:
`tests/writer_choices_contracts.rs`, `tests/plan_lints_contracts.rs` (never edit them).

## Files you own

`src/plan/schema/types_v2.rs`, `src/plan/schema/types.rs`, `src/plan/schema/mod.rs`,
`src/models/stage/types.rs`, `src/models/stage/methods.rs`, `src/models/stage/defaults.rs`,
`src/fs/stage_loading.rs`, `src/verify/contracts/store.rs`, `src/verify/contracts/record_view.rs` (new),
`src/verify/contracts/record_view_tests.rs` (new, wired from `record_view.rs` with `#[cfg(test)] #[path = ".."]
mod tests;`), `src/verify/contracts/mod.rs`.

## Edits

1. `src/plan/schema/types_v2.rs` (117 lines)
   - `ContractSpec.test` becomes `Option<String>` with `#[serde(default, skip_serializing_if =
     "Option::is_none")]`; the doc comment says it is a hint the writer may replace with `--test` and keeps
     the per-runner format sentences (full name for JS, Dart and Flutter; `Class::method` for PHPUnit and
     Pest; `Module.Class/method` for Swift).
   - Add `ContractChoice` (derive `Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize`,
     `#[serde(deny_unknown_fields)]`, three `Option<String>` fields each `#[serde(default,
     skip_serializing_if = "Option::is_none")]`) and `pub type ContractChoices = BTreeMap<String,
     ContractChoice>`.
   - `ContractSpec::with_choice` and `required_test` exactly as `common.md` states. `required_test` errors
     with `contract '<id>' has no test identifier: the writer names it with --test <id>=<identifier>`.
   - Add `deserialize_consumer_deferred` beside `deserialize_reasoning_effort` (same `pub(super)` shape):
     `Option<String>` in; `None` stays `None`; a value is accepted when, trimmed, it is non-empty, at most 300
     bytes and holds no `char::is_control`; otherwise the error is `consumer_deferred must be one line of at
     most 300 bytes with no control characters`. The accepted value is stored trimmed. This is parse-time
     validation on purpose: `validation.rs` is a ledgered file and may not grow.
   - Inline `#[cfg(test)] mod tests` for `with_choice` (all eight combinations of planner-set and
     choice-set runner and cwd, test from the choice winning over a hint), `required_test`, a YAML round trip
     of a spec with no `test`, and the deserializer's four refusals (empty, 301 bytes, a newline, a tab)
     and acceptance (exactly 300 bytes of ASCII).
2. `src/plan/schema/types.rs` and `src/plan/schema/mod.rs`
   - `StageDefinition` gains `consumer_deferred: Option<String>` with `#[serde(default,
     skip_serializing_if = "Option::is_none", deserialize_with =
     "super::types_v2::deserialize_consumer_deferred")]`, documented as the one-line reason no consumer of
     this stage's new units exists anywhere in the plan.
   - `impl StageDefinition { pub fn has_wiring_coverage(&self) -> bool }`: `!wiring.is_empty() ||
     !wiring_tests.is_empty() || !reachable.is_empty()`. W2 replaces two inline conditions in `validation.rs`
     with it.
   - Re-export `ContractChoice` and `ContractChoices` from `mod.rs` beside `ContractSpec`.
3. `src/models/stage/types.rs`, `methods.rs`, `defaults.rs`, `src/fs/stage_loading.rs`
   - `Stage` gains `consumer_deferred: Option<String>` (`#[serde(default, skip_serializing_if =
     "Option::is_none")]`, "Copied from the plan's StageDefinition."). `Stage::from_definition` copies it.
     `Stage::default` sets `None`. `stage_loading.rs` rebuilds the definition with it.
   - Ledger: see the table in `common.md`. `types.rs` must stay at 591 lines, `methods.rs` at 492, the
     `default` function at 76. Count with `wc -l` after editing and fix the difference.
4. `src/verify/contracts/store.rs` (399 lines today; it may not cross 400)
   - `FrozenContract` gains `test: Option<String>` (`#[serde(default, skip_serializing_if =
     "Option::is_none")]`, "The test identifier the freeze ran; `None` for a record that predates the
     field or a refrozen contract that never ran"). `From<&ContractRunReport>` sets `test: None`; the freeze
     handler records it (W1b).
   - `RefrozenContract` gains `test: Option<String>`; `upsert_contract` copies it onto an existing entry and
     into a new one. Read the existing `upsert_contract` and keep its shape.
   - Move `recorded_cwd` (lines 126-147) unchanged into the new `record_view.rs` and re-export it
     (`pub use super::record_view::{effective_contract, recorded_cwd};`) so every `store::recorded_cwd` caller
     compiles unchanged. Make `fn stage_dir` `pub(crate)` (W1b's `choices.rs` needs it; same line count).
5. `src/verify/contracts/record_view.rs` (new)
   - `pub fn effective_contract(record: &FreezeRecord, spec: &ContractSpec) -> ContractSpec`: find the entry
     the way `recorded_cwd` does (same id and file, else same file, else same id with no file); the result
     is `spec.clone()` with `test` = the entry's `test` when it holds one else `spec.test`, and `runner` =
     `spec.runner` else the entry's `adapter`. `cwd` is left as the spec has it: completion already runs
     from `recorded_cwd`. No entry: `spec.clone()`.
   - Tests in `record_view_tests.rs`: planner runner kept over a recorded adapter; recorded test beats a hint
     and fills a missing test; an entry for another file is not used; no entry returns the spec.
6. `src/verify/contracts/mod.rs`: add `mod record_view;` and `pub mod choices;` (W1b writes `choices.rs`
   in wave 2; its absence until then is expected). Do not touch the test module's `ContractSpec` literal at
   `fn contract(file: &str)` beyond wrapping its `test` value in `Some(..)`.

## Traps

- `ContractSpec` is `deny_unknown_fields`; a plan YAML that gives `test` still parses, and one that omits it
  now parses too.
- `check_definition_hash` hashes `stage.contracts`; this brief adds no `Stage` field it reads.
- Other literals of `ContractSpec { .. }` and `FrozenContract { .. }` in test files are W0b's; the literals in
  non-test files are owned by W1b (`stage_service/contracts.rs`) and are not yours.
- `Stage` is also stored as stage-file YAML: `#[serde(default)]` lets a stage file written without the new
  key load. The round-trip tests for `consumer_deferred` (in `src/fs/tests_stage_loading_round_trip.rs`) and
  for `FrozenContract.test` and `RefrozenContract.test` (in `src/verify/contracts/store_tests.rs`) are W0b's.

## Done

Your one check: none (the crate cannot compile until wave 2). Report the changed assertion lines (none expected)
and the final line counts of `types.rs`, `methods.rs` and the `default` function.
