# built-and-new: fixtures for the contract session

The stage description names the contracts; this file holds the fixture texts they need, so the
contract session writes them verbatim. The run review format is follow-ups-core's
(`loom/src/fs/plan_review`): the sections `## Spec Conformance`, `## Open Questions` and
`## Open Findings` (headings as pub consts there), every bullet written by `review_item_line` as
``- **<stage name>** (`<stage-id>`): <text>``, read back by
`loom::planning::run_review::review_items(review, ReviewSection::…)`. The `### <name> (<id>)`
and `#### Key Decisions` blocks of `## Changes by Stage` are the existing generator's
(`fs/plan_review/stages.rs` `render_stage_section`); they are a distractor here.

## Rust contract `built-view-reads-the-newest-run-review-of-each-done-plan`

Write each file under a `tempfile::TempDir` root (create parent directories), then
`Project::load(root)` and `ProjectChecklistView::from(&project)`, and take the checklist whose
`feature` is `alpha`.

- `doc/product/alpha/index.md`:

  ```text
  # Alpha

  A demo.

  ## Specification sections

  | # | Section |
  | --- | --- |
  | 01 | Export |
  ```

- `doc/product/alpha/section_01.md`: `# Export\n\nBody.\n`
- `doc/plans/alpha/index.md`:

  ```text
  # Alpha engineering

  ## 1. One (`PLAN-alpha-01.md`)

  ## 2. Two (`PLAN-alpha-02.md`)

  ## 3. Three (`PLAN-alpha-03.md`)
  ```

- `doc/plans/alpha/archive/DONE-PLAN-alpha-01.md`: `# Plan: alpha-01\n`
- `doc/plans/alpha/archive/DONE-PLAN-alpha-02.md`: `# Plan: alpha-02\n`
- `doc/plans/alpha/PLAN-alpha-03.md`: `# Plan: alpha-03\n` (written, not done)
- `doc/plans/REVIEW-PLAN-alpha-01.md` (where `fs::plan_review::generate` writes a review: the top
  of `doc/plans/`, named after the plan file's stem):

  ```text
  # Code Review: Alpha one

  **Plan:** PLAN-alpha-01 | **Generated:** 2026-11-01 10:00 UTC

  ## Summary

  Built alpha one.

  ## Changes by Stage

  ### Integration Verification (integration-verify)

  **Status:** Completed
  **Purpose:** Verify the plan.

  #### Files Changed

  No changes recorded.

  #### Key Decisions

  - Rebuilt the web bundle

  #### Notes

  No notes recorded.

  ## Spec Conformance

  - **Integration Verification** (`integration-verify`): Corrected doc/product/alpha/section_01.md to match the build: exports are CSV

  ## Open Questions

  - **Integration Verification** (`integration-verify`): Does section_01 cover empty exports?
  - **Chat polish** (`chat-polish`): Should the sheet close on send?

  ## Open Findings

  - **Chat polish** (`chat-polish`): Settings statistics look bland
  ```

- `doc/plans/alpha/archive/REVIEW-PLAN-alpha-02.md`:

  ```text
  # Code Review: Alpha two

  **Plan:** PLAN-alpha-02 | **Generated:** 2026-11-02 10:00 UTC

  ## Summary

  Built alpha two.

  ## Changes by Stage

  No stage memory recorded.

  ## Open Questions

  No open questions.

  ## Open Findings

  No open findings.
  ```

- `doc/plans/REVIEW-PLAN-alpha-03.md`: the alpha-01 review text with `alpha one` and
  `PLAN-alpha-01` replaced by `alpha three` and `PLAN-alpha-03`, and its open finding line
  replaced by ``- **Chat polish** (`chat-polish`): Not done yet``.

Assert on `checklist.built`:

- `reviews` equals, in this order, `alpha-01` with path `doc/plans/REVIEW-PLAN-alpha-01.md`, and
  `alpha-02` with path `doc/plans/alpha/archive/REVIEW-PLAN-alpha-02.md`; no entry names
  `alpha-03`.
- `spec` has exactly one entry: text
  `Corrected doc/product/alpha/section_01.md to match the build: exports are CSV`, stage
  `Integration Verification`, stage_id `integration-verify`, review
  `doc/plans/REVIEW-PLAN-alpha-01.md`. No entry's text is `Rebuilt the web bundle`.
- `candidates` holds exactly three entries: a `Finding` with stage `Chat polish`, stage_id
  `chat-polish`, text `Settings statistics look bland` and review
  `doc/plans/REVIEW-PLAN-alpha-01.md`; a `Question` with stage_id `chat-polish` and text
  `Should the sheet close on send?`; a `Question` with stage_id `integration-verify` and text
  `Does section_01 cover empty exports?`. Nothing from `REVIEW-PLAN-alpha-03.md`
  (`Not done yet`) appears.

## Web checklists for `web/src/plan/feature/built.contracts.test.tsx`

Build every checklist with `checklistViewSchema.parse({ ...checklistFixture.checklists[0],
...patch })`, as `web/src/plan/feature/polish.contracts.test.tsx` `atlas()` does (copy the
helpers; never import from a frozen file). The finished atlas checklist:

- `feature: "atlas"`, `metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }]`;
- `plan_rows`: two rows, slugs `atlas-01` and `atlas-02`, paths
  `doc/plans/atlas/archive/DONE-PLAN-<slug>.md`, title `Plan <slug>`, state `done`,
  `pressure_date: null`, `listed: true`;
- `plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 }`, `untested: 0`;
- `current: { kind: "ready_all_done" }`, every step's mark `done`, `primary: null`, `items: []`;
- `secondary: [{ id: "plan-follow-ups", kind: "plan_follow_ups", label: "Plan follow-ups",
  target: null }]`;
- `built`:

  ```json
  {
    "reviews": [
      { "slug": "atlas-01", "path": "doc/plans/REVIEW-PLAN-atlas-01.md" },
      { "slug": "atlas-02", "path": "doc/plans/atlas/archive/REVIEW-PLAN-atlas-02.md" }
    ],
    "spec": [
      { "stage": "Integration Verification", "stage_id": "integration-verify", "text": "Corrected section_01 to match the build", "review": "doc/plans/REVIEW-PLAN-atlas-01.md" }
    ],
    "candidates": [
      { "kind": "finding", "stage": "Chat polish", "stage_id": "chat-polish", "text": "Settings statistics look bland", "review": "doc/plans/REVIEW-PLAN-atlas-01.md" }
    ]
  }
  ```

The markup contract replaces the candidate's text with
`<img src=x onerror="alert(1)"> Bold **claim**`.

## Web checklists for `web/src/plan/new/threshold.contracts.test.tsx`

Three features in `projectAtom` (`{ checklists, standalone: [], moves: [] }`), each built with
`checklistViewSchema.parse` over the fixture as above:

- `atlas`: the finished atlas checklist above (built, one candidate);
- `beacon`: `metaplans: [{ path: "doc/plans/beacon/index.md", title: "Beacon" }]`, one plan row
  `beacon-01` in state `written`, `current: { kind: "ready" }`, `built` with empty
  `reviews`, `spec` and `candidates`;
- `comet`: `metaplans: []`, `plan_rows: []`, `current: { kind: "step", number: 3 }`, empty
  `built`.

The listbox lists `atlas` and `beacon` (features with a metaplan, in the server's order, which
is by name) and never `comet`.
