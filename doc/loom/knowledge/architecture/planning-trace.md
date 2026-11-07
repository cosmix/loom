# Planning Trace

>

## Not the Source Graph

The planning trace links the planning documents of a feature: vision rows, product sections and their headings, metaplan entries, plans and stages, briefs and findings. It is not the `loom map` source graph ([Source Graph](source-graph.md)): no code symbol is a node, nothing in it reads or writes `.loom/` graph state, and code, wire, routes and UI say `trace`, never `graph`. Code: `loom/src/planning/trace/` (model, `tokens`, `settle`, `build/`), `loom/src/planning/findings/` (ledger), `loom/src/plan_server/trace/` (routes), `loom/src/plan_server/wire/trace.rs` (wire), `web/src/plan/trace/` (browser).

## Edge Lines and Tokens

Edges are plain lines in the documents, so a reader without `loom plan` follows them; no sidecar file holds edges or settle state. Each starts at column 0, outside fenced code, before the first `<!-- loom METADATA` line:

| Line | Where | Meaning |
| --- | --- | --- |
| `Covers: <tokens>` | in a metaplan entry, under its `##` heading | the entry's plan delivers these spec nodes (`MetaplanEntry.covers`, `bad_covers`) |
| `Implements: <tokens>` | the plan's declaration (`planning/implements.rs`) | the plan delivers these sections or headings |
| `Implements (stage:<id>): <tokens>` | anywhere in a plan's prose | one stage delivers these headings; the stage id lives in the YAML, so the plan schema is unchanged |
| `About: <tokens>` | in a findings ledger entry | the nodes a finding concerns |

`planning::trace::tokens` parses the tokens: `section_NN[.md]`, `section_NN#<anchor>`, `vision[#anchor]`, `metaplan[#anchor]`, `PLAN-<slug>.md` (any lifecycle prefix), `PLAN-<slug>.md#stage:<id>` or `#<anchor>`, and `[label](<target>)`. An anchor is `sections::github_slug`; a malformed token is kept as written (`bad_tokens`) and a well-formed token naming nothing resolves to `Unresolved`; both reach `loom plan check` (codes in [Planning Document Model](planning-document-model.md#project-view-and-checklist)). `section_refs(value) -> (Vec<SectionRef>, Vec<String>)` is shared by `Covers:`, stage lines and `Implements:`; `is_plan_token` (built on `tokens()`) validates a `planned` finding detail. A brief's link to its stage is derived from its bundle path `briefs/<slug>/<stage-id>/...`; there is no `Serves:` line.

## Settled Line

`Settled: <body> [stub <stub>] [upstream <upstream>]` (`planning/trace/settle.rs`; each hash is the first 12 lowercase hex digits of a SHA-256). The line is the document's own record that a reader settled it; the name avoids `Status:`, which prose already uses. Placement: its own paragraph directly after the first `#` heading, or after an `Implements:` anchor line that follows the heading; text glued under an `Implements:` line makes it no anchor and the mark goes under the heading, and a document with no level-1 heading gets the line at the top. `mark` replaces in place and returns the text unchanged for the same line; `clear` removes every `Settled:` line with the blank line before each.

- **Body hash**: `normalized_hash` of the text without its Settled lines. For a plan the body also covers the brief bundle (`briefs::bundle_files`), each brief's path and text in path order; a brief without Markdown text is hashed by `Project::fingerprint`.
- **Stub hash**: a product section's vision table row; a plan's metaplan entry span. The vision, the metaplan and a plan with no entry have none.
- **Upstream hash** (plans only): the spans of every implemented heading plus, for a section implemented whole, `settle::body_hash` of its text. It is not the section's `file_hash`, which includes the section's own Settled line: settling a section would flip every plan implementing it to `upstream_changed`.
- **States**, in this order: `draft` (no line or an unparsable one, plan problem `settled-bad`; a second line is `settled-duplicate`), `revised` (body differs), `stub_changed`, `upstream_changed`, `settled`.
- **Writers**: only the settle route, at the reader's request. Sessions never write or edit a Settled line; a session that edits a settled document says so, and the document reads `revised` until settled again. A locked document (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, archive) keeps the line it had and cannot be settled.
- **Unresolved points**: a document holding `(My call` markers (case-insensitive) or open questions needs `confirm=1`; without it the route answers 409 `unresolved` with the counts.
- **Settled version**: `GET /api/settled-version` finds the newest commit whose version of the file holds the current line (`git log -S<line> -1`); a line never committed answers `commit: null` and the page shows no diff.

## The Trace Model

`build_trace` (`planning/trace/build.rs`, `build/docs.rs`, `claims.rs`, `gaps.rs`, `ledger.rs`) runs for every feature at the end of `Project::derive`, after every feature's documents are in place (a trace reads other features' plans), reads only the loaded project and is kept on `Feature.trace`. `Trace` holds `documents` (vision, sections listed or written, metaplan, plans written or only named by an entry), `headings` (level-2 headings of written sections, except the anchors `open-questions`, `out-of-scope` and `out-of-scope-for-this-section`, which the section skill mandates), `entries`, `findings`, `gaps`, `frontier` and `problems` (`TraceProblem` is `planning::problems::Problem`).

- **`TraceDoc`**: `role` (`Vision`, `Section`, `Metaplan`, `Plan`), `progress` (`Stub`, `Written`, `Planned`, `Built`, `Running`, `Done`), `settle`, `settleable`, `stub`, `bundle`, `my_calls`, `open_questions` and `current`, the line settling now would write. `settleable` is true for a written, unlocked document except the vision and metaplan of a feature whose plans are all done; done work is final, so it carries no settle state, gap or control, while product sections stay open to follow-up plans.
- **Coverage**: a heading is covered by an entry whose `Covers:` names it or its section, a plan whose `Implements:` names it (or its section with no anchor), or a stage line naming it. Coverage re-reads the `Implements:` line with `section_refs`, because `ImplementsLine.anchors` merges `section_04#a, section_04` and loses that the section was named whole. A stage line naming a section whole covers all its headings. A heading nothing covers is an `Uncovered` gap.
- **Gaps** (`GapKind` order): `Revised`, `StubChanged`, `UpstreamChanged`, `Revisit`, `Settle`, `Uncovered`, `Triage`. The first four exist only for written, unlocked documents. `Revisit` marks a metaplan entry whose plan has not started running (the node is the written plan, else `metaplan#entry`) and carries the ids of findings folded into it and of open findings whose About names a heading the entry covers or its plan implements; this is how a finding from wave 1 reaches plan 4 before it runs.
- **Frontier**: the first five gaps, one per document, in gap order; `Uncovered` counts only headings of settled sections; one `Triage` gap carries the open-finding count. A hint is a short link; the frontier starts no session (next actions stay offers).
- **Stub drift** is a session duty plus a recorded state. Whether a body outgrew its stub cannot be decided from hashes, so the skills tell sessions to update the stub in the same turn (or ask) and a stub that changes after settling reads `stub_changed`; no agent judges drift in the background.

## Findings Ledger

`doc/plans/<f>/findings.md` is one file per feature, plain Markdown (`DocKind::Other`): `# Findings`, then entries `## F-<n> <title>` with `About:`, optional `Source:` and `Status:` lines and one text paragraph. `Status:` is `open`, `planned PLAN-<slug>.md`, `folded metaplan#<anchor>` or `dismissed: <reason>`; a missing or unknown value reads open and raises `findings-bad-status`. Run reviews stay the immutable evidence; the ledger holds triage state.

- **Import**: `findings::import_review(root, plan_rel, review_text)` appends one entry per item of the run review's `## Open Findings` and `## Open Questions` (`run_review::review_items`) not already present, keyed by `Source: REVIEW-<stem>.md#<section anchor> (<stage id>, <12-hex hash of the item text>)`, with About `PLAN-<slug>.md#stage:<id>` followed by the plan's `Implements:` tokens, and returns the count; a standalone plan imports nothing. It runs in `fs::plan_lifecycle::mark_plan_done_if_all_merged` right after `plan_review::generate`, in the daemon's host job against the main checkout, so the ledger is an uncommitted change there. A failure keeps the plan retryable, and a repeat run adds nothing.
- **Untrusted text**: item text is agent-written. The title is the first 80 characters cut on a char boundary with newlines as spaces; the paragraph is one line, and text that starts with a heading `#`, a code fence (`` ` `` or `~`), a metadata key or a backslash gets a leading backslash so a finding never changes the ledger's structure (`findings/text.rs` `paragraph`, `unescape`). A dismissal reason is one line, control characters removed (whitespace controls become spaces), trimmed, at most 200 characters (`REASON_MAX`), never empty. The browser draws every ledger string as text, never as HTML or Markdown.
- **Writes**: import runs under `fs::locking::lock_plan` and writes with `replace_no_follow` or `restore_no_follow`, refusing a symlinked ledger file or directory above it; never `locking::locked_write`, which opens the parent following symlinks. The status route writes through `files::save::save_document`.
- **Folding**: a finding `folded metaplan#<anchor>` points at an entry whose plan has not started running; that entry reads "revisit before writing" or "revisit before running". `Status` lines are set by the metaplan skill after it writes or edits the entry.

## Routes and trace_changed

`plan_server/trace/mod.rs` `routes()` registers four routes with one line in `routes::all_routes`; the write routes build the new text and call `files::save::save_document`, so authoring, the lock, the opened-hash check, locked documents and symlinks follow the save route. `path` is relative to `doc/` as in `/api/save`.

| Route | Role | Effect |
| --- | --- | --- |
| `GET /api/trace/<feature>` | Viewer | the `TraceView` (`wire/trace.rs`); 404 `not_found` for an unknown feature |
| `POST /api/settle?path=&hash=[&confirm=1][&clear=1]` | HolderOrTake | writes or removes the Settled line; 409 on a stale hash, 409 `unresolved`, 409 `stale` while the loaded project trails the file, 422 for a plan with validation errors, 403 `locked`, 400 for a path with no trace document or a role that cannot be settled |
| `POST /api/findings/status` | HolderOrTake | JSON `{ feature, id, hash, status: { kind, detail } }`; 409 when the ledger hash no longer matches, before the new text is built; 400 for an unknown id, an empty detail, a `planned` detail that is no plan token or a `folded` detail naming no metaplan entry |
| `GET /api/settled-version?path=` | Viewer | `{ commit }` |

`TraceView` mirrors `Trace` with repository-relative `/` paths, snake_case enums, `null` options and `deny_unknown_fields`; a finding's status is `{ kind, detail }`. `wire/fixtures/trace.json` is written by a writer test from a fixed project (`LOOM_WRITE_FIXTURES=1`) and checked byte for byte; `web/src/plan/api/fixtures/trace.json` holds the same bytes and a web schema test parses it (`schema-trace.ts`). `PlanMessage::TraceChanged { feature }` goes out after a watcher batch for each feature whose `TraceView` differs (`trace/changed.rs`, compared as the checklist views are); the browser refetches that feature's trace. A socket test waits for a specific frame with an explicit deadline over `socket.read()`, because the server keeps sending other frames ([the hang](../mistakes/planner-follow-ups-delivery.md#a-socket-test-waited-on-a-frame-that-never-came)).

## Plan Summaries and the Bundle

A plan's summary covers its brief bundle: the plan may cite every brief file, must cite each brief with Markdown text, and carries a `bundle` front matter map; a changed brief makes the summary out of date ([Summary Store](summary-store.md#file-format)). The plan's summary and source pages list its briefs (`TraceDoc.bundle`) as chips grouped by folder (`briefs-card.tsx`, `brief-groups.ts`), each linked to its source; briefs are not spine rows, because the 18rem column cannot hold them. Summaries exist for drafts and settled documents alike.

## Browser

- **State** (`trace/state.ts`): `traceAtom(feature)` is fetched on first use and refetched on `trace_changed` for that feature and on resync; a failed refetch keeps the loaded view and records the error. `trace-gate.tsx` draws the load and error states for the two trace pages. Every reader of a settle state goes through `shownSettle` (null unless `settleable`).
- **Settle flow** (`settle-control.tsx`, `settle-flow.ts`): the document heading shows the state and Mark settled, Unsettle and, for `revised`, Changes since settled (`settled-changes.tsx`, the review module's commit comparison against `/api/settled-version`). The hash posted is the opened source document's save hash (`useKeptDoc(path).doc.hash`), never a summary page's `file_hash`, which is the summary store's hash. A 409 `unresolved` opens a confirmation listing My calls and open questions; cancel and Escape return focus to Mark settled. Locked state comes from the loaded `DocView` (`docIsLocked`), falling back to the file-name rule only until it loads.
- **Stub panel** (`stub-panel.tsx`): a section or plan shows its stub above the body, linked to its place in the vision or metaplan, with a note when it changed since settling.
- **Edge chips** (`edge-chips.tsx`): computed in the browser from a summary sentence's `rests_on` and the trace: on product pages the entries, plans and stages covering the headings it rests on plus open findings; on plan pages the headings the cited stage or plan implements, each with its section's settle state. Chips hide by opacity (0.7 at rest), never `display`, so they stay in the tab order; the settle word inside a chip keeps `--foreground` and the dot carries the tone.
- **Turn footer** (`chat/turn-footer.tsx`, `impact-rows.tsx`): under each changed document, the related documents now `revised`, `stub_changed` or `upstream_changed` and the plans covering a changed section, each listed once. Sessions keep writing files directly; the footer is the review surface, because a pre-acceptance changeset would need sessions that propose without writing.
- **Tone**: one `--tone-warning` for revised, stub changed and upstream changed, `--tone-completed` for settled and a muted outline for draft; the heading's pill is a `--muted` pill with a toned dot (`web/src/plan/ui/DESIGN.md`).

## Coverage and Findings Pages

`/f/<f>/coverage` (`coverage-page.tsx`, `coverage-model.ts`, `coverage-keys.ts`; `maps.css` holds the width steps and imports the base `maps-columns.css`): columns Vision rows, Spec headings grouped by section, Metaplan entries, Plans and stages, Findings. Selecting a card marks its lineage and opens a focus panel (Why: stub and section; How: covering entries, plans and stages; Findings). Keys: `j`/`k` within a column, `h` to the why side and `l` to the how side (`l` skips the entries column for a heading only a plan covers), `Enter` opens the node. Filters: All, Uncovered, Changed since settled, Open findings.

`/f/<f>/findings` (`findings-page.tsx`, `findings-model.ts`, `findings-cards.tsx`, `ledger-hash.ts`, `trace-gate.tsx`; style `maps-findings.css`): findings grouped by source run, newest first, then reviews; each card shows About chips, status and, while open, Plan this, Fold into entry (a picker of entries whose plan has not started running) and Dismiss (reason required, `dismissReason` mirrors the server's cleaning). The ledger hash comes from `ledger-hash.ts`, fetched when a trace loads, so the 409 guard compares against the text the cards were drawn from. After a saved dismissal focus moves to the finding's heading.

### Findings actions

Plan this and Fold into entry start `PlanFollowUps` with a seed (`findings-actions.ts` `followUpSeed`) naming the finding id, title and About tokens; the metaplan skill then sets the finding's `Status:` to `planned PLAN-<slug>.md` or `folded metaplan#<anchor>`. Finding text enters the session as the user's first message, so an instruction-like finding is treated as the user's own words ([concern](../concerns/planning-document-model-followups.md#planning-trace-limits)). The Review the feature offer (`Offer::ReviewFeature`, skill `loom-plan:review`) appends findings in the ledger format and writes nothing else.

## Skill Duties

The authoring skills in `plan-plugin/skills/` and `skills/loom-plan-writer/SKILL.md` carry the duties: the metaplan skill writes a `Covers:` line naming the most specific headings and sets a folded finding's Status; the plan skill writes `Implements:` with heading anchors where a plan covers part of a section and an `Implements (stage:<id>):` line for each narrower stage; every authoring skill never writes a `Settled:` line and says so when it edits a settled document. A seed takes precedence over the Plan follow-ups opening target.
