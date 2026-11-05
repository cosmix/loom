---
name: metaplan
description: Load when a planning session creates a feature's metaplan, plans follow-ups for a finished feature, or adds an unlisted plan to the metaplan.
---

# Write a metaplan

Use the feature name given in the session's opening prompt.

A feature has one metaplan: `doc/plans/<feature>/index.md`. Write it when the feature has none.
When it exists, add to it: plan follow-ups by appending entries under a `## Follow-ups` heading
at the end of the file (add the heading if it is missing), or add an unlisted plan in its
execution order. Also write any other editable document under `doc/` that a settled decision or
an open point binds; put a decision that spans product sections under `## Decisions` in
`doc/product/<feature>/index.md`. Never stage, commit, or branch, and never run `loom init`,
`loom run`, or `loom pressure`.

Keep plans with an `IN_PROGRESS-`, `DONE-`, or `RETIRED-` prefix, their brief
bundles, everything under `doc/plans/<feature>/archive/` or `doc/plans/archive/`,
and all `REVIEW-*.md` files read-only. Leave finished entries unchanged, and every entry whose
plan exists. If asked to change locked work, explain the limit and offer a new metaplan entry or
a refinement of the product spec. Never write knowledge or summaries directly;
knowledge changes go through `loom knowledge`, and summary changes go through
`loom plan summary seal` using the turn-end delegation below.

## Read before discussing

Read the feature's product spec, including `doc/product/<feature>/index.md` when present
and every written product section. Read the metaplan and every plan
in the feature in any state, including running, done, retired, and archived plans. Use these to establish
what is already planned, underway, or delivered so the new entries repeat none
of that work. For follow-ups, also read the feature's `REVIEW-*.md` run reviews: their open
findings are the default candidates.

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

Start a new metaplan with a level-1 title and an introduction stating the feature's
goal and what it builds on. Give every plan one level-2 entry heading, naming
the plan file in backticks before the file exists:

```markdown
## <n>. <Title> (`PLAN-<slug>.md`)
```

Follow each entry with three or four paragraphs covering what the plan delivers,
the product sections it implements by number, what it depends on and what
depends on it, and its risks. State explicitly when it implements no product
section. Keep the entry's description precise enough to bind the later plan's
scope. Plans live beside the metaplan, and their slugs must be unique across the
repository. Do not put `Implements:` lines in a metaplan; those belong in plans.

Put entries in their intended execution order. Use prose headings without a
plan file name for material such as policy notes. Keep such headings unnumbered;
if a numbered prose heading is needed, mark it `(no plan file)` so it cannot be
mistaken for a malformed entry.

## Plan follow-ups

For a finished feature, append the follow-up entries under `## Follow-ups` at the end of
`doc/plans/<feature>/index.md`. The heading names no plan and is not numbered, so the app does
not read it as an entry. Write each follow-up as a level-2 heading in the entry form above,
numbered on from the last entry, with the same paragraphs. Leave the existing entries as they
are. The checklist then offers "Write plan <slug>" for each new entry.

## Add an unlisted plan

Read the unlisted plan and the metaplan. Add an entry naming its
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
