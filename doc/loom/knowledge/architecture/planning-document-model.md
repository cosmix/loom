---
sources:
- loom/src/planning/mod.rs
- loom/src/planning/paths.rs
- loom/src/planning/checklist.rs
- loom/src/commands/plan/check.rs
---
# Planning Document Model

> Docs

## Module Map

`loom/src/planning/` (`mod.rs`) models a project's `doc/` tree for the planning app, the planning sessions and
`loom plan check`. Every rule below lives in one module and is pure unless the row says otherwise:

| Module | Owns |
| --- | --- |
| `kinds` | `DocKind` and `DocId` from a path alone; a vision is `vision_path(feature)` = `doc/product/<feature>/index.md` |
| `slug` | `plan_slug`, `plan_state`, `resolve_plans` (the winner per slug; strays and cross-home conflicts) |
| `discover` | no-follow walk of `doc/` (`DocTree`), bounded reads (`read_doc`, `read_doc_bytes`) |
| `sections`, `sections/metadata` | heading sections, GitHub anchors, line spans, hashes, plan metadata and stage spans |
| `paths` | `LockedSet`, `classify_write`, `check_relpath_syntax`, `symlink_component` |
| `watch`, `watch/pending` | polling `Watcher`, `Batch`, `OwnWrites` |
| `runtime` | per-project runtime directory: path computation, `LOOM_PLAN_STATE` provenance checks, and `create_runtime_dir` shared by the plan server and the summary CLI |
| `summary/` | the summary store: format, evaluation, seal, claims, engine; see [Summary Store](summary-store.md) |
| `vision`, `metaplan`, `implements`, `pressure`, `briefs`, `xref` | parsers over document text. `implements::parse` takes `section_NN#<anchor>` beside whole-section tokens (`ImplementsLine.anchors`, `whole`, `claims()`); `metaplan` reads each entry's `Covers:` line (`MetaplanEntry.covers`, `bad_covers`, `span_hash`); `briefs::bundle_files(project, plan_rel)` lists a plan's brief files from the loaded project (`briefs::plan_briefs` reads the disk for the run preflight) |
| `trace/`, `findings/` | the planning trace and the findings ledger; see [Planning Trace](planning-trace.md) |
| `problems`, `checklist`, `checklist/{items,offers}` | convention problems; the checklist |
| `project` | `Project::load` and `refresh`: tree, parsed documents, problems, checklist |

`loom/src/commands/plan/check.rs` is the CLI over `Project`. Contract tests: `loom/tests/planning_documents_contracts.rs`,
`planning_checklist_contracts.rs`, `planning_check_cli_contracts.rs`; each rule a contract names was mutation-checked
against its test, so a test going green under the mutated rule is a defect in the test.

## Kinds, Ids and Slugs

`classify(rel)` reads no file. A feature is a name (`[a-z0-9][a-z0-9-]*`, not `archive`, `briefs`, `plans` or `overview`; a bad name classifies as `other`) shared by `doc/product/<feature>/` and
`doc/plans/<feature>/`. `doc/product/<feature>/index.md` is the vision; `doc/product/<feature>/section_<digits>.md` a
product section; anything else under `doc/product/<feature>/` a product note; `doc/plans/<feature>/index.md` the feature's one metaplan (another `*metaplan*.md` there is an `Other` document, and follow-ups are entries under `## Follow-ups` in `index.md`); a `PLAN-` file in `doc/plans/<feature>/` or its `archive/` a feature plan;
`doc/plans/<feature>/briefs/<slug>/<file>` a brief. A `PLAN-` file in `doc/plans/` or `doc/plans/archive/` is a
standalone plan with briefs under `doc/plans/briefs/<slug>/`. A vision, section or metaplan at the top level
(an `index.md` directly under `doc/product` or `doc/plans`) raises `needs-feature`. `REVIEW-<plan slug>.md` directly in a plan home, its `archive/`, or the top of `doc/plans/` (where `fs::plan_review::generate` writes it) or `doc/plans/archive/` is a run
review ([run reviews](planner-feature-screens.md#run-reviews)); `doc/loom/knowledge/` is knowledge; the rest other.

`doc_id` is `product/<feature>/index`, `product/<feature>/section_NN`, `plan/<slug>` (the state is not part of the id,
so an id survives `loom run` renaming the file), else the path without `doc/`.

`DocKind` carries the owning feature (`Option<String>`; `None` for a top-level or standalone document). `slug::plan_feature` and `slug::plan_home` name a plan's feature and its home directory (`doc/plans/<feature>/` or `doc/plans/`, with a trailing `archive` removed); `PlanEntry.feature` is the winning copy's feature and `PlanEntry.conflicts` the copies in other homes. Any code that builds a brief-bundle or archive path takes the home from `plan_home` and never spells `doc/plans/briefs` or `doc/plans/archive` ([why](../mistakes/plan-server-delivery.md#moving-a-path-convention-left-hardcoded-homes-in-the-crate-2026-10-29)).

`plan_slug` strips at most one of `IN_PROGRESS-`, `DONE-`, `RETIRED-` (constants in `fs::plan_lifecycle`, imported, never
copied) and then requires `PLAN-` and a nonempty remainder. `plan_state`: `RETIRED-` is `Retired`; the archive
directory or `DONE-` is `Done`; `IN_PROGRESS-` is `Running`; else `Written`. `PlanState` orders
`Written < Running < Done < Retired`. `resolve_plans` keeps the most advanced copy per slug (ties: the smallest path);
a losing copy in the winner's home is a `stray` (`stray-plan-copy` problem), one in another home a conflict (`duplicate-plan-slug` problem). Everything referring to a plan goes through the
slug, so a link spelled `PLAN-foo.md` keeps working after the rename. The `RETIRED-` prefix marks an abandoned plan
under `<home>/archive/` (`doc/plans/archive/` or `doc/plans/<feature>/archive/`); a retired plan closes its metaplan entry and is read-only.

## Sections, Anchors and Hashes

`sections_of(text)` returns the preamble (lines before the first heading), one `HeadingSection` per ATX heading outside
a fence (`fence_mask`, shared with the knowledge splicer), each running to the next heading of the same or a shallower
level, and for plans the `<!-- loom METADATA -->` block plus one span per stage entry (extra addresses over the same
lines, not a re-partition). Setext headings are not recognised.

- Anchor = `github_slug(title)`: inline Markdown stripped, lowercased, only alphanumerics, space, `-`, `_` kept, each
  space becomes `-`; repeats get `-1`, `-2` in document order.
- `DocSections::find(anchor)`: `""` is the preamble, `loom-metadata` the metadata block, `stage:<id>` a stage entry,
  anything else the heading with that anchor. `loom-metadata` and `stage:` are reserved (app-only anchors): a heading
  slugged `loom-metadata` is never reachable, so an anchor's meaning does not change when a metadata block appears.
- `Span` lines are 0-based and end-exclusive. A text splits on `\n` with a final newline closing the last line, so N
  lines yield N and enclosing spans end at N.
- `normalized_hash`: `\r\n` and `\r` become `\n`, trailing spaces and tabs are dropped per line, whether the text ends
  with a newline is kept, then `source_graph::body_hash` (`sha256:<hex>`). A span's hash covers its lines joined with
  `\n` plus a final `\n`. The watcher, the summaries and `OwnWrites` all compare these hashes.

## Locked Set and Write Classification

`LockedSet::scan(root)` is recomputed from the current tree on every call; an error (including one from
`plan::inputs::collect_inputs`) must make the caller deny. The planning app's save route and the planning-session hook
ask the same `classify_write(locked, rel)`, which returns `DocArea`: `Locked(LockReason)`, `Knowledge`
(`doc/loom/knowledge/`, writable only through `loom knowledge`), `Summaries` (`doc/loom/summaries/`, only through
`loom plan summary seal`), `Doc`, or `OutsideDoc` (a `..` or root component, or nothing below `doc/`).

The locked set, by name and by reference:

- **By name** (locks a path whether or not it exists): `IN_PROGRESS-`, `DONE-` and `RETIRED-` plan spellings
  (`LockReason::PlanState`), anything under a plan home's `archive/` (`Archive`), a plan home's `REVIEW-*.md` (`RunReview`); a plan home is `doc/plans/` or `doc/plans/<feature>/`.
  Every comparison is made on the lowercased path (`fold`), so a plan file typed in lowercase is locked on a
  case-insensitive file system; folding only adds matches.
- **Brief-to-plan rule:** the briefs of a locked plan are locked (`BriefOfLockedPlan { slug }`): its bundle directory
  `<home>/briefs/<slug>/` by prefix, and every input `collect_inputs` lists for it. A `Written` plan and its briefs
  stay editable.
- `locked_paths()` feeds the spawn-time deny lists: it lists existing locked files outside the archive plus the
  `doc/plans/archive` directory itself, because the sandbox forbids a deny entry beneath another denied directory.
  `LockReason::PlanState` serialises through `serialize_with` (a newtype variant of a unit enum cannot be internally
  tagged): `{"reason":"plan_state","state":"running"}`.

`check_relpath_syntax(raw)` takes a request path relative to `doc/` and rejects an empty path, a leading `/`, a
backslash, a control character, more than 1024 bytes, an empty component and a `.` or `..` component; `symlink_component`
returns the first symlinked prefix of a path (lstat, no follow) and errors on a non-plain component.

## Discovery and Bounded Reads

`discover(root)` lists directories from a dirfd without following symlinks, because sessions can write `doc/`. It skips
dotfiles and editor temporaries and keeps a symlink only when it resolves to a regular file inside the canonical `doc/`
and outside `doc/loom/summaries` (`SUMMARIES`, defined once in `discover.rs`). Limits: depth 16, 20,000 entries,
`MAX_DOC_BYTES` = 8 MiB per document (`plan::inputs` shares the cap). `Project::load` surfaces a walk error as
`read_error` and `loom plan check` exits 2 on a load error. A public doc comment must not intra-doc-link the
`pub(crate)` `read_doc_bytes` (`cargo doc -D warnings` rejects it).

## Watcher and OwnWrites

`Watcher::poll` walks `doc/` (summary store included), compares each file's `(size, mtime_ns, inode)` with the previous
walk and hashes only differing files. The first successful walk returns `Batch { initial: true }` listing every file as
added. Differences accumulate in `Pending` and are delivered as one `Batch { added, changed, removed, renamed }` by the
first walk that finds nothing new; a removed file and an added file with the same hash pair as a rename. An unreadable
file gets an `unreadable:` sentinel hash and never pairs as a rename.

`OwnWrites` is shared with the summary engine: `register(rel, hash)` before a write, `unregister` when the write fails.
A registration is consumed by the first delivery that reports its path, and the check compares against the current
hash at delivery, so an own write is not reported while a later foreign edit is. `Watcher::spawn` returns
`Result<JoinHandle<()>>` (thread `loom-plan-watch`); a failed walk delivers nothing and leaves its message in the error
slot, which the next successful walk clears. `spawn` consumes the watcher, so a caller takes `error_slot()` first and
checks it, or failures go unseen.

## Runtime Directory

`runtime::runtime_paths_from(state, session, loom_dir, root)` computes paths without creating anything;
`runtime::create_runtime_dir` (pub, `loom/src/planning/runtime.rs`) creates the loom home and `<home>/plan/<key>` at 0700,
refusing symlinks, for both the plan server and `loom plan summary`. Outside a
planning session the directory is `<loom user dir>/plan/<project key>` (the user dir follows `LOOM_HOME`); the key is
`<sanitised root dir name>-<first 12 hex of sha256(canonical root)>`. `RuntimePaths` holds `lock`, `drafts`, `claims`,
`summaries.lock`, `summary-cost.jsonl` and `sessions` under it. Inside a session `LOOM_PLAN_STATE` names the directory
and must pass `check_state`: absolute, a real directory (not a symlink), owned by the current uid (`libc::getuid`;
`nix::unistd::getuid` needs a feature the tree does not enable), named exactly the key, a well-formed `LOOM_PLAN_SESSION`
(8 to 64 hex digits or dashes), and either a parent the caller cannot write (Seatbelt) or, under srt's private tmpfs over the
read-denied planning root, `drafts/`, `claims/` and `summaries.lock` mounted from another device than the state
directory (`runtime.rs::check_parent`). `server_runtime_paths` ignores `LOOM_PLAN_STATE` and returns its value so the caller can warn.

## Project View and Checklist

`Project::load(root)` discovers the tree, reads every Markdown document (bounded), parses vision, the metaplan,
`Implements:` lines, pressure records, cross-references and briefs, then derives problems and the checklist.
`Project::refresh(&Batch)` keeps a private text cache, re-reads only the batch paths plus documents the cache lacks, and
re-derives every field from the cached texts: a document's references depend on the whole tree (paths, slugs, other
documents' anchors), so a refreshed view equals a fresh `load` only if everything is re-derived. The cost is `doc/`
text held in memory.

`build_checklist(&DocTree, &ParsedDocs)` is a pure function. Six `Step`s (vision, product spec, metaplan, plans,
pressure test, run) carry a `StepMark` (`Done`, `Gap`, `Active`, `Optional`); `CurrentStep` is the earliest
gap, `Ready`, or `ReadyAllDone`. Rules the contracts pin:

- The vision's Specification sections table decides which sections are expected, in table order (the first missing
  section in that order is the gap). A table with header and delimiter but no body rows is a vision with no product spec
  (`Ok(vec![])`); body rows none of which is numbered are `NoNumberedRows` and raise `vision-table-unreadable`, so no
  offer guesses from an unreadable table.
- `ParsedDocs.sections_written` lists product sections holding a non-whitespace character; an unreadable file with size
  above zero counts as written, so no offer invites overwriting it.
- A metaplan entry is a level-2 heading naming a backticked plan file, in any lifecycle spelling (resolved by slug). A
  numbered entry heading without a plan file is a `metaplan-entry-without-plan` problem unless it says `no plan file`;
  an unnumbered level-2 heading is prose. Entries without a plan file are left out of step marks, items and
  `WritePlan` offers.
- Pressure-tested status never makes step 5 a gap (`ItemStatus::Untested`); offers exist per feature: `StartVision`, `FixVisionTable`, `WriteSection`, `AddSectionToVision`, `StartMetaplan`, `WritePlan`, `PressureTest`, `DraftAllSections`, `DraftAllPlans`, `PlanFollowUps` (the one offer of a finished feature, `CurrentStep::ReadyAllDone`) and `MoveIntoFeature` for a misplaced top-level document (`checklist/offers.rs`).

Problem codes (`problems.rs`): `bad-feature-name`, `broken-reference`, `broken-brief-reference`, `doc-missing`, `duplicate-plan-slug`,
`implements-anchor-missing`, `implements-broken`, `implements-duplicate`, `implements-missing`, `metaplan-entry-foreign-plan`,
`metaplan-entry-without-plan`, `missing-title`, `needs-feature`, `stage-line-unknown`, `stray-plan-copy`, `unlisted-section`,
`vision-table-unreadable`, and the trace codes `covers-bad-token`, `covers-anchor-missing`, `settled-bad`, `settled-duplicate`, `about-unresolved`,
`findings-bad-entry`, `findings-duplicate-id` and `findings-bad-status` (`problems/trace.rs`, `trace::collect`; each feature's `trace.problems` is appended by `problems::collect`, and the trace build leaves locked documents out). A locked document reports none; a stray copy is reported wherever it is.

Edge links between the documents are heading-level: an anchored `Implements:` token, a `Covers:` line and an `About:` line name headings. The concept page's `ImplementsView` (`SectionLink`) and `implemented_by` stay section-level, so a plan that implements one heading of a section links the whole section there; the heading-level edges are in the trace.
Cross-reference extraction masks fenced code (`fence_mask`).

## `loom plan check`

Prints the checklist and problems from `Project::load` of the git toplevel. Exit codes: 0 with no problem, 1 with any
problem, 2 on an error that prevents checking (no current directory, not inside a git repository, `Project::load`
failure). `--json` prints one pretty object `{schema: 1, root, checklist, problems}` pinned by
`loom/tests/fixtures/planning/check_schema.json`; human output passes every field through `inline_safe` (backticks
become U+02CB). Plans without an `Implements:` line raise `implements-missing`, so the command exits 1 on a tree that has
such plans; tests assert the JSON, not the exit code, for this repository's own `doc/`. `loom plan verify` resolves a
retired plan's brief bundle through `plan::inputs::bundle_dir`, which strips `RETIRED-` too.
