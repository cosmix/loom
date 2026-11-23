# planner-durability / w1-chat-queue: queued chat input survives Pressure Start and a restart; atomic session record

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/planner-backend.md`, sections P1, P2, P3.

This is the long unit of the stage. Do the steps in the order given; each step compiles on its own.

## Findings covered
- P1: the queue a Pressure Start returns reaches the browser as an unsolicited `queue_returned` frame, which the browser drops.
- P2: a message accepted while the session is busy (or being pasted) lives only in memory; a server restart loses it.
- P3: `session.json` is truncated in place, and `record_step` clears the dirty flag before the write, so a failed write is never retried.

## Files
Own (write), paths under `loom/`:
- `src/plan_server/wire/chat.rs`
- `src/plan_server/pressure/driver/chat.rs`
- `src/plan_server/pressure/tests/authoring.rs`
- `src/plan_server/chat/dispatch.rs`, `src/plan_server/chat/mod.rs`
- `src/plan_server/chat/returned.rs` (new), `src/plan_server/chat/tests_returned.rs` (new)
- `src/plan_session/bridge/mod.rs`, `src/plan_session/bridge/commands.rs`, `src/plan_session/bridge/session.rs`
- `src/plan_session/bridge/recovery.rs`, `src/plan_session/bridge/registry.rs`
- `src/plan_session/bridge/pending.rs` (new), `src/plan_session/bridge/tests_pending.rs` (new)
- `src/plan_session/bridge/fake_host.rs`, `src/plan_session/bridge/tests_end.rs`

Read only: `src/fs/safe_replace.rs` (`replace_private`), `src/fs/safe_read.rs` (`read_bounded`, `is_not_found`), `src/fs/safe_fs/write.rs` (`safe_remove_in_workdir`), `src/plan_server/authoring.rs`, `src/plan_server/ws/hub.rs`, `src/plan_server/chat/tests.rs` (fixture), `src/plan_session/bridge/harness.rs`, `src/plan_server/wire/tests_chat.rs`, `web/src/plan/api/fixtures/chat-frames.json`.

Frozen (contract tests, never edit): `src/plan_server/pressure/tests/queue_cause_contract_tests.rs`, `src/plan_session/bridge/pending_contract_tests.rs`, `src/plan_server/chat/returned_contract_tests.rs`, `src/plan_session/bridge/record_contract_tests.rs`.

Never edit `web/src/plan/api/fixtures/chat-frames.json` (the browser stage owns it) and never edit `src/plan_server/wire/tests_chat.rs` (397 lines). Its parity test reads that fixture; the fixture's `queue_returned` entry has no `cause`, deserializes to `None` and serializes back without the field, so it stays green unchanged. Do not add a fixture-coverage test for the new enum: it would need fixture entries this stage may not add.

## Changes

### Step 1 (P1): the wire field
`src/plan_server/wire/chat.rs`, enum `ChatFrame` (line 15-20). Add, beside it:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueReturnCause { Pressure, Recovery }
```

and change the variant to `QueueReturned { texts: Vec<String>, #[serde(default, skip_serializing_if = "Option::is_none")] cause: Option<QueueReturnCause> }`. Doc comment on `cause`: absent means the texts answer the receiving tab's own interrupt, end or set-aside. The wire shape, pinned for the browser stage: `{"type":"chat","frame":"queue_returned","texts":[...],"cause":"pressure"}` or `"cause":"recovery"`; no other field.

Constructors: `plan_server/chat/dispatch.rs` `answer` (line 166) gets `cause: None`. `plan_server/pressure/driver/chat.rs` `return_queue` (line 61) gets `cause: Some(QueueReturnCause::Pressure)`. Add two tests to the `tests` module of `wire/chat.rs`: a frame with `cause: Some(Pressure)` round-trips as the JSON above; a frame JSON without `cause` parses to `cause: None` and serializes without the key.

### Step 2 (P3): atomic session record
- `plan_session/bridge/registry.rs` `write_record` (line 85-91): serialize as today, then `crate::fs::safe_replace::replace_private(dir.path(), Path::new(RECORD), &bytes).with_context(...)`, same context text. Remove the now unused imports (`AsRawFd`, `safe_locked_write_in_workdir`, `safe_open_dirfd`). Doc: "Replace `session.json` atomically (temp file and rename, mode 0600); a symlink at the path is replaced, never followed."
- `plan_session/bridge/recovery.rs` `record_step` (line 207-223): keep clearing `record_dirty` before the write (a change during the write sets it again); on `Err`, after the warning, lock, find the session and set `record_dirty = true`. The next driver tick retries.
- `plan_session/bridge/fake_host.rs` `fail_record` and its use in `spawn` (line 73-76, 122-127): a symlink no longer fails the write. Replace the swap with: remove `spec.dir.record()`, create a directory at that path and a file `kept` inside it (a rename onto a non-empty directory fails for any uid). Update the doc comment ("A spawn puts a non-empty directory at `session.json`: the record write completing it fails.").
- `plan_session/bridge/tests_end.rs` `a_failed_record_and_teardown_after_the_spawn_keep_the_only_handle` line 99: `s.dir.record().is_file()` becomes `s.dir.record().exists()`, message unchanged. Report this assertion change.

### Step 3 (P2, persistence): pending input next to the record
New file `plan_session/bridge/pending.rs`, declared `mod pending;` in `bridge/mod.rs`.
- `const PENDING: &str = "pending.json";` and `const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;` (21 messages of `MAX_DELIVERED_BYTES` with JSON escaping fit).
- `#[derive(Serialize, Deserialize)] struct PendingInput { texts: Vec<String> }`.
- `fn write_pending(dir: &SessionDir, texts: &[String]) -> anyhow::Result<()>`: `replace_private(dir.path(), Path::new(PENDING), &bytes)` with context `cannot write <dir>/pending.json`.
- `pub(super) fn read_pending(dir: &SessionDir) -> Vec<String>`: `read_bounded(dir.path(), Path::new(PENDING), MAX_PENDING_BYTES)`; not found (`crate::fs::safe_read::is_not_found`) gives empty; any other read or parse error gives a `tracing::warn!` and empty.
- `impl Session { pub(super) fn pending_texts(&self) -> Vec<String> }`: first `flow.delivering` when it is not `closing`, has `opening == None`, and is not a local command whose `local.confirmed_at` is set, as `parse_unwrapped(&d.text).0`; then every `flow.queue` item's `user_text`, in order. An opening (the start's first message) is not covered: its kickoff text is kept by the draft; say so in the doc comment.
- `plan_session/bridge/session.rs` `Flow` (line 200): add `pub(super) pending_saved: Vec<String>` with doc "What `pending.json` holds; empty when it is absent. A recovered session starts empty, so its file keeps the texts recovery read until the directory goes."
- `bridge/mod.rs` `Bridge` struct (line 247): add field `pending_io: Mutex<()>` (doc: "Orders `pending.json` writes; taken before the bridge lock, never inside it"), initialised in `Bridge::new`.
- Move `Bridge::send` out of `commands.rs` (line 140-151) into `pending.rs`, same signature `pub fn send(&self, id: &str, text: String, attachments: Vec<Attachment>) -> Result<SendOutcome, BridgeError>`. New body, in this order: sanitize and `check_delivered` as today; take `pending_io`; under the bridge lock find the session (`NoSession`), refuse with `NotRunning` unless `accepts_input()` (make that `fn` in `commands.rs` `pub(super)`), and copy its dir and `pending_texts()`; drop the lock; push `parse_unwrapped(&text).0` to the copy; `write_pending` it, on failure return `BridgeError::Io(format!("the message could not be saved: {error:#}"))` with nothing accepted; then `with_session(id, |session, step| { session.flow.pending_saved = prospective; session.send(text, attachments, step) })`; `flush()`; call `save_pending_held(id)`; return the send result (`NoSession` when `with_session` gave `None`).
- `fn save_pending_held(&self, id: &str)` (caller holds `pending_io`): under the lock, skip unless the session's phase is `Spawning`, `Launching` or `Live`; compare `pending_texts()` with `flow.pending_saved`; when they differ copy the dir and texts, drop the lock, write; on success lock again and set `pending_saved` to what was written; on failure `tracing::warn!` and leave `pending_saved` (the next tick retries).
- `pub(super) fn save_pending(&self, id: &str)`: take `pending_io`, call `save_pending_held`.
- `recovery.rs` `record_step`: first statement `self.save_pending(id);` (before the dirty check). The driver calls `record_step` once per session per tick (`driver.rs:58`, do not edit `driver.rs`, it is 397 lines), so withdrawals, interrupts, ends and confirmed deliveries reach the file within one tick. Until that tick the file is a superset; a superset only returns a text the user already got back or withdrew, never loses one.

### Step 4 (P2, recovery event)
- Move `RecoveredSession` and `RecoveryReport` (`bridge/mod.rs` line 223-237) into `recovery.rs` unchanged and re-export them from `mod.rs` with `pub use self::recovery::{RecoveredSession, RecoveryReport};`. Every outside path stays `crate::plan_session::bridge::RecoveryReport`. This keeps `mod.rs` under 390 lines.
- `BridgeEvent` (`mod.rs` line 140): add variant `InputReturned { id: String, texts: Vec<String> }`, doc "Recovery found input the last server accepted and never saw confirmed: return it to the holder, never replay it."
- `recovery.rs` `clean`: after `terminate_children` and before `remove_session_dir`, `let texts = read_pending(dir);`; only when the removal succeeded and `texts` is non-empty, push `Out::Event(BridgeEvent::InputReturned { id, texts })` before the `Ended` event. `resume`: read at the start, push the event into the outbox together with the step's output. The `LeftAlone` branches read nothing: their file stays for a later start. `recover` already flushes the outbox before `wait_ended` (line 56), so the event is heard before the final writes finish.

### Step 5 (P2, delivery to the holder)
New file `plan_server/chat/returned.rs`, declared `mod returned;` in `chat/mod.rs`. The authoring holder is memory only and a restart frees it, so the server records which browser held authoring and returns the texts to that browser's tabs.
- Runtime files under `ctx.runtime().dir`, written with `replace_private`, read with `read_bounded` (8 MiB), removed with `safe_remove_in_workdir` (not found is fine): `chat-holder.json` = `{"holder":"<tag>"}`, `returned-input.json` = `{"entries":[{"holder":"<tag>" or null,"texts":[...]}]}`. `<tag>` is `crate::utils::sha256_hex(browser.as_str().as_bytes())`: the cookie value itself never reaches disk.
- `#[derive(Default)] pub(super) struct Returned { prior: Option<String>, browser: Option<BrowserId>, entries: Vec<Entry> }`; `ChatState` gains `returned: Mutex<Returned>`.
- `pub(super) fn load(ctx: &PlanContext) -> Returned`: `prior` from `chat-holder.json`, `entries` from `returned-input.json`; unreadable gives a warning and empty. `install_with` stores it into `state.returned` before `publish::subscribe` (before the hub hooks and recovery exist).
- `pub(super) fn follow_holder(ctx: &Arc<PlanContext>)`, called from `install_with`: an authoring subscriber; on `AuthoringEvent::Taken` write `chat-holder.json` with the tag of `ctx.authoring().holder_browser()`; on `Freed` remove it. Failures are warnings.
- `pub(super) fn on_returned(ctx: &PlanContext, state: &ChatState, texts: Vec<String>)`, called from `follow_sessions` on `BridgeEvent::InputReturned { texts, .. }`: lock `state.returned`, push `Entry { holder: prior.clone(), texts }`, persist, then deliver live: when `browser` is set and `ctx.hub().connections_of(browser) > 0`, take the entries whose holder equals `prior`, persist the rest, `send_to_browser` one frame.
- `pub(super) fn on_open(ctx: &PlanContext, state: &ChatState, browser: &BrowserId) -> Option<PlanMessage>`, called at the end of the `open_snapshots` hook (after the snapshot and the draft frames): when the browser's tag equals `prior`, remember `browser`; take the entries whose holder is that tag, or `None`; persist the rest (remove the file when none remain); return one frame with all their texts in order.
- The frame is `PlanMessage::Chat(Box::new(ChatFrame::QueueReturned { texts, cause: Some(QueueReturnCause::Recovery) }))`. Persisting before sending means a write failure can deliver twice, never lose. Lock order: `state.returned`, then the hub (the hub calls no hook while holding its lock).
- Entry with holder `None` (no `chat-holder.json` was readable): goes to the first tab that opens. Comment it as the accepted limit.

## Interfaces shared with other workers
`crate::plan_session::bridge::RecoveredSession` and `RecoveryReport` keep their path and fields (w2 reads `report.torn_down`, `report.closed`, `children`). Nothing else is shared.

## Tests
- Step 1, `plan_server/pressure/tests/authoring.rs` `the_open_chat_ends_first_and_its_queue_returns_to_the_holder`: add `assert!(frames.iter().all(|f| f["cause"] == "pressure"))` over the `queue_returned` frames (helper `returned`, line 69). Fails today (no field).
- Step 2, `tests_pending.rs` (declare `#[cfg(test)] mod tests_pending;` in `mod.rs`): `a_symlinked_record_is_replaced_with_the_newest_generation`: `Harness::ready`, `h.idle(Authoring)`, swap `session.json` for a symlink to a copy, `h.bridge.set_owner(&s.id, Some("alpha".into()), None)`, `wait_until` `read_record(&s.dir)` has feature `alpha` and the path is a regular file. `a_failed_record_write_keeps_the_previous_record`: skip when `libc::geteuid() == 0` (pattern `tokens/tracker_tests.rs:363`); write a record, chmod the dir 0500, `write_record` with another feature returns `Err`, `read_record` returns the first; restore 0700.
- Step 3, same file: `a_busy_send_is_saved_before_it_is_acknowledged` (idle, `h.working(&s)`, send, then `pending.json` holds `["text"]`); `a_send_that_cannot_be_saved_is_refused` (create a non-empty directory at `pending.json`, send while working gives `BridgeError::Io`, `h.bridge.chat_snapshot(true).queue` is empty); `a_withdrawn_message_leaves_the_file` (queue, `withdraw`, `wait_until` the file holds `[]`).
- Step 4, same file: `a_pasted_but_unconfirmed_message_is_returned_too`: idle, send, wait until `h.host.delivered(&s.socket)` holds it, build a second bridge as in contract `queued-send-kept-in-memory-only`, recover on a thread, wait for `InputReturned` with the text.
- Step 5, `chat/tests_returned.rs` (declare in `chat/mod.rs` under `#[cfg(test)]`), fixture `chat/tests.rs` (`Fixture::new`, `connect`, `browser`, `wait_until`): set `state.returned` to a `Returned` whose `prior` is the holder's tag, then: the holder's new tab gets the frame in `opened`, another browser's tab gets none, the file is gone after delivery; an already open holder tab gets it through `on_returned` live.
- Contracts to turn green: `pressure-queue-without-cause`, `queued-send-kept-in-memory-only`, `recovered-queue-replayed-or-dropped`, `record-dirty-flag-cleared-before-write`.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib plan_session::bridge::`; `cargo test --lib plan_server::chat::`; `cargo test --lib plan_server::wire::`; `cargo test --lib plan_server::pressure::tests::authoring`. Confirm `wc -l src/plan_session/bridge/mod.rs src/plan_session/bridge/commands.rs src/plan_session/bridge/session.rs` stay under 395 after `cargo fmt`.

## Traps
- Replaying the recovered texts into the closing session, or keeping the old `safe_locked_write_in_workdir` writer with only the dirty retry: both fail a contract.
- Taking `pending_io` while holding the bridge lock deadlocks against `send`.
- Reading `pending.json` after `remove_session_dir` in `clean`: the file is gone.
- Writing the pending file from `Session::send` under the lock: the module rule is no filesystem under the lock.
- Storing the raw `BrowserId` in a runtime file: store the tag.
- `cargo fmt` lengthens files: `commands.rs` (395 now) only shrinks in this unit; `driver.rs` (397) is not touched.
