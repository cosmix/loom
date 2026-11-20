# schema-v2 / W3 — v2 validation, wiring into `validate` and `plan verify`, fixture plans

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D1, D3, D4. Knowledge:
`architecture/plan-lifecycle-and-fields.md` "What `loom plan verify` Rejects Before a Stage Runs
(2026-09-19)".

Wave 2: W1's types exist. W4 writes the lint modules in parallel with you. Its entry point is
pinned in DESIGN D4 (`v2_lints::run(&LintContext) -> Vec<LintFinding>`); you call it.

## Files you own

`loom/src/plan/schema/validation.rs`, `loom/src/plan/schema/validation/v2_fields.rs` (new),
`loom/src/plan/schema/tests/mod.rs`, `loom/src/plan/schema/tests/v2_tests.rs` (new),
`loom/src/commands/plan/verify.rs`, `loom/tests/fixtures/plans/v2-valid.md` (new),
`loom/tests/fixtures/plans/v1-uses-contracts.md` (new).

## Tasks

1. Version check (validation.rs:237-246): accept 1 and 2; message
   `Unsupported version: <n>. Supported versions: 1, 2.` Edit in place; net zero lines.
2. `validation/v2_fields.rs`: `pub(super) fn push_v2_field_errors(metadata: &LoomMetadata, errors: &mut Vec<ValidationError>)`:
   - on v1, one error per v2-only field use (DESIGN D1 wording, stage-scoped where the field is
     on a stage): `contracts`, `harness`, `reachable`, `wiring[].literal: true`,
     `ratchet_files`;
   - on v2, the D3 rules.

   Split into functions of at most 50 lines.
3. `validate()` (ledgered function, 410 lines; file 1238) calls `push_v2_field_errors` once and
   maps `v2_lints::run` findings. Error findings on v2 go into the error list. `run` needs
   `repo_root`, which `validate` lacks, so make the lint call in `commands/plan/verify.rs::execute`
   (ledgered, 138 lines; file 555) next to `validate_structural_preflight`: v2 error findings
   become hard errors, every other finding a `structural` warning. Pay for every added line
   inside a ledgered function by extracting lines from that same function; lower the ledger
   entries you change and report the new exact counts.
4. Declare `mod v2_fields;` and `pub(crate) mod v2_lints;` in `validation.rs` (W4 creates
   `v2_lints/mod.rs`), and `#[cfg(test)] mod v2_tests; mod v2_lint_tests;` in
   `plan/schema/tests/mod.rs` (W4 writes `v2_lint_tests.rs`). In `tests/mod.rs` also update the
   `make_stage` and `create_valid_metadata` literals the way W2 does elsewhere (spread
   `..Default::default()`), and add `create_valid_metadata_v2()` (version 2, one standard stage
   with one contract).
5. Fixture plans (full markdown plans with the `<!-- loom METADATA -->` markers):
   - `v2-valid.md`: `version: 2`, a knowledge-bootstrap stage, one standard stage with one
     contract (`runner: cargo-test`), an integration-verify stage whose acceptance includes
     `cargo test --manifest-path loom/Cargo.toml --all-targets`, and a knowledge-distill stage.
     It must pass `loom plan verify --strict` with zero warnings at the end of this stage; check
     with `cargo run -q --manifest-path loom/Cargo.toml -- plan verify --strict <path>` once your
     code compiles.
   - `v1-uses-contracts.md`: `version: 1`, one standard stage with a `contracts:` entry. It must
     fail `plan verify` with ``requires `version: 2` ``.

## Named tests (binding), in `plan/schema/tests/v2_tests.rs`

- `v1_plan_with_contracts_requires_version_2`: v1 metadata whose standard stage has a contract
  → an error containing ``requires `version: 2` `` and the stage id.
- `v2_standard_stage_without_contracts_is_rejected`
- `v2_plan_with_valid_contract_passes`: `create_valid_metadata_v2()` validates with no error.
- `unsupported_version_3_is_rejected`: error contains `Supported versions: 1, 2`.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib plan::schema::tests::v2_tests`

## Report

Files changed; exact new counts of every ledgered item touched (`validation.rs`, `validate`,
`validate_structural_preflight`, `commands/plan/verify.rs`, `execute`); the fixture plans'
`plan verify` output; the proof result.
