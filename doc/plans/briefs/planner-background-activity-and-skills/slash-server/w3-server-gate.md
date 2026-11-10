# W3: the send gate, the commands route and the catalogue owner (steps B2 server, B3 server)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run alone in wave 3, after W1 (`plan_session::commands`, `launch::script::{claude_argv, claude_env}`) and W2 (`Bridge::command_cell`, `Bridge::launch_spec`, the notices) returned. Decisions B-D1 ("Delivery to the web", "Lifetime", "Bounds", "Catalogue states") and B-D2 of the plan are the specification.

## Files owned

`src/plan_session/input.rs`, `src/plan_server/commands/**` (new), `src/plan_server/mod.rs` (the `pub mod commands;` line), `src/plan_server/routes.rs` (one line), `src/plan_server/chat/mod.rs` (one line), `src/plan_server/chat/dispatch.rs`, `src/plan_server/chat/tests_dispatch.rs`, `src/plan_server/chat/tests_dispatch/**`, `src/plan_server/wire/**`, `tests/plan_session_bridge_core_contracts.rs`, `tests/plan_session_bridge_core_contracts/records.rs`, `web/src/plan/api/fixtures/session-commands.json` (new), `web/src/plan/api/fixtures/slash-vectors.json` (new).

Frozen, read only: `src/plan_session/input_contract_tests.rs` and `src/plan_server/commands/contract_tests.rs` (declared by the contract session; keep their `mod` lines).

## What exists (verified 2026-11-10)

- `plan_session/input.rs:16-21` `InputRefusal { SlashCommand, TooLong, TooManyAttachments, Empty }` with its `Display`; `:58-68` `check_send(text)` sanitizes, skips leading whitespace and U+FEFF and accepts only `/compact`; its tests `:103-149` pin `/model opus` and `\n/compactx` refused and the disguise cases.
- `plan_server/chat/dispatch.rs:317-345` `checked(ctx, text, inputs)` resolves attachments, sanitizes, then `check_send(&text)` (`:337`) and `check_delivered`; `start` (`:142-`) checks its text before `launch`. `tests_dispatch.rs:145-148` and `tests_dispatch/start.rs:69-73` expect `refused(InputRefusal::SlashCommand)`.
- `tests/plan_session_bridge_core_contracts.rs:91-116` `check_send_refuses_slash_commands_except_compact`; `:148-170` `noise_records_and_repeated_uuids_produce_no_items` over `plan_session_bridge_core_contracts/records.rs:101-130` (its `local_command` line now yields W2's notice item).
- `plan_server/routes.rs:69-80` `all_routes()`, one `routes.extend(super::<module>::routes());` line per route module; `plan_server/guidance/brief_route.rs:20-42` is the Viewer route pattern and `:44-61` its `include_str!` fixture test; `guidance/tests.rs:232-242` calls a route handler in a test.
- `plan_server/chat/mod.rs` `install_with` (background-server added `background::install`); `ctx.own_worker` owns a thread; `ctx.hub().on_open` and `Bridge::subscribe` take several callers.
- `wire/message.rs` `PlanMessage` and `wire/tests_schema.rs` `assert_exhaustive` and `all_plan_messages()` (every variant), whose tag parity test reads `PLAN_MESSAGE_TYPES` from `web/src/plan/api/schema.ts` (W4 adds `"commands"`).

## Steps

1. **Gate, `input.rs`.** `check_send(text, table)`, `InputRefusal::Command(String)` and `MAX_SLASH_LEN` as `common.md` fixes them. After today's stripping, a text whose first character is not `/` is accepted. Otherwise: more than one line, or more than 300 characters, is refused with its text; the first token without its slash goes to `table.lookup`; a native row's class decides (Run and Run with handling accepted, Terminal only refused with its reason); a non-native name is accepted when the table is `Ready` and holds it, refused with `Not in this session's command list` when `Ready` without it, `Commands are still loading` when `Loading`, `This session's command list is unavailable` when `Unavailable`. Rewrite the module tests at `:103-149` (the plan's "Planned edits": `/model opus` refused as Terminal only, `/compactx` as unlisted, the disguise cases kept).
2. **`dispatch.rs`.** `checked` takes the table: `send` passes the chat session's (`bridge.command_cell(&id)`'s current table), `start` passes `CommandTable::loading()` (B-D2 "Before a session exists"; the opening the server pastes itself never goes through `check_send`). A slash message with attachments is refused with `Send a command alone, on one line`. Update `tests_dispatch.rs:147` and `tests_dispatch/start.rs:71` to the class refusal and its reason.
3. **`plan_server/commands/mod.rs`** (new; the contract session may have created it as a declaration-only root):
   - `CommandsView` and `CatalogueWireState` (`common.md`), built from a `CommandTable` with every string through `inline_safe`;
   - `pub fn routes() -> Vec<RouteSpec>` with `GET /api/session/commands`, `Role::Viewer`: the view of the `session` query's catalogue, or the `Loading` view of the native rows for no session, an unknown one, or a context without the catalogue extension; never 404;
   - `pub(crate) fn install(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>)`, called from `chat/mod.rs` `install_with` beside `background::install`: it inserts the catalogue (an `Arc` with a `Mutex` per-session map of `SideProcess` handle and latest table) into `ctx.extensions()`, starts one owner thread (`command catalogue`, owned with `ctx.own_worker`) and subscribes a bridge listener that only sends requests to that thread over a channel, so the driver thread never spawns or waits on a process;
   - the owner thread: for the chat session (`bridge.chat_session_id()`), once `bridge.launch_spec(id)` is `Some` and no side process exists for it, build a `SideSpec` (program `spec.claude_bin`; args `side_argv(&claude_argv(&spec, &spec.dir.settings()), &<session dir>/side-settings.json)`; env `claude_env(&spec, &[])` without `LOOM_PLAN_SESSION` and `LOOM_PLAN_STATE`; cwd `spec.project_root`; settings `side_settings` of the session's settings file read size-bounded) and `SideProcess::start` it with `STARTUP_DEADLINE`; on each table, store it, swap it into the session's `CommandCell`, and broadcast `PlanMessage::Commands`; on `BridgeEvent::Ended` for a session, `stop()` its process and drop its entry; at shutdown stop every process. One start per session start; no retry within it;
   - an `on_open` hook replaying the chat session's latest `Commands` frame.
4. **`routes.rs`**: `routes.extend(super::commands::routes());` in `all_routes()`.
5. **Wire.** `PlanMessage::Commands` (`common.md`) in `message.rs`, in `assert_exhaustive` and `all_plan_messages()`; `web/src/plan/api/fixtures/session-commands.json` written from the Rust serializer (pretty, trailing newline) and round-tripped in a new `plan_server/commands/tests.rs` with `include_str!`, as `brief_route.rs:44-61` does.
6. **Vectors.** `web/src/plan/api/fixtures/slash-vectors.json` as `common.md` specifies; a Rust test in `plan_server/commands/tests.rs` (or `plan_session/input.rs` tests) runs `check_send` over every case with the case's table rebuilt from its view and asserts `expect`.
7. **Core contracts** (`tests/plan_session_bridge_core_contracts.rs`): rewrite `check_send_refuses_slash_commands_except_compact` to the class table (built with `CommandTable::from_initialize` over the probe rows; `/compactx` refused as unlisted; the `check_delivered` half unchanged), and update `noise_records_and_repeated_uuids_produce_no_items` to the items W2's mapping now gives (the `task-notification` line adds no item; the `local_command` line adds a notice). Keep every other assertion.
8. **Tests** in `plan_server/commands/tests.rs` (fake `claude` scripts in a TempDir, `chat::tests::Fixture` for the server side): a `commands_changed` line replaces the list and broadcasts `Commands` with generation 2; a process that never answers gives `Unavailable` after the deadline with native rows only (the owner takes the deadline from a field the test lowers, defaulting to `STARTUP_DEADLINE`); the side process is started once per chat session and stopped when the session ends; the route answers `ready` with the session's entries once the table arrived; the frame replays to a new tab.
9. **Run** `cargo test --lib plan_session::input plan_server::commands:: plan_server::chat:: plan_server::wire::` and `cargo test --test plan_session_bridge_core_contracts` once ready. One failure is expected and not yours: `wire::tests_schema::plan_message_wire_values_are_in_plan_schema` reads `PLAN_MESSAGE_TYPES` from `web/src/plan/api/schema.ts`, which gains `"commands"` in wave 4 (W4). Report it; everything else must pass.

## Done when

The two frozen contracts and your tests pass; `rg -n 'SlashCommand' src tests` finds nothing; `chat/mod.rs` is at most 400 lines; `routes.rs` names `super::commands::routes()`.

## Traps

- The listener runs on whatever thread flushes the bridge, the driver's included: it must only send to the channel.
- A request for another session than the chat session starts nothing; a summary session never gets a side process.
- The side settings file lives in the session directory (`spec.dir.path()`), beside the session's own settings file, never in the project tree.
