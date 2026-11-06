# R3: skills, guidance and spec text (trace-server)

Read `../common.md` in full. You write instructions for planning sessions and the product spec
sections that describe them, as present truth.

## Files you own

`plan-plugin/skills/{metaplan,plan,section,vision,refine}/SKILL.md`,
`loom/src/planning/guidance/text/common_brief.md`, `skills/loom-plan-writer/SKILL.md`,
`doc/product/feat-planner/section_09.md`, `section_10.md`, `section_11.md`.

## Tasks

- **metaplan.** Every entry carries a `Covers:` line directly under its heading paragraph,
  naming the most specific spec headings (`section_NN#anchor`, or `section_NN` when the plan
  covers the whole section). Follow-up entries take theirs from the finding's About. Folding a
  finding: edit the unwritten entry so its paragraphs include the finding, then set the finding's
  Status line to `folded metaplan#<entry anchor>`. Planning one: write the follow-up entry, then
  set `planned PLAN-<slug>.md`. Read the feature's `findings.md` when planning follow-ups.
- **plan.** The `Implements:` line names headings where the plan covers part of a section; a
  stage covering a narrower part gets `Implements (stage:<id>): section_NN#anchor, ...` in the
  plan's prose (under the stage's heading). The metaplan entry's `Covers:` and the plan's
  Implements agree; a scope change updates both in the same turn.
- **Every authoring skill and `common_brief.md`.** Never write or edit a `Settled:` line. When a
  turn edits a document that has one, say so in the reply ("section 04 was settled; it now reads
  revised"). The stub of a section is its vision row and the stub of a plan its metaplan entry:
  when a body's scope moves past its stub, update the stub in the same turn or ask.
- **section and vision.** The vision row of a section is its stub: keep it a true one-line scope.
- **refine.** Read `findings.md` when an attachment is a finding.
- **loom-plan-writer.** In Section 3 (plan structure), document the anchored `Implements:` form
  and the stage line, with one `text` example.
- **section_09.md.** The skill table gains `review` (Review the feature), and the duties above.
- **section_10.md.** The review loop: settling from the document page, changes since settled, the
  turn footer's downstream impact, findings triage (Plan this, Fold into entry, Dismiss with a
  reason) and that approval is the settled mark.
- **section_11.md.** After a run: the run review's open findings and questions enter the ledger
  automatically; the Findings page is the wave screen; Review the feature at any time after a plan
  ran; folded findings mark the entry "revisit before writing".

## Done

The stage's after_stage greps pass. Keep every pinned phrase a guidance test reads (search
`loom/src/planning/guidance/tests*` and `loom/tests/planning_guidance_contracts.rs` for strings
from these files before editing); report any you had to change.
