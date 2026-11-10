# 03 — Document Model and Conventions

Part of the [Loom Plan product vision](index.md).

This section defines which paths hold which documents, how the server discovers them, how a
document's kind and parent follow from where it sits and what names it, how documents refer to
each other, and how a heading becomes an address that summaries and the reader can link to.
Section 01 set the minimum conventions its detector needs; this section generalises them into
one model and does not change any of them.

## The model in one paragraph

A document's kind comes from its path and file name, nothing else: no front matter, no marker
comment, no sidecar file. Parent links run one way, from the document that lists to the document
listed (the vision lists its sections, a metaplan lists its plans, a plan's slug names its brief
bundle), with one exception: edge lines. A metaplan entry names the sections or headings its
plan will deliver in a `Covers:` line, a plan names those it implements in an `Implements:` line,
and a stage in an `Implements (stage:<id>):` line. Every link is a path, a file name, or a heading
a person can read, so a reader without the app can follow the same tree the app draws. (My call:
one direction per link means two files can never disagree about a relationship.) The one other
record the documents hold is the reader's `Settled:` line, below.

## Discovery

The server walks `doc/` on disk at start and on every change the file watcher reports (section
04), including feature directories under `doc/product/` and `doc/plans/` and standalone plans
at the top of `doc/plans/`. A feature's membership comes from its directory name, shared across
the two trees; either directory alone is enough for a feature. No document has to declare its
feature.

It reads files git ignores as well: this repository ignores `doc/plans/DONE-*`,
`doc/plans/REVIEW-*` and their feature-directory equivalents, and both kinds matter to a
returning author. File mode marks an ignored file "not committed" so nobody mistakes it for
shared content. A clone may
lack these plans, and a user may have deleted them; a missing plan file may be deleted or never
committed, and the app does not try to tell which.

The walk skips dotfiles (`.DS_Store`), the summary store `doc/loom/summaries/` (project content
owned by section 04, never shown as a document), and every symlink except one whose target,
resolved, is a regular file inside `doc/` (outside `doc/loom/summaries/`); symlinked
directories are always skipped (section 12, "Symlinks under `doc/`").

## Document kinds

| Kind            | Path                                                                                           | Parent                                            |
| --------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| Standalone plan | `doc/plans/PLAN-<slug>.md` and its lifecycle spellings, including `doc/plans/archive/`         | none                                              |
| Brief           | any file under `doc/plans/briefs/<slug>/`, and briefs a standalone plan names                  | the plan with that slug                           |
| Run review      | `REVIEW-<plan file stem>.md` in `doc/plans/` or `doc/plans/archive/`                           | the plan with that slug                           |
| Vision          | `doc/product/<f>/index.md`                                                                     | none (root of the feature's product tree)         |
| Product section | `doc/product/<f>/section_<nn>.md`                                                              | the feature's vision, through its sections table  |
| Product note    | any other file under `doc/product/<f>/`                                                        | the feature's vision, as an attachment            |
| Metaplan        | `doc/plans/<f>/index.md`                                                                       | none (root of the feature's engineering tree)     |
| Feature plan    | `doc/plans/<f>/PLAN-<slug>.md` and its lifecycle spellings, including `doc/plans/<f>/archive/` | the metaplan in the feature whose entry names it  |
| Brief           | any file under `doc/plans/<f>/briefs/<slug>/`, and briefs a feature plan names                 | the plan with that slug                           |
| Run review      | `REVIEW-<plan file stem>.md` in `doc/plans/<f>/` or `doc/plans/<f>/archive/`                   | the plan with that slug                           |
| Knowledge       | any file under `doc/loom/knowledge/`                                                           | the knowledge index (see below)                   |
| Other           | anything else under `doc/`                                                                     | none                                              |

Any other `*metaplan*.md` file in a feature is an `Other` document: a feature has one metaplan.
So is `doc/plans/<f>/findings.md`, the feature's findings ledger, which has a format of its own
(below).

Here `<f>` is a feature name: `[a-z0-9][a-z0-9-]*`, at most 64 characters. `archive`, `briefs`,
`plans` and `overview` are reserved feature names. At the first directory under `doc/plans/`,
`archive` and `briefs` name the standalone archive and brief bundles instead of features.
Documents under an invalid feature name are `other`, with a `bad-feature-name` problem on the
directory. Plans and run reviews live directly in their home or its `archive/`; files nested
deeper under an archive are `other`. Brief bundles may contain nested directories.

A vision at `doc/product/index.md`, a section directly under `doc/product/`, or a metaplan
at `doc/plans/index.md` keeps its kind but raises `needs-feature`: it belongs in a feature
directory. The top of `doc/plans/` holds standalone plans, their archive, briefs and run reviews.

The title of a Markdown document is its first level-1 heading, else its file name. Product
notes and other documents appear in file mode only; neither has a summary nor a checklist line.

### Product sections

Section 01's rules stand: the vision's `Specification sections` table lists sections by number,
row `07` expects `section_07.md` in that feature's product directory, and a section file the
feature's table does not list is an unlisted section. The number identifies a section within
its feature; its title comes from its own first heading, and
when that differs from the table's name the app shows both in file mode so the user can fix
whichever is wrong. (My call.)

A section opens with a line linking back to the vision (`Part of the [Loom Plan product
vision](index.md).`). The writing skill adds it for plain readers (section 09); discovery ignores
it, since the table already declares the parent.

### Plans and their lifecycle

A plan's identity is its **slug**: the file stem with `IN_PROGRESS-`, `DONE-`, `RETIRED-` and
`PLAN-` stripped. Slugs are unique repository-wide, across features and standalone plans.
A plan's **home** is `doc/plans/` for a standalone plan or `doc/plans/<f>/` for a feature plan;
an archived plan keeps the same home. The app and `loom run` use the same slug and home to find
its brief bundle. Every spelling below with one slug identifies the same plan:

| File                                                                             | State   |
| -------------------------------------------------------------------------------- | ------- |
| `doc/plans/PLAN-<slug>.md`                                                       | written |
| `doc/plans/IN_PROGRESS-PLAN-<slug>.md`                                           | running |
| `doc/plans/DONE-PLAN-<slug>.md`, `doc/plans/archive/DONE-PLAN-<slug>.md`         | done    |
| `doc/plans/archive/RETIRED-PLAN-<slug>.md`                                       | retired |
| `doc/plans/<f>/PLAN-<slug>.md`                                                   | written |
| `doc/plans/<f>/IN_PROGRESS-PLAN-<slug>.md`                                       | running |
| `doc/plans/<f>/DONE-PLAN-<slug>.md`, `doc/plans/<f>/archive/DONE-PLAN-<slug>.md` | done    |
| `doc/plans/<f>/archive/RETIRED-PLAN-<slug>.md`                                   | retired |

`RETIRED-` identifies an abandoned plan: one retired before it runs, or one whose unfinished
run is abandoned (section 11, "Abandoning an unfinished run"). Abandoning moves the running
plan into its own home's `archive/RETIRED-PLAN-<slug>.md`. A retired plan closes its metaplan
entry, since retiring is a decision, and the checklist shows it as retired instead of done.
Retired plans are read-only, like running and done ones; each home's archive is read-only too.
Discovery also recognises `RETIRED-PLAN-<slug>.md` directly in the home. In an archive, any
other recognised plan spelling is done, even without `DONE-`.

If two files carry one slug, the most advanced state wins (retired, then done, then running,
then written); equal states are settled by the lexicographically first path. A losing copy
in the winning plan's home, including its archive, raises `stray-plan-copy`. A losing copy in
another feature's home or between a feature and the standalone shelf raises
`duplicate-plan-slug`. Everything that refers to a plan
(metaplan entries, cross-references, summary citations) resolves through the slug, so a link
written as `PLAN-foo.md` keeps working after `loom run` renames the file.

### Metaplans

A metaplan entry is a level-2 heading that names a plan file in backticks (section 01). Any
lifecycle spelling in the backticks resolves by slug, so an entry appended after a plan ran can
name `DONE-PLAN-foo.md`. The entry's heading section, from the heading to the next level-2
heading, is the plan's description in the metaplan; the plan's view links to it. A plan named
by two entries has both as parents. An entry naming a plan from another
feature or the standalone shelf raises `metaplan-entry-foreign-plan`.

### Briefs

A plan's briefs are the files `loom run` treats as its inputs, found the way
`collect_inputs` finds them (`loom/src/plan/inputs.rs:89`):

1. the conventional bundle, `<plan home>/briefs/<slug>/`, every file under it:
   `doc/plans/<f>/briefs/<slug>/` for a feature plan, `doc/plans/briefs/<slug>/` for a standalone plan;
2. every `doc/plans/<f>/briefs/...` or `doc/plans/briefs/...` path token in the plan, read by
   `extract_brief_paths` (`loom/src/plan/schema/structural_checks.rs`);
3. the same tokens in every brief already found, recursively.

The app calls the same code, so the brief list it shows is the list `loom run` checks. A brief
reference that does not resolve shows as a broken reference on the plan.

Inside a bundle, the layout is read as structure (`<plan home>` has the meaning above):

```text
<plan home>/briefs/<slug>/
  common.md            plan-wide brief, read by every worker
  <stage-id>/          one directory per stage, named by the stage's id
    common.md          stage-wide brief
    w1-<name>.md       one brief per worker
  <other files>        fixtures, mockups, JSON (shown as text or raw)
```

A directory whose name matches a stage id in the plan's metadata is grouped under that stage;
anything else is listed under the plan as "other briefs". A bundle whose slug matches no plan is
an **orphan bundle**, listed in file mode under its own heading and not a checklist gap.

The files under the conventional bundle directory are the plan's **bundle**; the settled mark
below and the plan's summary (section 04) both cover it. A brief has no summary of its own: the
plan's summary has one detail paragraph per stage that cites the stage and its briefs. The link
from a brief to its stage comes from its path, `briefs/<slug>/<stage-id>/...`, so no line names
it. A plan's pages list its briefs, each linked to its source; they are not rows in the spine.
(My call.)

### Run reviews

`loom run` writes a code review per plan as `REVIEW-<plan file stem>.md`
(`generate` in `loom/src/fs/plan_review/mod.rs`), for example `REVIEW-PLAN-foo.md` or
`REVIEW-IN_PROGRESS-PLAN-foo.md`. The app attaches it to the plan by slug and shows it in file
mode as the record of what was built. It is never summarized and never edited by planning
sessions. (My call: section 10 uses it to show what changed since a plan ran.)

### The knowledge base

`doc/loom/knowledge/` keeps its own hierarchy, and file mode presents it the way the knowledge
CLI defines it (`doc/loom/knowledge/architecture/knowledge-hierarchy.md`, "Layout Predicate" and
"INDEX.md Generation"):

- When `INDEX.md` exists, it is the entry page. The tree shows the tier-1 files, then the
  tier-2 topics grouped by category directory, each with the blurb and line count the index
  gives. `mistakes.md` and `mistakes/` sit in the same tree.
- Without `INDEX.md` (a flat, legacy directory), the files are listed by name.

Knowledge files are read-only in the app and change only through `loom knowledge`, run by a chat
session. `INDEX.md` is regenerated by every knowledge write, so the app shows it with a notice
saying so. The knowledge base has no summaries, no
overview, and no checklist lines (vision, Decisions).

## Product-to-plan links

A plan names the product sections or headings it implements in one plain line in its prose,
above the `<!-- loom METADATA -->` marker, conventionally directly under its title:

```markdown
# PLAN: Session Bridge

Implements: section_07, section_08#session-states
```

Rules:

- The line starts with `Implements:` at the start of a line, outside fenced code. The first such
  line in the plan counts; a second is flagged as a duplicate.
- Each value is a section token, `section_<nn>` or `section_<nn>#<anchor>`, separated by commas.
  A link (`[section_07](../../product/<f>/section_07.md)`) or a file name (`section_07.md`) is
  read as the same token, so an author can make the line clickable on GitHub. A section named
  both whole and by heading counts as the whole section.
- `Implements: section_07` resolves against the plan's own feature's vision table and
  `doc/product/<f>/section_07.md`. A link's target does not select another feature.
- `Implements: none` states that the plan implements no product section (tooling, cleanup).
  A standalone plan writes `Implements: none`; section tokens on it raise `implements-broken`.
  A plan with no line at all is "unmapped"; an unmapped feature plan raises `implements-missing`.
  Concept mode (section 05) shows unmapped plans
  apart from plans that declared `none`, because the first is a gap in the record and the
  second is a decision. (My call.)
- A token naming a section the feature's vision does not list, or a heading the section does not
  have, is a broken reference, shown on the plan.
- A plan that implements part of a section names the headings it delivers. The anchor is the
  heading's GitHub anchor (below), so renaming a heading breaks the link visibly, as a broken
  reference on the plan, and never silently.

The plan-writing skill adds the line (section 09). The line sits in prose, so plan validation,
which reads only the metadata block (`loom/src/plan/parser/extraction.rs:75`), is unaffected.
Metaplans carry no `Implements:` lines; their entries carry `Covers:` lines (below), and section
05 builds the links between each feature's two overviews from the edge lines only (vision,
Decisions).

## Edge lines

Four line types carry the links between documents. Each starts at the beginning of a line,
outside fenced code and before the first `<!-- loom METADATA` line, and holds comma-separated
tokens; the app trims the whitespace around them.

| Line                                | Where                                                                                                | Meaning                                        |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| `Covers: <tokens>`                  | inside a metaplan entry, after its `##` heading and before the next heading of level 2 or shallower | the entry's plan will deliver these spec nodes |
| `Implements: <tokens>`              | the plan's declaration, above                                                                        | the plan delivers these sections or headings   |
| `Implements (stage:<id>): <tokens>` | anywhere in a plan's prose                                                                           | one stage delivers these headings              |
| `About: <tokens>`                   | inside a findings ledger entry                                                                       | the nodes a finding concerns                   |

The stage line exists because stage ids live in the plan's YAML, which gains no field. (My call.)
A stage line names headings the stage delivers, for example:

```text
Implements (stage:scheduler): section_04#rebuild-budget, section_04#invalidation
```

A token resolves relative to the document's feature `<f>`:

| Token                                                                  | Node                                                        |
| ---------------------------------------------------------------------- | ----------------------------------------------------------- |
| `section_NN` or `section_NN.md`                                        | `doc/product/<f>/section_NN.md`, the whole section          |
| `section_NN#<anchor>`                                                  | one heading of that section                                 |
| `vision` or `vision#<anchor>`                                          | `doc/product/<f>/index.md` or one of its headings           |
| `metaplan` or `metaplan#<anchor>`                                      | `doc/plans/<f>/index.md` or one of its entries              |
| `PLAN-<slug>.md`, also with a lifecycle prefix (`DONE-PLAN-<slug>.md`) | the plan with that slug, whichever file spelling is current |
| `PLAN-<slug>.md#stage:<id>` or `PLAN-<slug>.md#<anchor>`               | a stage or a heading of that plan                           |
| `[label](<target>)`                                                    | the target's file name and fragment, read as above          |

`NN` is one or more digits. An anchor is a heading's GitHub anchor (see "Headings as anchors");
`stage:<id>` is the stage anchor of "Plan metadata". Anything else is a bad token. The app keeps
a bad token as written and reports it on its document. A well-formed token that names a missing
document or heading is unresolved and is reported as well. `Covers:` and both `Implements` lines
accept section tokens; `About:` accepts every kind of token.

## Settled marks

A reader who agrees to a document settles it. The mark is one plain line in the document:

```text
Settled: <body> [stub <stub>] [upstream <upstream>]
```

For example, `Settled: 3f9a1c0b7d42 stub 9e01b5c2a8f3`. Each hash is the first 12 lowercase hex
digits of a SHA-256.

- **Placement.** The line is its own paragraph, directly after the document's first `#` heading.
  When the next non-blank line after that heading is an `Implements:` line, the Settled line goes
  after it. Settling again replaces the line in place; unsettling removes it. Lines that start
  with `Status:` are ordinary prose and unrelated, which is why the mark is named `Settled:`.
- **Body hash.** The content hash (section 04) of the document's text with its Settled line
  removed. For a plan the body is the plan and its brief bundle: the hash covers the plan's text
  and then, in path order, each brief's repository path and text.
- **Stub hash.** The stub is the document's scope before it was written. For a product section it
  is the vision's table row for that section; for a plan it is its metaplan entry's heading
  section. The vision, the metaplan and a plan with no entry have no stub.
- **Upstream hash.** Plans only. It covers each heading the plan implements, through its
  `Implements:` and stage lines, plus each section it implements whole.
- **States.** Draft (no line, or one that does not parse), revised (the body hash differs), stub
  changed (the body matches, the stub differs or is gone), upstream changed (body and stub
  match, the upstream differs) and settled (everything matches), checked in that order.
  Section 01 gives the names the app shows.
- **Who writes it.** Only the app writes or removes the line, at the reader's request from the
  document's page. Sessions never write or edit a Settled line. A session that edits a settled
  document says so in its reply; the document reads as revised until it is settled again.
- **Which documents.** The vision, product sections, the metaplan, and plans with no lifecycle
  prefix. A locked document (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, archive) cannot be settled or
  unsettled, and keeps whatever line it held when it locked.
- **Unresolved points.** A document that holds `(My call.)` markers or open questions asks the
  reader to confirm before it is settled, listing them. (My call.)
- **Changes since settled.** The page for a revised document compares it with the settled
  version: the newest commit whose version of the file holds the current Settled line. Settling
  and unsettling commit the line (section 10, "The settle commit"), so a settle normally has one.
  When no commit holds it, the page says the settled version was never committed and shows no
  comparison.
- **Problems.** A Settled line that does not parse (a hash that is not 12 lowercase hex digits,
  an unknown word, words out of order) raises `settled-bad` and reads as draft. A second Settled
  line raises `settled-duplicate`.

## The findings ledger

Each feature keeps its findings in one plain Markdown file, `doc/plans/<f>/findings.md`, an
`Other` document. Run reviews stay the unchanged evidence; the ledger holds the triage state.

```markdown
# Findings

## F-1 Rebuild threshold ignores briefs

About: PLAN-feat-planner-03-summaries.md#stage:scheduler, section_04#rebuild-budget
Source: REVIEW-PLAN-feat-planner-03-summaries.md#open-findings (scheduler, 1a2b3c4d5e6f)
Status: open

The rebuild budget counts plan summaries but not brief changes.
```

- **Entries.** An entry is a `## F-<n> <title>` heading (n is 1 or more and unique in the file),
  then an `About:` line, an optional `Source:` line and an optional `Status:` line in any order,
  then one paragraph of text. A heading of any other shape raises `findings-bad-entry`.
- **Statuses.** `Status:` is `open`, `planned PLAN-<slug>.md`, `folded metaplan#<entry anchor>`
  or `dismissed: <reason>`. A missing or unknown value reads as open and is reported.
- **Import.** When a plan completes and its run review is written, each item under the review's
  `## Open Findings` and `## Open Questions` headings is added as an open entry that is not
  already present; importing the same review twice adds nothing. The entry's `Source:` line
  names the review heading, the stage and a 12-hex hash of the item text, which is how a repeat
  is recognised. Its `About:` line names the stage and then the plan's `Implements:` tokens. A
  standalone plan, or a plan whose feature cannot be read from its path, imports nothing.
- **Review the feature.** The same session that section 01 describes appends entries with
  `Status: open`, an `About:` line naming the most specific nodes it can, and either no `Source:`
  line or `Source: review <YYYY-MM-DD>`.
- **Triage.** The reader plans a finding (a follow-up session sets `planned PLAN-<slug>.md`),
  folds it into a metaplan entry whose plan has not started running (the session edits the entry
  and sets `folded metaplan#<anchor>`), or dismisses it with a reason. A reason is one line of at
  most 200 characters after control characters are removed, and cannot be empty. The metaplan
  entry that received a folded finding reads as "revisit before writing" or "revisit before
  running" until its plan runs.
- **Untrusted text.** Item text is written by agents. The title is the first 80 characters of the
  text on one line; the paragraph is the whole text on one line, and a paragraph that starts with
  `#` gets a leading backslash, so a finding never creates a heading. The app shows finding
  text, titles and reasons as plain text.
- **Where the file lands.** Import runs in the daemon's host job against the main checkout, so
  the new entries are an uncommitted change there, like the summaries the planner rebuilds. The
  user commits them. Writes refuse a symlinked `findings.md` or a symlinked directory above it
  (section 12, "Symlinks under `doc/`").

## Cross-references

The app recognises three forms of reference in any Markdown document, and resolves each to a
document and, when given, a heading section:

| Form                   | Example                                                 |
| ---------------------- | ------------------------------------------------------- |
| Relative Markdown link | `[detection rules](section_01.md#detection-rules)`      |
| Repository path token  | `` `doc/plans/briefs/loom-ui-mods/loom-ui/common.md` `` |
| Plan file name         | `` `PLAN-02-delivery-truth.md` ``                       |

Plan file names resolve by slug from anywhere in `doc/`. Path tokens follow the scan
`extract_brief_paths` uses, widened to any `doc/` path; placeholders such as
`doc/plans/briefs/<plan>/<stage-id>/` name a layout and are skipped. Links to code (`src/...`,
`loom/src/...:120`) are shown as text: the app reads `doc/`, not the source tree. (My call.)

A reference that does not resolve is listed on its document as broken, in file mode (section
06). It is never a checklist gap: a broken link does not stop any step.

## Headings as anchors

Summary sentences cite heading sections of raw documents (sections 04 and 05), and open source
passages are keyed to the section they came from (vision, Decisions, "Refinement"). A heading
section is the unit for both.

**Headings.** CommonMark headings outside fenced code blocks. Lines starting with `#` inside a
fence (plan YAML, shell comments, examples) are not headings.

**Extent.** A heading section runs from its heading to the line before the next heading of the
same or a higher level, so it contains its subsections. The text before a document's first
heading is its **preamble**, addressed by the document alone.

**Anchor.** The anchor is the heading's slug as GitHub computes it: lower-cased, punctuation
other than `-` and `_` removed, spaces turned into `-`, and repeated slugs in one file numbered
`-1`, `-2` in document order. `## 4.2 Line Grammar` becomes `#42-line-grammar`. A citation is
the document's id plus the anchor. (My call: these are the anchors GitHub and most editors
already render, so a cross-reference a session writes works outside the app too.)

**Document id.** The id is stable across renames that do not change what the document is: the
vision is `product/<f>/index`, a section is `product/<f>/section_<nn>`, a plan is `plan/<slug>`
whether it belongs to a feature or is standalone, and every other document is its path under
`doc/`. A plan's citations survive `PLAN-` to `IN_PROGRESS-` to `DONE-` or `RETIRED-`, moves into
`archive/`, and moves between homes. Changing a product document's feature changes its id.
Misplaced top-level visions and sections retain `product/index` and `product/section_<nn>`
while carrying `needs-feature`.

**Document paths on the wire.** On the wire every document path is repository-relative
(`doc/...`); only request parameters naming a document and the browser's `/files/<path>`
addresses are relative to `doc/`.

**Plan metadata.** The YAML between `<!-- loom METADATA -->` and `<!-- END loom METADATA -->`
is one section with the anchor `#loom-metadata`, and each stage inside it is addressable as
`#stage:<stage-id>`, so a summary can cite the stage that does a piece of work. These two anchors
are app-only; nothing in the file carries them. The `## Pressure tests` record after the end
marker (section 01) is an ordinary heading section.

**Renamed and removed headings.** Renaming a heading changes its anchor. A citation whose
anchor no longer resolves is stale: the summary engine rebuilds the summary that holds it
(section 04), and an open passage keyed to it folds back with a note (vision, Decisions). The
app does not guess which new heading an old anchor meant.

## What the writing sessions keep to

Section 09 gives the sessions their skills; these are the conventions the skills must produce:

- Files at the paths in the kinds table, named as given there.
- A level-1 heading as the first line of every document.
- In each product section: the back-link line, decisions under headings that name the topic,
  open questions under a heading named `Open questions` (section 01).
- In each metaplan entry heading: the plan file name in backticks, before the plan exists.
- In each plan: an `Implements:` line under the title; brief paths written out in full.
- Cross-references as relative links with GitHub anchors, or as `doc/` path tokens.
- Stable headings: a session rewording a heading that summaries cite does so on purpose, since
  it costs a rebuild.

## Out of scope for this section

- Detection of steps, gaps and pressure-test records: section 01.
- Hashing, the manifest, invalidation, and the summary store's layout: section 04.
- How overviews, document summaries and product-to-plan links are shown: section 05.
- How the trace, the Coverage and Findings pages, and the settle controls are shown: section 01
  (states and pages), sections 05 and 10 (reading and review).
- Rendering, navigation, editing, save-time validation and broken-reference display: section 06.
- The skills that write these conventions: section 09.
- What a run review is used for after execution: section 10.
- Which paths sessions may write, and symlink handling: section 12.
