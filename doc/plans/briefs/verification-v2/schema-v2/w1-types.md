# schema-v2 / W1 — schema and runtime types (foundation)

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` sections D0, D1, D2, D3. Knowledge:
`conventions/plan-yaml-and-hooks.md` "Additive Schema Fields: Prefer `#[serde(default)]` Over
Bespoke Migration"; `mistakes/pinned-literals-ledgers-and-wiring.md` "The Maintainability Ledger
Is EXACT-Match, Not a Ceiling".

## Files you own

`loom/src/plan/schema/types.rs`, `loom/src/plan/schema/types_v2.rs` (new),
`loom/src/models/stage/types.rs`, `loom/src/models/stage/checks.rs` (new),
`loom/src/models/stage/methods.rs`, `loom/src/models/stage/methods_v2_tests.rs` (new),
`loom/src/fs/stage_loading.rs`, `loom/src/commands/init/plan_setup.rs`.

W2 updates every test construction site in parallel with you, against the names pinned in
DESIGN D2/D3. Do not edit W2's files. If the crate fails to compile only because of files W2
owns, say so in your report.

## Tasks

1. `plan/schema/types_v2.rs`: `ContractSpec` and `ReachableCheck` exactly as DESIGN D3 (derives,
   `deny_unknown_fields`, serde attributes, field comments). Re-export both from
   `plan/schema/types.rs`.
2. `plan/schema/types.rs` (ledgered at 465 lines, must not grow):
   - `StageDefinition` (types.rs:198-324) gains `contracts`, `harness`, `reachable` with
     `#[serde(default, skip_serializing_if = "Vec::is_empty")]`;
   - `LoomConfig` (types.rs:148-169) gains `ratchet_files` the same way;
   - give `StageDefinition` and `LoomConfig` a `Default` (derive when every field type
     implements it; otherwise a hand-written impl in `types_v2.rs`);
   - pay for the added lines by moving a self-contained block out of `types.rs` into
     `types_v2.rs` (for example `deserialize_reasoning_effort` and its helpers, types.rs:427-446),
     re-exported so no importer changes. Report the exact new line count.
3. `models/stage/types.rs` (ledgered at 1103, must not grow): move `WiringCheck` (line 52),
   `TruthCheck` (line 395), `AcceptanceCriterion` (line 427) and their impls into
   `models/stage/checks.rs`, re-exported through the same public path, so no importer changes.
   Then:
   - `WiringCheck` gains `#[serde(default, skip_serializing_if = "std::ops::Not::not")] pub literal: bool`;
   - `Stage` (line 578) gains `plan_version: u32` (serde default `1` through a named default
     function), plus `contracts`, `harness`, `reachable`, `ratchet_files` with the serde
     attributes of D3.

   Lower the ledger line to the new exact count and report it.
4. `models/stage/methods.rs` (739, ledgered):
   - `Stage::from_definition` (methods.rs:28-65) takes `&PlanIdentity<'_>` (DESIGN D2; define
     `PlanIdentity` in `models/stage/types.rs` or `checks.rs`, re-exported beside `Stage`) and
     copies `plan_version`, `ratchet_files` from it and `contracts`, `harness`, `reachable`
     from the definition;
   - the ledgered test `from_definition_copies_all_runtime_policy_fields` (116 lines) may not
     grow. Put new assertions in the new test named below.
5. `fs/stage_loading.rs::definition_from_stage` (line 46): list the three new stage fields
   explicitly. It is the one production construction; it never uses a default spread.
6. `commands/init/plan_setup.rs`: `create_stage_from_definition` (line 333) and its two callers
   (lines 63, 288) build a `PlanIdentity` from the parsed plan (`parsed_plan.id`, the metadata's
   `loom.version` and `loom.ratchet_files`). Read how `parsed_plan` exposes the metadata before
   editing.

## Named test (binding)

- `from_definition_copies_v2_fields` in `models/stage/methods_v2_tests.rs`, declared from
  `methods.rs` with `#[cfg(test)] #[path = "methods_v2_tests.rs"] mod methods_v2_tests;`.
  It builds a definition with one contract, one harness glob, one reachable check, and a
  `PlanIdentity` with version 2 and one ratchet file, and asserts every value landed on the
  `Stage`. It also asserts that a `Stage` deserialised from YAML without `plan_version` reads 1.

## Constraints

- v1 behaviour does not change: no validation, execution or rendering change beyond the new
  fields. Validation is W3's.
- New files ≤ 400 lines, functions ≤ 50.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib -- from_definition_copies_v2_fields`

## Report

Files changed; exact new line counts for `plan/schema/types.rs`, `models/stage/types.rs`,
`models/stage/methods.rs` and the ledgered test function; the proof result or the compile error
and whose file it is in; anything you could not do.
