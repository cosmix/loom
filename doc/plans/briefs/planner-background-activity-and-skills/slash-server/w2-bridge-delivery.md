# W2: per-class slash delivery, command records and notices (step B4)

Read `common.md` in this directory first. You are a `loom-senior-software-engineer`, spawned with effort xhigh. You run alone in wave 2, after W1 returned (`plan_session::commands::{CommandTable, CommandCell, CommandEntry, CommandClass}` exist). Decisions B-D2 ("Checked again at delivery", "Length"), B-D3 and B-D4 of the plan are the specification; read them with `probe-notes.md` "Record shapes per class", "Timing" and "Fork skill".

## Files owned

`src/plan_session/bridge/**`, `src/plan_session/transcript/**`, `src/plan_session/model/**`, `src/plan_session/state.rs`, `src/plan_session/state_tests.rs`, `src/plan_session/state_tests_more.rs`, and the chat fixtures `web/src/plan/api/fixtures/chat-snapshot.json` and `chat-deltas.json` (repository paths: add `"command": null` to each user item, because the Rust parity test `plan_server/wire/tests_chat.rs` loads them and must match the serializer; keep their formatting).

Frozen, read only: `src/plan_session/bridge/slash_contract_tests.rs` (two contracts) and `src/plan_session/transcript/commands_contract_tests.rs` (one). Not yours: `src/plan_session/input.rs` (W3), `tests/plan_session_bridge_core_contracts.rs` (W3 updates its noise test to your notices after you return), everything under `src/plan_server/`.

## What exists (anchor by symbol; `background-server` changed these files)

- `bridge/session.rs` `Delivering { item_ids, text, match_text, sent_at, closing, opening, compact }`; `Flow.compact: Option<Option<Instant>>`.
- `bridge/flow.rs`: `deliver_step` pops the queue into `delivery()`, which adds the `sent` user item (`sent_item`) and returns a `Delivering`; `time_out_delivery` marks a delivery `Undelivered`, keeps it in `flow.undelivered` (8 kept) and pauses the queue; `confirm_prompt` confirms on any `UserPromptSubmit` that is not a background prompt; `confirm_compact` confirms a pasted `/compact` on the boundary record or `SessionStart { compact }`; `confirm_text` confirms the delivery whose `match_text` equals a human record's text and frees the slot at once; `late_confirm` confirms a timed-out one by text.
- `bridge/driver.rs`: `COMPACT_SETTLE` (2 s) and the timers step that turns a settled `/compact` into a synthetic idle; `apply_event`'s `PreCompact` arm only adds the "Compacting the context" notice; `on_session_start` ends a delivered `/compact`.
- `transcript/records.rs` (at the 400-line cap after background-server): `WRAPPER_TAGS` skips user records starting `<command-` or `<local-command-`; `system()` skips `local_command` and `informational` records; `cap_chars`, `ITEM_CHARS`.
- `state.rs` `CompactDelivered` input and the `compact` source handling; `state_tests.rs` `a_delivered_compact_then_session_start_compact_is_idle` and `bridge/tests_queue.rs` `a_compact_boundary_with_no_stop_after_it_goes_idle` feed a synthetic `UserPromptSubmit` for `/compact`, which 2.1.321 never sends (`probe-notes.md` per-paste matrix).
- `model/mod.rs` `Item::User { id, text, attachments, delivery, origin }`; every construction site is under `src/plan_session/` (`bridge/flow.rs` twice, `transcript/opening.rs`, `model/tests.rs`, and test files: `rg -n 'Item::User \{' src/plan_session` and add the field where a site has no `..`).

## Design (settled)

1. **The session's table.** `Session` holds a `CommandCell` created with `CommandTable::loading()`, and keeps the `LaunchSpec` it was spawned with when it leaves `Phase::Spawning`. `Bridge::command_cell(id)` and `Bridge::launch_spec(id)` (`common.md`) return clones; both take the bridge lock only for the lookup. `bridge/mod.rs` is near its cap: put the accessors' bodies in a small new `bridge/catalogue.rs` if the file would pass 400 lines.
2. **Classing a delivery.** When a queued or sent message starts with `/` (after the control and whitespace stripping `input.rs` does), its delivery records the canonical name (through `lookup`, so `/cost` is `usage`), its trimmed args and its class from the cell's table at that moment. Just before the paste, `deliver_step` reads the cell again: an item whose name is now refused is never pasted; it is dropped with the `Not sent (<reason>): <first line>` notice of `common.md`, and the next queued item is eligible in the same step.
3. **Confirmation per class** (B-D3), matched by canonical name, then args trimmed, falling back to name plus order:
   - Run: today's `confirm_prompt` path (the `UserPromptSubmit` carries the typed line), or the prompt-type `<command-message>`/`<command-name>` user record;
   - `/context`: the first `system` `local_command` record whose `commandRun.command`, else whose trimmed `<command-name>`, is the canonical name;
   - `/compact`: `PreCompact` with `trigger: "manual"`, the compact boundary or `SessionStart { compact }`; the `PreCompact` arm of `apply_event` calls one function for the notice and the confirmation, so the function gains no lines;
   - a name that left the list after the delivery check: the `Unknown command: /x` `informational` record confirms a delivery of `/x` as a local one.
4. **The local delivery** (B-D4). `Delivering` gains `local: Option<LocalSettle>` with `LocalSettle { delivery_id: String, confirmed_at: Option<Instant> }`, `delivery_id` the queue item id. A local delivery (`/context`, `/compact`, the `informational` case, and a fork skill once its `<forked-skill-launch>` record arrives) holds the delivering slot from paste until it settles, so no other item is eligible meanwhile. It settles `COMPACT_SETTLE` (2 s) after its last command or output record; if its paste set busy (a fork skill's `UserPromptSubmit`), only then does the state go Idle, through the same synthetic idle the `/compact` settle uses today. Newer main-turn evidence (another `UserPromptSubmit`, a `SelfStarted` turn, an assistant record, a Stop) settles it at once and cancels that Idle. The window is the standard 10 s delivery timeout; with no confirming record by then the queue pauses as for any undelivered item, never resending; a record arriving later confirms it through `late_confirm`, matched by name and args. Output records that follow a confirmation attach to that delivery's notice.
5. **Records, `transcript/commands.rs`** (new): `pub(super) fn command_record(record: &serde_json::Value) -> Option<CommandRecord>` reads the four shapes of `probe-notes.md` "Record shapes per class" (the prompt-type user record with `origin.kind: "human"`, args element left out when empty; the local user-record shape with 12-space indentation and the args element always present; the `system` `local_command` shape with `commandRun` on its stdout record; the fork skill's plain `/n` record and its `system` record holding `Running in the background as @n` and `<forked-skill-launch>`; the `informational` `Unknown command: /x` record), trimming each element. `records.rs` only calls it: make room there by moving `system()` with `is_api_error` and `describe` into `commands.rs` or a new `transcript/system.rs`, unchanged in behaviour. The `isMeta` markdown copy `/context` writes stays skipped.
6. **Notices.** `<local-command-stdout>` and `<local-command-stderr>`, in user or `system` `local_command` records, and `system` `informational` records become an `Item::Notice { kind: NoticeKind::Info, text }` under the command: ANSI escape sequences removed (CSI and OSC forms), other controls but newline and tab removed (`input::sanitize`), whitespace trimmed, capped at `ITEM_CHARS` (a `/context` table is longer than the 500-character notice cap).
7. **The chip.** `Item::User` gains `command: Option<CommandRef>`; the prompt-type record that confirms a delivery sets the delivery's user item's `command` (`replace_server_item`), so the web draws the chip. A record never creates a second user item for a delivery.
8. **Fork skills.** The `UserPromptSubmit` confirms the paste and sets busy; the `<forked-skill-launch>` record turns the delivery local (point 4), so it settles 2 s later and the state goes Idle. The fork's agent is already a background-table entry (background-server reads the same record), and its notification opens the next turn.
9. **No new spool events.** `SPOOL_EVENTS` (`spool.rs`) and `hook_cli.rs` are untouched.

## Tests (yours, in new files `bridge/tests_slash.rs`, `bridge/tests_slash_more.rs`, `transcript/tests_commands.rs`; each under 400 lines)

From the P2 fixtures (`transcripts/p2-*.jsonl`, `spool/p2-*.jsonl`; the background-server `fixture_replay.rs` helper replays them):

- `p2-skills`: the `UserPromptSubmit` confirms `/loom-plan:args alpha beta` once and its record gives the chip; `/probe-fork` confirms, settles to Idle after its `<forked-skill-launch>` record, and its notification opens the next turn;
- `p2-usage-context`: `/context` confirms on its `system` record and leaves a notice without ANSI escapes;
- `p2-compact`: `/compact` confirms on `PreCompact` before the 10 s timeout and settles from its boundary; rebuild `state_tests.rs` `a_delivered_compact_then_session_start_compact_is_idle` and `tests_queue.rs` `a_compact_boundary_with_no_stop_after_it_goes_idle` from it (no synthetic `UserPromptSubmit`);
- `p2-unlisted-lengths`: a name that left the list confirms on its `informational` record and leaves a notice; the 300-character line is pasted and confirmed;
- `p2-dialogs`: a paste that leaves no record times out at 10 s, pauses the queue and is never resent (lower `delivery_timeout` in the harness as `tests_background_prompt.rs` does).

Behaviour: a local command followed by a queued message (the message waits for the settle); output after the 10 s window attaches through the late confirm; a task notification during a settle cancels the Idle and opens the main turn; two identical `/context` in the queue confirm one each, in order.

Run `cargo test --lib plan_session::` once your code is ready.

## Done when

The three frozen contracts and your tests pass, `rg -n 'Item::User \{' src/plan_session` shows no construction without `command`, `records.rs`, `flow.rs`, `driver.rs`, `session.rs` and `bridge/mod.rs` are each at most 400 lines and `apply_event` at most 50.

## Traps

- The barrier is the point: a local command's output must never interleave with the next pasted message.
- The settle timer never closes a turn it did not start: any newer main-turn evidence cancels it.
- Args are compared trimmed; the local shapes indent with 12 spaces after each newline.
- `confirm_text` for a slash delivery must not match the typed line against the record body: the record holds the tags, not the typed text.
