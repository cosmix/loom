---
name: refine
description: Refine a planning document or passage and carry the change through the documents it binds or is bound by.
---

# Refine a document

Use the feature name given in the session's opening prompt. Its vision and
sections live under `doc/product/<feature>/`; its metaplans and plans live under
`doc/plans/<feature>/`. A standalone plan lives at `doc/plans/PLAN-<slug>.md`,
has `Implements: none`, and has no metaplan.

1. Read the target document and any attached passage in context. Read the documents it
   binds or is bound by: for a product section, its vision; for a plan, its metaplan
   entry if any and every product section named in its `Implements:` line. Follow references
   to find the other documents that state the same requirement or decision.
2. Check the write limits before editing. Refuse changes to files prefixed
   `IN_PROGRESS-`, `DONE-`, or `RETIRED-`, anything under `doc/plans/<feature>/archive/`
   or `doc/plans/archive/`, a locked plan's brief bundle, and `REVIEW-*.md` files.
   Explain that running and completed
   plans and their briefs preserve the execution record, and run reviews are read-only.
   Offer a new metaplan entry or a spec refinement for the correction instead.
3. Make the change the user asks for. Resolve material ambiguity with the user in
   plain text, one question at a time. Record each settled decision immediately in
   the document it affects, with its reason when the reason is not obvious.
4. Carry the change into every document that states the same thing in the same turn.
   If you cannot reconcile a related document, record the conflict under its
   `## Open questions`, naming the conflicting statements, options, and your
   recommendation. If that document is locked, preserve it and record the conflict
   in the affected editable spec or proposed metaplan entry instead; explain the limit.
5. Keep headings that summaries cite unless the requested change concerns the heading
   itself. Keep cross-references consistent with any deliberate heading change.

Write only the target and affected editable documents under `doc/`, including an
agreed new metaplan entry under `doc/plans/<feature>/`. Never edit code or configuration.
Never write `doc/loom/knowledge/` with a file tool; use the `loom-plan:knowledge`
skill for an agreed knowledge change. Never write `doc/loom/summaries/` directly;
summary agents write only the runtime draft paths their briefs name and publish
through `loom plan summary seal <id>`. Never stage, commit, branch, or run
`loom init`, `loom run`, or `loom pressure`.

After any turn that edited documents, run `loom plan check` and fix what it reports,
then run `loom plan summary status --json`. Name the summaries being rebuilt and
spawn one `loom-plan:summary` subagent per document summary the turn made stale,
incomplete, or missing, in parallel; after they return, spawn one `loom-plan:overview`
subagent for the overviews that cite them. Give each agent only its assigned ids;
let its definition supply the brief, draft, seal, and release procedure. Spawn nothing
when nothing is stale. Finish the turn only after both waves return, and report any
summaries the agents released without sealing.
