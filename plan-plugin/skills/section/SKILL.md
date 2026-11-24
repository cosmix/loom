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
Never write directly under `doc/loom/summaries/`. Keep code, configuration, locked
plans and their brief bundles, archived plans and `REVIEW-*.md` files read-only. Never
write knowledge with a file tool; knowledge changes belong to `loom knowledge`.

## Read the constraints

Identify the target's two-digit number, title and scope from the vision's section list
and the session brief. Read `doc/product/<feature>/index.md` and every product
section already written in the feature, including the target when revising it.
Treat the vision and written sections
as binding constraints; identify the decisions and section boundaries that apply.

Read the vision's `## Planning notes` and `## Deferred to planning` and follow them. When
the section describes an existing product, you may read its code and knowledge
(`doc/loom/knowledge/INDEX.md`, then the topics the section touches) to learn what users
see and do today, and write that as behaviour. A `path:line` reference is allowed only as a
pointer to current behaviour, never as the substance of a rule.

Apply the organisation's policies that set product requirements, as the session brief
describes: look for them before asking, link them, and state the requirement each imposes.

## Settle decisions as you write

Load `loom-plan:brainstorming` to discuss the section's open decisions with the user,
using `doc/product/<feature>/section_<nn>.md` as its target, at product level: follow the
session brief's rule for product documents. Discuss one decision at a time; offer
choices where the answer space is small and two or three product approaches with
trade-offs and a recommendation before settling a direction. Play your understanding back.
Raise no technical question. When the user raises a technical point, record it under the
section's `## Deferred to planning` as their constraint, in their words. When the user says
how they want the planning conducted, record it under the vision's `## Planning notes` in
the same turn; in team mode follow the team mode rules below.

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
plainly, as behaviour users can notice.

Append `(My call.)` to any decision you make that the user, the vision or an earlier
section did not make. Keep that marker beside the decision so a reviewer can find
choices nobody asked for.

Include `## Open questions`. For each unresolved point, write a list item containing
the question, its options and your recommendation. Do not treat an open question as
a settled requirement; leave the heading without question items when none remain. Open
questions are product questions; technical points go under `## Deferred to planning`,
included only when the section has any.

Include `## Out of scope for this section`. Name the section that owns each excluded
topic, keeping the boundaries consistent with the vision and the sections you read.

## Team mode

Use team mode only when the opening message says to use it for a draft-all run.
Otherwise keep the guided, one-document-at-a-time behavior above. The opening
names the missing documents and the teammate count: at most five teammates,
fewer when fewer documents are missing.

The lead reads the vision and settled decisions, then writes a short shared
brief in the runtime drafts directory the session may write. Include terms,
decisions, which document owns each topic and the cross-references to expect. The
brief also carries the session brief's rule for product documents, the vision's
`## Planning notes` and `## Deferred to planning`, and the policies that apply, because
teammates read the shared brief and the vision only.
Create one team with the stated teammate count and assign each teammate its
documents and the shared brief. Give each file exactly one owner.

Teammates follow the section-writing guidance above within their assignments.
While drafting, they message each other with `SendMessage` to agree on a term,
settle an overlap or ask which document owns a topic. Keep unresolved product
choices under `## Open questions`; record technical points under the document's own
`## Deferred to planning`. Keep proposed decisions marked as the writer's calls. A
teammate that hears steering about how the planning should be conducted reports it to
the lead. The vision is outside the run's write scope, so the lead adds the steering to the
shared brief and lists it in the hand-over as text for the vision's `## Planning notes`.

A teammate writes only its own documents. The run's listed documents are the
complete write scope under `doc/`; the planning hook refuses a write to any
other path. This scope also governs turn-end duties. Report needed changes to
the vision, summaries or other documents outside the list rather than writing
them. Never try to work around a refusal.

When every teammate has finished, the lead makes one consistency pass over
the set: terms, cross-references, duplicated or missing topics, and a check of
each document with `loom plan check`. Make corrections only within the run's
write scope. Hand the user a review queue in the vision's table order, the
order the app's sidebar and section queue use, and suggest starting at its
first document. Give each document its open-question count and a note on what
the pass changed, including when no changes were needed. Report anything that
could not be resolved.

The lead never starts a second team. If the user says “Stop the team”, end it;
documents already written stay.

## Settled documents

Never write or edit a `Settled:` line; the app's Mark settled action writes it. When the turn
edits a section that has one, say so in the reply ("section 04 was settled; it now reads
revised"). The section's stub is its row in the vision's `Specification sections` table: keep
the row a true one-line scope. After editing a section, re-read the vision (`index.md`, including
its `## Decisions`) for statements the edit makes untrue, and correct them in the same turn; when
the correction is a product choice, record it under the vision's `## Open questions` and ask. In
team mode, report such statements instead of writing them. When this section's scope moves past
its row, update the row in
the same turn or ask.

## Close the session

When the closing-write message arrives, ask no further questions. Write any settled
decisions still missing and add every unresolved point under `## Open questions`,
with options and your recommendation. Finish the turn-end duty, reply as the closing
message asks (`Closing write done.` and its `loom-closing` block), then stop. If there is
nothing to write, reply at once.

## Finish each editing turn

After any turn that edited documents, run `loom plan check` and fix what it reports
before you reply. Do not rebuild summaries at the end of the turn: the app rebuilds
them in the background once the documents stop changing.
