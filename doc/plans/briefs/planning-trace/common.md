# Planning trace: shared brief

Every worker of `PLAN-planning-trace.md` reads this file in full before its own brief. It holds the
user's decisions, the grammar of every new line in `doc/`, the domain and wire shapes, and the
rules no stage may bend. Where a worker brief and this file disagree, this file wins; where the
plan YAML and this file disagree on a gate or a file owner, the YAML wins.

## The user's decisions (2026-11-05, do not reopen)

- A document has three layers. The **stub** exists before the document and sets its scope (a
  vision table row for a product section, a metaplan entry for a plan). The **body** is the
  document. The **summary** is generated from the body for human review. Stubs never replace
  summaries, and summaries never replace stubs.
- Summaries are built for drafts and for settled documents alike: long plans need them while they
  are still drafts.
- **Settled marks are document-level.** A plan is settled together with its brief bundle.
- **Links may point at headings** (`section_04#rebuild-budget`).
- **Plan summaries cover the plan's briefs.** Leaving briefs without summaries was a mistake
  (`doc/loom/knowledge/mistakes/planner-design-brainstorm.md`).
- Feature review can happen at any point once a plan has run, and its findings feed later plans.
- The planning trace is NOT the source-code graph (`loom map`). No code symbol is a node; nothing
  here reads or writes `.loom/` graph state. Name things `trace`, never `graph`, in code, wire,
  routes and UI so the two are never confused.
- Lanes: Claude only. Fable for visual work with `frontend-design:frontend-design`; sonnet and
  haiku for behaviour; opus for the trace model and the summary-store change.
- The project is unreleased: no migration code, no compatibility shims.

## Settled during planning (the planner's calls)

- **Edges are plain lines in the documents**, so a reader without `loom plan` can follow them
  (vision principle 1). No sidecar file holds edges or settle state.
- **Stage-level links use a prose line**, `Implements (stage:<id>): ...`, because stage ids live in
  the YAML and the plan schema does not change.
- **The brief-to-stage link is derived from the bundle path** (`briefs/<slug>/<stage-id>/...`), so
  there is no `Serves:` line.
- **Agents never accept edits piece by piece.** Sessions keep writing files directly; the review
  surface is the turn footer, which gains the downstream impact of each changed document. A
  pre-acceptance changeset would need sessions that propose without writing, which the session
  bridge does not support.
- **The findings ledger is one file per feature**, `doc/plans/<f>/findings.md`. Run reviews stay
  the immutable evidence; the ledger holds triage state.
- **Next actions stay offers.** The frontier is a list of short hints under the next-step sentence;
  it starts no session itself.
- **Stub drift is a session duty plus a recorded state.** Whether a body's scope has moved past its
  stub cannot be decided from hashes. The skills tell sessions to update the stub in the same turn
  (or ask), and a stub that changes after settling reads `stub_changed`. No agent judges drift in
  the background.
- **Briefs appear with their plan** on the plan's summary and source pages (a list of its bundle
  files, linked to their source), and not as spine rows: the spine's 18rem column cannot hold them.
- **Left out, from the brainstorm:** a generated linear "reading view" built from transcluded
  nodes (it belonged to the one-file-per-node option, which the user's prose-first process rules
  out), concepts shared across features, and merging `doc/loom/knowledge` into the trace. Each is
  a later plan if wanted.

## Edge lines

Every edge line starts at column 0, outside fenced code (`fs::knowledge::splice::fence_mask`), and
before the first `<!-- loom METADATA` line. Values are comma-separated tokens; surrounding
whitespace is trimmed.

| Line | Where | Meaning |
| --- | --- | --- |
| `Covers: <tokens>` | inside a metaplan entry: after its `## ` heading, before the next heading of level 2 or shallower | the entry's plan will deliver these spec nodes |
| `Implements: <tokens>` | the plan's existing declaration (`planning::implements`) | the plan delivers these sections or headings |
| `Implements (stage:<id>): <tokens>` | anywhere in a plan's prose | one stage delivers these headings |
| `About: <tokens>` | inside a findings ledger entry | the nodes a finding concerns |

### Tokens (`planning::trace::tokens`)

A token resolves relative to the document's feature `<f>`:

| Token | Node |
| --- | --- |
| `section_NN` or `section_NN.md` | `doc/product/<f>/section_NN.md`, the whole section |
| `section_NN#<anchor>` | one heading of that section |
| `vision` / `vision#<anchor>` | `doc/product/<f>/index.md` or one of its headings |
| `metaplan` / `metaplan#<anchor>` | `doc/plans/<f>/index.md` or one of its entries |
| `PLAN-<slug>.md`, also with a lifecycle prefix (`DONE-PLAN-<slug>.md`) | the winning file of that plan slug |
| `PLAN-<slug>.md#stage:<id>` or `PLAN-<slug>.md#<anchor>` | a stage or heading of that plan |
| `[label](<target>)` | the target's file name and fragment, read as above |

`NN` is one or more ASCII digits. An anchor is the GitHub anchor `sections::github_slug` produces
(the same one `Source { path, anchor }` links use); `stage:<id>` is the reserved stage anchor that
`DocSections::find` already accepts. Anything else is a bad token: it is kept as written in
`bad_tokens` and the plan check reports it. A well-formed token naming a missing document or
anchor resolves to `Unresolved` and is reported as well. Each line type accepts only some kinds of
target: `Covers:` and both `Implements` lines accept section tokens; `About:` accepts all of them.

`tokens` exposes `pub struct SectionRef { pub number: u32, pub anchor: Option<String> }` and
`pub fn section_refs(value: &str) -> (Vec<SectionRef>, Vec<String>)` (refs in order, deduplicated,
and bad tokens as written), shared by `Covers:`, stage lines and `Implements:`. `MetaplanEntry`
gains `pub covers: Vec<SectionRef>`, `pub bad_covers: Vec<String>` (from the entry's first
`Covers:` line) and `pub span_hash: String` (the heading span's hash, the plan's stub).

### `Implements:` gains anchors

`implements::parse` accepts `section_NN#<anchor>` beside the forms it takes today.
`Implements::Sections(Vec<u32>)` keeps its meaning (section numbers, now deduplicated in first-seen
order), and `ImplementsLine` gains `pub anchors: Vec<(u32, String)>`: the anchored tokens in
declaration order, deduplicated. A section named both with and without anchors counts as a whole
section.

## Settled marks (`planning::trace::settle`)

```text
Settled: <body> [stub <stub>] [upstream <upstream>]
```

- API: `settle::settled_line(text: &str) -> Option<SettledLine>` (the first valid line),
  `settle::mark(text: &str, line: &SettledLine) -> String`, `settle::clear(text: &str) -> String`,
  `settle::body_hash(text: &str, bundle: &[(&Path, &str)]) -> String`, and
  `settle::render(line: &SettledLine) -> String` (the line without its newline). `mark` on a text
  that already holds the same line returns the text unchanged.
- Each hash is the first 12 lowercase hex digits of a SHA-256 (`settle::short_hash`).
- **Placement.** One line, its own paragraph, directly after the document's first `# ` heading.
  When the next non-blank line after that heading is an `Implements:` line, the Settled line goes
  after it. `settle::mark` replaces an existing Settled line in place; `settle::clear` removes it
  and the blank line it added. A second Settled line is the plan-check problem `settled-duplicate`.
  A `Settled:` line that does not parse (a hash that is not 12 lowercase hex digits, an unknown
  word, words out of order) is `settled-bad` and reads as Draft. Lines starting `Status:` are
  prose and unrelated: the feat-planner vision opens with `Status: agreed vision (2026-10-26).`,
  which is why the mark is named `Settled:`.
- **Body hash.** `normalized_hash` of the document's text with its Settled line removed. For a plan
  the body is the plan plus its brief bundle: the hash covers the plan's text and then, in path
  order, each brief's repository path and text.
- **The bundle.** `planning::briefs::bundle_files(project: &Project, plan_rel: &Path) ->
  Vec<PathBuf>` (stage trace-core, T2): the loaded project's documents classified
  `DocKind::Brief { slug }` for the plan's slug under `slug::plan_home(plan_rel)/briefs/<slug>/`,
  sorted. It reads the loaded project, never the disk; `briefs::plan_briefs` reads the disk for
  the run preflight and is not used here. The trace (settle hashes) and the summary store (stage
  bundle-summaries) share this one helper.
- **Stub hash.** A product section: the normalized text of its vision table row (`VisionRow.line` is the row's 0-based line in the vision; the row text is that line of the vision's text). A plan: the span
  hash of its metaplan entry (`HeadingSection.span.hash`). The vision, the metaplan and a plan with
  no entry have no stub.
- **Upstream hash.** Plans only: over the spans of every heading the plan implements (anchored
  tokens and stage lines) in path-then-anchor order, plus the file hash of each section implemented
  whole.
- **States**, checked in this order:

| State | Meaning |
| --- | --- |
| `draft` | no Settled line, or a bad one |
| `revised` | the body hash differs |
| `stub_changed` | the body matches, the stub hash differs (or the stub is gone) |
| `upstream_changed` | body and stub match, the upstream hash differs |
| `settled` | everything matches |

- **Who writes it.** Only the server's settle route (stage trace-server), at the reader's request.
  Sessions never write or edit Settled lines. A session that edits a settled document says so in its
  reply; the document then reads `revised` until it is settled again.
- **Settled version.** "Changes since settled" compares with the newest commit whose version of
  the file contains the current Settled line (found with `git log -S'<line>' -1 -- <path>`).
  When that commit does not exist (the line was never committed), the page says the settled
  version is not committed and shows no diff. The browser shows the comparison with the review
  module's commit view (`web/src/plan/review/changed-since.tsx`, `/api/compare` and
  `/api/at-commit`); `openDiff` has no commit kind and is not used for it.
- **Which documents can be settled:** the vision, product sections, the metaplan and plans with no
  lifecycle prefix. A locked document (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, archive) cannot be
  settled or unsettled; it keeps whatever line it had when it locked.
- **Unresolved points.** Settling a document that holds `(My call.)` markers (case-insensitive
  `(my call`) or open questions (`ProjectDoc.open_questions`) needs a confirmation. The route
  answers 409 `unresolved` with `{ my_calls, open_questions }` unless the request carries
  `confirm=1`.

## Findings ledger (`planning::findings`)

`doc/plans/<f>/findings.md` (plain Markdown, `DocKind::Other`; no new kind):

```markdown
# Findings

## F-1 Rebuild threshold ignores briefs

About: PLAN-feat-planner-03-summaries.md#stage:scheduler, section_04#rebuild-budget
Source: REVIEW-PLAN-feat-planner-03-summaries.md#open-findings (scheduler, 1a2b3c4d5e6f)
Status: open

The rebuild budget counts plan summaries but not brief changes.
```

- An entry is a `## F-<n> <title>` heading (n ≥ 1, unique in the file), then its `About:`,
  optional `Source:` and `Status:` lines in any order, then one text paragraph. Headings of any
  other shape are ignored by the parser and reported by the plan check (`findings-bad-entry`).
- `Status:` values: `open`, `planned PLAN-<slug>.md`, `folded metaplan#<entry anchor>`,
  `dismissed: <reason>`. A missing or unknown value reads as open and is reported.
- **Import.** `findings::import_review(root, plan_rel, review_text) -> Result<usize>` reads the
  run review's `## Open Findings` and `## Open Questions` items (`run_review::review_items`) and
  appends one entry per item not already present. The duplicate key is the `Source:` line:
  `REVIEW-<plan stem>.md#<section anchor> (<stage id>, <12-hex hash of the item text>)`. About is
  `PLAN-<slug>.md#stage:<stage id>`, followed by the plan's `Implements:` tokens (anchored ones as
  written, whole sections as `section_NN`). Status is `open`. It returns how many entries it added.
  A standalone plan, or a plan whose feature cannot be read from its path, imports nothing and
  returns `Ok(0)`.
- **Untrusted text.** Item text is agent-written. The title is the text's first 80 characters,
  cut on a char boundary, with newlines turned into spaces. The paragraph is the whole text on one
  line; a paragraph starting with `#` gets a leading backslash, so a finding never creates a
  heading. A dismissal reason is one line, control characters removed, trimmed, at most 200
  characters, and must not be empty.
- **Writes.** The file is created with `# Findings` when absent. Import runs under
  `fs::locking::lock_plan(<findings path>)` and writes with `fs::safe_replace::replace_no_follow`
  (an existing ledger) or `fs::safe_replace::restore_no_follow` (a new one), the no-follow writers
  file mode uses; never `locking::locked_write`, which opens the parent directory following
  symlinks. Every write refuses a symlinked `findings.md` or a symlinked
  directory between it and the repository root (checked with `symlink_metadata` on each
  component), returning an error and leaving the target untouched. The server's status route
  writes through `plan_server::files::save::save_document`, which already locks, checks the hash
  and refuses symlinks.
- **When import runs.** `fs::plan_lifecycle::mark_plan_done_if_all_merged`, right after
  `plan_review::generate` returns the review path, reads the review and imports it. A failure is
  returned with context, like a review failure, so the plan stays retryable. Import is idempotent
  because `generate` runs again when the plan is already done.
- **Folding.** A finding marked `folded metaplan#<anchor>` points at a metaplan entry whose plan
  has not started running (unwritten, or written and not yet `IN_PROGRESS-`); that entry is marked
  "revisit before writing" or "revisit before running".
- **Where the file lands.** Import runs in the daemon's host job against the main checkout, so
  after a plan completes `findings.md` is an uncommitted change there, like the summaries the
  planner rebuilds. The user commits it.

## The trace model (`planning::trace`)

A `Trace` is built per feature at the end of `Project::derive`, after `parse_docs`, and kept on
`Feature` (`pub trace: Trace`). It reads only the parsed project. `project.rs` is at 393 lines:
the derive step is a single call into `trace::build`; anything more moves out of `project.rs`.

```rust
pub struct Trace {
    pub documents: Vec<TraceDoc>,      // vision, sections (listed or written), metaplan, plans
    pub headings: Vec<TraceHeading>,   // level-2 headings of written sections
    pub entries: Vec<TraceEntry>,      // metaplan entries
    pub findings: Vec<TraceFinding>,   // the ledger, in file order
    pub gaps: Vec<Gap>,
    pub frontier: Vec<Gap>,            // at most 5, ordered as below
    pub problems: Vec<TraceProblem>,   // bad tokens, unresolved targets, Status and ledger problems
}
pub struct TraceDoc {
    pub path: PathBuf, pub role: DocRole, pub title: String,
    pub progress: Progress, pub settle: SettleState,
    pub stub: Option<NodeRef>, pub number: Option<u32>, pub slug: Option<String>,
    pub bundle: Vec<PathBuf>,          // plans: brief files
    pub my_calls: u32, pub open_questions: u32,
    pub current: SettledLine,           // the line settling now would write
}
pub struct SettledLine { pub body: String, pub stub: Option<String>, pub upstream: Option<String> }
pub enum DocRole { Vision, Section, Metaplan, Plan }
pub enum Progress { Stub, Written, Planned, Built, Running, Done }
pub enum SettleState { Draft, Revised, StubChanged, UpstreamChanged, Settled }
pub struct NodeRef { pub path: PathBuf, pub anchor: Option<String> }
pub struct TraceHeading {
    pub path: PathBuf, pub anchor: String, pub title: String,
    pub covered_by: Vec<CoverRef>, pub findings: Vec<String>,   // finding ids
}
pub struct CoverRef {
    pub kind: CoverKind,               // Entry, Plan, Stage
    pub path: PathBuf, pub anchor: Option<String>,
    pub slug: Option<String>, pub stage: Option<String>,
}
pub struct TraceEntry {
    pub anchor: String, pub title: String, pub slug: Option<String>,
    pub plan: Option<PathBuf>, pub covers: Vec<NodeRef>, pub revisit: Vec<String>,
}
pub struct Finding {                   // planning::findings, parsed without the project
    pub id: String, pub title: String, pub about: Vec<String>,   // About tokens as written
    pub source: Option<String>, pub status: FindingStatus, pub text: String,
}
pub struct TraceFinding { pub finding: Finding, pub about: Vec<NodeRef> }  // resolved by trace::build
pub enum FindingStatus { Open, Planned(String), Folded(String), Dismissed(String) }
pub struct Gap { pub kind: GapKind, pub node: NodeRef, pub count: Option<u32> }
pub enum GapKind { Revised, StubChanged, UpstreamChanged, Revisit, Settle, Uncovered, Triage }
```

- **Progress.** Section: `Stub` (a vision row, no file), `Written`, `Planned` (some entry or plan
  covers it or one of its headings), `Built` (it is covered, and every plan covering it is
  `DONE-` or `RETIRED-` with at least one `DONE-`). Plan: `Stub` (an entry with no plan file),
  `Written`, `Running` (`IN_PROGRESS-`), `Done` (`DONE-` or `RETIRED-`). Vision and metaplan:
  `Written`.
- **Headings** are the level-2 headings of each written section, except the anchors
  `open-questions` and `out-of-scope`.
- **Coverage.** A heading is covered by: an entry whose `Covers:` names it or its whole section; a
  plan whose `Implements:` names it, or names its section with no anchor for that section; a stage
  whose stage line names it. A heading with nothing covering it is an `Uncovered` gap.
- **Findings on nodes.** A finding is attached to every heading its About names, and to every
  heading of a section its About names whole.
- **Revisit.** An entry whose plan has not started running (no plan file, or a plan with no
  lifecycle prefix) is marked with the ids of findings folded into it, and of open findings whose
  About names a heading the entry covers or a heading its written plan implements. This is how a
  finding from wave 1 reaches plan 4 when plan 4 is written and not yet run.
- **Frontier order** (first five, one gap per document): `Revised`, `StubChanged`,
  `UpstreamChanged`, `Revisit`, `Settle` (a written draft section or plan, in spine order),
  `Uncovered` (headings of settled sections only), then one `Triage` gap carrying the count of open
  findings.

## Wire (`plan_server::wire::trace`, mirrored by `web/src/plan/api/schema-trace.ts`)

`GET /api/trace/<feature>` (`Role::Viewer`) answers a `TraceView`: the `Trace` with every path as a
repository-relative string, enums in snake_case, `Option` as `null` (no `skip_serializing_if`), and
`#[serde(deny_unknown_fields)]` on every struct, as the checklist wire does. Field names match the
Rust model above. A finding's status is `{ "kind": "open" | "planned" | "folded" | "dismissed",
"detail": string | null }`. An unknown feature is 404 `not_found`.

`loom/src/plan_server/wire/fixtures/trace.json` is written by a writer test from a fixed fixture
project and checked byte-for-byte; `web/src/plan/api/fixtures/trace.json` is the same bytes, and a
web schema test parses it. A change to the wire regenerates both.

New message `PlanMessage::TraceChanged { feature }`, sent after a watcher batch for each feature
whose `TraceView` changed (compared as `plan_server/watch.rs` compares checklist views). The web
refetches that feature's trace.

### Routes (stage trace-server)

| Route | Role | Body | Effect |
| --- | --- | --- | --- |
| `GET /api/trace/<feature>` | Viewer | none | the `TraceView` |
| `POST /api/settle?path=&hash=[&confirm=1][&clear=1]` | HolderOrTake | none | writes or removes the Settled line through `save_document`; 200 `Saved`, 409 `Conflict` on a stale hash, 409 `unresolved` (see Settled marks), 409 `stale` while the loaded project trails the file, 422 for a plan with validation errors, 403 `locked` for a locked document, 400 for a document that cannot be settled |
| `POST /api/findings/status` | HolderOrTake | JSON `{ feature, id, hash, status: { kind, detail } }` | rewrites the entry's `Status:` line through `save_document`; 400 for an unknown id, an empty or invalid detail, a `planned` detail that is not a `PLAN-<slug>.md` token, or a `folded` detail that does not name an entry of the feature's metaplan |
| `GET /api/settled-version?path=` | Viewer | none | `{ commit: string \| null }`, the commit holding the current Settled line |

`path` in these routes is relative to `doc/`, as `/api/save` takes it.

## Offers and sessions

- **New offer `Offer::ReviewFeature`.** Id `review-feature`, wire kind `review_feature`, label
  `Review the feature`, skill `loom-plan:review` (new, `plan-plugin/skills/review/SKILL.md`),
  target `doc/plans/<f>/findings.md`. Secondary on any feature with a plan `Running` or `Done`.
  Opening target sentence: `Record findings from reviewing the feature in doc/plans/<f>/findings.md.`
- **The review skill.** It asks what the user saw (running the feature, reading code, reading a
  run review), reads code and documents read-only, and appends findings in the ledger format
  (next free `F-<n>`, `About:` naming the most specific nodes, `Status: open`, no `Source:` or a
  `Source: review <YYYY-MM-DD>` line). It writes nothing else.
- **Seeds.** "Plan this" and "Fold into entry" start `PlanFollowUps` with a seed text (the seed
  mechanism built-and-new adds) naming the finding id, its title and its About tokens. The metaplan
  skill, after writing or editing the entry, sets the finding's Status line to
  `planned PLAN-<slug>.md` or `folded metaplan#<anchor>`.
- **Skill duties** (stage trace-server):
  - metaplan: every entry carries a `Covers:` line naming the most specific spec headings;
    folding a finding edits the entry and sets its Status.
  - plan: the `Implements:` line names headings where the plan covers part of a section; each
    stage that covers a narrower part gets an `Implements (stage:<id>):` line.
  - Every authoring skill: never write or edit a `Settled:` line; when you edit a settled document,
    say so in the reply.
  - The plan-writer skill (`skills/loom-plan-writer/SKILL.md`) documents the anchored
    `Implements:` form and the stage line.

## The browser

- **Trace state.** `traceAtom(feature)` in `web/src/plan/trace/`: fetched on first use, refetched
  on `trace_changed` for that feature and on resync; a failed refetch keeps the loaded view and
  records the error, as `page-state.ts` does.
- **Spine.** Each document row shows its settle state beside its existing glyphs. The frontier
  hints (at most five lines, one link each) sit under the next-step sentence. Stage
  trace-web-maps adds a `Coverage` row to the Product spec group and a `Findings` row (with the
  open count) to the Run group.
- **Document heading.** On a document's summary page and source page: the settle state; `Mark
  settled` (with a confirmation that lists My calls and open questions when the server answers
  `unresolved`), `Unsettle`, and for `revised`, `Changes since settled` (the compare view against
  `/api/settled-version`'s commit). Sections and plans show their stub above the body, linked to
  its place in the vision or metaplan, with a note when the stub changed since settling. A plan's
  pages list its brief files (`TraceDoc.bundle`), each linked to its source page.
- **Edge chips.** A summary sentence shows chips computed in the browser from the sentence's
  `rests_on` and the trace: on product pages, the entries, plans and stages covering the headings
  it rests on, plus a count of open findings; on plan pages, the spec headings the cited stage or
  plan implements, each with its section's settle state. The existing plan glyph (`plansOf`) stays.
- **Turn footer.** Under each changed document, the related documents that are now `revised`,
  `stub_changed` or `upstream_changed`, plus plans covering a changed section, with their states.
- **Coverage page** `/f/<f>/coverage`: columns Vision rows | Spec headings (grouped by section) |
  Metaplan entries | Plans and stages | Findings. Selecting a card highlights its lineage across
  columns and opens a focus panel: Why (stub and section), How (covering entries, plans, stages),
  Findings. Keys: `j`/`k` move within a column, `h`/`l` move to the why/how column, `Enter` opens
  the node. Filters: All, Uncovered, Changed since settled, Open findings.
- **Findings page** `/f/<f>/findings`, the wave screen: findings grouped by source run (newest
  first), then reviews; each with About chips, status and the actions Plan this, Fold into entry
  (picker of entries whose plan has not started running: unwritten, or written with no lifecycle prefix) and Dismiss (reason required). Built-and-new's Built
  page reads its candidates from the ledger and links here.
- Finding text, titles, reasons and headings are rendered as text, never as HTML or Markdown.

## Rules every stage keeps

- No file under `loom/src` crosses 400 lines and no function crosses 50; split into sibling
  modules. `loom/maintainability-baseline.txt` does not change. Web files stay under 400 lines
  (checked by `wc -l` in each stage's last step).
- Frozen files (`*_contracts.rs`, `*contract_tests.rs`, `*.contracts.test.tsx`) are never edited
  except as a stage's description lists; a stage that must edit one files one `dispute-integrity`
  after its commit naming each edit.
- Fixture JSON changed on one side is changed on the other in the same stage.
- Record mistakes, decisions and surprises with `loom memory` immediately; never Claude Code
  auto-memory. Record a knowledge claim the tree contradicts as `loom memory note
  "stale-knowledge: ..."`.
