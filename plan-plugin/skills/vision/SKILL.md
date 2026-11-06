---
name: vision
description: Load when a planning session creates or revises the product vision, fixes its section list, or adds an unlisted section to it.
---

# Write the product vision

Use the feature name given in the session's opening prompt. It is shared by
`doc/product/<feature>/` and `doc/plans/<feature>/` and matches
`[a-z0-9][a-z0-9-]*`, excluding `archive`, `briefs`, `plans`, and `overview`.

Write the target `doc/product/<feature>/index.md`, plus any other editable document
under `doc/` that a settled decision or an open point binds. Put a decision that spans sections under
`## Decisions` in the vision. Never stage, commit or branch, and never run `loom init`,
`loom run` or `loom pressure`. Rebuild summaries only
through the turn-end procedure below; never write directly under `doc/loom/summaries/`.
Keep code, configuration, locked plans and their brief bundles, archived plans and
`REVIEW-*.md` files read-only. Never write knowledge with a file tool; knowledge changes
belong to `loom knowledge`.

## Discuss the direction

Read the feature's existing vision before revising it. Read any existing product
sections needed to understand the direction and the section list. Keep settled decisions unless the user is
revising them.

Load `loom-plan:brainstorming` to run the discussion, with
`doc/product/<feature>/index.md` as its target. Ask about the problem, users and
goals one question at a time. Offer choices when
the answer space is small. Before settling a direction, propose two or three approaches
with their trade-offs and your recommendation. Play your understanding back to the user.

Write each part as the discussion settles it; never wait until the discussion ends.
Record decisions in words a reader without the chat can understand, including the reason
when it is not obvious.

## Shape the vision

Start the document with a level-1 title. Use this default outline, letting the user add
or drop sections:

- Problem
- Vision
- Principles
- Users
- Goals
- Non-goals
- Anti-goals
- Specification sections
- Decisions

Under `## Specification sections`, put a Markdown table with the zero-padded two-digit
section number in the first column, then the section name and what it covers:

```markdown
## Specification sections

| Number | Section | Covers |
| --- | --- | --- |
| 01 | Problem and users | The problem, its users and their needs. |
```

Explain that this table is the only part of the vision the app parses. It reads the
first Markdown table under the `Specification sections` heading and takes the number
from the first cell of each row; `01` maps to `doc/product/<feature>/section_01.md`. A table with
rows but no numbered first cell is an error. Without that heading and a table, the app
cannot read the section list; it never infers sections from prose.

Each row is the stub of its section: keep the `Covers` cell a true one-line scope. When a
section's scope moves past its row, update the row in the same turn or ask.

Under `## Decisions`, list decisions that span sections. Name the section numbers each
decision binds, so a section writer can identify its constraints.

## Repair the section list

For “fix the vision's section list”, edit the table into the parseable form above,
preserving the intended sections and their numbers. Resolve unclear names or scope
through the same discussion procedure.

For “add this unlisted section to the vision”, read
`doc/product/<feature>/section_<nn>.md` and add its number, title and coverage to
the table. Keep the existing file and its number;
write only the vision. Do not create or rewrite the section as part of this repair.

## Settled documents

Never write or edit a `Settled:` line; the app's Mark settled action writes it. When the turn
edits the vision and it has one, say so in the reply ("the vision was settled; it now reads
revised"). Editing a row of the table also changes the stub of the section it names, so say
which sections' stubs changed.

## Close the session

When the closing-write message arrives, ask no further questions. Write any settled
decisions still missing and put unresolved points under `## Open questions`, each with
options and your recommendation. Finish the turn-end duty, reply as the closing message
asks (`Closing write done.` and its `loom-closing` block), then stop. If there is nothing
to write, reply at once.

## Finish each editing turn

After any turn that edited documents, run `loom plan check` and fix what it reports;
then run `loom plan summary status --json`. Spawn one `loom-plan:summary` subagent per
stale, incomplete or missing document summary in parallel. Wait for them to return,
then spawn one `loom-plan:overview` subagent for the overviews that cite them. Spawn
nothing when nothing is stale.
