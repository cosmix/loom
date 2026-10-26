# Chat and the review loop: common brief

Read this first, then your own brief. The plan is `doc/plans/PLAN-plan-chat.md`
(`IN_PROGRESS-PLAN-plan-chat.md` while it runs). Product rules: `doc/product/section_08.md`
(the panel), `doc/product/section_10.md` (the review loop), `doc/product/section_07.md` "What the
chat receives" (the model), `doc/product/section_12.md` "Suggestions and restores", "Paths in
requests", "Untrusted content in the browser" (access and safety).

## What exists when you start

Plans 1 to 8 are merged. Their interfaces, by the names the producing plans' interface sections
settled:

- **Loaded project (plan 1)**, `loom::planning::project::Project`: `docs` (`ProjectDoc` with
  `title`, `sections`, `open_questions`, `lock`), `tree.plans` (slug, path, state),
  `implemented_by` (section number to slugs), `content_hash(rel)`; also
  `planning::sections::sections_of`, `planning::paths::check_relpath_syntax`,
  `planning::checklist::Offer::id`.
- **Planning server (plan 2)**, `loom::plan_server`: `routes.rs` (`all_routes()`,
  `install_all()`), `RouteSpec { method, path, body, role: Viewer|Holder|HolderOrTake, handler }`,
  `ApiRequest { ctx, head, browser, query, body }`, `ApiResponse`, `PlanContext` (`project()`,
  `authoring()` with `is_holder`, `holder()` and `subscribe`, `presence().name_of`,
  `hub()` with `broadcast`, `send_to_browser`, `send_to_holder`, `on_open`, `on_batch`,
  `extensions()`, `read_only()`), `wire/message.rs` (`PlanMessage`, `ClientMessage`),
  `ws/dispatch.rs`. API paths start with `/api/`. In `web/src/plan/`: `dock.tsx` (the right
  dock), `header-extras.tsx`, `help-keys.ts` (`PLAN_HELP_KEYS`), `api/client.ts` (`apiGet`,
  `apiPost`), `api/socket.ts` (`subscribe`, `send`), `state/atoms.ts` (`isHolderAtom`,
  `canAuthorAtom`, `presenceAtom`, `authoringAtom`, `serverInfoAtom`, `connectionAtom`). Plan 2
  owns the Share control and its popover.
- **File mode (plan 3)**: `plan_server::files::save::save_document(ctx, browser, SaveRequest { path,
  text, opened_hash, force }) -> Result<SaveOutcome { Saved | Conflict | Invalid }, SaveRefusal>`,
  `plan_server::files::texts::TextStore` in `ctx.extensions()` (`turn_pin`, `turn_end` giving
  `PinnedText { text, hash }`), `GET /api/diff/turn`; in `web/src/plan/`: `<Markdown>`,
  `<DiffView before after path />`, `openDiff(kind, path, turn?)`, `pendingAttachmentsAtom`,
  `addAttachmentAtom`, `AttachmentDraft`, `<AddToChat />`, `files/tree-entry-extras.tsx`.
- **Concept mode (plan 5)**: the browser files `web/src/plan/concept/{page-header-extras, key-chip-extras, empty-offer}.tsx` that
  render nothing or a disabled offer until this plan fills them.
- **Session bridge (plan 7)**: `PlanMessage::Chat(ChatFrame)` (`snapshot` with `ChatSnapshot {
  session: Option<SessionView>, turns, queue, previous }`, `delta` with `ChatDelta` ops, `result`
  with `client_id`, `queue_returned { texts }`) and `ClientMessage::Chat(ChatCommand)` (`start`,
  `send`, `withdraw`, `permission`, `interrupt`, `end`), all in
  `loom/src/plan_server/wire/chat.rs`, with full field lists in
  `doc/plans/briefs/plan-session-bridge/common.md` "The chat model"; fixtures
  `web/src/plan/api/fixtures/chat-{snapshot,deltas,commands,frames}.json`; the `Arc<Bridge>` in
  `ctx.extensions()` with `turn_running()`; turn pins keyed `<session id>/<turn id>`;
  `loom::plan_session::attachments::{format_message, Attachment, AttachmentInput}` with fixtures
  in `loom/tests/fixtures/plan_session/attachments/`; `plan_session::input::check_delivered`
  (100,000 bytes). Plan 7 has no web stage: this plan writes the zod schema and the store.
- **Guidance (plan 8)**: `GET /api/session/brief?session=<id>` -> `SessionBriefView {
  session_id, label, brief, model, effort, display_name }` (fixture
  `web/src/plan/api/fixtures/session-brief.json`); start ids `refine:<path>`,
  `refine-passage:<path>`, `knowledge`, `knowledge:<path>` beside plan 1's `Offer::id`s.

Every name above was the producing plan's when this plan was written. Confirm each with
`loom map --find-all <name>` or `rg`; when the merged name differs, use it and record
`loom memory note "producer-name: <assumed> is <merged> (<file>:<line>)"`.

## Rules for every worker

- Files at most 400 lines, functions at most 50; no row of `loom/maintainability-baseline.txt`
  changes. No new dependency.
- Rust: paths from requests through `check_relpath_syntax` (and `symlink_component` before any
  read); git through `crate::git::runner` with paths after `--` and refs only as resolved hex;
  no `unwrap` outside tests.
- Web: erasable TypeScript only; `z` from `@/api/zod`; no `dangerouslySetInnerHTML`; every
  string from the model, a suggestion or a route renders as text or through plan 3's renderer.
  Typography per `doc/loom/knowledge/conventions/web-dashboard-typography.md`: message text
  Inter 14px/1.5, chrome 12-13px, monospace only for paths, commands and identifiers, no
  coloured spine anywhere in the panel, state as glyph plus words.
- Every glyph has a text label. Animations respect `prefers-reduced-motion`.
- Never run git (tests that build git fixtures excepted), never touch `.loom/`. Record mistakes
  and decisions with `loom memory`.

## Fix-round rules (pressure test)

- **Path forms.** Every wire field named `path` (plan 7's `ChangedFile.path`, `Attachment.path`,
  this plan's routes) is repo-relative `doc/...`. Plan 7's `AttachmentInput.path` and every
  `rests_on` entry are repo-relative too. Only plan 3's `?path=`, `SaveRequest.path` and `/files/`
  addresses are relative to `doc/`. Convert with plan 3's `docRel`/`repoRel` (`@/plan/files/address`)
  in the browser and plan 2's `plan_server::wire::paths::{to_doc_rel, to_repo_rel}` on the server;
  never by hand.
- **Holder checks** server-side: `ctx.require_authoring(&browser, Take::HolderOnly)`.
- **Shared UI patterns from plan 2.** A control this browser may not use is plan 2's
  `<DisabledReason reason={string | null} onClick>{label}</DisabledReason>`
  (`@/plan/ui/disabled-reason`; `reason` null is an ordinary button; focusable `aria-disabled`
  button, reason as accessible description), with the reason from plan 2's
  `authorBlockReasonAtom` when the block is authoring. Fetches render through plan 2's
  `<LoadState state="loading|empty|not_found|error" title? message? action?={{ label, onClick }} />`
  (`@/plan/ui/load-state`).
- **Resync.** Plan 2's `planSocketAtom` handle's `onResync(fn) -> unsubscribe` (fires on every
  `hello` after the page's first; `connectionEpochAtom` bumps at the same moment): the chat model resyncs through plan 7's `on_open` snapshot;
  everything else this plan fetched (open-question counts, hashes, the compare list) refetches.
- **Untrusted text, visibly.** Every string from a transcript, a permission request, a tool row
  or a path renders through W1's `visibleControls` (`src/plan/chat/visible-controls.ts`): each
  Unicode Cf character (bidi controls U+202A-202E and U+2066-2069, U+200B-200F, U+061C, U+FEFF)
  becomes a visible `⟨U+202E⟩` marker, inside a container with `unicode-bidi: plaintext;
  direction: ltr`. Plan 7 keeps the raw text; the escape is display only.
- **Markdown in chat** passes `path="doc/README.md"` (plan 3's `path` prop is repo-relative, so
  relative links resolve inside `doc/`) and
  `headingIds={false}`.
- **Storage keys** use plan 2's `ServerInfo.project_key`.
