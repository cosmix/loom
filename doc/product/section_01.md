# 01 — Workflow Model and Project State

Part of the [Loom Plan product vision](index.md).

This section defines the seven workflow steps as states, how the app reads the project's state
from the files under `doc/`, what it offers next, and how it reads a project that skipped or
reordered steps. Where a document convention is needed to make detection possible, this section
states the minimum the detector relies on; section 03 owns the full conventions.

## The model in one paragraph

The app stores no workflow state. On every start, and on every change under `doc/`, the server
reads the tree and builds a **project checklist**: one line per document the workflow expects,
each either present or a gap. The **current step** is the earliest step that has a gap. The app
shows the current step in the header tracker and offers to close its first gap, and lists every
other gap as a secondary offer. Nothing is blocked: a gap in an early step never stops the user
from working on a later one.

## The seven steps

| Step | Name          | Done when                                                             |
| ---- | ------------- | --------------------------------------------------------------------- |
| 1    | Vision        | `doc/product/index.md` exists                                         |
| 2    | Product spec  | every section the vision lists has its file                           |
| 3    | Metaplan      | at least one metaplan exists                                          |
| 4    | Plans         | every metaplan entry has a plan file                                  |
| 5    | Pressure test | optional: the user decides which plans to pressure-test               |
| 6    | Review        | never recorded; the user reading is the step (vision, Decisions)      |
| 7    | Run           | the user starts `loom run`; the app shows plans running or done       |

Step 5 never holds the project back. Once steps 1 to 4 have no gaps, the tracker marks step 5
while any written plan has no pressure-test record, and offers to test it next to the offer to
start a run; the user picks either. An untested plan is a suggestion, never a gap.

Steps 6 and 7 share one state, "ready", because the app records no approval. The tracker marks
step 7 as active while any plan is `IN_PROGRESS-`, and as done when every metaplan entry's plan
is `DONE-` or `RETIRED-`.

## Detection rules

Every rule reads plain files with names and headings a person can see. None needs app-specific
markup or a sidecar file.

### Vision

The vision is `doc/product/index.md`. Its listed sections are the rows of the first Markdown
table under a heading named `Specification sections` whose first cell is a number. Row `01`
expects `doc/product/section_01.md`; numbers are zero-padded to two digits.

A vision with no such heading, or a heading with no parseable table, is a gap of its own kind:
the checklist says "the vision lists no sections in a form the app can read" and offers a
session to fix the list. The app never guesses sections from prose.

### Product sections

A listed section is present when its file exists and is not empty. Its open questions (the items
under a heading named `Open questions`, which sessions write per the vision's
"Sessions start cold" decision) are counted and shown beside the section, but do not make it a
gap. Holding a step back until every question is closed would stop users from moving on with
questions they mean to settle later. (My call.)

A `section_<nn>.md` file the vision does not list is an **unlisted section**: shown in file mode
and in the checklist, with an offer to add it to the vision's table. It is not a gap for step 2.

### Metaplans

A metaplan is `doc/plans/index.md` or any `doc/plans/*metaplan*.md`. Metaplans live only in `doc/plans/`; no configuration moves them.
A project can hold several, for example one per planning round; each
contributes its own entries. (My call: the returning author adds a round rather than rewriting
a finished metaplan.)

A metaplan entry is a heading of level 2 that names a plan file in backticks, the form the
existing metaplan uses:

```markdown
## 1. Delivery truth (`PLAN-02-delivery-truth.md`)
```

A heading without a plan file name is prose, not an entry ("Policy notes (no plan file)" stays
prose). The metaplan-writing skill (section 09) names the plan file in every entry heading, even
before the plan exists, so that step 4's gaps are countable.

### Plans

A plan is identified by its slug, the part of the file name after the `PLAN-` prefix. A metaplan
entry naming `PLAN-foo.md` is satisfied by any of these:

| File                                                       | Plan state |
| ---------------------------------------------------------- | ---------- |
| `doc/plans/PLAN-foo.md`                                    | written    |
| `doc/plans/IN_PROGRESS-PLAN-foo.md`                        | running    |
| `doc/plans/DONE-PLAN-foo.md` or under `doc/plans/archive/` | done       |
| `doc/plans/archive/RETIRED-PLAN-foo.md`                    | retired    |

A retired plan was abandoned, either before it ran or by abandoning its unfinished run from the
app (section 11, "Abandoning an unfinished run"); it closes its metaplan entry. Running, done and
retired plans are read-only in the app (vision, Decisions, "Locked plans").

A `PLAN-*.md` file that no metaplan names is an **unlisted plan**: read and reviewed like any
other, offered for pressure testing like any other, and shown with an offer to add it to a metaplan. It is
not a gap for step 4. Projects that predate `loom plan` mostly consist of unlisted plans.

### Pressure tests

`loom pressure` deletes its `codex-<plan file name>` review report when it finishes, so no
file beside the plan survives to show a test happened. The record goes into the plan itself.

When a pressure run completes (both reviews and the address step), the `loom pressure` driver
appends one line to a `## Pressure tests` section at the very end of the plan file, after the
`<!-- END loom METADATA -->` marker, creating the section on the first run:

```markdown
## Pressure tests

- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 2 rounds, review addressed
- 2026-11-02: Claude opus, Codex gpt-6-sol; 1 round, Codex review missing in round 1
```

Each line gives the date, the `/pressure` model, the Codex model that ran after any fallback, the
round count, and the outcome; section 11 owns the exact format. The driver
writes the line, not the agents, so a run that fails or is interrupted leaves no line. A plan
is pressure-tested when this section has at least one line. (My call: the date and the models
are what a reviewer needs to judge how much a test is worth; the findings themselves live in
the plan edits the address step made.)

The record is plain Markdown a reader without the app can follow, and it travels with the plan
through its `IN_PROGRESS-` and `DONE-` renames. Plan parsing reads only the metadata block, so
text after the end marker does not affect validation; the change to `loom pressure` that writes
the line must confirm `loom plan verify` and `loom init` still accept the file.

Editing a plan after a test does not mark it untested. The address step edits the plan by
design, and refinements would otherwise reopen the offer on every change. The plan's view shows
the last test's date beside the plan's last change, so a reviewer can judge for themselves.
(My call.)

## What the app offers

The **primary offer** closes the first gap of the current step. Every other gap is listed in a
compact checklist under it, each with its own offer. Accepting any offer opens a new session
briefed for that step and that document (section 09).

| Current step    | Primary offer                                                          |
| --------------- | ---------------------------------------------------------------------- |
| 1               | Start a vision discussion                                              |
| 2               | Write the lowest-numbered missing section                              |
| 3               | Start a metaplan from the product spec                                 |
| 4               | Write the first metaplan entry's missing plan                          |
| 5               | Pressure-test the first untested plan, or start a run (user's choice)  |
| ready           | Refine any document; start a run (section 11)                          |
| ready, all done | Refine the spec in light of what was built; start a new metaplan round |

"First" follows document order: the vision's table order for sections, the metaplan's heading
order for plans, then unlisted plans by file name.

Refining is available in every state from any document, independent of the offers.

## Skipped and reordered steps

Because the current step is the earliest gap and nothing blocks, a project that went out of
order reads as a normal project with an early gap:

- **Plans with no product spec** (this repository today): the current step is 1 or 2, and the
  primary offer is to start the vision or the missing section. The plans, metaplan, and their
  checklist lines show and work as usual. The engineering overview is available; the product
  overview shows its empty state.
- **A spec with no vision**: section files exist but `index.md` does not. They are unlisted
  sections; the primary offer is a vision discussion that lists them.
- **Plans with no metaplan**: every plan is unlisted; step 3 is the gap, and the plans that
  exist still get pressure-test offers.
- **Gaps that are deliberate**: the user can dismiss a secondary offer, or the primary one, for
  the length of the browser session. The dismissal lives only in that browser; the next
  `loom plan` start shows it again, since the app keeps no state of its own. (My call.) A
  permanent "we have no product spec" is expressed in the files, for example a vision that lists
  no sections, never in app state.

## When the state is recomputed

The checklist is a pure function of the file tree under `doc/` (paths, file names, the headings
and tables named above). The server recomputes it at start and whenever the file watcher the
summary engine uses (section 04) reports a change, and pushes the new checklist to every open
tab. An edit from an editor, another session, or a `git pull` moves the tracker without a
reload.

Read-only tabs (vision, Decisions, "One authoring session per project") see the same checklist
but no offers.

## Out of scope for this section

- The full document conventions, `Implements:` lines, and cross-references: section 03.
- Whether a product section changed after the plans that implement it ran: section 10.
- How offers brief their sessions: section 09.
- Starting pressure tests and runs from the app: section 11. The `## Pressure tests` record
  above is a change to `loom pressure` itself, needed whether the test starts from the app or
  the terminal.
