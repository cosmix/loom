# G3: the adjudication side of `--field`, and goal-backward gap labels

Stage `completion-gates`, wave 2, tier opus. Read `../common.md` first. G2 has already added
`CriterionField` (`loom/src/models/dispute.rs`: `Acceptance`, `Wiring`, `WiringTests`;
`as_str()`, `gap_label()`, `amendment_field()`), the required `field` on
`DisputeKind::Criterion { criterion_index, field }`, and `DisputeKind::criterion(field, index)`.
The crate does not compile until your files follow. Read G2's `models/dispute.rs` before you
start.

You own `loom/src/orchestrator/adjudication/{prompt.rs, prompt/criterion.rs,
prompt/criterion_entry.rs (new), prompt/tests.rs, prompt/tests_kinds.rs, prompt/tests_golden.rs,
verdict.rs, verdict_tests.rs, plan_patch.rs, apply.rs, tests.rs, record_tests.rs, mod.rs,
tests_criterion_field.rs (new), closed_disputes.rs, feedback.rs}` and
`loom/src/verify/goal_backward/mod.rs`.

## 1. Literal migration (mechanical)

`DisputeKind::Criterion { criterion_index: N }` becomes
`DisputeKind::criterion(CriterionField::Acceptance, N)` in `tests.rs` (lines about 42 and 309),
`record_tests.rs`, `prompt/tests.rs`, `prompt/tests_kinds.rs` and `prompt/tests_golden.rs`. No
assertion line changes.

## 2. The briefing shows the disputed entry

`prompt.rs::build` (about line 134) routes
`DisputeKind::Criterion { criterion_index, field }` to the criterion builder with the field.

- **Acceptance disputes read exactly as today.** `prompt/tests_golden.rs` pins the acceptance
  briefing byte for byte; it must pass unchanged. Keep `criterion.rs`'s acceptance text as is.
- **Wiring and wiring-tests disputes** get their own text, in a new sibling
  `prompt/criterion_entry.rs` (criterion.rs is 240 lines; keep both under 400), reusing
  `criterion.rs`'s verdict semantics, citation rules and `input.verdict_protocol(..)` rather
  than copying them. Settle the wording yourself; it must hold:
  - "Your Job" names ONE disputed wiring check (or wiring test).
  - Step 1 shows the entry and how to observe it from `input.site.path`: for a wiring check,
    its `source`, `pattern`, `literal` and `description`, and the command to test it (`rg -n -F`
    for a literal pattern, `rg -n -e` otherwise; a `source` holding `*`, `?` or `[` is a glob,
    searched with `rg --glob`), plus the plan v2 rule that a match on the line defining the
    named item does not count (`verify/goal_backward/wiring_v2.rs`); for a wiring test, its
    `name`, `command` and `success_criteria`, run from the site with `echo "exit: $?"` after
    it. A missing index says the entry may have been amended away, as the acceptance text does.
  - The verdict schema names `"field": "wiring"` (or `"wiring-tests"`) as the only field the
    patch may amend, the index is into that list, and the value is YAML deserialized into a
    `WiringCheck` (`source`, `pattern`, `description`, optional `literal`) or a `WiringTest`
    (`name`, `command`, `success_criteria`, optional `description`).
  - The evidence lists every entry of that list with the `→` marker on the disputed one, the
    field and index, the agent's reason, the failure output and the plan excerpt, as the
    acceptance evidence does.
- Tests in `prompt/tests_kinds.rs`: `a_wiring_dispute_shows_the_wiring_entry` (the pattern and
  source appear, and the schema names `"wiring"`) and
  `a_wiring_tests_dispute_runs_the_wiring_test_command`.

## 3. The verdict must amend the disputed list

- `plan_patch.rs::normalize` (about lines 45-60) accepts `"wiring-tests"` and `"wiring_tests"`
  as `AmendmentField::WiringTests`; its error says `must be acceptance|wiring|wiring-tests`.
  `canonical_inner` writes the kebab spelling (serde already does).
- `verdict.rs`:
  - `parse_and_validate_for` (about line 58): `(Ok(json), DisputeKind::Criterion { field, .. })
    => classify_and_validate(json, *field)`.
  - `classify_and_validate` and `validate_accept` take the field. After `normalize`, the
    amended field must be one the dispute admits: an `Acceptance` dispute admits `acceptance`
    and `wiring` (today's rule, unchanged); a `Wiring` dispute admits only `wiring`; a
    `WiringTests` dispute only `wiring-tests`. Anything else becomes `needs_more_evidence`
    with a question naming the disputed list and the field to use. The re-emit hint in the
    malformed-patch question names the disputed field.
  - `parse_and_validate` (no kind) keeps acceptance semantics: `commands/stage/adjudicate.rs:109`
    uses it only to detect `Escalate`; the recorded verdict goes through
    `record.rs:110 parse_and_validate_for` with the request's kind.
  - Tests in `verdict_tests.rs`: `a_wiring_tests_accept_must_amend_wiring_tests`,
    `a_wiring_accept_may_not_amend_acceptance`, and
    `an_acceptance_accept_may_still_amend_wiring`.
- SILENT sites, checked: `verdict_kinds.rs`'s `(_, other)` arm is never reached by a criterion
  dispute (`parse_and_validate_for` routes it away), and the `apply_kinds.rs` `_ =>` arms send
  every criterion verdict to `apply_accept`/`apply_reject`, which read the field from the
  amendment. Both are NO-OPs; say so in your report.

## 4. Applying an accepted wiring-tests verdict

`apply.rs::apply_accept` → `amend_plan` → `plan::amendment::apply_amendment`, which already
handles `AmendmentField::WiringTests` (`plan/amendment_fields.rs`). Two resyncs miss it:

- `resync_after_amendment` (about line 242) reloads `acceptance`, `wiring` and `contracts`;
  add `wiring_tests`.
- `persist_verdict_result` (about lines 140-182) re-applies the verdict-owned `acceptance` and
  `wiring` onto the fresh stage; add `wiring_tests`, and update its doc comment.
- `apply_reject`'s review reason reads "Adjudicator upheld the disputed acceptance criterion
  (dispute {dispute_id}): ..."; two assertions nobody may edit match its words
  (`tests/adjudication_e2e.rs:259` and `adjudication/tests_verdicts.rs:33` check
  `contains("upheld the disputed acceptance criterion")`). Keep the prefix
  `Adjudicator upheld the disputed` and follow it with the entry: `acceptance criterion`
  (unchanged, so both assertions hold), `wiring check` or `wiring test`, or `criterion` when
  the request cannot be read. Put the lookup in a helper
  `fn disputed_entry(work_dir: &Path, stage_id: &str, dispute_id: u32) -> &'static str` that
  reads the request with `super::read_request`; `apply_reject` is 37 lines and must stay under
  50.
- `feedback.rs` (the rejection feedback file the next session reads) says "The acceptance
  criterion stands." for every rejected criterion dispute. Make it "The disputed check
  stands." so a wiring or wiring-test rejection is not described as an acceptance criterion.
  If an existing assertion line matches the old words, leave the old sentence for acceptance
  disputes and use the new one only for wiring and wiring-tests disputes (the function has
  the request kind or can read it).
- `closed_disputes.rs::close_open_disputes`: its doc comment says it is "Called only once the
  stage has been escalated to `NeedsHumanReview`", and its warning names escalation. G2 now
  also calls it from `loom stage reset`: say both callers in the doc comment and make the
  warning neutral ("could not close an open dispute").

End-to-end test in a new `tests_criterion_field.rs`, declared in `mod.rs` beside the other
`#[cfg(test)]` modules: `an_accepted_wiring_tests_verdict_amends_the_plan_and_the_stage`. It
writes a plan with the `<!-- loom METADATA -->` markers and `config.toml`'s `[plan]
source_path` (model it on `tests/adjudication_e2e.rs` `write_plan_with_metadata_markers`/`write_config`
and `plan/tests/amendment.rs::setup_env_with_plan`, both read-only for you), a stage with one
wiring test, a request `DisputeKind::criterion(CriterionField::WiringTests, 0)` and an accept
verdict replacing that wiring test, runs `AdjudicatorRegistry::new().apply_verdict(..)`, and
asserts the plan file and the stage file both carry the new wiring test.

## 5. Goal-backward gaps name their index

`verify/goal_backward/mod.rs::run_goal_backward_verification` verifies `wiring` and
`wiring_tests` as whole lists today, so a gap cannot be tied to the index
`dispute-criteria --field ... --criterion-index` takes. Verify them entry by entry
(`std::slice::from_ref(check)` into `verify_wiring` and `verify_wiring_tests`), and prefix each
resulting gap's `description` with `[wiring <n>]` or `[wiring_tests <n>]` (0-based). Use
`CriterionField::gap_label()` for the label text. `run_goal_backward_verification` is 49
lines today: put the per-entry loop in a helper (for example
`fn indexed_gaps(field: CriterionField, index: usize, gaps: Vec<..>) -> Vec<..>`) so the
function stays under 50. Do not edit `wiring.rs`, `wiring_v2.rs` or
`wiring_tests.rs` (`verify_wiring_tests` is ledgered at 111 lines). Inline test module in
`mod.rs`: `gaps_name_their_wiring_and_wiring_tests_index` (exact name; acceptance runs
`verify::goal_backward::tests::gaps_name_their_wiring_and_wiring_tests_index`): a stage with two
wiring checks, the second failing, and one failing wiring test (`false`) yields gaps starting
`[wiring 1]` and `[wiring_tests 0]`.

## Check

`cargo test --lib orchestrator::adjudication::`, once, after all your edits. It builds the
whole library, including G1's, G2's and G4's unchecked wave-1 edits. When the build fails in a
file you do not own, stop: do not edit it, and report each error with its file:line so the
main agent can route it to a fresh worker.

## Contract your code must satisfy

`wiring-tests-verdict-must-amend-wiring-tests` (scenario in the plan's YAML).
