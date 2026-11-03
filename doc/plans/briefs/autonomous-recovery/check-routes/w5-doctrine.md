# check-routes W5: doctrine names every dispute field and the owner route

Tier: haiku. Wave 2 (text only, independent of compilation). Read
`doc/plans/briefs/autonomous-recovery/common.md` first.

## Why

Agents learn the dispute routes from three places: the stage signal text
(`src/orchestrator/signals/helpers.rs`, and the integration-verify review section
`src/orchestrator/signals/v2_section_review.rs`), the plan-writer skill and the orchestration
skill. Most spell the `--field` values as `acceptance|wiring|wiring-tests`; the live-test-lane
plan's live-checks R4 added `--field live` and a "Live entries" row to
`references/v2-contracts.md` Section 5 and live rows to `SKILL.md` Sections 6 and 8. After this
stage the values are `acceptance`, `wiring`, `wiring-tests`, `live`, `reachable`, `artifacts`,
`dead-code-check`, `regression-test`, `after-stage` (plus `impact-tests`), every gap is labelled
`[<list> n]`, and integration-verify disputes a completed stage's wiring or reachable gap
`[<stage>/<list> n]` with `--owner <stage>`. Keep every sentence and row R4 wrote.

## Edits (you own these files)

1. `src/orchestrator/signals/helpers.rs`: the sentence(s) that list dispute fields (find with
   `rg -n 'wiring-tests' src/orchestrator/signals`). One sentence: "A failing check is disputed
   with `loom stage dispute-criteria <stage> --field <list> --criterion-index <n>`, the list
   and index its `[<list> n]` label names; integration-verify disputes a completed stage's
   `[<stage>/<list> n]` with `--owner <stage>`." Keep the stable-prefix size test
   (`orchestrator/signals/tests_size.rs`) passing: replace text, do not append.
2. `src/orchestrator/signals/v2_section_review.rs`: only if it lists fields (rg first; if not,
   no edit).
3. `../skills/loom-plan-writer/SKILL.md`, Section 9 paragraph that starts "A criterion that
   FAILS": the field list, the labels, and the owner route in one sentence.
4. `../skills/loom-plan-writer/references/v2-contracts.md`, Section 5 table row "Reachable
   re-verification": "Resolved by" becomes "wiring the unit, or `loom stage dispute-criteria
   integration-verify --owner <stage> --field reachable --criterion-index <n>`".
5. `../skills/loom-plan-writer/references/verification-rules.md` (`loom stage amend --field
   acceptance|wiring|wiring-tests`) and `references/authoring-detail.md` Section 9: the full
   field list.
6. `../skills/loom-orchestration/SKILL.md`: the paragraph near `dispute-criteria` (rg
   `dispute-criteria`) gains the field list and the owner route.

Plain prose, CLAUDE.md Rule 19, every fenced block with a language tag. Change only the
sentences that restate the field list.

## Done

No check; report each file and the sentence you replaced.
