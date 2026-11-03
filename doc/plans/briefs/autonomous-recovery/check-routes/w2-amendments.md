# check-routes W2: the amendment engine for the new fields

Tier: opus. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/check_routes_contracts.rs`.

## Why

An accepted dispute changes the plan through `crate::plan::amendment::apply_amendment`
(`src/plan/amendment/apply.rs`): it validates the patch against the real schema type, writes a
numbered snapshot and an audit row under the plan-versions lock, replaces the plan file and
rewrites the stage file (`sync_stage_from_definition`). The new `AmendmentField` variants need
the same path, and two defects must not ship with it:

- `sync_stage_from_definition` (`src/plan/amendment_fields.rs`) sets
  `stage.completed_commit = None` whenever the amended list changed. Integration-verify amends
  a COMPLETED, merged stage. `verify/transitions/state.rs` refuses a merged dependency with no
  `completed_commit` ("marked merged but has no completed_commit"), so clearing it would stall
  integration-verify and knowledge-distill.
- `apply_amendment` treats an audit row with the same `(stage_id, dispute_id)` as the same
  amendment and returns it without applying (`apply.rs`, the `rows.iter().find` guard).
  Dispute ids are per stage, so an amendment of stage-a from integration-verify's dispute 1,
  keyed `"1.0"`, would match stage-a's own dispute 1 row and be skipped. W4 builds owner
  requests with the dispute id `"<filer-stage>/<dispute>.<k>"`; your code keeps
  `base_dispute_id` splitting on the last `.` so `"integration-verify/1.0"` counts as verdict
  `"integration-verify/1"`, and the idempotency guard stays an exact string match.

## Edits (you own these files)

1. `src/plan/amendment_fields.rs`
   - `current_field_len`: `reachable.len()`, `artifacts.len()`, `after_stage.len()`, and
     `usize::from(x.is_some())` for `dead_code_check` / `regression_test`.
   - `parse_amendment_value`: `ReachableCheck`, `DeadCodeCheck`, `RegressionTest`,
     `TruthCheck` through `parse_yaml`; `Artifacts` taken verbatim like `Harness`, then
     refused unless it is a non-empty relative path with no `..` component and no leading `/`
     (`bail!("Invalid artifact path '{value}' in amendment for stage '{stage}': an artifact
     path is relative to the stage's working_dir and stays inside it")`). The message must
     contain `artifact path`.
   - `apply_patch_to_stage_def`: lists through `apply_patch_vec`; the two singletons through a
     new `apply_patch_option<T: Clone>(slot: &mut Option<T>, patch, value) -> Result<()>`:
     index must be 0; `Insert` only when `None` (else `bail!("... is already set; replace it")`);
     `Replace` and `Delete` only when `Some`; `Delete` sets `None`.
   - `stage_field_matches` / `sync_stage_from_definition`: the new fields (compare by
     serialized form where the type has no `PartialEq`, as `Wiring` does). Clear
     `completed_commit` only when the stage is NOT `StageStatus::Completed`, for every field,
     the live-test-lane plan's `Live` included; a completed stage
     keeps `status`, `completed_commit` and `merged` untouched (doc comment says why: the
     integration-verify re-verification amends a merged predecessor).
   - Every `Live` arm the live plan's R3 wrote in these files stays as it is.
2. `src/plan/amendment.rs` is W0's file (W0 widened `AmendmentField` and
   `ParsedAmendmentValue` there). Put every piece of dispatch in `amendment_fields.rs` (split a
   sibling `amendment_fields_single.rs` if it would pass 400 lines); if you need a change in
   `amendment.rs`, report it.
3. `src/plan/amendment/journal.rs`: the audit field names for the new variants are
   `reachable`, `artifacts`, `dead_code_check`, `regression_test`, `after_stage`, matching
   `field_from_name` (which turns `_` into `-` before parsing kebab-case). Add a round-trip test.
4. `src/plan/amendment_catch_up.rs`: compare and copy the new fields in both match blocks.
5. `src/commands/stage/amend.rs`: the operator's `loom stage amend` accepts every new
   `--field` (W0 added the clap values); confirm it routes through `apply_amendment` unchanged
   and update its help text if it lists fields.
6. Tests in `src/plan/tests/amendment.rs` (existing file, add cases only): each new field
   inserts, replaces and deletes; a singleton refuses index 1 and a second insert; an
   artifact path `../x` and `/x` are refused; amending a Completed stage keeps
   `completed_commit`; the dispute id `"integration-verify/1.0"` is applied when the stage
   already has a `"1.0"` row; a reachable amendment leaves `contracts/<stage>/freeze.json`
   byte-identical; an amendment after `fs::plan_lifecycle::update_plan_source_path` moved the
   plan to an `IN_PROGRESS-` name writes the renamed file.

## Traps

- `crate::plan::schema::validate(&metadata)` runs after the patch: confirm it accepts an
  emptied `reachable` list and a `None` dead-code check (it should; a stage without them is
  valid). If validation rejects a delete that leaves a v2 standard stage with no goal check
  AND no acceptance, let the error stand: the verdict then needs a replace.
- The cap (`max_amendments_per_stage`) counts rows of `request.stage_id`, the AMENDED stage.
  That is the intended owner semantics; contract `cap-counts-the-owner-rows` pins it.
- Never widen the mutable scope beyond the listed fields.

## Done

Your one check after the stage compiles: `cargo test --lib plan::`.
