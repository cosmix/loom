# schema-v2 / W2 — construction sites and version tests

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D1, D2, D3. Knowledge:
`mistakes/pinned-literals-ledgers-and-wiring.md` "The Maintainability Ledger Is EXACT-Match,
Not a Ceiling".

W1 is adding, in parallel, `StageDefinition.{contracts, harness, reachable}`,
`LoomConfig.ratchet_files`, `WiringCheck.literal`, `Default` for `StageDefinition` and
`LoomConfig`, and changing `Stage::from_definition(definition, &PlanIdentity { id, version, ratchet_files })`.
Work against those names; do not edit W1's files.

## Files you own

Every file listed in your row of the stage's worker table (the test construction sites, the
version tests, and `loom/tests/integration/plan_verify.rs`).

## Tasks

1. At every `StageDefinition { ... }` and `LoomConfig { ... }` literal in your files: end the
   literal with `..Default::default()` and delete the fields whose value equals the default
   (`None`, `vec![]`, `false`, `Default::default()`). Every edited literal ends up shorter
   than before, which keeps ledgered files from growing. `plan/amendment.rs` (1084) and
   `commands/init/tests.rs` (473) are ledgered files; report their new exact counts.
2. At every `WiringCheck { ... }` literal: add `literal: false`, or use a spread if one exists.
3. Every `Stage::from_definition(def, "plan-id")` call in your files becomes
   `Stage::from_definition(def, &PlanIdentity { id: "plan-id", version: 1, ratchet_files: &[] })`.
4. Version tests move from 2 to 3 (DESIGN D1): `tests/integration/plan_verify.rs`
   (`invalid_version_plan`, `test_invalid_version`, which now expects
   `Supported versions: 1, 2`), `plan/schema/tests/validation_tests.rs`
   (`test_validate_unsupported_version`, `test_validate_multiple_errors`, and fix that test's
   comment count), `plan/parser/validation.rs` (`test_validate_unsupported_version`),
   `plan/parser/mod.rs` (`test_parse_validation_fails_unsupported_version`),
   `tests/e2e/criteria_validation/structure.rs` (if it asserts the unsupported message).
   Search your files for `version: 2` and `Unsupported version` and handle each hit; hits in
   handoff files (`handoff/schema/*`) are a different version field and stay as they are.
5. `fs/tests_stage_loading_round_trip.rs`: `build_full_stage_definition` sets the three new
   fields with non-default values.

## Named test (binding)

- `v2_fields_survive_stage_file_round_trip` in `fs/tests_stage_loading_round_trip.rs`: a
  `Stage` built from a definition with contracts, harness, reachable and a v2 `PlanIdentity` is
  saved with `verify::transitions::serialization::serialize_stage_to_markdown` and read back with
  `fs::stage_loading::extract_stage_definition`. It asserts the three definition fields are
  equal, and that the saved `Stage` frontmatter re-parses with `plan_version == 2`.

## Proof (one command, once, after W1's names exist)

`cargo test --manifest-path loom/Cargo.toml --lib -- v2_fields_survive_stage_file_round_trip`

## Report

Files changed; exact new counts of the two ledgered files; each version-test change; the proof
result or the compile error and whose file it is in.
