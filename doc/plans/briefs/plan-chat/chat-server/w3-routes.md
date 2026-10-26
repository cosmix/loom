# chat-server W3: Restore, turn sections, routes, WS commands, registration

Lane note: codex units own `review/restore.rs` (C5: `restore_file`, `save_path_of`) and
`review/sections.rs` (C4: `turn_sections`); both are in the tree when you start. The production
caller `restore_files` and the `turn-sections` handler live in your `routes.rs`; the sections
below on restore.rs and sections.rs describe what the codex units built.

Working dir `loom/`. You own `src/plan_server/review/{mod.rs, routes.rs, commands.rs,
route_tests.rs}`, `tests/plan_chat_operator.rs`, two lines in plan 2's `src/plan_server/routes.rs` (one in
`all_routes()`, one in `install_all()`) and one match arm in plan 2's
`src/plan_server/ws/dispatch.rs`. Read plan 2's existing route modules and plan 3's save route
first and copy their shape. Frozen contracts: `tests/plan_review_contracts.rs` and
`tests/plan_review_contracts/**`.

## mod.rs

Keep the foundation's module declarations; add `pub use routes::{install, routes};` once the
items exist.

## restore.rs

`restore_file` exactly as the PUBLIC SURFACE states (pure; the save function is a parameter).
Also `pub(crate) fn restore_files(req: &ApiRequest<'_>, body: RestoreRequest) -> RestoreResponse`,
the production caller (the route's role is `Holder`, so plan 2's gate already refused other
browsers):

- `turn_running`: `Bridge::turn_running()` on plan 7's `Arc<Bridge>` from `ctx.extensions()`
  (true while the chat session is Starting, Working or Permission).
- per path: `TextStore` from `ctx.extensions()` with plan 7's pin key `TurnId(format!("{session}/{turn}"))`
  (`plan_server/chat/pins.rs`); `start_text` = `turn_pin(&key, rel)`, `end_hash` =
  `turn_end(&key, rel)`'s hash. A turn id the store has never seen is a
  404 for the whole request.
- `save` closure over `plan_server::files::save::save_document(ctx, req.browser, SaveRequest { path,
  text, opened_hash, force })`, mapping `Ok(Saved { .. })` to `SaveResult::Saved`,
  `Ok(Conflict { current_text, current_hash })` to `HashMismatch`, `Ok(Invalid { validation })` to
  `Invalid` with the validation's error messages, and `Err(refusal)` to `Refused(<refusal's
  message>)`. The shared path predicate therefore runs inside plan 3's save path; never
  re-implement it here.

## sections.rs

`pub(crate) fn turn_sections(start: &str, end: &str) -> Vec<String>`: anchors (plan 1
`sections_of`, plus the preamble as `""`) whose span hash differs between the two texts or that
exist in only one. The route builds `TurnSectionsResponse` for every file the turn changed
(plan 3's `TurnChange` list for the turn, or every path with both a pin and an end) from the pin
and end texts.

## routes.rs

`pub fn routes() -> Vec<RouteSpec>` and `pub fn install(ctx: &Arc<PlanContext>)`. Register with
`review::routes()` in `all_routes()` and `review::install(` in `install_all()` (the stage's
wiring checks grep `review::routes()` in `routes.rs`).

| Method and path | Role | Body / query | Answer |
| --- | --- | --- | --- |
| `POST /api/restore` | Holder | `RestoreRequest`, `BodyKind::Json` | `RestoreResponse` |
| `GET /api/turn-sections` | Viewer | `session`, `turn` | `TurnSectionsResponse` |
| `GET /api/hashes` | Viewer | none | `HashesResponse` (`Project::content_hash` for every document; head from `rev-parse HEAD`, `None` in an unborn repository) |
| `GET /api/ref` | Viewer | `name` | `RefResponse`; `Invalid` 400, `Unknown` 404 |
| `GET /api/compare` | Viewer | `commit` (40 hex) | `CompareResponse`; `available: false` and no docs when `commit_exists` is false |
| `GET /api/at-commit` | Viewer | `commit`, `path` | `AtCommitResponse`; bad path 400 |
| `GET /api/built` | Viewer | `section` (relative to `doc/`) | `BuiltResponse` |
| `GET /api/open-questions` | Viewer | none | `OpenQuestionsMessage` |
| `GET /api/implemented-by` | Viewer | none | `ImplementedByResponse` |

Errors through `ApiResponse::error(status, code, message)`. Read-only mode needs nothing here:
plan 2's role gate and plan 3's save path refuse writes in it.

`install`: put one `Mutex<SuggestionBox>` in `ctx.extensions()`; `ctx.authoring().subscribe` so
`Taken` and `Freed` call `drop_for_holder_change` and send each returned `dropped` delivery to its
sender's browser; `ctx.hub().on_open` so a connection of the
holder's browser receives `pending_for()` as `PlanMessage::Review` frames; `ctx.on_batch` so
`open_question_counts(&ctx.project())` is recomputed and broadcast as `PlanMessage::Review` kind `open_questions` when it
changed since the last batch.

## commands.rs

`pub(crate) fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ReviewCommand)`, called from
one arm in plan 2's `ws/dispatch.rs` (spelled `review::commands::dispatch(`; the stage's wiring
check greps it). Plan 2 exposes the holder to server code as `is_holder(&BrowserId)` and
`holder() -> HolderView { public_id, name, away_since }`, never the holder's cookie value, so in
production every `BrowserRef.id` is the browser's public id (unique per browser, already sent to
every browser in presence, and never the cookie): the sender's from `conn.public_id`, the
holder's from `holder().public_id`. Refuse a `suggest` from a browser for which
`ctx.authoring().is_holder(&conn.browser)` (plan 2's holder query; writes go through
`ctx.require_authoring`) is true before calling the box. `suggest` calls
`SuggestionBox::submit` with the sender's name from `ctx.presence().name_of(&conn.browser)`; on
success send the `Delivery.message` with `ctx.hub().send_to_holder`, keep `id -> conn.browser`
in a map beside the box, and send `suggestion_status` `sent` (holder name, away flag from
`away_since`) to `send_to_browser(&conn.browser)`. `suggestion_answer` from the holder calls
`answer` and delivers the status to the sender's `BrowserId` from the map. A refusal answers the
sender's connection (`send_to_conn`) with `review` `refused` and the reason text ("Nobody holds
authoring", "The note is longer than 1,000 characters", "20 suggestions are already waiting",
"Only the author can answer a suggestion", "This browser holds authoring").

## route_tests.rs

Through plan 2's route-test harness: restore refused for a non-holder by the gate; restore of an
unknown turn 404; `ref?name=--output=x` 400; compare with a non-hex commit 400; at-commit with
`..` 400; built for a section with no plans returns an empty list; a `suggest` from the holder
refused; `open_questions` broadcast once after a batch that adds an Open questions item and not
after an unrelated batch. Proof: `cargo test --lib plan_server::review::`.

## Required test cases (coverage audit; named tests in `route_tests.rs` unless noted)

"restore refused for a non-holder by the gate"; "unknown turn 404"; "restore of a locked plan is
refused by save_document" (drives the real save path on a fixture project with a DONE- plan);
"invalid maps to invalid"; "ref option-like 400"; "compare non-hex 400"; "compare available
false"; "at-commit dot-dot 400"; "built with no plans is empty"; "suggest from the holder
refused"; "status delivered to the sender"; "sent status carries away"; "holder reconnect
receives pending"; "open_questions broadcast once after a batch". In `sections.rs`: "changed
heading listed", "unchanged heading omitted", "added and removed headings listed", "preamble
change listed as empty anchor".

## Fix-round additions (pressure test px-seams-03, px-state-23, px-exec-09)

- `pub fn save_path_of(changed_path: &str) -> Option<&str>` in `restore.rs`: a thin wrapper over plan 2's
  `plan_server::wire::paths::to_doc_rel` returning the doc-relative form, `None` for a path
  outside `doc/` or one that fails `check_relpath_syntax`. `restore_files` maps every requested path
  through it before `PinnedFile` lookup and `SaveRequest` (a `None` is `refused` "outside doc/").
  Frozen contract `restore-path-from-changed-file` pins it.
- Turn pins are keyed `<session id>/<turn id>` (plan 7); `RestoreRequest` and `turn-sections`
  take `session` and `turn` separately and join them.
- Suggestions dropped on a holder change send `suggestion_status` `dropped` to each sender
  (test "dropped suggestions notify senders").
- `tests/plan_chat_operator.rs` (yours): one `#[ignore]` test `real_session_attach_send_restore`
  that builds a fixture git repository in a TempDir (vision, two sections, one written plan),
  starts the planning server in-process on port 0 with `LOOM_HOME` in a TempDir, takes authoring
  over its WebSocket, sends `ChatCommand::start` with one source `AttachmentInput`, waits (bounded
  10 minutes) for a turn whose `changed_files` is non-empty, POSTs `/api/restore` for one changed
  file, and asserts the file's text equals its turn-start text. It FAILS (never skips) when `tmux`
  or a logged-in `claude` is missing, naming what is missing, and it asserts the installed Claude
  Code writes a `Stop` after the turn (print `claude --version` in every failure message), so a
  behaviour change is a visible failure. Integration-verify blocks on the operator running it.
