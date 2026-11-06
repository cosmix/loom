# D1: the product spec to the new truth (trace-core, wave 1)

Read `../common.md` in full. You write the product contract before the code: describe how the
planner works after this plan, as present truth, with no history ("was", "used to", "now").
Keep each document's existing conventions (`# NN — Title`, back-link line, `(My call.)`,
`## Open questions`). Mark every addition the user did not decide with `(My call.)`; the user's
decisions are listed in common.md and need no mark.

## Files you own

`doc/product/feat-planner/index.md`, `section_01.md`, `section_03.md`.

## index.md (the vision)

- **Decisions.** Rewrite `Approval (10)`: approval is a document-level settled mark, written by the
  reader from the document's page as a `Settled:` line; a settled document that changes
  reads as revised. Rewrite `Summary levels`: plan summaries cover the plan's brief bundle, one
  detail paragraph per stage citing the stage and its briefs; remove "Briefs are reached through
  their plan's summary and through file mode, with no summaries of their own (my call)". Rewrite
  `Product-to-plan links`: plans link to sections or headings (`Implements: section_04#anchor`),
  metaplan entries carry `Covers:` lines, stages carry `Implements (stage:<id>):` lines; links
  come only from these lines, never from inference. Add `Findings (01, 10, 11)`: the feature's
  ledger `doc/plans/<f>/findings.md`, filled from run reviews and Review the feature sessions,
  triaged on the Findings page. Add `Planning trace (01, 03, 05)`: the trace is built from these
  lines on every load, is separate from `loom map`'s source graph, and stores nothing outside the
  documents.
- **Principles 2 and 3** stay; add to principle 1 that edge lines, Settled lines and the ledger
  are plain Markdown lines a reader can follow.
- **The screen**: add the Coverage and Findings rows of the spine, the frontier hints under the
  next step, and the settle state on rows.
- **Users**: the reviewer settles what they agreed to; the returning author sees what changed
  since settling and what runs found.

## section_01.md (workflow model)

Add document maturity (progress and settle state, common.md "The trace model"), the frontier and
its order, the Review the feature offer (secondary once a plan is running or done; its owner is the
Findings page), the Coverage and Findings pages in the screen list, and the rule that the frontier
starts no session. Keep the six steps and offers otherwise as the merged section states them.

## section_03.md (document model)

Add: edge lines and the token table, the anchored `Implements:` form, the stage line, the
`Settled:` line (grammar, placement, hashes, states, who writes it, which documents can be
settled), and `findings.md` (format, statuses, import, folding). Use `text` or `markdown` fences
for every example.

## Done

`rg -n 'Settled:|Covers:|findings.md' doc/product/feat-planner/section_03.md` finds all
three; the old `Approval (10)` text is gone from index.md; no stale mention of briefs having no
summaries remains (`rg -n -i 'no summaries of their own' doc/product/feat-planner`).
