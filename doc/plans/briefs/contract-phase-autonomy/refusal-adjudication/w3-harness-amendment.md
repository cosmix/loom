# W3: `harness` as an amendment field

Tier: sonnet (loom-software-engineer). Wave 1. Read `common.md` first. Mechanical, but every match arm counts: a missed arm in the stage-file sync is the failure the contract `harness-amendment-reaches-stage-record` catches.

## Files you own

`src/plan/amendment.rs`, `src/plan/amendment_fields.rs`, `src/plan/amendment_catch_up.rs`, `src/plan/amendment/journal.rs`, `src/plan/amendment/tests.rs`, `src/orchestrator/adjudication/apply_accept.rs`, `src/cli/types_stage_amend.rs`.

## Steps

1. `AmendmentField::Harness` (`plan/amendment.rs:60-71`, serde kebab `harness`), `field_from_name` (L73), `ParsedAmendmentValue::Harness(String)` (L219). Fix the module doc at `plan/amendment.rs:10` (list the fields) and L24 (the cap default is 10, not 3: read `plan/schema/types.rs:183-196`).
2. `plan/amendment_fields.rs`: `current_field_len` (16-23), `parse_amendment_value` (34-47; the value is a YAML string, validated by the whole-plan `schema::validate`, which already runs `path_problem` on harness entries), `apply_patch_to_stage_def` (61-94, `apply_patch_vec` works for `Vec<String>`), `stage_field_matches` (133-148), `sync_stage_from_definition` (159-172: copy `harness` to the stage file).
3. `plan/amendment_catch_up.rs`: `stage_def_field_matches` (105-116), `copy_field` (122-126). `plan/amendment/journal.rs:101-106`: the field name in the audit row, and confirm the reader maps `harness` back.
4. `orchestrator/adjudication/apply_accept.rs:74-81` `field_name`: add `harness`.
5. `cli/types_stage_amend.rs`: `AmendField::Harness` with doc "Mutate the `harness` array", `to_field` arm. The command's about text in the clap definition (`rg -n 'contracts array in place' src/cli`) lists harness too; if that string lives in a file you do not own, report it.

## Tests

In `plan/amendment/tests.rs`: insert, replace and delete on harness update both the plan file and the stage file; a harness value with a `..` component is refused by validation; the audit row reads back as `harness`. Run `cargo test --lib plan::amendment`, `cargo test --lib cli::` and clippy once each.
