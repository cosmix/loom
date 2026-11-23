# web-frontend / w2-sockets: terminal startup, dashboard liveness, planner socket, returned queue

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/web.md`, sections D2, D3, D5, CR-20; `doc/plans/briefs/review-remediation/validation/planner-backend.md`, sections P1, P2 (browser halves only).

All paths below are relative to `web/` unless they start with `doc/` or `loom/`. Run commands from `web/`.

## Findings covered

- D2 (browser half): the terminal client reports `live` and resets its retry budget when the socket opens, before the server attached the tmux session; a failed attach loops live/connecting forever.
- D3 (browser half): close code 4011 (terminal limit reached) must show the server's reason as a refusal, with no retry and never the cookie advice.
- D5: the dashboard socket reports `live` on open although no snapshot was applied.
- CR-20 a: the planner socket drops an invalid frame with only a `console.warn`.
- CR-20 c: the planner socket stays open after any REST call answered 401.
- P1 and P2 (browser half): `queue_returned` frames carry an optional `cause` (`"pressure"` or `"recovery"`); the browser must accept such a frame without a registered interrupt intent and refill the composer.

Server side is already merged by the two stages this one depends on. Wire shapes you build against:
- Terminal socket: after attach and before any output the server sends one text frame `{"ready":true}`. A failed attach closes with code 4010 and a reason. A full pool closes with code 4011 and the reason `terminal limit reached`.
- Planner chat: `{"frame":"queue_returned","texts":[...],"cause":"pressure"|"recovery"}`; `cause` is absent for a frame the server sends after Stop, End or set-aside.
- Before editing the fixture (item 6): confirm in `loom/src/plan_server/wire/chat.rs` that `ChatFrame::QueueReturned` has `cause: Option<QueueReturnCause>` with variants serialised as `pressure` and `recovery` and `skip_serializing_if = "Option::is_none"`. If it does not, stop and report; do not guess.

## Files

Own (write):
- `src/api/terminal.ts`, `src/api/terminal.test.ts`
- `src/api/ws.ts`, `src/api/ws.test.ts`, `src/api/ws-watchdog.test.ts`, `src/api/ws-bootstrap.test.ts`
- `src/plan/api/socket.ts`, `src/plan/api/socket-recovery.test.ts` (new)
- `src/plan/api/schema-chat.ts`, `src/plan/api/fixtures/chat-frames.json`
- `src/plan/chat/model.ts`, `src/plan/chat/model-commands.test.ts`

Read only: `src/components/terminal/terminal-notice.tsx`, `src/components/terminal/use-terminal.ts`, `src/plan/chat/stop-control.tsx`, `loom/src/plan_server/wire/chat.rs`, `loom/src/plan_server/wire/tests_chat.rs`.
Frozen (contract tests, never edit): `src/api/terminal.contract.test.ts`, `src/api/ws.contract.test.ts`, `src/plan/api/socket.contract.test.ts`, `src/plan/chat/queue-returned.contract.test.ts`.

## Changes

### 1. D2: `src/api/terminal.ts`, ready frame and startup deadline

Today `handleOpen` sets `attempts = 0`, `openedOnce = true` and emits `live`. `handleMessage` only forwards `ArrayBuffer` data.

- Add `export const STARTUP_TIMEOUT_MS = 10000;` beside `RETRY_DROPPED_MS`, and a private field `startupTimer: ReturnType<typeof setTimeout> | undefined` with a private `cancelStartup()` that mirrors `cancelRetry()` (use the injected `this.schedule` and `this.cancel`).
- `handleOpen(socket)`: keep the `this.socket !== socket` guard and `this.openedOnce = true` (it now means "the handshake completed", which is what separates a refusal before any open from a drop); remove `this.attempts = 0` and the `live` emit; the state stays `connecting` (already emitted by `openSocket`). Start the deadline: `this.startupTimer = this.schedule(() => this.startupExpired(socket), STARTUP_TIMEOUT_MS)`.
- `startupExpired(socket)`: clear `this.startupTimer`; return when `this.socket !== socket || this.stopped`; set `this.socket = undefined`, call `socket.close()`, then run the generic drop path with the reason `"terminal did not start"`.
- Extract the generic branch of `handleClose` (the `!openedOnce` / `attempts === 0` / else chain) into a private `dropped(reason: string)` used by both `handleClose` and `startupExpired`. Its behaviour is unchanged: never opened -> `refused` with `"handshake refused"`; first failure -> `attempts = 1`, `connecting`, retry after `RETRY_DROPPED_MS`; second failure -> `dropped` with the reason. Because `attempts` now resets only at ready, this is bounded.
- `handleMessage(socket, data)`: keep the `this.socket !== socket` guard. An `ArrayBuffer` goes to the sink as today (binary before ready is still sunk). A `string` is parsed with `JSON.parse` inside `try`; only a parsed object whose `ready === true` acts, anything else (invalid JSON, other text) is ignored. On ready: if `this.state?.phase === "live"` do nothing; otherwise `this.attempts = 0`, `this.openedOnce = true`, `this.cancelStartup()`, `this.emit("live")`.
- Call `this.cancelStartup()` in `handleClose` right after its `this.socket !== socket || this.stopped` guard (so a stale socket's close cannot cancel the live one's deadline), and in `retry()`, `close()` and `openSocket()`.
- 4010 needs no mapping: it falls through `handleKnownClose` to the generic path and the server's reason is shown.

### 2. D3: `src/api/terminal.ts`, `handleKnownClose`

Add `4011` to the refused branch: `event.code === 4004 || event.code === 4008 || event.code === 4011` -> `this.emit("refused", event.reason)`, no retry. No change in `terminal-notice.tsx`: `refusedText` returns the reason unless it equals `"handshake refused"`, so the capacity reason is shown as it is.

### 3. D5: `src/api/ws.ts`

- `socket.onopen`: keep `this.opened = true`, `this.attempts = 0`, `this.lastMessageAt = this.now()`; remove `this.setConnection("live")`. The phase stays what it was (`connecting`, or `reconnecting`/`offline` during an outage). The existing code after `applyFrame` in `receiveMessage` (`if phase !== "live" -> setConnection("live")`) now reaches `live` at the first applied `full`; leave it.
- Add field `private unprimedSince = 0;` and a private `unprime()` that sets `this.primed = false; this.unprimedSince = this.now();`. Use it in `openSocket` (replacing `this.primed = false`) and in the two failure branches of `receiveMessage` (invalid JSON, rejected frame). The `primed = true` assignment for a `full` stays.
- `checkLiveness`: after the `if (!this.opened) { ... return; }` block and before the phase-based check, add: `if (!this.primed && this.now() - this.unprimedSince >= STALE_AFTER_MS) { this.reconnect("no snapshot for 30s"); return; }`. This fires independent of `lastMessageAt`, so ticks that are dropped before a snapshot do not keep a snapshotless socket alive. Update the comment above the constants to say so in one sentence.

### 4. CR-20 a and c: `src/plan/api/socket.ts`

- (a) Add a private `resync(): void` to `PlanSocketConnection` that does what `socket.onerror` does: `const current = this.socket; this.socket = undefined; current?.close(); this.retry();`. The order matters: clearing `this.socket` before `close()` makes the handlers of the retired socket ignore the close event. Make the `onerror` handler use it too (`if (this.stopped || this.socket !== current) return; this.resync();`). In `receive`, replace both `return` statements after the two `console.warn` calls (JSON parse failure and `safeParse` failure) with `this.resync(); return;`. Keep the exact warning text `plan socket: invalid frame`: `scripts/bundle-rules.json` uses it as a sentinel that must occur in a chunk. A reconnect that receives a valid `hello` resets `attempts`, so this is bounded; `retry()` shows `PlannerConnectionDialog` meanwhile, which is the accepted trade-off (no error atom).
- (c) In `start()`, subscribe: `this.stopAuth = this.store.sub(unauthorizedAtom, () => { if (this.store.get(unauthorizedAtom)) this.unauthorized(); });` with a new private field `stopAuth: (() => void) | undefined` and a private `unauthorized()` that: sets `this.stopped = true`; cancels `this.timer` if set (`this.cancel(this.timer)`, then `this.timer = undefined`); `const current = this.socket; this.socket = undefined; current?.close();` and `this.setPhase("unauthorized")`. `dispose()` calls `this.stopAuth?.()` first. Nothing reconnects after a 401: the shell replaces the UI with the sign-in notice, and the user reopens the tokenized URL.
- Do not touch `probe()`: its own 401 branch sets the atom, which now also runs `unauthorized()`; the end state is the same (`unauthorized`, no timer, no socket).
- The review says `this.url` is assigned twice; the tree assigns it once. Nothing to remove.

### 5. P1 and P2 browser half: `schema-chat.ts` and `model.ts`

- `src/plan/api/schema-chat.ts`: in `queueReturnedFrameSchema` (a `z.strictObject`) add `cause: z.enum(["pressure", "recovery"]).optional()`.
- `src/plan/chat/model.ts`, case `"queue_returned"` of `applyChatFrame`: when `frame.cause !== undefined`, skip the intent check entirely (neither require nor consume an intent) and `store.set(queueReturnedAtom, frame.texts)`. When `cause` is undefined keep today's rule (break unless `pendingInterrupts` for the store is non-empty; consume one intent; set the atom). `useQueueRefill` in `stop-control.tsx` already moves the atom's texts into the draft; do not change it. `model.ts` is 382 lines; keep the change under 8 lines.

### 6. Shared fixture: `src/plan/api/fixtures/chat-frames.json`

Append, after the existing last frame (`queue_returned` with one text), two frames: `{ "frame": "queue_returned", "texts": ["Held while the pressure round started."], "cause": "pressure" }` and `{ "frame": "queue_returned", "texts": ["Not delivered before the server restarted."], "cause": "recovery" }`. Do not reorder: `replay.test.tsx` reads frames by index (`[1]`, `[2]`). The only Rust reader is `loom/src/plan_server/wire/tests_chat.rs` (`chat_frames_fixture_matches_serde_output`, `chat_frames_fixture_deserializes`, `chat_frame_variants_in_fixtures`); it round-trips every frame through `ChatFrame`, which is why you confirmed the server type first. Run `rg -n "chat-frames.json" loom web/src` from the repository root to re-confirm there is no other reader. `schema-chat.test.ts` already parses every frame of the fixture with `chatFrameSchema` and expects it unchanged; the new frames are covered by it.

## Interfaces shared with other workers

- `STARTUP_TIMEOUT_MS` (number, 10000) exported from `src/api/terminal.ts`; the contract writer uses it.
- Contract-visible: `new TerminalConnection(stageId, mode, sink, onState, deps)` (unchanged); `connectStatusSocket(store, deps)` (unchanged); `connectPlanSocket(store, deps)` (unchanged); `applyChatFrame(store, frame)` where `frame` may carry `cause`; `chatFrameSchema` accepts `cause`.
- `w4-tooling` owns `scripts/bundle-rules.json`; the sentinel `plan socket: invalid frame` must survive in `socket.ts`.

## Tests

1. D2, in `src/api/terminal.test.ts` (harness: `FakeSocket.open()/closeFromServer()/receive()`, injected timers `timers`/`fireNextTimer()`, `connect()`):
   - Rewrite "live on open": after `socket.open()` the last state is `connecting`; after `socket.receive('{"ready":true}')` it is `live` with mode `control`.
   - New: open, `closeFromServer(1005, "")`, `fireNextTimer()` (the retry; the startup timer is cancelled by the close), open the second socket, `closeFromServer(1005, "")`: no state ever had `phase: "live"` and the last is `dropped`, no active timer left.
   - New: open and never send ready, fire the startup timer (it is the first non-cancelled timer; its delay is `STARTUP_TIMEOUT_MS`): the first socket is `closed`, the last state is `connecting` and a retry timer with delay `RETRY_DROPPED_MS` exists.
   - New: ready resets the budget: open, ready, `closeFromServer(1006)` (retry), open, `closeFromServer(1006)` -> `dropped`; versus ready, 1006, retry, open, ready, 1006 -> `connecting` again.
   - Add rows to "close code table": `{ code: 4010, phase: "connecting", reason: "server reason", retry: true }` and `{ code: 4011, phase: "refused", reason: "server reason", retry: false }`.
   - "close cancels a pending retry": `timers[0]` is now the startup deadline, not the retry; select the retry with `timers.find((timer) => timer.delay === RETRY_DROPPED_MS)` and keep the same assertions.
   - "writes binary server frames and ignores text frames" stays as it is; add a line sending `'{"ready":false}'` and `'{"other":1}'` and assert no `live` state.
2. D5: new tests go in `src/api/ws-watchdog.test.ts` (it injects `now`, `setInterval` and `tick(ms)`; `ws.test.ts` is already over 400 lines): open then assert `connecting`; `receive(full(fixture))` then `live`; open, `receive("not json")`, `tick(30000)` -> `reconnecting` with message `no snapshot for 30s` and the socket closed; open and a `tickMessage` before any full: phase stays `connecting`. Existing tests to adjust (assertions change because `live` now waits for the first applied full): `ws.test.ts` "opens the expected socket URL and goes live on open" (assert `connecting` after open, `live` after `receive(full(fixture))`); `ws-bootstrap.test.ts` "ignores a tick or state before the socket's first full" (the phase after `onopen` and the dropped frames is `connecting`; add `live` after the full); `ws-watchdog.test.ts` "reconnects after 30s of silence on a socket that never closes" and "checks on visibilitychange and stops watching once disposed" (add `socket.receive(full(fixture))` after `socket.open()` so they keep testing the `no data for 30s` path). Run all `ws*.test.ts` files; any other failing assertion must be of the same kind (a `live` expectation with no applied full): fix it the same way and list it in your report.
3. CR-20, new file `src/plan/api/socket-recovery.test.ts` (copy the small `FakeSocket`, `deps`, `beforeEach` fake timers and `fetch` stub from `socket-background.test.ts`): (a) after `open()` and `receive(hello)`, call `latest.onmessage?.({ data: "not json" })`: the socket is closed (`readyState` 3), `connectionAtom` phase is `reconnecting`, and after `advanceTimersByTimeAsync(1000)` a second socket exists; repeat with a schema-invalid frame (`{ type: "presence", entries: [], extra: true }`); spy `console.warn` and assert the text `plan socket: invalid frame`. (c) open, then `store.set(unauthorizedAtom, true)`: the socket is closed, phase `unauthorized`, and after `advanceTimersByTimeAsync(30000)` no new socket and no new `fetch`. Both fail today.
4. P1/P2, in `src/plan/chat/model-commands.test.ts` after "queue_returned ignored without a pending interrupt": a frame with `cause: "pressure"` and no intent sets `queueReturnedAtom`; same for `cause: "recovery"`; a frame with a cause does not consume a registered intent (register one, send the cause frame, then a causeless frame still sets the atom); the causeless frame without intent still does not. The cause tests fail today.

Contracts to turn green: `terminal-live-only-after-ready`, `terminal-capacity-reason-shown`, `dashboard-live-after-snapshot`, `planner-invalid-frame-reconnects`, `queue-returned-recovery-refills`.

## Proof

```text
node_modules/.bin/vitest run src/api src/plan/api src/plan/chat/model-commands.test.ts src/plan/chat/model-identity.test.ts src/plan/chat/commands.test.ts src/plan/chat/replay.test.tsx
node_modules/.bin/tsc -b --noEmit
node_modules/.bin/oxlint --deny-warnings
node_modules/.bin/oxfmt --check src/api src/plan/api src/plan/chat
```

Do not run `vite build`. Report that you saw each new test fail before and pass after.

## Traps

- Removing `attempts = 0` from `handleOpen` is the whole D2 fix for the loop; keep `openedOnce = true` there or a first-ever attach failure reads as "handshake refused" and shows the cookie advice.
- The startup timer is the first timer the fake `schedule` records after an open; tests that index `timers[0]` for the retry break (item 1 names one).
- A ready frame is a text frame; `handleMessage` must not run `JSON.parse` on an `ArrayBuffer`.
- `checkLiveness` in `ws.ts` runs only while `socket !== undefined`; the new unprimed check goes after the `opened` check, not before, or a pending handshake would report the wrong message.
- In `socket.ts` the `receive` failure path runs before `lastMessageAt` is updated; keep it that way. After `resync()` the retired socket's `onclose` fires synchronously in the test fake; it is ignored only because `this.socket` was cleared first.
- The `cause` frames must also parse through `chatMessageSchema` (the `type: "chat"` wire form): it extends `queueReturnedFrameSchema`, so the one schema edit covers both; verify with a quick parse in your test.
- `ws.test.ts`, `terminal.test.ts` and `socket.test.ts` are near or over 400 lines: add tests only where this brief says, and do not grow `ws.test.ts` beyond the one-line edit.
