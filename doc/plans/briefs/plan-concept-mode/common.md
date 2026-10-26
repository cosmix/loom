# Concept mode: common brief

Read this first, then your own brief. The plan is `doc/plans/PLAN-plan-concept-mode.md`
(`IN_PROGRESS-PLAN-plan-concept-mode.md` while it runs); the product rules are
`doc/product/section_05.md`, the summary format is `doc/product/section_04.md` ("The summary
file", "What out of date means"), anchors are `doc/product/section_03.md` ("Headings as
anchors").

## What exists when you start

Plans 1 to 4 are merged. Their interfaces, by role (names from the producing plans' interface
sections):

- **Loaded project (plan 1)**, `loom::planning::project::Project`: `tree` (documents and plans
  with state), `docs` (`ProjectDoc` with `title`, `sections`, `open_questions`, `lock`),
  `vision_rows`, `metaplans`, `implements` (per slug, `ImplementsLine`), `implemented_by`
  (section number to slugs), `content_hash(rel)`, `section_text(rel, anchor)`. Also
  `planning::pressure::records`, `planning::watch::{Batch, OwnWrites}`.
- **Summary store (plan 4)**, `loom::planning::summary`: `store::scope(&Project) -> Scope {
  product, engineering, outside }` in reading order; `format::{SummaryFile, Block, Sentence,
  Link, LinkTarget}`; `evaluate::{evaluate(&Project, &[Claim]) -> Evaluation, SummaryEval,
  SentenceEval, SummaryState}`; `engine::{SummaryEngine::new(root, runtime, own),
  on_batch(&Project, &Batch) -> EngineUpdate}`; `claims::list_claims`.
- **Planning server (plan 2)**, `loom::plan_server`: `routes.rs` (`all_routes()`,
  `install_all()`), `RouteSpec { method, path, body, role, handler }`, `ApiRequest`,
  `ApiResponse`, `PlanContext` (`project()`, `runtime()`, `on_batch`, `extensions()`,
  `read_only()`, `hub().broadcast`), `wire/message.rs` (`PlanMessage`), parity tests in
  `wire/tests_<area>.rs`. API paths start with `/api/`. In `web/src/plan/`: `router.tsx`
  (`routes`), `tabs.ts` (`PLAN_TABS`), `help-keys.ts` (`PLAN_HELP_KEYS`), `api/client.ts`
  (`apiGet`), `api/socket.ts` (`subscribe(type, handler)`), `state/atoms.ts` (`canAuthorAtom`).
- **File mode (plan 3)**: `GET /api/doc?path=` (`DocView { path, title, text, hash, sections:
  [{ level, title, anchor, start_line, end_line }], ... }`), `<Markdown source path sections
  onNavigate docPaths anchorsOf marks? />` in `web/src/plan/markdown/`, `/files/<path>#<anchor>` (relative to `doc/`), `docRel(path)`/`repoRel(path)` in
  `web/src/plan/files/address.ts`, and in
  `web/src/plan/attachments/`: `<AddToChat draft rect onAdded />` (no `secondary` prop; plan 9 registers through
  `registerSelectionSecondary`),
  `useSelectionAttachment(container, provenance) -> { draft, rect, clear }`,
  `addAttachmentAtom`, `AttachmentDraft` (its `summary` arm exists for this plan).

Confirm each name with `loom map --find-all <name>` or `rg`; when the merged name differs, use it
and record `loom memory note "producer-name: <assumed> is <merged> (<file>:<line>)"`.

## Rules for every worker

- Files at most 400 lines, functions at most 50 (`cargo test --test maintainability` and the
  exact ledger `loom/maintainability-baseline.txt`, which no row of this plan changes).
- No new dependency in either package.
- Rust: `anyhow` context as the surrounding planning code does; no `unwrap` outside tests.
- Web: erasable TypeScript only, `z` from `@/api/zod`, no `dangerouslySetInnerHTML`; summary
  text renders as text and passages through plan 3's renderer only. Typography and colour per
  `doc/loom/knowledge/conventions/web-dashboard-typography.md`: no coloured left border or spine
  on insets, monospace only for paths, slugs and anchors, `eyebrow` for the column label.
- Never run git, never touch `.loom/`. Record mistakes and decisions with `loom memory`.

## Fix-round rules (pressure test)

- **Path forms.** Every wire `path` (and every path inside `wire::concept`) is repo-relative,
  `doc/...`. Plan 3's request parameters (`/api/doc?path=`) and `/files/` addresses are relative to
  `doc/`: convert with plan 3's `docRel`/`repoRel`, never by hand.
- **Shared UI patterns from plan 2.** A control this browser may not use is plan 2's
  `<DisabledReason reason={string | null} onClick>{label}</DisabledReason>`
  (`@/plan/ui/disabled-reason`; `reason` null is an ordinary button; focusable `aria-disabled`
  button, reason as accessible description), never a `disabled` button with a tooltip. Fetches
  render through plan 2's `<LoadState state="loading|empty|not_found|error" title? message?
  action?={{ label, onClick }} />` (`@/plan/ui/load-state`).
- **Resync.** After a reconnect, plan 2's `planSocketAtom` handle's `onResync(fn) ->
  unsubscribe` fires (every `hello` after the page's first): deltas were missed, refetch
  everything loaded.
- **Excerpts.** Rendering a cut of a document with plan 3's `<Markdown>` passes `lineOffset` (the
  excerpt's first line minus one) and `headingIds={false}`, so no duplicate or wrong ids enter the
  DOM.
