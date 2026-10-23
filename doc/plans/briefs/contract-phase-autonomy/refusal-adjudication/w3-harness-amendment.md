# W3: `harness` as an amendment field

Tier: sonnet (loom-software-engineer). Wave 1. Read `common.md` first. Mechanical, but every match arm counts: a missed arm in the stage-file sync is the failure the contract `harness-amendment-reaches-stage-record` catches.

## Files you own

`src/plan/amendment.rs`, `src/plan/amendment_fields.rs`, `src/plan/amendment_catch_up.rs`, `src/plan/amendment/journal.rs`, `src/plan/amendment/tests.rs`, `src/orchestrator/adjudication/apply_accept.rs`, `src/cli/types_stage_amend.rs`, `src/cli/types_stage.rs` (the `amend` about text only, about L274).

Every exhaustive match on `AmendmentField` / `ParsedAmendmentValue` is in these files, so wave 1 leaves the library compiling. Line numbers are from plan time; locate each symbol with `rg -n`.

## Steps

1. `AmendmentField::Harness` (`plan/amendment.rs:60-71`, serde kebab `harness`), `field_from_name` (L73), `ParsedAmendmentValue::Harness(String)` (L219). Fix the module doc at `plan/amendment.rs:10` (list the fields) and L24 (the cap default is 10, not 3: read `plan/schema/types.rs:183-196`).
2. `plan/amendment_fields.rs`: `current_field_len` (16-23), `parse_amendment_value` (34-47; the harness value is taken verbatim as `ParsedAmendmentValue::Harness(value.trim().to_string())`, refused when empty, and NEVER passed through `parse_yaml` (L50-53, serde_saphyr): `**/fixtures/*.json` would parse as a YAML alias and fail every apply, and `tests/x #1` would silently lose its tail as a comment; the whole-plan `schema::validate` still runs `path_problem` on harness entries, `v2_fields.rs:101-104`), `apply_patch_to_stage_def` (61-94, `apply_patch_vec` works for `Vec<String>`), `stage_field_matches` (133-148), `sync_stage_from_definition` (159-172: copy `harness` to the stage file).
3. `plan/amendment_catch_up.rs`: `stage_def_field_matches` (105-116), `copy_field` (122-126). `plan/amendment/journal.rs:101-106`: the field name in the audit row, and confirm the reader maps `harness` back.
4. `orchestrator/adjudication/apply_accept.rs:74-81` `field_name`: add `harness`.
5. `cli/types_stage_amend.rs`: `AmendField::Harness` with doc "Mutate the `harness` array", `to_field` arm. The command's about text in `cli/types_stage.rs` (about L274; `rg -n 'contracts array in place' src/cli`) lists harness too.

## Tests

In `plan/amendment/tests.rs`: insert, replace and delete on harness update both the plan file and the stage file; `**/fixtures/*.json` and `tests/x #1` round-trip verbatim; an empty value is refused; a harness value with a `..` component is refused by validation; the audit row reads back as `harness`. In the clap tests (`rg -n 'AmendField' src/cli` for their home in your files): `loom stage amend <id> --field harness ...` parses to `AmendmentField::Harness`.

Your one check, at the end: `cargo test --lib plan::amendment`. Do not run clippy or `--all-targets` (the frozen contract file names `FreezeRefusal`, which W2 adds in wave 2). If the build fails in a file you do not own, report it and stop.
