# 01 — Workflow Model and Project State

Part of the [Loom Plan product vision](index.md).

This section defines the six workflow steps as states, how the app reads the project's state
from the files under `doc/`, what it offers next, and how it reads a project that skipped or
reordered steps. Where a document convention is needed to make detection possible, this section
states the minimum the detector relies on; section 03 owns the full conventions.

## The model in one paragraph

The app stores no workflow state. On every start, and on every change under `doc/`, the server
reads the tree and builds one **checklist per feature**, scoped to `doc/product/<f>/` and
`doc/plans/<f>/`: one line per document the workflow expects, each either present or a gap.
Each feature's **current step** is the earliest step that has a gap. The app shows that step
in the feature's spine, offers to close its first gap, and lists every other gap as a secondary
offer. Nothing is blocked: a gap in an early step never stops the user from working on a later
one. The project view lists features and standalone plans; standalone plans have no checklist.

## The six steps

| Step | Name          | Done when                                                       |
| ---- | ------------- | --------------------------------------------------------------- |
| 1    | Vision        | `doc/product/<f>/index.md` exists                               |
| 2    | Product spec  | every section the feature's vision lists has its file           |
| 3    | Metaplan      | at least one metaplan exists in the feature                     |
| 4    | Plans         | every entry in the feature's metaplans has a plan file          |
| 5    | Pressure test | optional: the user decides which plans to pressure-test         |
| 6    | Run           | the user starts `loom run`; the app shows plans running or done |

Reviewing the result is the user reading, and the app records nothing about it (vision,
Decisions). It is no step and has no state: the screens of sections 05 and 06 are where it
happens.

Step 5 never holds a feature back. Once steps 1 to 4 have no gaps, the spine marks step 5
while any written plan has no pressure-test record, and offers to test it next to the offer to
start a run; the user picks either. An untested plan is a suggestion, never a gap.

With steps 1 to 4 complete the feature is "ready", because the app records no approval. The
spine marks step 6 as active while any plan is `IN_PROGRESS-`, and as done when every metaplan
entry's plan is `DONE-` or `RETIRED-`.

## Detection rules

Every rule reads plain files with names and headings a person can see. None needs app-specific
markup or a sidecar file.

### Features

A **feature** is the name shared by `doc/product/<f>/` and `doc/plans/<f>/`. Either directory
alone is enough: a spec can have no plans yet, and an engineering-only feature can have no spec.
Membership comes from location. Names match `[a-z0-9][a-z0-9-]*`, contain at most 64 characters,
and exclude `archive`, `briefs`, `plans` and `overview`. Documents under an invalid feature
directory are classified as `other`, with a `bad-feature-name` problem.

A vision at `doc/product/index.md`, a section directly under `doc/product/`, or a metaplan
directly under `doc/plans/` raises `needs-feature` when the document is unlocked. Its
`MoveIntoFeature` offer starts a conversation to move the document and what it lists into a
feature and fix the references. These documents do not form a project-wide checklist.

### Vision

The feature's vision is `doc/product/<f>/index.md`. Its listed sections are the rows of the first
Markdown table under a heading named `Specification sections` whose first cell is a number. Row `01`
expects `doc/product/<f>/section_01.md`; numbers are zero-padded to two digits.

A vision with no such heading, or a heading with no parseable table, is a gap of its own kind:
the checklist says "the vision lists no sections in a form the app can read" and offers a
session to fix the list. The app never guesses sections from prose.

### Product sections

A listed section is present when its file exists and is not empty. Its open questions (the items
under a heading named `Open questions`, which sessions write per the vision's
"Sessions start cold" decision) are counted and shown beside the section, but do not make it a
gap. Holding a step back until every question is closed would stop users from moving on with
questions they mean to settle later. (My call.)

A `section_<nn>.md` file in the feature's product directory that its vision does not list is an
**unlisted section**: shown in file mode and in the feature's checklist, with an offer to add it
to the vision's table when the vision exists. It is not a gap for step 2.

### Metaplans

A metaplan is `doc/plans/<f>/index.md` or any `doc/plans/<f>/*metaplan*.md`, directly in the
feature's plans directory; no configuration moves them. A feature can hold several, for
example one per planning round (`metaplan-NN.md`); each contributes its own entries. (My call:
the returning author adds a round rather than rewriting a finished metaplan.)

A metaplan entry is a heading of level 2 that names a plan file in backticks, the form the
existing metaplan uses:

```markdown
## 1. Delivery truth (`PLAN-02-delivery-truth.md`)
```

A heading without a plan file name is prose, not an entry ("Policy notes (no plan file)" stays
prose); a numbered heading with no plan file name is a convention problem unless it says
`(no plan file)`, as `## 6. Policy notes (no plan file)` does. The metaplan-writing skill (section 09) names the plan file in every entry heading, even
before the plan exists, so that step 4's gaps are countable.

### Plans

A plan is identified by its repository-wide slug, the part of the file name after the `PLAN-`
prefix. Within a feature, a metaplan entry naming `PLAN-foo.md` matches these lifecycle paths:

| File                                                               | Plan state |
| ------------------------------------------------------------------ | ---------- |
| `doc/plans/<f>/PLAN-foo.md`                                        | written    |
| `doc/plans/<f>/IN_PROGRESS-PLAN-foo.md`                            | running    |
| `doc/plans/<f>/DONE-PLAN-foo.md` or under `doc/plans/<f>/archive/` | done       |
| `doc/plans/<f>/archive/RETIRED-PLAN-foo.md`                        | retired    |

A retired plan records abandonment before running or during an unfinished run from the app
(section 11, "Abandoning an unfinished run"); it closes its metaplan entry. Running, done and
retired plans are read-only in the app (vision, Decisions, "Locked plans").

Copies of one slug in the same plan home, including its archive, raise `stray-plan-copy`;
copies across features or the standalone home raise `duplicate-plan-slug`. A metaplan naming
a plan outside its feature raises `metaplan-entry-foreign-plan`. That slug counts as present,
so the app offers no replacement plan to write; the problem calls for moving or relisting it.

`Implements: section_07` resolves against the plan's own feature's vision table. Brief bundles
live under `doc/plans/<f>/briefs/<slug>/`.

A plan in a feature that none of its metaplans names is an **unlisted plan**: read and reviewed
like any other, and offered for pressure testing like any other. It has no offer to add it to
a metaplan and is not a gap for step 4.

### Standalone plans

Standalone plans live directly under `doc/plans/`, with their archive under `doc/plans/archive/`
and briefs under `doc/plans/briefs/<slug>/`. They have their own lifecycle (written,
pressure-tested, running, done or retired), no six-step checklist, and write `Implements: none`.
Work with a product spec, a metaplan, or several related plans belongs in a feature.

The home screen shows standalone plans on one shelf with "Add plan" and
"Move plan". The shelf has no per-plan workflow offers.

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
text after the end marker does not affect validation.

Editing a plan after a test does not mark it untested. The address step edits the plan by
design, and refinements would otherwise reopen the offer on every change. The plan's view shows
the last test's date beside the plan's last change, so a reviewer can judge for themselves.
(My call.)

## What the app offers

Each feature's **primary offer** closes the first gap of its current step. Every other gap is
listed in the page's queue (below), each with its own offer. Accepting any offer opens a
new session briefed for that step and that document (section 09).

| Current step    | Primary offer                                                         |
| --------------- | --------------------------------------------------------------------- |
| 1               | Start a vision discussion                                             |
| 2               | Write the lowest-numbered missing section                             |
| 3               | Start a metaplan from the product spec                                |
| 4               | Write the first metaplan entry's missing plan                         |
| 5               | Pressure-test the first untested plan, or start a run (user's choice) |
| ready           | Refine any document; start a run (section 11)                         |
| ready, all done | none                                                                  |

A finished feature has no primary offer. Its two follow-ups, comparing the spec with what was
built and starting a new metaplan round, are secondary offers drawn only on the Run page
(section 11).

Offer labels are short, and the sentence beside the button explains what it does:

| Offer                            | Label                          |
| -------------------------------- | ------------------------------ |
| Start the vision                 | `Start the vision`             |
| Fix the vision's section list    | `Fix the section table`        |
| Write a missing section          | `Write section NN`             |
| Add an unlisted section          | `Add section NN to the vision` |
| Start a metaplan                 | `Start the metaplan`           |
| Write a missing plan             | `Write plan <slug>`            |
| Pressure-test a plan             | `Pressure-test <slug>`         |
| Start a run                      | `Start a run`                  |
| Compare spec with what was built | `Compare spec with build`      |
| Start a new metaplan round       | `New metaplan round`           |
| Draft all sections or plans      | `Draft all with a team`        |

**No screen draws one offer twice.** The chat panel carries no offer to start a step (section
08). On the product and engineering pages the page owns the primary offer while its pace and
queue show, and the spine keeps the sentence only; a queue row whose offer is the primary shows
"Next" with no button.

"First" follows document order: the vision's table order for sections, the metaplan's heading
order for plans, then unlisted plans by file name.

Refining is available in every state from any document, independent of the offers.

### Pace

Product spec and Plans offer a pace. **Guided** opens one `WriteSection` or `WritePlan` session
for the first missing document and stops for the user to read it. With two or more missing
sections, `DraftAllSections` follows the first `WriteSection` offer; with two or more missing
plans, `DraftAllPlans` follows the first `WritePlan` offer. Each drafts all missing documents
of that kind in the feature.

**Draft all, with a team** runs one authoring session leading up to five teammates, fewer when
fewer documents are missing. The lead shares a brief and assigns each file one owner;
teammates coordinate while drafting. The lead checks consistency and hands the user a review
queue ordered by open-question count. The cost line shows assignments and model before launch;
"Stop the team" ends it and keeps written documents. Section 09 owns the session procedure.
Pace belongs to the session request; it adds no stored workflow state or approval record.

## Where a feature opens

Opening `/f/<feature>` redirects by what exists for the feature:

| State                                | Opens on      |
| ------------------------------------ | ------------- |
| A vision exists                      | `product`     |
| No vision, a metaplan exists         | `engineering` |
| No vision and no metaplan            | `vision`      |

A feature with a vision opens on the product overview, where its reading starts, whether or not
every listed section is written. The `vision` page of a feature with no vision offers to start
one. The pages behind these routes are section 05's table.

## Skipped and reordered steps

Because each feature's current step is its earliest gap and nothing blocks, a feature with
steps out of order reads as a normal feature with an early gap:

- **Feature plans with no product spec**: the current step is 1 or 2, and the
  primary offer is to start the vision or the missing section. The plans, metaplan, and their
  checklist lines show and work as usual. The engineering overview is available; the product
  overview shows its empty state.
- **A feature spec with no vision**: section files exist but its `index.md` does not. They are
  unlisted sections; the primary offer is a vision discussion that lists them.
- **Feature plans with no metaplan**: every plan is unlisted; step 3 is the gap, and the plans that
  exist still get pressure-test offers.
- **Standalone plans**: no missing vision or metaplan creates a checklist gap for them.
- **Top-level spec or metaplan**: `needs-feature` and `MoveIntoFeature` describe the layout
  problem; the app does not infer a feature from these files.
- **Gaps that are deliberate**: the user can dismiss a secondary offer, or the primary one, for
  the length of the browser session. The dismissal lives only in that browser; the next
  `loom plan` start shows it again, since the app keeps no state of its own. (My call.) A
  permanent "we have no product spec" is expressed in the files, for example a vision that lists
  no sections, never in app state.

## When the state is recomputed

Each feature's checklist is a pure function of the file tree under `doc/` (paths, file names,
the headings and tables named above). The server recomputes the checklists at start and whenever
the file watcher the summary engine uses (section 04) reports a change, and pushes them to every
open tab. An edit from an editor, another session, or a `git pull` moves the spine without a reload.

Read-only tabs (vision, Decisions, "One authoring session per project") see the same feature
checklists but no offers.

## Out of scope for this section

- The full document conventions, `Implements:` lines, and cross-references: section 03.
- Whether a product section changed after the plans that implement it ran: section 10.
- How offers brief their sessions: section 09.
- Starting pressure tests and runs from the app: section 11. The `## Pressure tests` record
  above applies whether the test starts from the app or the terminal.
