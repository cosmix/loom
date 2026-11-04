# K1 — kickoff drafts that outlive the session, rename, set aside, continue (issues 10, 11; server)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first (D10, D11, D12).

## Store

New module `loom/src/plan_server/chat/drafts.rs`. It stays the module root (the plan's artifact and before/after checks name it); child files go under `chat/drafts/` if it nears 400 lines. It also holds `pub fn routes()` for the listing routes (the path literal `"/api/kickoffs"` lives here), re-exported from `chat/mod.rs` as `chat::routes`, and `routes.rs` `all_routes` gains `routes.extend(super::chat::routes());`. `routes.rs` holds no path literals.

- Directory `<runtime>/kickoffs/` (`ctx.runtime().dir.join("kickoffs")`), created with `safe_fs::safe_create_dir_all` at 0700 on first write. File `<session id>.json`.
- The session id is a canonical lowercase hyphenated UUID, checked as `plan_session/bridge/requests.rs` `canonical` checks it (`pub(super)` there: copy the one-line check, `Uuid::try_parse(id).is_ok_and(|u| u.hyphenated().to_string() == id)`). Apply it to file names on read, to route parameters and to command fields, before any id becomes a path. The listing reads only `<canonical uuid>.json` names, so temp files are ignored.
- Shape:

  ```json
  {"session": "<id>", "started_at": "<rfc3339>", "updated_at": "<rfc3339>", "messages": ["<typed text>", "…"], "shape": { /* the agent's last KickoffShape, without the override, or null */ }, "attachments": [ /* the start command's AttachmentInput values, wire JSON */ ], "name_override": null, "target": null, "live": true}
  ```

- Caps: 40 messages of at most 4 KiB each. Text beyond the cap is cut at a char boundary with " […]". The oldest messages are dropped first, keeping the first message. A file is read only up to 256 KiB, and the caps and the shape's string limits are applied again on read; a file that fails either is skipped with a warning and left in place.
- Writes go to a temp file in `kickoffs/`, then rename. Every read-modify-write of a draft holds one `Mutex` in `ChatState` (the bridge listener, the dispatch thread and the `Ended` handler all write), and nothing writes `live: true` after `Ended`.
- Written at kickoff start, with the user's typed text (not the guidance) and the start's attachment inputs (at most `MAX_ATTACHMENTS`), from the start command in `chat/dispatch.rs` `launch()`. `target` and `attachments` default (`null`, `[]`) when a file lacks them.
- Every draft write calls `crate::plan_server::uploads::touch(runtime_dir, id)`, written as `uploads::touch(` in `chat/drafts.rs` itself (a plan wiring check greps it there), (the chat stage's U1 provides it) for each `file` attachment it holds, so the 30-day attachment sweep never removes a file an unfinished draft references.
- Updated:
  - on every `KickoffTracker::observe` that changes the view (`chat/team.rs` `observe_kickoff`);
  - on every user `send` in a kickoff session;
  - on `rename_kickoff`;
  - on `confirm_kickoff` (`chat/kickoff_cmd.rs`), which records `target` (the confirmed document's repository path);
  - on `BridgeEvent::Ended` for the kickoff session (`chat/mod.rs`), which writes `live: false`. Crash recovery (`plan_session/bridge/recovery.rs`) ends through the same event.
- `live` in every view is computed when read: the stored flag AND `bridge.chat_session_id() == Some(session)`. After a server restart nothing is running, so every draft reads `live: false` whatever the file says (the stage contract writes `live: true` and expects `false`).
- Landing: landing is detected only in the browser today (`new/kickoff-landing.ts` watches the checklist). The store checks it itself: on every listing and every write, a draft whose `target` exists under the project root (a plan file under any lifecycle prefix, `IN_PROGRESS-`/`DONE-`/`RETIRED-`, or moved to `archive/`, included) is deleted and never listed.
- Removed also when the user discards it (`discard_kickoff {session}`, Holder).
- Ended drafts whose `updated_at` is older than 30 days are deleted at server start (`chat::install`).

## Wire

- `loom/src/plan_server/wire/kickoff.rs`: `KickoffDraftView {session, name: Option<String>, size: Option<KickoffSize>, problem: Option<String>, started_at: String, updated_at: String, live: bool}`. `name` is `name_override` or `shape.name`.
- `GET /api/kickoffs` returns `{drafts: [KickoffDraftView]}`, newest `updated_at` first. Role Viewer.
- `GET /api/kickoffs/<session>` returns `{draft: KickoffDraftView, shape, messages}`. Role Viewer.
- A `PlanMessage::KickoffDrafts { drafts }` with tag `kickoff_drafts`, in `wire/message.rs` (there is no `messages.rs`), is broadcast after every store change and sent on hello. The web arms are K2's (`schema.ts` `PLAN_MESSAGE_TYPES` plus the schema spread; `api/schema.test.ts` checks both arms). K2 writes `web/src/plan/api/fixtures/kickoff-drafts.json` from this brief with every optional field populated, e.g. `{"type":"kickoff_drafts","drafts":[{"session":"11111111-1111-4111-8111-111111111111","name":"microvm-test-env","size":"feature","problem":"Tests need isolation.","started_at":"2026-11-04T10:47:11Z","updated_at":"2026-11-04T10:50:00Z","live":false}]}`. Your Rust parity test `include_str!`s that web file, as `wire/tests_chat.rs` reads the web fixtures. Write no fixture under `wire/fixtures/` and no web file.

## Commands (`wire/chat.rs`, `chat/draft_cmd.rs`, `chat/dispatch.rs`)

- The four variants go in `wire/chat.rs` `ChatCommand` (`#[serde(tag = "command", rename_all = "snake_case")]`): `RenameKickoff { name, client_id }`, `SetAsideKickoff { session, client_id }`, `ContinueKickoff { session, client_id }`, `DiscardKickoff { session, client_id }`. Add each to `wire/tests_chat.rs`'s exhaustive `ChatCommand` match and its tag list; K2 adds an example of each to `web/src/plan/api/fixtures/chat-commands.json` from these shapes.
- `chat/dispatch.rs` is 392 lines at base. Their handlers go in a new `chat/draft_cmd.rs`; `dispatch.rs` gains only the match arms.
- `rename_kickoff {name}`, Holder, for the current kickoff session.
  - It refuses a name failing `is_feature_name` (`planning/kinds.rs`) with the same refusal path the confirm uses.
  - It refuses before any shape exists ("Nothing to rename yet") and once `view.target` is set ("The name is fixed once the work is confirmed": `confirm_kickoff` armed the write grant for the old name).
  - It stores `name_override` BESIDE `KickoffTracker`'s `current`, never inside it: `observe` compares raw agent shapes (it skips when `view.shape == shape`), so an override written into `current` would make every later agent block look changed. `view()` and the draft view apply the override as `shape.name`; later agent blocks cannot replace it. Broadcast `Kickoff` and `KickoffDrafts`.
  - Send the agent one user message, delivered like `confirm`'s message: "I renamed this work to `<name>`. Use that name from now on." It shows as a user turn in the transcript; that is intended.
- `set_aside_kickoff {session}`, Holder: ends that session through the normal end path, so the draft stays with `live: false`.
- `continue_kickoff {session}`, Holder: starts a new kickoff (refused like any start while another chat is open), as `AuthoringStart::Kickoff { small }` with `small = (shape.size == plan)`. It carries the whole draft, not only its text:
  - The effective name (`name_override`, else `shape.name`) becomes the new draft's `name_override`, and the opening says "The work is named `<name>`." The complete shape (size, name, problem, decisions, questions, sections, proposal) seeds the new session's `KickoffTracker` view and the new draft's `shape`, so Taking shape shows it before the agent's first block.
  - The draft's attachment inputs go on the new start as its attachments. A `file` input whose id no longer resolves, or a document input whose path is gone, is dropped before `checked()` and named in the opening ("(attachment no longer available: <name>)"), so one missing file cannot refuse the continuation.
  - Ordering: the old draft is deleted only after the new session's start returned Ok AND the new draft file is renamed into place. A refused or failed start, or a failed write of the new draft, leaves the old draft file as it was. Its opening carries the draft:
  - the guidance (`planning/guidance/opening.rs` `kickoff_opening`, private: call it through the public start path) plus one paragraph: "This continues an unfinished draft. Its problem, settled decisions, open questions and the user's earlier messages follow; pick up where it left off."
  - then the three shape fields and the messages, as plain quoted text, through the same `sanitize` as any opening. The whole opening must fit `plan_session/input.rs` `MAX_DELIVERED_BYTES` (100,000 bytes), and a full draft (40 × 4 KiB) does not: carry the first message plus as many of the newest messages as fit in 60,000 bytes, and write "(earlier messages omitted)" where any were dropped.
- `discard_kickoff {session}`, Holder, refused for a live session ("Set it aside first").
- Commands use the existing refusal and Holder machinery.

## Product spec

Rewrite the kickoff passage of the feat-planner product spec (`rg -l 'Taking shape' doc/product/feat-planner`) to the new truth: drafts persist, New work starts fresh, rename, set aside and continue. Update `plan-plugin/skills/kickoff/SKILL.md`:

- a user rename arrives as a message, and the agent uses that name in later `loom-kickoff` blocks;
- a continuation opening lists the earlier draft.

## Tests

- The frozen stage contract `drafts-survive-a-restart`.
- Unit tests, with these exact names for the three the plan's `after_stage` greps:
  - `a_started_kickoff_writes_its_draft`: the production lifecycle through the dispatch test harness (`chat/tests_dispatch/kickoff.rs`) and its fake bridge, with no hand-written file: a kickoff start with typed text and one attachment writes a draft holding both; a user send appends to `messages`; an agent shape observed through `chat/team.rs` `observe_kickoff` updates `shape`; `rename_kickoff` sets `name_override`; `Ended` sets `live: false`. Read the emitted file after each step, then build a fresh store over the same directory (the restart) and list it: the draft reads back with the override name, the shape, the messages and `live: false`. Removing any one production write call fails the test;
  - `a_rename_wins_over_a_later_agent_block`: after a rename, a later agent block with another name leaves the view's and the draft's name at the override;
  - `a_landed_kickoff_leaves_the_drafts`: a draft whose `target` exists is not listed and its file is gone; `discard_kickoff` on an ended draft deletes its file;
  - `a_continued_draft_carries_its_name_size_and_attachments` (also grepped): a standalone-plan kickoff started with an attachment only (no typed text), renamed, set aside; a fresh store over the same directory (the restart); `continue_kickoff` starts a session with `small: true` and the file attachment, its opening names the override, the new draft holds the override, the shape and the attachment, and only then is the old file gone. A second case whose start is refused leaves the old file unchanged;
  - a dispatch test for each of the four commands through `tests_dispatch`, success and refusal: rename (accepted; bad name; before a shape; after confirm), set aside (ends the session, the draft stays with `live: false`; unknown session), continue (above; refused while another chat is open; unknown session), discard (above; refused for a live session; non-canonical id). Each new variant also round-trips through the `chat-commands.json` parity test;
  - and: the caps on write and on read; corrupt and oversized file skips; a non-canonical session id refused; refusal of `rename_kickoff` before a shape and after confirm; refusal of `discard_kickoff` on a live session; the continuation opening's text and its byte budget; the 30-day sweep.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- plan_server::chat:: planning::guidance::` (one command; filters after `--`). `plan_server::wire::` reads K2's web fixtures, so the main agent runs it once both of you have returned.
