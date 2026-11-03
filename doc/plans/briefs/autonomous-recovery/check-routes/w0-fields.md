# check-routes W0: widen the field enums (foundation)

Tier: sonnet. Wave 1, alone. Read `doc/plans/briefs/autonomous-recovery/common.md` first. The
stage description's PUBLIC SURFACE is authoritative.

## Why

Plan 08's integration-verify could not complete because a `reachable` check had no dispute or
amend route (`blockages.md`, "Plan 08"). Today `CriterionField` covers acceptance, wiring,
wiring tests and live (`src/models/dispute.rs`, `CriterionField`; the live-test-lane plan's
live-checks R3 added `Live` next to `WiringTests` at every site), `AmendmentField` adds
contracts and harness (`src/plan/amendment.rs`), and the clap mirrors `DisputeField`
(`src/cli/types_stage_disputes.rs`) and `AmendField` (`src/cli/types_stage_amend.rs`) follow
them. You widen all four, add the owner, and leave every consumer's compile error to the worker
the stage table names for that file. Every `Live` arm stays as it is.

## Edits

1. `src/models/dispute.rs` is near 390 lines (383 before the live plan's `Live` arms), so first
   move `CriterionField` and its `impl`, `Live` included, into a
   new `src/models/dispute_field.rs`, declared `pub mod dispute_field;` in `src/models/mod.rs`
   next to `dispute`, and re-exported from `dispute.rs` with
   `pub use super::dispute_field::{gap_prefix, owner_gap_label, CriterionField};` so every
   `crate::models::dispute::CriterionField` path keeps resolving. The label helpers below live
   there too.
   - `CriterionField` gains, in this order after `Live`: `Reachable`, `Artifacts`,
     `DeadCodeCheck`, `RegressionTest`, `AfterStage` (serde kebab-case already applies:
     `reachable`, `artifacts`, `dead-code-check`, `regression-test`, `after-stage`).
   - `as_str`: the same kebab spellings. `gap_label`: `reachable`, `artifacts`,
     `dead_code_check`, `regression_test`, `after_stage`. `amendment_field`: the matching new
     `AmendmentField` variant.
   - Add `pub fn reexamined_by_integration(self) -> bool`: true for `Wiring` and `Reachable`,
     the two lists integration-verify re-runs for completed stages
     (`commands/stage/complete_verification.rs::aggregated_wiring`,
     `complete_verification_v2.rs::reverify_reachable`).
   - Add `pub fn is_singleton(self) -> bool`: true for `DeadCodeCheck` and `RegressionTest`
     (each an `Option` on the stage; index 0 addresses it).
   - `DisputeKind::Criterion` gains `#[serde(default, skip_serializing_if =
     "Option::is_none")] owner: Option<String>` after `field`. Keep
     `DisputeKind::criterion(field, indices)` (owner `None`) and add
     `pub fn owned_criterion(owner: &str, field: CriterionField, criterion_indices: Vec<usize>)
     -> Self`.
   - Add `pub fn owner_gap_label(owner: &str, field: CriterionField, index: usize) -> String`
     returning `[<owner>/<gap_label> <index>]`, and
     `pub fn gap_prefix(field: CriterionField, index: usize) -> String` returning
     `[<gap_label> <index>]`. W1 calls both; never format the label anywhere else.
2. `src/plan/amendment.rs`: `AmendmentField` gains `Reachable`, `Artifacts`, `DeadCodeCheck`,
   `RegressionTest`, `AfterStage` (kebab-case serde) after `Live`. Update the module doc's
   "Scope" bullet to list every mutable field, `live` included. `ParsedAmendmentValue` gains `Reachable(ReachableCheck)`,
   `Artifact(String)`, `DeadCode(DeadCodeCheck)`, `Regression(RegressionTest)`,
   `AfterStage(TruthCheck)` (the real schema types; import from `crate::plan::schema` /
   `crate::models::stage` as the existing imports do). W2 fills the dispatch.
3. `src/cli/types_stage_disputes.rs`
   - `DisputeField` gains `Reachable`, `Artifacts`, `DeadCodeCheck`, `RegressionTest`,
     `AfterStage` after `Live`; `to_field` maps each to its `CriterionField`. `try_from`'s
     existing refusals (for example "--field live needs --criterion-index") stay.
   - `DisputeCriteriaArgs` gains `#[arg(long, value_parser = clap_id_validator)] owner:
     Option<String>` with doc: "With --field wiring or --field reachable from an
     integration-verify stage: the completed stage whose list --criterion-index indexes, as
     `loom stage complete` labels its gap `[<stage>/<list> n]`."
   - `CriteriaDispute::Entry` gains `owner: Option<String>`. `try_from` refuses `--owner` with
     `--field impact-tests` ("--owner names another stage's wiring or reachable list; it takes
     no --test") and with a field whose `reexamined_by_integration()` is false
     ("--owner takes --field wiring or --field reachable"). Update the `--field` doc comment to
     list every value and its label.
   - Keep the existing tests passing; add parse tests for `--field reachable --owner stage-a
     --criterion-index 0` and each refusal.
4. `src/cli/types_stage_amend.rs`: `AmendField` gains the five variants after `Live`; `to_field`
   maps them.

## Done

`cargo build --all-targets 2>&1 | rg -c '^error'` reports errors only in files other workers
own (match exhaustiveness and the new `owner` field). Report that list grouped by file. Your one
check: `cargo test --lib cli::types_stage_disputes::` is NOT runnable until the consumers
compile, so run none; report instead.
