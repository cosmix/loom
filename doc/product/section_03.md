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
bundle), with one exception: a plan names the product sections it implements in an
`Implements:` line. Every link is a path, a file name, or a heading a person can read, so a
reader without the app can follow the same tree the app draws. (My call: one direction per link
means two files can never disagree about a relationship.)

## Discovery

The server walks `doc/` on disk at start and on every change the file watcher reports (section
04). It reads files git ignores as well: this repository ignores `doc/plans/DONE-*` and
`doc/plans/REVIEW-*` (`.gitignore:68-69`), and both kinds matter to a returning author. File
mode marks an ignored file "not committed" so nobody mistakes it for shared content. A clone may
lack these plans, and a user may have deleted them; a missing plan file may be deleted or never
committed, and the app does not try to tell which.

The walk skips dotfiles (`.DS_Store`), the summary store `doc/loom/summaries/` (project content
owned by section 04, never shown as a document), and every symlink except one whose target,
resolved, is a regular file inside `doc/` (outside `doc/loom/summaries/`); symlinked
directories are always skipped (section 12, "Symlinks under `doc/`").

## Document kinds

| Kind            | Path                                                                  | Parent                                      |
| --------------- | --------------------------------------------------------------------- | ------------------------------------------- |
| Vision          | `doc/product/index.md`                                                | none (root of the product tree)             |
| Product section | `doc/product/section_<nn>.md`                                         | the vision, through its sections table      |
| Product note    | any other file under `doc/product/`                                   | the vision, as an attachment                |
| Metaplan        | `doc/plans/index.md`, `doc/plans/*metaplan*.md`                       | none (root of the engineering tree)         |
| Plan            | `PLAN-<slug>.md` and its lifecycle spellings (below)                  | each metaplan whose entry names the slug    |
| Brief           | any file under `doc/plans/briefs/<slug>/`, and briefs a plan names    | the plan with that slug                     |
| Run review      | `REVIEW-<plan file stem>.md` in `doc/plans/` or `doc/plans/archive/`  | the plan with that slug                     |
| Knowledge       | any file under `doc/loom/knowledge/`                                  | the knowledge index (see below)             |
| Other           | anything else under `doc/`                                            | none                                        |

The title of a Markdown document is its first level-1 heading, else its file name. Product
notes and other documents appear in file mode only; neither has a summary nor a checklist line.

### Product sections

Section 01's rules stand: the vision's `Specification sections` table lists sections by number,
row `07` expects `section_07.md`, and a section file the table does not list is an unlisted
section. The number is the section's identity; its title comes from its own first heading, and
when that differs from the table's name the app shows both in file mode so the user can fix
whichever is wrong. (My call.)

A section opens with a line linking back to the vision (`Part of the [Loom Plan product
vision](index.md).`). The writing skill adds it for plain readers (section 09); discovery ignores
it, since the table already declares the parent.

### Plans and their lifecycle

A plan's identity is its **slug**: the file stem with `IN_PROGRESS-`, `DONE-`, `RETIRED-` and
`PLAN-` stripped. `loom run` finds a plan's brief bundle by stripping `IN_PROGRESS-`, `DONE-` and
`PLAN-` only (`loom/src/plan/inputs.rs:140-142`); that lookup gains `RETIRED-` as part of this
work, so the app and `loom run` agree on every plan's slug. Every spelling below is the same
plan:

| File                                                       | State   |
| ---------------------------------------------------------- | ------- |
| `doc/plans/PLAN-<slug>.md`                                 | written |
| `doc/plans/IN_PROGRESS-PLAN-<slug>.md`                     | running |
| `doc/plans/DONE-PLAN-<slug>.md`, `doc/plans/archive/DONE-PLAN-<slug>.md` | done    |
| `doc/plans/archive/RETIRED-PLAN-<slug>.md`                 | retired |

The first three rows are section 01's. `RETIRED-` is a convention the archive already uses
(`doc/plans/archive/RETIRED-PLAN-secure-distilled-loom-v2.md`) for an abandoned plan: one
retired before it ran, or one whose unfinished run was abandoned (section 11, "Abandoning an
unfinished run"), which moves `doc/plans/IN_PROGRESS-PLAN-<slug>.md` to this path. A retired plan closes its metaplan entry, since retiring was a decision, and the checklist
shows it as retired instead of done. Retired plans are read-only, like running and done ones.

If two files carry one slug, the most advanced state wins (retired, then done, then running,
then written) and file mode flags the others as stray copies. Everything that refers to a plan
(metaplan entries, cross-references, summary citations) resolves through the slug, so a link
written as `PLAN-foo.md` keeps working after `loom run` renames the file.

### Metaplans

A metaplan entry is a level-2 heading that names a plan file in backticks (section 01). Any
lifecycle spelling in the backticks resolves by slug, so a metaplan written after a plan ran can
name `DONE-PLAN-foo.md`. The entry's heading section, from the heading to the next level-2
heading, is the plan's description in that metaplan; the plan's view links to it. A plan named
by two metaplans has both as parents.

### Briefs

A plan's briefs are the files `loom run` treats as its inputs, found the way
`collect_inputs` finds them (`loom/src/plan/inputs.rs:89`):

1. the conventional bundle, `doc/plans/briefs/<slug>/`, every file under it;
2. every `doc/plans/briefs/...` path token in the plan, read by `extract_brief_paths`
   (`loom/src/plan/schema/structural_checks.rs:41`);
3. the same tokens in every brief already found, recursively.

The app calls the same code, so the brief list it shows is the list `loom run` checks. A brief
reference that does not resolve shows as a broken reference on the plan.

Inside a bundle, the layout loom plans already use is read as structure:

```text
doc/plans/briefs/<slug>/
  common.md            plan-wide brief, read by every worker
  <stage-id>/          one directory per stage, named by the stage's id
    common.md          stage-wide brief
    w1-<name>.md       one brief per worker
  <other files>        fixtures, mockups, JSON (shown as text or raw)
```

A directory whose name matches a stage id in the plan's metadata is grouped under that stage;
anything else is listed under the plan as "other briefs". A bundle whose slug matches no plan is
an **orphan bundle**, listed in file mode under its own heading and not a checklist gap.

Briefs have no summaries of their own (vision, Decisions); they are reached from their plan in
file mode and from the plan's summary where it cites them.

### Run reviews

`loom run` writes a code review per plan as `REVIEW-<plan file stem>.md`
(`loom/src/fs/plan_review/mod.rs:33`), for example `REVIEW-PLAN-foo.md` or
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

A plan names the product sections it implements in one plain line in its prose, above the
`<!-- loom METADATA -->` marker, conventionally directly under its title:

```markdown
# PLAN: Session Bridge

Implements: section_07, section_08
```

Rules:

- The line starts with `Implements:` at the start of a line, outside fenced code. The first such
  line in the plan counts; a second is flagged as a duplicate.
- Each value is a section token `section_<nn>`, separated by commas. A link
  (`[section_07](../product/section_07.md)`) or a file name (`section_07.md`) is read as the
  same token, so an author can make the line clickable on GitHub.
- `Implements: none` states that the plan implements no product section (tooling, cleanup).
  A plan with no line at all is "unmapped". Concept mode (section 05) shows unmapped plans
  apart from plans that declared `none`, because the first is a gap in the record and the
  second is a decision. (My call.)
- A token naming a section the vision does not list is a broken reference, shown on the plan.
- Links are at section granularity. A plan that implements part of a section names the whole
  section; which part is for its prose to say. (My call: a heading-level link would break on
  every rename of a product heading.)

The plan-writing skill adds the line (section 09). The line sits in prose, so plan validation,
which reads only the metadata block (`loom/src/plan/parser/extraction.rs:75`), is unaffected.
Metaplans carry no `Implements:` lines; the link is the plan's, and section 05 builds the
links between the two overviews from these lines only (vision, Decisions).

## Cross-references

The app recognises three forms of reference in any Markdown document, and resolves each to a
document and, when given, a heading section:

| Form                    | Example                                                  |
| ----------------------- | -------------------------------------------------------- |
| Relative Markdown link  | `[detection rules](section_01.md#detection-rules)`       |
| Repository path token   | `` `doc/plans/briefs/loom-ui-mods/loom-ui/common.md` `` |
| Plan file name          | `` `PLAN-02-delivery-truth.md` ``                        |

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
vision is `product/index`, a section is `product/section_<nn>`, a plan is `plan/<slug>`, every
other document is its path under `doc/`. A plan's citations survive `PLAN-` to `IN_PROGRESS-`
to `DONE-` to `archive/`.

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
- Rendering, navigation, editing, save-time validation and broken-reference display: section 06.
- The skills that write these conventions: section 09.
- What a run review is used for after execution: section 10.
- Which paths sessions may write, and symlink handling: section 12.
