---
name: section
description: Load when a planning session writes or revises a numbered product specification section.
---

# Write a product section

Use the feature name given in the session's opening prompt.

Write the target `doc/product/<feature>/section_<nn>.md`, plus any other editable
document under `doc/` that a settled decision or an open point binds. Put a decision
that spans sections under `## Decisions` in `doc/product/<feature>/index.md`.
Never stage, commit or branch, and never run `loom init`, `loom run` or `loom pressure`.
Rebuild summaries only through the turn-end procedure below; never write directly under
`doc/loom/summaries/`. Keep code, configuration, locked plans and their brief bundles,
archived plans and `REVIEW-*.md` files read-only. Never write knowledge with a file tool;
knowledge changes belong to `loom knowledge`.

## Read the constraints

Identify the target's two-digit number, title and scope from the vision's section list
and the session brief. Read `doc/product/<feature>/index.md` and every product
section already written in the feature, including the target when revising it.
Treat the vision and written sections
as binding constraints; identify the decisions and section boundaries that apply.

Read `doc/loom/knowledge/INDEX.md`, then pull only the knowledge topics this section
touches. Read the source code each rule about current behavior rests on. Ground those
rules in the tree, and keep their `path:line` references for the document.

## Settle decisions as you write

Load `loom-plan:brainstorming` to discuss the section's open decisions with the user,
using `doc/product/<feature>/section_<nn>.md` as its target. Discuss one decision
at a time; offer
choices where the answer space is small and two or three approaches with trade-offs
and a recommendation before settling a direction. Play your understanding back.

Write each decision into the section as it settles, in words a reader without the chat
can understand. Include the reason when it is not obvious. Keep points still open
explicit rather than silently deciding them.

## Shape the section

Start with the numbered level-1 title and the vision back-link, using the vision's
actual title:

```markdown
# <nn> — <Title>

Part of the [<vision title>](index.md).
```

Follow with a short paragraph stating what the section covers and which other sections
it leans on. Put decisions under headings that name their topic and state the rules
plainly. Cite current code as `path:line` wherever a rule rests on it.

Append `(My call.)` to any decision you make that the user, the vision or an earlier
section did not make. Keep that marker beside the decision so a reviewer can find
choices nobody asked for.

Include `## Open questions`. For each unresolved point, write a list item containing
the question, its options and your recommendation. Do not treat an open question as
a settled requirement; leave the heading without question items when none remain.

Include `## Out of scope for this section`. Name the section that owns each excluded
topic, keeping the boundaries consistent with the vision and the sections you read.

## Team mode

Use team mode only when the opening message says to use it for a draft-all run.
Otherwise keep the guided, one-document-at-a-time behavior above. The opening
names the missing documents and the teammate count: at most five teammates,
fewer when fewer documents are missing.

The lead reads the vision and settled decisions, then writes a short shared
brief in the runtime drafts directory the session may write. Include terms,
decisions, which document owns each topic and the cross-references to expect.
Create one team with the stated teammate count and assign each teammate its
documents and the shared brief. Give each file exactly one owner.

Teammates follow the section-writing guidance above within their assignments.
While drafting, they message each other with `SendMessage` to agree on a term,
settle an overlap or ask which document owns a topic. Keep unresolved choices
under `## Open questions`; keep proposed decisions marked as the writer's calls.

A teammate writes only its own documents. The run's listed documents are the
complete write scope under `doc/`; the planning hook refuses a write to any
other path. This scope also governs turn-end duties. Report needed changes to
the vision, summaries or other documents outside the list rather than writing
them. Never try to work around a refusal.

When every teammate has finished, the lead makes one consistency pass over
the set: terms, cross-references, duplicated or missing topics, and a check of
each document with `loom plan check`. Make corrections only within the run's
write scope. Hand the user a review queue ordered by open-question count,
highest first, with a note per document on what the pass changed, including
when no changes were needed. Report anything that could not be resolved.

The lead never starts a second team. If the user says “Stop the team”, end it;
documents already written stay.

## Close the session

When the closing-write message arrives, ask no further questions. Write any settled
decisions still missing and add every unresolved point under `## Open questions`,
with options and your recommendation. Finish the turn-end duty, reply
`Closing write done.` and stop. If there is nothing to write, reply at once.

## Finish each editing turn

After any turn that edited documents, run `loom plan check` and fix what it reports;
then run `loom plan summary status --json`. Spawn one `loom-plan:summary` subagent per
stale, incomplete or missing document summary in parallel. Wait for them to return,
then spawn one `loom-plan:overview` subagent for the overviews that cite them. Spawn
nothing when nothing is stale.
