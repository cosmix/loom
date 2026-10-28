---
name: metaplan
description: Load when a planning session creates a metaplan, starts a later planning round, or adds an unlisted plan to a metaplan.
---

# Write a metaplan

Write only the target metaplan: `doc/plans/index.md` for the first round,
`doc/plans/metaplan-<nn>.md` for a later round, or the selected existing metaplan
when adding an unlisted plan. Write nothing else directly. Never stage, commit,
or branch, and never run `loom init`, `loom run`, or `loom pressure`.

Keep plans with an `IN_PROGRESS-`, `DONE-`, or `RETIRED-` prefix, their brief
bundles, everything under `doc/plans/archive/`, and all `REVIEW-*.md` files
read-only. Leave finished metaplans unchanged; add a new round instead. If asked
to change locked work, explain the limit and offer a new metaplan entry or a
refinement of the product spec. Never write knowledge or summaries directly;
knowledge changes go through `loom knowledge`, and summary changes go through
`loom plan summary seal` using the turn-end delegation below.

## Read before discussing

Read the product spec, including `doc/product/index.md` and every written
product section. Read every existing metaplan and every plan in any state,
including running, done, retired, and archived plans. Use these to establish
what is already planned, underway, or delivered so the new round repeats none
of that work.

For a later round, choose the next free two-digit number for
`doc/plans/metaplan-<nn>.md`. Keep the first round at `doc/plans/index.md`.

## Settle the cut into plans

Load `loom-plan:brainstorming` and give it the target metaplan path. Discuss
what each plan delivers, which product sections it implements by number, the
execution order, and what each plan needs from another. Ask one plain-text
question per message, never through a question tool. Offer choices when the
answer space is small and two or three approaches with trade-offs and a
recommendation before settling the direction.

Write each settled decision into the metaplan as it settles, with its reason
when that is not obvious. Make the document understandable without the chat.

## Shape the metaplan

Start the document with a level-1 title and an introduction stating the round's
goal and what it builds on. Give every plan one level-2 entry heading, naming
the plan file in backticks before the file exists:

```markdown
## <n>. <Title> (`PLAN-<slug>.md`)
```

Follow each entry with three or four paragraphs covering what the plan delivers,
the product sections it implements by number, what it depends on and what
depends on it, and its risks. State explicitly when it implements no product
section. Keep the entry's description precise enough to bind the later plan's
scope. Do not put `Implements:` lines in a metaplan; those belong in plans.

Put entries in their intended execution order. Use prose headings without a
plan file name for material such as policy notes. Keep such headings unnumbered;
if a numbered prose heading is needed, mark it `(no plan file)` so it cannot be
mistaken for a malformed entry.

## Add an unlisted plan

Read the unlisted plan and the selected metaplan. Add an entry naming its
`PLAN-<slug>.md` file in the heading and describing its actual scope, product
sections, dependencies, and risks in the same form above. Place it in execution
order. Keep the existing plan unchanged, including its lifecycle spelling on
disk; references resolve by slug.

## Finish the turn

After any turn that edited documents, run `loom plan check` and fix what it
reports. Then run `loom plan summary status --json`; spawn one
`loom-plan:summary` subagent per document summary the turn made stale, incomplete,
or missing, in parallel. Wait for them to finish, then spawn one
`loom-plan:overview` subagent for the overviews that cite those summaries. Spawn
nothing when nothing is stale. Finish both waves before ending the turn.
