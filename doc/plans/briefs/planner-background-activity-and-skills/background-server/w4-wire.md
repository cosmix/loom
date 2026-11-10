# W4: plan-server join, thread and wire (step A5, Rust side)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run alone in wave 3, after W3 returned: `BridgeEvent::Background`, `bridge::background::BackgroundTask` and `bridge::registry::read_agent_meta` exist, and so do W2's `tokens::{planner_pass, spawn_planner_tokens, PlannerGroup}`. Decisions A-D1c and A-D4 of the plan are the specification.

## Files owned

`src/plan_server/chat/mod.rs` (two lines only), `src/plan_server/chat/background/**` (new directory module), `src/plan_server/wire/**`, `web/src/plan/api/fixtures/background.json` (new; path from the repository root), `.github/workflows/ci.yml` (one line).

Frozen, read only: `src/plan_server/chat/background/contract_tests.rs` (contracts `the-background-frame-flattens-transcript-text` and `install-with-publishes-background-token-totals`). The contract session created `chat/background/mod.rs` as a declaration-only root holding `#[cfg(test)] mod contract_tests;` and may have added `#[cfg(test)] mod background;` to `chat/mod.rs`: the module is production code, so the line becomes a plain `mod background;`.

## What exists (verified 2026-11-10)

- `plan_server/chat/mod.rs` has 395 lines (cap 400). `install_with` (`:126-158`) builds the bridge, subscribes listeners (`publish::subscribe`, `open_snapshots`, `follow_authoring`, `follow_sessions`), starts the driver and owns it with `ctx.own_worker("chat driver", driver, Some(stop))`.
- `Bridge::subscribe` and `ctx.hub().on_open` both take any number of callers (`plan_server/ws/hub.rs:92-94`); `on_open` hooks return the frames a new tab gets. `chat/team.rs:135-144` `snapshots()` is the replay pattern; `team::publish_draft_team` (`:96-124`) stores the view and broadcasts it.
- `ctx.own_worker(name, handle, stop)` (`plan_server/context.rs:270-277`) sets `stop` and joins `handle` at shutdown. `ctx.runtime().dir` is the runtime directory (`planning/runtime.rs:30-45`).
- `Bridge::chat_session_id()` and `Bridge::transcripts(id) -> Option<Transcripts { main, subagents: Vec<(String, PathBuf)> }>` (`plan_session/bridge/mod.rs`); `transcripts_of` lists teammate files too.
- `wire/message.rs:38-40` `PlanMessage` is `#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]`; struct variants such as `KickoffDrafts { drafts }` exist. `wire/tests_schema.rs:200-226` `assert_exhaustive` and `:228-` `all_plan_messages()` must name every variant; `:322` checks each tag is in `PLAN_MESSAGE_TYPES` of `web/src/plan/api/schema.ts` (W5 adds `"background"` there).
- `wire/tests_chat.rs` has 397 lines: the new parity test goes in a new file. The parity pattern is `assert_parity` in `tests_chat.rs:20-24` (parse the fixture, serialize, compare values).
- `crate::context::untrusted::inline_safe` (`context/untrusted.rs:178`) is the flattening the status payload uses (`status/data/sanitize.rs:100-102`).
- CI's Rust path filter lists `"web/dist/**"` and `"web/src/api/**"` (`.github/workflows/ci.yml:45-46`) and not `web/src/plan/api/**`, so a fixture-only change there skips the Rust parity tests.

## Steps

1. **`wire/background.rs`** (new, `pub mod background;` in `wire/mod.rs`): `BackgroundTaskView` as `common.md` defines it, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]`, and `PlanMessage::Background { session: String, tasks: Vec<BackgroundTaskView> }` in `message.rs` with a one-line doc. Add the variant to `assert_exhaustive` and a populated value to `all_plan_messages()` (parse `background.json`).
2. **`web/src/plan/api/fixtures/background.json`**: the five-task frame `common.md` lists, written from `serde_json::to_string_pretty` of the Rust value plus a trailing newline, so the Rust parity test can compare text too. A new `wire/tests_background.rs` (declared in `wire/mod.rs` beside `tests_chat`) round-trips it with `include_str!("../../../../web/src/plan/api/fixtures/background.json")`.
3. **`chat/background/mod.rs`**:
   - `pub(crate) fn frame(session: &str, tasks: &[BackgroundTask], flows: &[AgentFlow]) -> PlanMessage`: one view per task in the given order, joined to the flow whose `id` equals the task id (`total` and `tokens_per_min` from `TokenUsage::fresh`, `model`, `active`), every string through `inline_safe`.
   - `pub(super) fn install(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>)`, called from `install_with` right after `follow_sessions`. It holds its own state (`Arc` of a struct with a `Mutex`: the chat session id, its latest tasks, the latest flows for it, the last frame broadcast) and registers:
     - a bridge listener (weak references, as `follow_sessions` holds them): on `BridgeEvent::Background { id, tasks }` for `bridge.chat_session_id()`, store the tasks and broadcast the joined frame when it differs from the last one broadcast; on `BridgeEvent::Ended { id, .. }` for the stored session, clear the state and broadcast `Background { session: id, tasks: [] }`; when a `Background` event names a session other than the stored one and that session is the chat session now, broadcast the empty frame for the old one first;
     - an `on_open` hook returning the last frame broadcast, if any (replay);
     - the `planner-tokens` thread: `spawn_planner_tokens(<ctx.runtime().dir>/tokens, groups, sink, stop)`, owned with `ctx.own_worker("planner tokens", handle, Some(stop))`; a spawn error is logged with `tracing::warn!` and the chat runs without token figures.
   - `groups`: `bridge.chat_session_id()` then `bridge.transcripts(&id)`; no main transcript means no group; for each subagent file, `read_agent_meta(<its parent dir>, agent_id)`, skipping a teammate (`meta.teammate` is set) and taking `meta.agent_type`. The closure takes the bridge lock only through those two calls.
   - `sink`: store the flows for the chat session's group; broadcast the joined frame only when the table is non-empty and an active agent's `total` or `tokens_per_min` changed since the last frame broadcast (at most once per pass).
4. **`chat/mod.rs`**: the `mod background;` line and the `background::install(ctx, &bridge);` call. Nothing else changes there.
5. **`.github/workflows/ci.yml`**: add `- "web/src/plan/api/**"` under the `rust:` filter, after `"web/src/api/**"`.
6. **Tests**, in `chat/background/tests.rs` (declared `#[cfg(test)] mod tests;` in `chat/background/mod.rs`), on the chat `Fixture` of `src/plan_server/chat/tests.rs` (`Fixture::new(false)`, `connect`, `idle_chat`, `spool`; `tab.opened` holds the `on_open` frames, `tab.frames()` the rest):
   - a new connection gets the replay of the last frame;
   - nothing is broadcast when a `Background` event repeats the last tasks;
   - an `Ended` chat broadcasts the empty frame;
   - a pass that changes only an active agent's total broadcasts a frame with an unchanged table (call the sink path directly with two flow vectors);
   - a new chat session replaces the group: the old session's frame is emptied, and after the next pass its group is gone from `<runtime>/tokens/checkpoint.json`;
   - the group builder skips a subagent whose meta has `taskKind: in_process_teammate` and takes `agentType` from the meta;
   - shutdown joins the `planner-tokens` thread: after `install` ran, `ctx.stop_workers()` (`plan_server/context.rs:279-281`, which sets every owned stop flag and joins every owned handle) returns `Ok` within 10 s (the thread sleeps in 250 ms slices, `tokens/tracker.rs` `sleep_unless_shutdown`).
7. **Run** `cargo test --lib plan_server::chat:: plan_server::wire::` once your code is ready. One failure is expected and not yours: `wire::tests_schema::plan_message_wire_values_are_in_plan_schema` reads `PLAN_MESSAGE_TYPES` from `web/src/plan/api/schema.ts`, which gains `"background"` in wave 4 (W5). Report it; everything else must pass.

## Done when

The two frozen contracts and your tests pass, `chat/mod.rs` is at most 397 lines, `rg -n 'Background' src/plan_server/summaries` still shows W3's single arm, and `rg -qF '"web/src/plan/api/**"' .github/workflows/ci.yml` succeeds from the repository root.

## Traps

- A listener runs inside the bridge's flush, after its lock is released: it may call `chat_session_id()`, but must never block or wait on the bridge.
- The `groups` and `sink` closures and the listener hold `Weak` references to the bridge and the module state (as `follow_sessions` does), and the thread ends when they no longer upgrade, so a test context dropped without a shutdown leaves no thread behind.
- The token figures are the agents' own; the main agent's row and codex rows never appear, because only table entries are joined.
- `background.json`'s optional fields are all present (`null` where unset): serde writes `null` for `None`, and the strict zod schema requires every key.
