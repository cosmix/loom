# gate-waivers W0: dispute kinds, verdict and state edge (foundation)

Tier: sonnet. Wave 1, alone. Read `doc/plans/briefs/autonomous-recovery/common.md` first. The
stage description's PUBLIC SURFACE is authoritative. check-routes has merged: `CriterionField`
lives in `src/models/dispute_field.rs` (re-exported from `models::dispute`).

## Edits

1. New `src/models/dispute_gate.rs`, declared `pub mod dispute_gate;` in `src/models/mod.rs`
   and re-exported from `src/models/dispute.rs` (`pub use super::dispute_gate::{...};`),
   holding:
   - `pub const WAIVABLE_GATES: [&str; 3] = ["unwired", "duplicates", "impact-tests"];` with a
     doc comment: loom's own completion gates that have no plan entry to amend; an accepted
     gate dispute waives one for one stage. (`goal-backward`, `reachable`, `aggregated-wiring`,
     `after-stage` and `live` failures are addressed per entry by `dispute-criteria --field`.)
   - `pub enum PersonNeed { HostInstall, Credential, Decision }`, serde kebab-case
     (`host-install`, `credential`, `decision`), with `pub fn reason_prefix(self) -> &'static
     str` returning `needs a host install`, `needs a credential`, `needs a decision`.
   - `pub struct GateWaiver { pub gate: String, pub dispute_id: u32, pub reason: String, pub
     loom_version: String, pub waived_at: DateTime<Utc> }` (Serialize, Deserialize, Clone,
     Debug, PartialEq).
   - `pub const NEEDS_PERSON_WHAT_MAX_BYTES: usize = 300;`
2. `src/models/dispute.rs`
   - `DisputeKind::Gate { gates: Vec<String> }`; `name()` returns `"gate"`; `filed_by` treats it
     like every non-freeze kind.
   - `DisputeVerdict::NeedsPerson { need: PersonNeed, what: String, reasoning: String,
     citations: Vec<Citation> }` (tag `needs-person`). Doc: the dispute cannot pass until a
     person installs a host tool, provides a credential or makes a decision only a person can
     make; the stage is blocked with that reason.
3. `src/models/stage/dispute_budgets.rs` (`DisputeTally`, flattened into `Stage`, so
   `src/models/stage/types.rs` does not grow: it is pinned at 591 lines in
   `maintainability-baseline.txt`):
   - `#[serde(default)] pub gate_disputes: u32`, and `filed`/`spend` route `DisputeKind::Gate`
     to it.
   - `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub gate_waivers:
     Vec<GateWaiver>`.
   - `impl Stage { pub fn gate_waived(&self, gate: &str) -> Option<&GateWaiver> }` (here, not
     in `methods.rs`, which is 493 lines).
   - Tests: the tally round-trip keeps `gate_waivers` and `gate_disputes` top-level keys; three
     gate disputes exhaust `exhausted(stage, &Gate)` while the criterion and contract budgets
     stay unspent.
4. `src/models/stage/transitions.rs`: `NeedsAdjudication => &[Queued, NeedsHumanReview,
   Blocked]`. Update `exhaustive_policy_tests::enforcing_api_matches_every_state_pair`: the
   NeedsAdjudication row's bitmask gains the Blocked bit (`2050` becomes `2066`). Update every
   other test that lists NeedsAdjudication's targets (`rg -n 'NeedsAdjudication' src/models/stage/tests`).
   These are assertion-line edits: list each one in your report (the stage files them in one
   `dispute-integrity`).
5. `src/cli/types_stage_disputes.rs`: `DisputeField::Gate` (value `gate`, `to_field` returns
   `None` like `ImpactTests`); `#[arg(long = "gate", num_args = 1..)] gates: Vec<String>`;
   `CriteriaDispute::Gate(DisputeFiling)` built when `--field gate`: refuses
   `--criterion-index`, `--test`, `--owner` and `--failure-output`, needs at least one
   `--gate`, and checks every label is in `WAIVABLE_GATES` (`"'<label>' is not a waivable
   gate; waivable gates are unwired, duplicates, impact-tests"`). `DisputeTarget` (in
   `src/commands/stage/dispute_kinds.rs`) gains `Gate(Vec<String>)`; add that variant and its
   arm in `snapshot` (`DisputeKind::Gate { gates }`) yourself, it is one match arm. In
   `src/cli/dispatch_stage.rs`, `CriteriaDispute::Gate(filing)` goes to `file_dispute` exactly
   as `CriteriaDispute::ImpactTests` does (one arm). If `file_dispute` (or the transport it
   calls) refuses a plan version 1 stage on the client side, exempt `DisputeTarget::Gate`: a
   gate dispute files for both versions.
6. `src/models/dispute_tests.rs`: serde round trips for `Gate` and `NeedsPerson`.

## Done

`cargo build --all-targets` errors only in files other workers own; report them by file.
