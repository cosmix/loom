# doctrine-v2 / W1 — plan-writer skill for plan version 2

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` (all of it: you are teaching it) and
`doc/verification-report.md` (why v2 exists). Then read the MERGED code for every behaviour you
describe. Where code and DESIGN differ, describe the code and record the difference with
`loom memory note`. Knowledge: `mistakes/doctrine-and-acceptance.md` "After Landing a Doctrine,
Grep for the Phrasing It RETIRES (2026-07-28)" and "An Agent Doc May Only Name Commands Its Guard
Allows (2026-09-13)"; `patterns/doctrine-cross-surface.md`.

## Files you own

`skills/loom-plan-writer/SKILL.md`, `skills/loom-plan-writer/references/verification-rules.md`,
`skills/loom-plan-writer/references/bookend-stages.md`,
`skills/loom-plan-writer/references/v2-contracts.md` (new).

BLOCK-B inside `SKILL.md` is pinned byte for byte by `tests_doctrine.rs`. Do not edit it.

## Tasks

1. `references/v2-contracts.md` (new; add it to SKILL.md's references table, with a "Read when"
   line: "writing a `version: 2` plan: contracts, harness, reachable, ratchet files, the review
   gate"). Contents:
   - **Start with detection**: run `loom project detect` for the repository; load the language
     skill it names for each package the plan touches (via the loom-skills bridge for catalogued
     skills); take each contract's adapter and `test` format from that skill's
     `## Loom Test Runner Adapter` section. An unsupported package means contracts there fall
     back to exit codes, with a warning.
   - **The risk checklist** each standard stage walks to choose contracts. For each area that
     applies, at least one contract whose `rejects` names the plausible wrong implementation:
     untrusted input; filesystem paths and symlinks; process I/O volume and scale (output over
     pipe buffers, large repositories); configuration propagation (a setting actually read, not
     its default); lifecycle and concurrency (restart, retry, partial failure, shutdown);
     reachability from the entry point; external data correctness. Illustrate each with one
     escaped defect from `doc/verification-report.md`'s "What escaped the stages" table.
   - **Writing a contract**: `scenario` and `rejects` rules; one contract per behaviour; the
     contract file holds only contract tests; `harness` for declaration lines; the loom-spawned
     contract session writes them before any implementation, and loom freezes them.
   - **The v2 fields**: `reachable` and when to prefer it over a regex `wiring` (entry-point
     wiring); `literal: true` and glob `source`; `ratchet_files` (list every baseline or ledger
     file a stage could loosen, for example this repository's
     `loom/maintainability-baseline.txt`).
   - **What completion enforces in v2**: zero-test guard; contract check; impact-selected tests;
     the test-integrity events; the review gate; and the dispute kinds that resolve each.
     Integration-verify never defers a finding, and must list the full test command.
   - **Token discipline** (DESIGN D17), including the review order.
2. `SKILL.md`: Section 7's metadata skeleton shows `version: 2` as the default for new plans,
   with a note that `version: 1` keeps today's rules. Section 6 gets the v2 rows (`contracts`,
   `reachable`, `wiring.literal`, glob `source`, `ratchet_files`). Section 10's template becomes
   a v2 plan: one contract on each standard stage, and an IV stage with the full test command.
   The Pre-STOP checklist gains the v2 items (detection run; every standard stage has contracts
   from the risk checklist; IV lists the full test command). Keep SKILL.md's existing rules;
   they still apply.
3. `references/verification-rules.md`: add, after "Realizability", the rule that a criterion is
   worth the plausible wrong implementations it rejects, and point to v2 contracts for the
   behavioural part. `references/bookend-stages.md`: integration-verify considers reviewer
   suggestions and never defers a finding; knowledge-distill records unimplemented suggestions.
4. `rg` the skill for phrasing the change retires (for example any line saying acceptance
   criteria are the stage's detector) and fix every hit.

## Named test

W2 writes `plan_writer_skill_names_project_detect`, which requires `loom project detect` and
`references/v2-contracts.md` to appear in `SKILL.md`.

## Proof (one command, once)

`rg -n "loom project detect|v2-contracts.md|version: 2" skills/loom-plan-writer/SKILL.md`

## Report

Files changed; every place where code and DESIGN differed (also recorded in loom memory).
