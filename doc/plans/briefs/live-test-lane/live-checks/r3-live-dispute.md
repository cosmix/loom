# live-checks R3: `loom stage dispute-criteria --field live`

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("Receipts and
completion"). Frozen contracts: `tests/live_dispute_contracts.rs` and
`src/plan/live_amendment_contract_tests.rs` (with `src/plan/live_amendment_contract_tests/**`);
never edit them. The stage description's PUBLIC SURFACE is authoritative.

## Why

A required live entry that cannot pass for a reason the stage cannot fix (a facility this
platform's profile does not provide, an entry the plan got wrong) needs the normal route: a
criteria dispute an adjudicator judges, whose accept amends the plan's `live` list (typically
the entry with `required: false`, or a corrected command). Only a missing credential is a
person's need and a block (R2's rule). The live list is a plan list like `wiring_tests`, so it
takes the same route at every site.

## Edits

Mirror `WiringTests` at every site, and nothing more. `rg -n 'WiringTests' src --glob
'!**/*tests*'` lists them (19 files at authoring); for each, add the `Live` case the way the
`WiringTests` case is written, or record in your report why the site needs none (a match over
fields only contracts and harness reach, such as the freeze-refusal verdicts):

- `src/models/dispute.rs` `CriterionField::Live` (`as_str` `live`, `gap_label` `live`,
  `amendment_field` `AmendmentField::Live`). The file is 383 lines: keep it under 400.
- `src/plan/amendment.rs` `AmendmentField::Live` (kebab `live`) and
  `ParsedAmendmentValue::Live(LiveEntry)`; the module doc's "Scope" bullet names `live`.
- `src/plan/amendment_fields.rs`: `current_field_len`, `parse_amendment_value` (deserialize into
  the real `crate::plan::schema::LiveEntry`, so a malformed patch fails before any write),
  `apply_patch_to_stage_def`, `stage_field_matches`, `sync_stage_from_definition`
  (`stage.live = definition.live.clone()`); `src/plan/amendment/journal.rs`,
  `src/plan/amendment_catch_up.rs` as their `WiringTests` arms do.
- `src/cli/types_stage_disputes.rs` `DisputeField::Live` (doc: "the stage's live entries, as
  `loom stage complete` labels a failure `[live n]`"), `to_field`; the `--field` doc lists it.
  `src/cli/types_stage_amend.rs` `AmendField::Live`.
- `src/stage_service/dispute.rs`: the field length is `stage.live.len()`.
- `src/models/stage/dispute_budgets.rs`: whatever per-field handling `WiringTests` has.
- `src/orchestrator/adjudication/verdict.rs`: a `Live` dispute accepts a patch on the `live`
  field only (`"\"live\""` in the allowed-fields text).
- `src/orchestrator/adjudication/prompt.rs` and `prompt/criterion_entry.rs`: the briefing for a
  `Live` dispute shows each disputed entry's YAML and, when present, the stage's receipt outcome
  for it (`crate::live::receipt::load` and the entry's `live_entry_hash`), and names the two
  usual accepted patches: the entry with `required: false` when the platform cannot provide
  what it needs, or a corrected command.
- `src/orchestrator/adjudication/apply.rs`, `apply_accept.rs`, `plan_patch.rs`,
  `src/daemon/protocol_debug.rs`, `src/verify/goal_backward/mod.rs`: as the compiler and their
  `WiringTests` arms require.

## Traps

- An accepted live amendment changes the entry's definition hash, so that entry needs a fresh
  `loom check --live` when it stays required; the other entries' outcomes stay valid (R2
  matches by hash).
- The autonomous-recovery plan, which runs after this one, adds five more fields to these same
  enums; keep `Live` next to `WiringTests` and add nothing speculative.
- `cargo fmt`; your one check: `cargo test --lib plan::amendment --no-run`.

## Report

Files changed, every `WiringTests` site you left without a `Live` case and why, and every test
whose pinned field list you extended.
