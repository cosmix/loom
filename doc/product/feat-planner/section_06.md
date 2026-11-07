# 06 — File Mode

Part of the [Loom Plan product vision](index.md).

This section defines file mode: the navigation tree and what it groups, how Markdown and plan
files render, the briefs and the knowledge base, viewing versus editing, the embedded editor and
which paths it may save, save-time plan validation, diffs, search, and taking a selection into
chat. Section 03 owns the document model this section displays (paths, kinds, parents, heading
anchors, discovery); where this section names a kind or a parent, it uses 03's rules.

## The model in one paragraph

File mode is a reader first. The left pane is a tree of every document under `doc/` that
section 03 discovers, grouped by kind and feature; the reading column renders the selected
file. Anyone connected can read, search, follow links, and open diffs. The browser holding authoring can also
open the file in a CodeMirror editor and save it; the server writes the file only if it is still
the version that was opened, validates plan files before writing, and from then on treats the
save like any other change to the file (section 04). File mode never shows anything outside
`doc/`, and never shows the summary store.

## Layout and addresses

The header's Files link opens file mode, which adds a tree pane, about 18rem wide, to the left of the reading
column and the chat panel (vision, "The screen"). Below 900px the tree becomes a drawer opened
from the top of the reading column. Every document has an address, `/files/<path relative to doc/>#<anchor>`, so the back button,
reloads, links pasted into chat, and the presence list (section 02) all name a place in a
document. (My call.)

**One document header** serves file mode and summary pages: `web/src/plan/ui/doc-heading.tsx`,
used by `web/src/plan/files/doc-heading-view.tsx` and `web/src/plan/concept/page-heading.tsx`.
In file mode it has, in order:

- A breadcrumb naming the feature, kind and document: feature › kind › document, after
  "Planner". A standalone document has no feature crumb.
- A title row in a three-column grid: DocMark, title, actions. The actions are **Summary | Source**,
  **Edit** when editing is allowed, and **Discuss** for the authoring browser. Summary | Source
  switches between the same document's summary page and file mode; Summary is disabled when
  no summary page is available. A metaplan has one, its Metaplan route (section 05), and the
  spine's Metaplan row opens it with Source one click away. Discuss opens chat with the document attached and focuses the
  composer.
- One wrapping row of state chips, where applicable: state, Implements links, pressure-test
  state, validation, and last change. The validation chip expands the errors and warnings.
- The document's path with a copy button and the **Uncommitted changes** mark linking to its diff.
- The lock notice when locked, below the path.

## The navigation tree

The tree has five groups, in this order; empty groups are hidden. Product, Plans and Archive
group their documents by feature name, alphabetically. Documents without a feature follow
the feature groups. Every document entry shows its title (its first `#` heading, else its file
name); the file name shows on hover.

| Group     | Contents, in order                                                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Product   | Per feature: its vision; listed sections in the vision's table order; unlisted sections; then misplaced top-level product documents        |
| Plans     | Per feature: the metaplan with its entries in heading order, then unlisted active plans; then misplaced metaplans and the standalone shelf |
| Knowledge | `INDEX.md`; the tier-1 files; one folder per tier-2 category                                                                               |
| Other     | Other Markdown under `doc/` that section 03 discovers, grouped by feature where applicable, then by path                                   |
| Archive   | Per feature, then standalone: done and retired plans no metaplan names, other archived files, and orphan brief bundles                     |

A feature is the name shared by `doc/product/<feature>/` and `doc/plans/<feature>/`; either
directory alone is enough. Each feature's Product and Plans groups use only its own vision,
sections and metaplan. The standalone shelf holds active plans whose home is `doc/plans/`,
with no feature or checklist. A top-level product document or metaplan remains reachable with
the `needs-feature` problem section 03 defines.

**Gaps show in place.** A section the vision lists but whose file is missing, and a metaplan
entry whose plan file does not exist, appear dimmed at their position with the word "missing".
Opening one shows the checklist line for that gap and, to the authoring browser, its offer
(section 01). (My call: the tree then doubles as the project checklist in document order.)

**Plan entries** carry the plan's state from section 01 as a glyph and a word: written,
running (`IN_PROGRESS-`), done (`DONE-` or under its home's `archive/`), or retired
(`RETIRED-` under its home's `archive/`). A plan's home is `doc/plans/<feature>/` or, for a
standalone plan, `doc/plans/`. A pressure-tested plan also shows the date of its last
`## Pressure tests` line. A plan that fails validation shows an error count (see "Plan
validation"). Running, done and retired plans show a lock glyph.

**Briefs sit under their plan.** A plan entry expands to its brief bundle, laid out as it is on
disk: `common.md` first, then one folder per stage directory, each holding that stage's brief
files by name. A bundle is `<plan home>/briefs/<slug>/`: `doc/plans/<feature>/briefs/<slug>/`
for a feature plan and `doc/plans/briefs/<slug>/` for a standalone plan. Section 03 owns the
rule that ties a bundle to its plan. A bundle whose plan is not found is listed under Archive
in its feature group or among standalone entries as an orphan bundle, so no brief is unreachable.

**Archive** is collapsed by default. Within each feature group, then among standalone entries,
it holds done and retired plans no metaplan names, other files under the plan home's
`archive/` (such as `REVIEW-PLAN-*.md`), and orphan brief bundles. A done or retired plan named
by its feature's metaplan stays under that metaplan in Plans.

**Knowledge** follows the hierarchy described in `architecture/knowledge-hierarchy.md`:
`INDEX.md` first, then the seven tier-1 files, then the tier-2 categories (architecture,
entry-points, patterns, conventions, mistakes, concerns), each topic listed by slug with its
INDEX blurb as a second line. The tree reads the blurbs from `INDEX.md` itself, the file loom
regenerates on every knowledge write, so the app keeps no copy. Categories are collapsed by
default; `mistakes` alone holds about sixty topics.

**The open document's outline** expands under its tree entry: its `##` and `###` headings,
nested, each a link to its anchor. The heading in view is marked as the reader scrolls.
(My call: the reading column stays at reading width, so the outline goes in the tree rather
than a third column.)

**The tree updates live.** It is rebuilt from the same file watcher as the checklist
(section 01, section 04). A file added, removed, or renamed appears, disappears, or moves without
a reload. When a plan is renamed from `PLAN-` to `IN_PROGRESS-` or `DONE-` while it is open,
the reading column follows it to the new path, since section 01 identifies a plan by its slug.

## Rendering Markdown

Markdown renders in the browser from the raw file text the server sends, as CommonMark with the
GitHub extensions loom documents already use: tables, fenced code, task lists, strikethrough,
autolinks. The parser goes in the planning app's entry bundle, not in the dashboard's. (My call:
rendering on the server would need a Markdown crate loom does not have, and the browser needs
the raw text anyway for the editor and diffs.)

- **Raw HTML is never rendered.** HTML comments are dropped from the view (this hides the
  `<!-- loom METADATA -->` markers); any other HTML shows as escaped text. Documents are written
  by sessions and arrive by `git pull`; the reading column must not run what they contain.
- **Headings** get the anchors section 03 defines. Hovering a heading shows a link control that
  copies the heading's address. The body suppresses the document's first H1 when its text
  equals the header title, comparing rendered text with whitespace trimmed and collapsed
  (`web/src/plan/markdown/render.tsx`). Its anchor remains a link and scroll target; a
  different first H1 and later H1s remain visible. The editor shows the full source.
- **Tables** render in the dashboard's ledger style (`--ledger-rule`, `--ledger-head-rule` in
  `web/src/index.css`). A table wider than the reading column scrolls sideways inside its own
  box; the page never scrolls sideways.
- **Code blocks** render in the monospace face on a muted ground, with the fence's language as
  a small label and a copy control, and scroll sideways when long. There is no syntax colouring
  in the reading view. (My call: colouring would pull a highlighter into the entry bundle for
  text the reader mostly skims; the editor colours source when opened.) A `mermaid` block is
  drawn as a diagram in the active theme's colours and redrawn when the theme changes; the
  source shows while the renderer loads, and stays as a code block with a muted note when the
  diagram does not parse. (My call: the renderer loads on the first diagram, so documents
  without one never pay for it.)
- **Inline code** that is exactly the path of a document file mode can open (for example
  `` `doc/plans/briefs/token-flow/common.md` ``, the form plans use) becomes a link to it.
  (My call: plans and briefs name each other this way far more often than with Markdown links.)
- **Links.** A relative link to a file under `doc/` opens it in file mode, at the anchor if the
  link has one; a `#anchor` link scrolls within the document. A link to a document or anchor that
  does not exist renders struck through with a "not found" hover. A link to a path outside
  `doc/` renders as plain text with its target on hover, since file mode serves only `doc/`.
  An `http(s)` link opens in a new tab with `rel="noopener noreferrer"`.

## How plans render

A plan file has three parts: prose, the metadata block between `<!-- loom METADATA -->` and
`<!-- END loom METADATA -->` (a fenced `yaml` block; extraction in
`loom/src/plan/parser/extraction.rs:53`), and, after the end marker, the `## Pressure tests`
record (section 01). The plan view renders them as:

1. **The shared document header** above the prose ("Layout and addresses"). Its wrapping chip
   row shows the plan state, Implements links to its feature's product sections (section 03),
   pressure-test state, validation counts, and last change (section 01), where available.
   Pressure-test state says "Not pressure-tested", "Being pressure-tested", or the last test's date.
2. **The prose**, rendered as Markdown with a matching first H1 suppressed.
3. **A Stages panel** in place of the metadata block: the stage graph drawn with the dashboard's
   graph layout (`web/src/lib/graph.ts`, React Flow and dagre) without live state, then one
   row per stage with its id, name, type, summary, and dependencies. A row expands to show the
   stage's description rendered as Markdown (tables, lists, code and links work, and a single
   line break stays a break), then its acceptance criteria and file lists as text. Clicking a
   node scrolls to its row. (My call: a reviewer reads a plan's shape from its graph and summaries; the YAML is
   the authoring form.)
4. A **"Show YAML"** control on the Stages panel replaces it with the raw block as a code
   block, so a reader can always see exactly what loom will parse.
5. **The pressure-test record**, rendered as ordinary Markdown.

When the metadata block is missing (a plan still being drafted, like
`doc/plans/PLAN-loom-state-confinement.md` today) the panel says so in one line. When the YAML
does not parse, the panel shows the parse error and the raw block. The Stages panel's data
comes from the server's plan parse, the same one validation uses, so the panel and the
validation result never disagree.

## Viewing and editing

**Who may edit.** Only the browser holding authoring (section 02) sees the Edit control. A
browser that saves while nobody holds authoring takes authoring, as starting a chat would
(section 02). Read-only browsers, and every browser when `loom plan` runs in its read-only mode
(section 02, "Missing prerequisites"), see no Edit control and no save route
accepts them.

**What may be edited.** The server decides; the browser only hides controls. A path is editable
when all of these hold:

- it is a Markdown file under `doc/product/` or `doc/plans/`, or under the Other group;
- it is not a running, done or retired plan: no `IN_PROGRESS-`, `DONE-` or `RETIRED-` prefix,
  not under its plan home's `archive/`;
- it is not a brief of a running, done or retired plan (My call: `loom run` reads briefs from the
  committed tree, so an edit during a run reaches no stage and misleads a later reader about what
  the stages were told);
- it is not under `doc/loom/`: the knowledge base and the summary store are read-only in file
  mode;
- it is at most 1 MiB, the limit `loom plan verify` applies to plans
  (`loom/src/commands/plan/verify.rs:18`).

The knowledge base is read-only because loom's own tools keep it consistent: every write goes
through `loom knowledge`, which regenerates `INDEX.md` and enforces the section rules, and a
direct edit would skip both. Knowledge is changed from chat, where the session uses those tools.
The vision says the knowledge base is browsed in file mode. Section 12 owns which
paths sessions may write; this list governs the editor only.

**Locked files** show a notice below the path in the shared document header saying why, and
no Edit control:

- running plan: "This plan is running. `loom run` works from the committed copy.";
- done plan or archive: "This plan is done. Record changes in a new plan.";
- brief of a locked plan: the same notice, naming the plan;
- knowledge: "The knowledge base changes through `loom knowledge`. Ask in chat."

File mode never creates, renames, moves, or deletes a file. Those happen through a session.
(My call.)

## The editor

The Edit control swaps the rendered view for the file's full source in CodeMirror 6, in the
reading column, opening in live preview (below). CodeMirror and its language modes load as a lazy chunk the
first time any browser opens the editor or a diff, the way the dashboard loads xterm
(`web/src/components/terminal/emulator.ts:134-161`), so a reader who never edits never
downloads it. Every `.md` file opens in Markdown mode; fenced `yaml` blocks, which includes the
plan metadata block, are highlighted as YAML inside it. The editor is themed from the dashboard's
CSS tokens (`web/src/aurora-ui/shared/styles/tokens.css`, `web/src/index.css`) through CSS
variables, so it follows the active theme and the `.dark` class without a second palette. There
is no schema-aware completion (vision, Decisions).

### Live preview and source

The editor has two modes over the same CodeMirror document, switched by a "Live preview /
Source" toggle in the editor's toolbar. Both edit the Markdown text itself; live preview only
changes how that text is drawn. Switching keeps the text, the cursor, the selection and the undo
history, because nothing is converted. A save writes exactly the text in the editor, so bytes the
user did not touch never change. (Decided 2026-11-06. A rich-text editor that rebuilds Markdown
from a document tree on save was rejected: it normalises list markers, emphasis, table padding
and wrapping across the file, so a one-word fix would become a whole-file diff in git and in a
turn's changes (section 10), and it mishandles the `<!-- loom METADATA -->` comments and the
fenced YAML plan block.)

- **Live preview** is the mode a Markdown file opens in. Text is drawn styled: headings at their
  rendered sizes, bold, italic, strikethrough and inline code styled, links drawn as their link
  text, list bullets and quote bars drawn as in the reading view. The Markdown markers (`#`,
  `**`, `` ` ``, `[`, `](url)`, and so on) are hidden except on the lines the cursor or selection
  touches, where they show so the user sees exactly what is being changed. A plain click on a
  link places the cursor; Ctrl-click / Cmd-click follows it as the reading view would.
  Task-list boxes are drawn as checkboxes, and clicking one toggles `[ ]` and `[x]` in the text.
- **Blocks that stay close to source in live preview.** Fenced code blocks show in the
  monospace face on the muted ground with their language label, and their fence lines show only
  when the cursor is inside the block. Fenced `yaml` blocks, including the plan metadata block,
  keep YAML highlighting. Tables show as their source rows in the monospace face with pipes
  dimmed; they are not drawn as grids. HTML comments, including the `<!-- loom METADATA -->`
  markers, show as dimmed source and are never hidden, so the user cannot delete one unseen.
  Other raw HTML shows as source, never rendered (Rendering Markdown, above).
- **Source** shows the plain text with Markdown and YAML highlighting and line numbers, the
  editor as it was before live preview. Line numbers show only in this mode.
- **Toolbar and shortcuts**, active in both modes, insert or remove the Markdown syntax around
  the selection: bold (Ctrl-B / Cmd-B), italic (Ctrl-I / Cmd-I), inline code, link (Ctrl-K /
  Cmd-K), heading level, bullet, numbered and task list, quote, code block, and a table
  skeleton. They edit the text the same way typing would, so undo reverses them one step at a
  time.
- The chosen mode is remembered in the browser's local storage, so the editor reopens in the
  mode last used. (My call.)
- Validation markers (Plan validation, below) attach to lines in either mode. The merge view
  (When the file changed underneath) and diffs (Diffs, below) always show source.

Live preview is built from CodeMirror's own decoration and widget facilities over the Markdown
syntax tree the Markdown mode already parses. It adds no second editor library to the lazy chunk.

### Saving and drafts

- **Save** is a button and Ctrl-S / Cmd-S. The request carries the path, the content hash of the
  text the editor opened (section 04's hash), and the new text.
- **Drafts.** Unsaved text is kept in the browser's local storage per path and opened hash,
  so a reload or a crashed tab keeps it. Leaving the document with unsaved text asks first.
  (My call.)
- **Done** returns to the rendered view; with unsaved text it asks to save or discard.

### What the server does on save

1. Refuse unless the browser holds (or takes) authoring and the path is editable.
2. Compare the file's current hash with the hash the editor opened. On a mismatch, refuse and
   return the current text and hash (below).
3. If the path is a plan file, validate the new text (next section). Errors stop the save
   unless the user confirmed "Save with errors".
4. Write the file atomically (a temporary file in the same directory, then rename).
5. Nothing else. The file watcher sees the write like any outside change, so the checklist,
   the tree, and the summaries follow through the same path (vision, Non-goals; section 04).

### When the file changed underneath

If the hashes differ, the editor opens a side-by-side merge view (`@codemirror/merge`): the
file's newer text on the left, read-only, and the user's unsaved edit on the right, with
controls to copy each changed chunk from left to right. The right side's base hash becomes the
newer one, so the next save succeeds unless the file changes again. The same view is offered,
without waiting for a save, as soon as the watcher reports that a file open in the editor
changed, so the user sees a session's edit arrive while typing.

## Plan validation

Plans are checked by the same code `loom plan verify` runs, called in-process by the server:
extract the metadata block, deserialize it, `plan::schema::validate`
(`loom/src/plan/schema/validation.rs:273`), and, when that passes, the soft checks, the input
checks, and base-tree evaluation (`loom/src/commands/plan/verify.rs:341-371`). Today that code
reads the plan from disk and prints or exits; implementing this section splits it into a
function over the proposed text and the plan's path that returns the result `--json` already
serialises (errors; structural, knowledge, sandbox, and baseline warnings; notes; stage levels),
with `execute` becoming a caller of it.

**When it runs.** On every save of a plan file, against the proposed text, before writing; and
whenever the watcher reports a change to a plan file, so the shared header's validation chip
and the tree show the current result for edits made anywhere. It runs on `PLAN-*.md` files
only; locked plans are not re-validated.

**What blocks.** Errors block the save, with a "Save with errors" confirmation that writes
anyway; warnings never block, and `--strict` is not applied. The files are the truth and any
other editor can write an invalid plan, so the app warns hard and does not forbid. `loom plan
verify` and `loom init` still reject such a plan later. A
plan with no metadata block yet is saved with a note, not an error, because drafting a plan's
prose first is normal.

**How errors show.** In the editor, a panel above the source lists errors, then warnings by
bucket, then notes, as `loom plan verify` groups them. Each item links to a line where one is
known:

- a YAML parse error carries a line and column within the block; the editor adds the block's
  starting line and marks that line in the gutter;
- a validation error names a stage (`stage_id` in `loom/src/commands/plan/verify.rs:30-33`)
  and no line; the editor marks the stage's `- id:` line;
- any other item is listed without a mark.

In the rendered view, the shared header's validation chip expands the same list, each
stage-specific item linking to that stage's row in the Stages panel.

## Diffs

A diff in file mode shows source text, never rendered Markdown, in `@codemirror/merge`:
side by side on wide screens, inline (unified) below 900px. Diffs are read-only and every
browser can open them. Three kinds exist:

| Diff                | Before                                    | After               | Opened from                                                |
| ------------------- | ----------------------------------------- | ------------------- | ---------------------------------------------------------- |
| What a turn changed | the file when the turn started            | the file at its end | the turn's file list (section 08)                          |
| Changed while open  | the version the reader had on screen      | the current file    | the "changed" note on the document                         |
| Uncommitted         | `HEAD`'s version (`git show HEAD:<path>`) | the current file    | the Uncommitted changes mark beside the path in the header |

**Turn diffs.** The server keeps the text of every file in file mode in memory (it reads them
to hash them anyway, section 04). When a turn starts (section 07 defines turn boundaries), it
pins the current text of every file; when the turn ends, each file the turn changed gets a diff
from its pinned text to its text at the end. The diff covers the whole turn, including edits
made by subagents and summary rebuilds the turn ran. A file the turn created diffs against
nothing; a file it deleted shows its last text as removed. Turn diffs live for the server's
lifetime and are lost on restart; the uncommitted diff still shows the work. (My call: keeping
history in the project would be sidecar state, an anti-goal.)

**Changed while open.** When an open document changes on disk and the reader has no unsaved
edit, the view updates in place, keeps the heading in view where it was, and shows a short
"changed just now" note linking to the diff.

## Search

One search box sits at the top of the tree. It does two things.

**As you type, it filters the tree** by title and file name, in the browser.

**On Enter, it searches text** across every document file mode shows, on the server, and lists
results in the tree pane: document title, the heading path of the matching section, and a
snippet with the terms marked. Opening a result opens the document at that heading's anchor
with the terms marked in the text. Results are capped at 50. A scope switch limits the search
to Product, Plans (briefs included), Knowledge, or all.

The index reuses loom's retrieval code instead of a second engine: the heading chunker that
cuts knowledge and `doc/` prose into sections (`loom/src/fs/knowledge/catalog/prose.rs`) and
the BM25 scoring behind `loom knowledge context` (`loom/src/context/rank/corpus.rs:76`). Three
things differ from `loom knowledge context`, so the server calls the library, not the command:

- the index is built in memory and updated from the file watcher; it writes no cache, since
  `loom plan` writes nothing in the project outside `doc/` (section 02);
- done plans, `doc/plans/archive/`, and the knowledge tree are all indexed, though prose
  indexing skips the first two today (`prose.rs:359-364`), because file mode shows them;
- results are ranked by score alone, with no token budget and no demotion of prose against
  curated knowledge (`loom/src/context/rank.rs:100`).

A query in double quotes is matched literally, case-sensitively, so identifiers that the BM25
tokenizer splits or drops, such as `IN_PROGRESS-` or `--strict`, can be found. (My call.)
Search covers `doc/` only; source code search is left to the terminal and to the sessions.

## Taking a passage into chat

In the rendered view, the editor, or a diff, selecting text shows a small "Add to chat" control
next to the selection. It attaches the passage to the next chat message as a reference made of:

- the document's path and the anchor of the heading that contains the selection start;
- the selected text, as the reader saw it (rendered text in the view, source in the editor,
  including the Markdown markers live preview hides);
- the line range, when the selection is in the editor or a diff;
- the stage id, when the selection is in a plan's Stages panel.

Only the browser holding authoring, or any browser while nobody holds it, sees the control.
Section 08 renders attachments as chips in the chat; section 10 defines the text delivered to the
session.

## Out of scope for this section

- Document kinds, the parent of a brief, heading anchor rules, and discovery: section 03.
- Content hashing, the file watcher, and summary rebuilds after a save: section 04.
- Overviews and summary-to-source expansion: section 05.
- Turn boundaries and what counts as a session's change: section 07.
- The chat panel, its per-turn file list, and attachment rendering: section 08.
- Reverting or refining what a turn changed: section 10.
- Pressure tests and runs started from the app: section 11.
- Authentication, the save route's access checks, and which paths sessions may write:
  section 12.
