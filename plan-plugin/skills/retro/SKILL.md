---
name: retro
description: Review completed plans with the user and record accepted spec divergences and proposed follow-up work.
---

# Review what was built

1. Read each done plan in the starting message, its run review
   `REVIEW-<plan file stem>.md`, every product section named in its `Implements:`
   line, and `doc/loom/knowledge/INDEX.md`. Use the index to locate relevant
   knowledge when the review needs more context. Treat the plans and reviews as
   read-only evidence. If required evidence is missing, name it and ask the user
   how to proceed; do not invent results or generate another run review.
2. Compare what the run review says was built with what the affected sections
   require. Walk the user through each divergence with references to the review
   and the spec, its effect, and the choice that needs settling. Ask in plain text,
   one question at a time. Keep proposed changes distinct from accepted decisions.
3. Record every divergence the user accepts as a decision in the affected product
   section as soon as it is settled. Update the requirement it affects so the
   section states the accepted behavior, and include the reason when it is not
   obvious. Keep headings that summaries cite unless the decision changes a heading.
4. Record each follow-up under `## Open questions` in the affected section, with
   the options and your recommendation, or as a proposed entry for a new metaplan
   round under `doc/plans/`. Preserve unresolved choices as open questions; do not
   present proposed work as approved or implemented.

Write only the affected editable product sections under `doc/product/` and proposed
entries in an editable metaplan for a new round under `doc/plans/`. Never edit done
plans or run reviews, and never run `loom review`. Refuse changes to files prefixed
`IN_PROGRESS-`, `DONE-`, or `RETIRED-`, anything under `doc/plans/archive/`, a locked
plan's brief bundle, and `REVIEW-*.md` files. Explain that these preserve the run's
record and offer a new metaplan entry or a spec refinement instead.

Keep `doc/loom/knowledge/` read-only in this retrospective; route any agreed knowledge
change through `loom-plan:knowledge`. Never write `doc/loom/summaries/` directly;
summary agents write only the runtime draft paths their briefs name and publish
through `loom plan summary seal <id>`. Never edit code or configuration. Never stage,
commit, branch, or run `loom init`, `loom run`, or `loom pressure`.

After any turn that edited documents, run `loom plan check` and fix what it reports,
then run `loom plan summary status --json`. Name the summaries being rebuilt and
spawn one `loom-plan:summary` subagent per document summary the turn made stale,
incomplete, or missing, in parallel; after they return, spawn one `loom-plan:overview`
subagent for the overviews that cite them. Give each agent only its assigned ids;
let its definition supply the brief, draft, seal, and release procedure. Spawn nothing
when nothing is stale. Finish the turn only after both waves return, and report any
summaries the agents released without sealing.
