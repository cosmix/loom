# Web frontend findings: validation against revision 62be2a3d2

Scope: Codex `U1 U2 U3 U6 D2 D3 D5`; AGY `CR-13 CR-20 CR-62 CR-63 CR-64 CR-65 CR-78 (fourth bullet)`.
All paths repo-relative. Web paths are under `web/`. No repository file was changed.

Order below follows the brief. Ordering advice for the plan: U1 + U2 + CR-65 (save flow, one worker, they share `edit-session.ts`, `save-flow.ts`, `edit-flow.ts`, `edit-mode.test.tsx`); U3 separately (draft storage design); D2 + D3 (one worker, one wire change, Rust + TS); the rest are independent.

---

## U1 — Save and close can navigate away from a newer unsaved edit

**Verdict:** CONFIRMED
**Evidence:**
- `web/src/plan/editor/save-flow.ts:54-55` (`save`) records `submittedText: state.text`; `:56-57` accepts a result while `saving`; `:90-101` (`saved`) sets `status: "saved"`, `savedText = submittedText`, keeps `text` as typed since, `clearDrafts: state.text === savedText` (false when newer text exists). The reducer is correct; the editor stays writable while saving (`edit-body.tsx:159-176` passes `readOnly={!session.canAuthor}` only).
- The close decision is in `web/src/plan/editor/edit-session.ts:23-27`, not `edit-body.tsx:174` (that citation drifted; `edit-session.ts:19` is the leave-guard line). It tests `state.status === "saved"` alone, then `setLeaveTo(...)`; `useEffect` at `:20-22` navigates.
- No guard defeats it: `useLeaveGuard(dirty && leaveTo === null)` (`edit-session.ts:19`, `leave-guard.ts:4-23`) is disabled the moment `leaveTo` is set, so neither the router blocker nor `beforeunload` fires. Disk holds the submitted text. The local draft written for the newer text is keyed by the pre-save `openedHash` (`edit-flow.ts:147`), so on reopen `startState` (`edit-flow.ts:35-39`) offers it as an older-hash merge, not silently lost, but the editor text is dropped from the live session.
**Fix seam:** `web/src/plan/editor/edit-session.ts:23-27` (the `closing` effect in `useLeaving`). Change the `saved` branch to leave only when nothing is newer than what was saved: `if (state.status === "saved" && state.text === state.savedText) setLeaveTo(...)`, otherwise `setClosing(false)` (stay in the editor, `dirty` stays true so the Done/leave guards keep working). Add `state.text` and `state.savedText` to the effect deps. No reducer change: do not reuse `dirty` from `useEditSession` (it is computed after this hook; pass the two strings from `flow.state`). Callers affected: `useEditSession` consumers only (`edit-mode.tsx`, `edit-body.tsx` via `session.saveAndClose`). Decision settled: after a saved-with-newer-text result the user stays in the editor with the newer text; they press Done again (which re-asks) or Save. No auto re-save.
**Regression test:** extend `web/src/plan/editor/edit-mode.test.tsx` (harness: `renderEdit()` `:92`, `source()` `:106`, hoisted `fake` editor whose textarea `input` event drives `onChange`; the Ctrl-S test at `:~245` shows `vi.stubGlobal("fetch", ...)`). Test: stub `fetch` with a manually resolved promise; input "first edit"; press Escape or Done, click "Save and close"; while pending `fireEvent.input(editor, "second edit")`; resolve with `{result:"saved", hash:"sha256:x", validation:null}`; assert `router.state.location.search` is still `?edit=1` and the textarea still holds "second edit". Today the router leaves the edit page. Optionally assert a second Save posts "second edit".
**Files a fix touches:** `web/src/plan/editor/edit-session.ts`, `web/src/plan/editor/edit-mode.test.tsx`.
**Size / tier:** S; mechanical (data-loss class, but the fix is two conditions).
**Overlap:** U2 (both are change-during-save), CR-65 second half (same `useLeaving` close path and the same test file), U3 (clearDrafts timing).

Relation of U1 and CR-65 (second half): both are about "Save and close". U1 is about whether it may leave at all (newer text exists); CR-65 is about what the reading view shows after it does leave (stale pre-edit text). Independent fixes in different functions (`edit-session.ts` effect vs `edit-flow.ts` save callback); neither makes the other unnecessary. Land them in the same worker so one test file and one close path are edited once.

---

## U2 — Errors shown before a forced save can belong to different text

**Verdict:** CONFIRMED
**Evidence:**
- `web/src/plan/editor/save-flow.ts:44-52` (`change`): only `saved` and `invalid` fall back to `idle`; `saving` stays `saving`, so typing during a request keeps the status.
- `:113-114` (`invalid`) sets `status: "invalid"` with the verdict for `submittedText`, never comparing it to the now-current `state.text`.
- `web/src/plan/editor/edit-body.tsx:243` shows "Save with errors" whenever `status === "invalid"`; it calls `flow.save(true)` (`edit-flow.ts:87-96`), which sends `latest.current.text` with `with_errors=1`.
- Server half `loom/src/plan_server/files/save.rs:90-94`: validation runs on the posted text, and `!req.force` is the only thing that turns an invalid verdict into a refusal, so the forced write goes through. The server is correct; the defect is client-only.
**Fix seam:** `web/src/plan/editor/save-flow.ts:113-114` (`saveResult`, case `invalid`). If `state.submittedText !== null && state.text !== state.submittedText`, return `{ ...state, status: "idle", validation: null, submittedText: null }` (discard the verdict; the panel then falls back to the watched validation through `useEditValidation`, `edit-body.tsx:~112`); otherwise unchanged. Do not touch `edit-flow.ts` or the server. For symmetry the `saved` case also carries `validation` for the submitted text; leave it (informational, cannot force anything).
**Regression test:** `web/src/plan/editor/save-flow.test.ts`, in `describe("saveFlow edits after a verdict")` (`:135`; `saveResultSchema.parse(invalidFixture)` already built there). Add: `change` -> `save` -> `change` (newer) -> `result` invalid; expect `status` `"idle"` and `validation` null (fails today: `"invalid"`). Add the control: unchanged text still ends `"invalid"`. Component-level force flag: `web/src/plan/editor/editor.contracts.test.tsx` has `savedQuery(fetchMock, n).get("with_errors")` (`:107`); optional test that no "Save with errors" button renders after the stale verdict.
**Files a fix touches:** `web/src/plan/editor/save-flow.ts`, `web/src/plan/editor/save-flow.test.ts`.
**Size / tier:** S; mechanical.
**Overlap:** U1 (same file and same race), CR-65.

---

## U3 — Two editing tabs share one recovery copy

**Verdict:** CONFIRMED (design limitation, as the review labels it)
**Evidence:**
- `web/src/plan/editor/drafts.ts:20-22` keys drafts `loom-plan-draft:<project>:<path>:<openedHash>`, in `localStorage`, shared across tabs. Two tabs on the same base hash overwrite each other's recovery text in `saveDraft` (`:87-117`).
- `drafts.ts:73-85` (`clearDrafts`) removes every hash of the path. Callers: `edit-flow.ts:113` (discard) and `edit-flow.ts:139` (after a clean save). So a discard or save in tab A deletes tab B's draft.
- No guard: `loom/src/plan_server/authoring.rs:1-5` states every tab of the holding browser holds authoring, and the save hash check (`save.rs:84-92`) protects disk but not the draft. Tab B's live editor text survives; only the recovery copy is lost.
**Fix seam:** `web/src/plan/editor/drafts.ts` (key, `saveDraft`, `findDrafts`, `clearDrafts`), `web/src/plan/editor/draft-writer.ts:32` (the only `saveDraft` caller), `web/src/plan/editor/edit-flow.ts:35,113,139` (callers of `findDrafts`/`clearDrafts`). Open design choice:
- A (recommend): add a per-tab editor id (module constant `crypto.randomUUID()` created once per page load) to the key and to `StoredDraft`; `saveDraft` writes under it; `clearDrafts(path, editorId)` removes only the drafts written under the caller's id; `findDrafts` still returns all tabs' drafts for the path (newest first), so a reopened tab recovers any tab's text. A tab that closes leaves its draft as orphan recovery text, which is the point.
- B: keep the shared key, clear only the version consumed (`openedHash` of the action). Does not fix two tabs on the same base overwriting each other, so reject.
Cost of A: orphan drafts accumulate until the existing 2 MiB eviction (`MAX_TOTAL`, `saveDraft` loop) trims them. Keep that eviction; no extra policy.
**Regression test:** `web/src/plan/editor/drafts.test.ts` (existing `describe("draft storage")`, first test `:30` already uses a stubbed project/localStorage). Add: save from "tab 1" and "tab 2" ids on the same hash, assert both listed; clear as tab 1, assert tab 2's survives. Needs the id injectable (export a setter or pass it as an argument) because the module constant cannot differ in one test file. Component level: `edit-mode.test.tsx` second mount is not needed.
**Files a fix touches:** `web/src/plan/editor/drafts.ts`, `web/src/plan/editor/draft-writer.ts`, `web/src/plan/editor/edit-flow.ts`, `web/src/plan/editor/drafts.test.ts`, possibly `web/src/plan/editor/draft-writer.test.ts`.
**Size / tier:** M; judgment (storage contract, data loss).
**Overlap:** U1 (draft clearing on close), CR-65 (`edit-flow.ts`).

---

## U6 — Attachment trays lack named group semantics

**Verdict:** CONFIRMED
**Evidence:** `web/src/plan/chat/chips.tsx:285` (`<div aria-label="Pending attachments">`) and `:359` (`<div aria-label="Sent attachments" className="chat-chips">`): `aria-label` on a role-less `div` is not exposed. Existing tests query `getByLabelText`, which matches the attribute regardless of role: `web/src/plan/chat/chips-labels.test.tsx:75,105,137,185`, `chips.test.tsx:65`, `session-home.test.tsx:349-365`. The project already uses `role="group" aria-label=...` elsewhere (`web/src/components/stage-thread.tsx:108`, `web/src/plan/trace/findings-sheet.tsx:39`).
**Fix seam:** add `role="group"` to both divs (`chips.tsx:285`, `:359`). `getByLabelText` queries keep passing. No CSS keys off these elements being role-less. Home/End on the resize separator (`panel.tsx:140`) is an optional enhancement in the review; leave it out.
**Regression test:** `web/src/plan/chat/chips-labels.test.tsx` (tray queries at `:137` pending and `:185` sent already exist): add `screen.getByRole("group", { name: "Pending attachments" })` and the sent equivalent. Fails today (no group role).
**Files a fix touches:** `web/src/plan/chat/chips.tsx`, `web/src/plan/chat/chips-labels.test.tsx`.
**Size / tier:** S; mechanical.
**Overlap:** none.

---

## D2 — Terminal startup shows "live" and resets the retry budget before attach

**Verdict:** CONFIRMED (client and server halves both real)
**Evidence:**
- Client `web/src/api/terminal.ts:124-129` (`handleOpen`) sets `attempts = 0`, `openedOnce = true` and emits `live` on socket open.
- Server `loom/src/commands/status/web/terminal/upgrade.rs:62-66` accepts the upgrade first; then `start_bridge` (`:121-150`) resolves the target and spawns the PTY. Two startup failures end with `socket.close(None)`: resolution error (`:131-135`) and PTY spawn failure (`:144-147`). A close frame with no payload reaches the browser as code 1005. Typed endings exist only for `Ok(Err(refusal))` (`close_refusal`, `:178-185`: 4004/4008/4009 from `protocol.rs:93-97`).
- Client `handleClose` (`terminal.ts:136-149`): code 1005 matches no known code, `openedOnce` is true, so the `attempts === 0` branch runs: `connecting`, retry in 1 s. The next open resets `attempts` to 0 again, so the `else` branch (`dropped`) is never reached: live, connecting, live, connecting indefinitely. The existing test "the second 1006 lands in dropped" (`web/src/api/terminal.test.ts:187-199`) never re-opens the second socket, which is why it passes.
- Reachability: both server paths are real runtime failures (tmux missing or failing to start, tmux server gone between the resolve and the attach).
**Fix seam (protocol change, settle all four parts):**
1. Server `upgrade.rs:131-135` and `:144-147`: replace `close(None)` with a typed close frame, new constant `CLOSE_ATTACH_FAILED: u16 = 4010` next to the others in `loom/src/commands/status/web/terminal/protocol.rs:93-97`, reason strings `"could not resolve the stage session"` and `"could not start the tmux attach client"`. Reuse `close_refusal`'s frame-building shape (`upgrade.rs:178-185`); generalise it to take `(code, reason)` instead of `Refusal`, keeping `Refusal` callers.
2. Server ready frame: in `start_bridge`, right after `PtyChild::spawn` succeeds and before `bridge::run` (`upgrade.rs:149`), send one text frame `{"ready":true}`. Placing it in `start_bridge` (not in `bridge::run_with`) leaves the `tests_bridge/*` harness, which reads binary output frames first, untouched.
3. Client `terminal.ts`: `handleOpen` (`:124`) stops resetting `attempts`, stops emitting `live`; it stays in `connecting` and starts a startup deadline timer (use the injected `schedule`/`cancel`, new constant `STARTUP_TIMEOUT_MS = 10000`; on expiry close the socket and fall into the existing generic drop path). `handleMessage` (`:131`) parses string data: on `{"ready":true}` set `attempts = 0`, `openedOnce = true`, cancel the deadline, emit `live`. Binary frames before ready are still sunk.
4. No client mapping is needed for 4010: unmapped codes already go through `handleClose`'s generic path (one 1 s retry, then `dropped` with the server's reason), which is now bounded because `attempts` resets only at ready.
Consumers: `web/src/components/terminal/use-terminal.ts:128` (fits and resizes on `live`, now at ready, correct), `web/src/components/terminal/terminal-notice.tsx` (shows the `dropped` stamp; the reason text is not rendered by `Stamp`, acceptable). `send`/`resize` use `socketIsOpen()`, so input before ready is buffered by TCP and read after attach; acceptable.
**Regression test:**
- TS: `web/src/api/terminal.test.ts` (harness: `FakeSocket.open()/closeFromServer()/receive()`, injected timers `timers`/`fireNextTimer()`, `connect()`). Add: open then `closeFromServer(1005, "")` repeated three times through retry timers; assert no `live` was ever emitted and the final state is `dropped`. Add: `receive('{"ready":true}')` after open emits `live` and resets `attempts`; no ready within the deadline closes and retries. Update the existing "live on open" test (`:118`) to open then send ready, and "writes binary server frames and ignores text frames" (`:292`) to keep text frames other than ready ignored.
- Rust: `loom/src/commands/status/web/terminal/tests_route.rs:141-155` pins literal wire values; add `CLOSE_ATTACH_FAILED == 4010`. A full upgrade-then-startup-failure test needs a tmux-less environment for the spawn failure: `tests_upgrade/admission.rs:94` (`terminal_unknown_stage_closes_4004`, `connect_terminal` helper) shows the harness; the resolution `Err` path is not reachable there without tmux, so cover the new frame via a small extracted helper unit test.
**Files a fix touches:** `web/src/api/terminal.ts`, `web/src/api/terminal.test.ts`, `loom/src/commands/status/web/terminal/upgrade.rs`, `loom/src/commands/status/web/terminal/protocol.rs`, `loom/src/commands/status/web/terminal/mod.rs` (the `use protocol::{...}` list at `:21`), `loom/src/commands/status/web/terminal/tests_route.rs`; plus `web/dist/` rebuild (embedded assets).
**Size / tier:** M; judgment (wire protocol across Rust and TS).
**Overlap:** D3 (same file `upgrade.rs`, same client switch), D1 (same `upgrade.rs`/`bridge.rs` area, not in this group).

---

## D3 — Exhausted terminal capacity tells an authenticated user to replace their cookie

**Verdict:** CONFIRMED (the cited line drifted: the 503 is `upgrade.rs:105-112`, line 96 is the origin check)
**Evidence:**
- Server `upgrade.rs:85-113`: host, lane, origin and cookie are checked first (401 on cookie mismatch at `:97-100`), then `acquire_terminal_slot` failure answers a plain HTTP 503 "terminal limit reached" at `:105-112`; a stopping server answers 503 at `:114-116`. This is before the WebSocket handshake completes.
- Browsers do not expose an HTTP status or body for a failed WebSocket handshake; the client sees `onclose` with code 1006. `web/src/api/terminal.ts:136-141` maps "never opened" to `refused` with the fixed reason `"handshake refused"`. `web/src/components/terminal/terminal-notice.tsx:44-50` (`refusedText`) renders that reason as "Dashboard cookie missing - open the tokenized URL..." (terminals on) or "Start the dashboard with loom status --web --terminals". A valid cookie plus a full pool therefore gets the wrong advice.
- Server test pinning current behaviour: `loom/src/commands/status/web/terminal/tests_upgrade/admission.rs:141-163` (`terminal_slots_are_capped`) asserts the HTTP 503 body.
**Fix seam (open design choice):**
- A (recommend): complete the handshake for authenticated clients and answer with a typed close. In `upgrade.rs` `admit` (`:70-119`) do not fail the stream for the slot and stopping cases: return an admission outcome that `handle_upgrade` (`:44-67`) turns into `tungstenite::accept_with_config` then `close_refusal`-style frame. New code `CLOSE_BUSY: u16 = 4011` ("terminal limit reached") in `protocol.rs`; the stopping case maps to the existing `CLOSE_SERVER_STOPPING` 1001 which the client already shows as "server stopping". Client `terminal.ts` `handleKnownClose` (`:151-164`): add `4011` to the `refused` branch with the server reason (`event.code === 4004 || 4008 || 4011`); `refusedText` falls through to `reason ?? "refused"`, so no UI change. Authentication still precedes the slot check, so capacity is never revealed to an unauthenticated peer. Cost: one accepted handshake per over-limit attempt from authenticated clients; the general connection limits bound it.
- B (cheap, weaker): change `refusedText` to "Terminal unavailable: dashboard cookie missing, or terminal capacity reached". Accurate but still unhelpful; leaves no retryable signal.
Recommend A; it is the review's "observable result". Keep the 401 and 403 paths as HTTP failures (nothing to tell the user beyond the existing text).
**Regression test:**
- Rust: rewrite `terminal_slots_are_capped` (`tests_upgrade/admission.rs:141`) to use `connect_terminal(port, &token).read()` and expect `Message::Close(Some(frame))` with `CloseCode::Library(4011)` while `MAX_TERMINALS` slots are held (the `acquire_terminal_slot(&limits)` loop is already there), then success after `drop(held)`. Add an unauthenticated-with-capacity-free case remains the existing 401 tests (`:60-91`).
- TS: `web/src/api/terminal.test.ts` close code table (`:145-172`): add row `{ code: 4011, phase: "refused", reason: "server reason", retry: false }`.
**Files a fix touches:** `loom/src/commands/status/web/terminal/upgrade.rs`, `loom/src/commands/status/web/terminal/protocol.rs`, `loom/src/commands/status/web/terminal/mod.rs`, `loom/src/commands/status/web/terminal/tests_upgrade/admission.rs`, `loom/src/commands/status/web/terminal/tests_route.rs`, `web/src/api/terminal.ts`, `web/src/api/terminal.test.ts`; `web/dist/` rebuild.
**Size / tier:** M; judgment (protocol and admission ordering).
**Overlap:** D2 (same `upgrade.rs` functions and `terminal.ts` close switch; do both in one worker), D1.

---

## D5 — Dashboard liveness can precede a usable snapshot

**Verdict:** NARROWED
**Evidence:**
- `web/src/api/ws.ts:138-144` (`onopen`) sets `opened`, `attempts = 0`, `lastMessageAt` and phase `live` before any frame. `:146-150` (`onmessage`) refreshes `lastMessageAt` before parsing. `:181-188`: a delta or tick before the first `full` is dropped (`return`) without changing the phase. `:171-179`: a rejected frame sets `primed = false` and phase `error`; `checkLiveness` (`:93-103`) only reconnects when no message at all arrived for 30 s, and ticks keep `lastMessageAt` fresh. So both described states are real: `live` with no applied snapshot, and `error` with unusable traffic suppressing the watchdog.
- Narrowing guard: the server bounds both. `loom/src/commands/status/web/feed.rs:37` sets `FULL_INTERVAL` to 30 s and `:64-80` (`accept`) sends a `full` at least that often (and on any key-set change); a new subscriber is primed with a `full` when a snapshot exists (`broadcast.rs:~80`, `subscribe`). So with a healthy server the window is at most about 30 s, not "indefinite". It persists indefinitely only if every `full` also fails to parse (schema drift), where a reconnect would not help either (same primer). Severity Low, as the review says.
**Fix seam:** `web/src/api/ws.ts`. (1) `onopen` (`:138-144`): do not emit `live`; keep `connecting` and record `unprimedSince = this.now()`. (2) `receiveMessage` (`:153-196`): the existing `if (this.store.get(connectionAtom).phase !== "live") this.setConnection("live")` already runs after `applyFrame`, so `live` is now reached at the first applied `full`. (3) On `primed = false` assignments (`:157`, `:168`) set `unprimedSince = this.now()`; on `primed = true` (`:181`) clear it. (4) `checkLiveness` (`:93-103`): if `opened && !primed && this.now() - unprimedSince >= STALE_AFTER_MS` call `this.reconnect("no snapshot for 30s")`, independent of `lastMessageAt`. Reuse `STALE_AFTER_MS`, `reconnect`. No server change. Check consumers that expect `live` right after open: only tests, `web/src/api/ws.test.ts:99` ("goes live on open").
**Regression test:** `web/src/api/ws.test.ts` (`FakeSocket`, `full()`, `dependencies()` at `:76-90`; it injects only `setTimeout`, so add `now` and `setInterval` fakes to `dependencies()` or use the `visibilitychange` path that calls `checkLiveness`). Add: open, assert phase `connecting`; receive `full` and assert `live`; second test: open, rejected frame, then ticks for 31 s of injected time, then `visibilitychange`, assert the socket reconnected. Update `:99`.
**Files a fix touches:** `web/src/api/ws.ts`, `web/src/api/ws.test.ts`; `web/dist/` rebuild.
**Size / tier:** S-M (about 30 changed lines); needs-exploration (test clock plumbing).
**Overlap:** CR-20 (the planner socket has the same missing-watchdog shape but a different class and file).

---

## CR-13 — `bundle-check.mjs` fails on committed `web/dist/` (stale sentinel)

**Verdict:** CONFIRMED
**Evidence:** `cd web && node scripts/bundle-check.mjs` prints `plan.html: sentinel This feature has no metaplan, so no Plan follow-ups can start. occurs in no chunk` and exits 1. `web/scripts/bundle-rules.json:50` holds that literal as the last `forbidText` of `plan.html`; `staleSentinels` (`bundle-check.mjs:~70-85`) rejects a sentinel found in no chunk. `rg` finds the literal nowhere in `web/src` (the surviving text is `This feature has no metaplan yet.`, `web/src/plan/feature/document-page.tsx:125`, present in `web/dist/assets/plan.js`), so `dist/` is in sync with `src/` for this string.
**Fix seam:** replace the string at `web/scripts/bundle-rules.json:50`. `web/src/plan/run/handoff-dialogs.tsx` itself contains no user-visible literal (it mounts `PressureDialog` and `RunDialog`, lines 8-14); the chunk `web/dist/assets/handoff-dialogs.js` holds both dialogs' text. Use `Commit the changes under doc/ first` (`web/src/plan/run/run-dialog.tsx:128`, unique in `web/src` non-test code, present once in `dist/assets/handoff-dialogs.js`, absent from `plan.js` and the other chunks). Verified read-only: running `runBundleCheck` from `bundle-check.mjs` against the committed `dist/` with that single substitution returns no violations (2 entries, 26 chunks). So `web/dist/` does NOT need a rebuild for the check to pass. Process gap behind the rot: `web/package.json` scripts (`build`, `check`, lines 8-17) never call `bundle-check.mjs`; recommend adding `"bundle-check": "node scripts/bundle-check.mjs"` and appending it to `build` (`tsc -b && vite build && node scripts/bundle-check.mjs`) so the next stale sentinel fails the build. Do that in the same change.
Note: `web/dist` was last committed before the newest `web/src` commit, so a separate dist rebuild is advisable but is not needed for this check.
**Regression test:** the existing unit tests `web/scripts/bundle-check.test.mjs` ("a sentinel that occurs in no chunk" at `:156`) already cover the mechanism. The real regression is the CLI against the real manifest: add a test in `bundle-check.test.mjs` `describe("bundle-check CLI")` (`:165`; it already spawns the script with `LOOM_BUNDLE_CHECK_DIST`) that spawns it with no override against the committed `dist/` and expects exit 0. Skip it when `dist/.vite/manifest.json` is absent.
**Files a fix touches:** `web/scripts/bundle-rules.json`, `web/package.json` (the `scripts` object only; no package add), `web/scripts/bundle-check.test.mjs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-64 (`web/package.json`), CR-65 first half (dist drift theme).

---

## CR-20 — Planner WebSocket drops invalid frames, lacks a liveness watchdog, stays open on REST 401

**Verdict:** NARROWED (two sub-claims confirmed as stated, one cannot be fixed as proposed; separate lines below)
**Evidence and verdicts by sub-claim:**
- (a) Invalid frames silently dropped: CONFIRMED. `web/src/plan/api/socket.ts:196-216` (`receive`): JSON and `safeParse` failures `console.warn` and `return`, before `lastMessageAt` is updated and without resync. The dashboard counterpart (`web/src/api/ws.ts:153-196`) sets an error phase and waits for a `full`; the planner has no such recovery frame, so the only resync is a reconnect (a new `hello`, `applyHello` `:111-135`, bumps `connectionEpochAtom` and runs resync handlers).
- (b) No stale-connection watchdog or `visibilitychange`: CONFIRMED as absent, but the recommended "ping watchdog matching `ws.ts`" is NOT implementable client-side alone. `ws.ts` works because the dashboard server sends a `tick` every 1-2 s. The planner server sends only WebSocket protocol pings (`loom/src/plan_server/ws/conn.rs:131-136`, `keep_alive`), which browsers answer automatically and JavaScript cannot observe, and application frames only on change. Silence is normal, so a "no message for N s" timer would reconnect an idle healthy session. Needs a protocol addition.
- (c) Open socket on REST 401: CONFIRMED but low impact. `web/src/plan/api/client.ts:38` sets `unauthorizedAtom`; `socket.ts` never reads it after `probe()` (`:246-258`). Effect: `web/src/plan/shell.tsx:45-51` replaces the whole UI with the sign-in notice, so the lingering socket only keeps dispatching into stores nobody renders; no data exposure beyond what the cookie already allowed. The close is still correct hygiene.
**Fix seam:** `web/src/plan/api/socket.ts`.
- (a): add a private `resync(reason)` to `PlanSocketConnection` that does what `onerror` does (`:288-293`: clear `this.socket`, `close()`, `this.retry()`), and call it from both failure branches of `receive` (`:205-212`). `retry()` shows `PlannerConnectionDialog` ("Loom feature planner is not responding", `web/src/plan/connection-dialog.tsx`) for the backoff; accepted trade-off, and bounded because a reconnect that gets a valid `hello` resets `attempts` (`:127`). Decision: reconnect, do not add an error atom.
- (b): needs the server to emit an application heartbeat. Recommend (server) a `heartbeat` `PlanMessage` frame sent from `keep_alive` (`conn.rs:131-136`, alongside the existing Ping), schema entry in `web/src/plan/api/schema.ts` (`socketMessageSchema`, `:256`) and `loom/src/plan_server/wire/message.rs:56`; client: track `lastMessageAt` (already stored at `:213`) and add a 5 s interval plus a `visibilitychange` check that calls `resync("no heartbeat")` after about 3 keep-alive periods. Without the server frame, do not add a client watchdog. If the plan keeps this client-only, treat (b) as out of scope and say so.
- (c): in `start()` subscribe to `unauthorizedAtom` via `store.sub` and, when it becomes true, set `stopped`-like behaviour: close `this.socket`, cancel the timer, `setPhase("unauthorized")` (same as the probe path `:250-252`); unsubscribe in `dispose()`.
Also found: `socket.ts:74-75` assigns `this.url` twice (duplicate line, harmless; remove while editing).
**Regression test:** `web/src/plan/api/socket.test.ts` (`FakeSocket` `:27`, `flush()` `:70`, `latest()` `:73`; tests such as "closes a socket that errors, retries once, and ignores later events from it" at `:325` show the retry assertions). Add: send `"not json"` and a frame failing `socketMessageSchema`; expect the socket closed and phase `reconnecting`, and after the retry delay a new probe. For (c): `store.set(unauthorizedAtom, true)` while open, expect the socket closed and phase `unauthorized`.
**Files a fix touches:** `web/src/plan/api/socket.ts`, `web/src/plan/api/socket.test.ts`; for (b) also `loom/src/plan_server/ws/conn.rs`, `loom/src/plan_server/wire/message.rs`, `web/src/plan/api/schema.ts`.
**Size / tier:** (a)+(c) S-M, needs-exploration; (b) L, judgment (protocol). Recommend splitting (b) into its own plan item or dropping it.
**Overlap:** D5 (same shape on the dashboard socket); CR-62/63 none.

---

## CR-62 — `MermaidBlock` allows `<foreignObject>` HTML and races global `mermaid.initialize`

**Verdict:** NARROWED (the version and the sanitizer mechanism are right; exploitability is bounded; the race is benign)
**Evidence:**
- Version: `web/package.json:47` declares `"mermaid": "^12.3.0"`; `web/bun.lock:1111` resolves `mermaid@12.3.0` with `dompurify ^3.4.17` (`bun.lock:867` `3.4.17`). The review's version is correct.
- Sanitizer: `web/node_modules/mermaid/dist/mermaid.core.mjs:1391-1395` runs `DOMPurify.sanitize(code, { ADD_TAGS: ["foreignobject"], ADD_ATTR: ["dominant-baseline"], HTML_INTEGRATION_POINTS: { foreignobject: true } })` for every non-sandbox, non-loose level (`strict` included). So HTML children of `foreignObject` (default DOMPurify allows `img`, `a`, `style`, `div`) survive sanitisation. `web/src/plan/markdown/mermaid-block.tsx:11-29` uses `securityLevel: "strict"` and `:67` injects the result with `dangerouslySetInnerHTML`. Not proven at runtime (mermaid needs layout; jsdom cannot run it), so exploitability stays unverified.
- Impact bound: CSP `loom/src/web_server/http.rs:240` is `img-src 'self' data: blob:`, so an `<img>` cannot reach another origin (no exfiltration, only same-origin GETs the user's own cookie already allows); `a` elements bypass `SafeLink` (`web/src/plan/markdown/inline.tsx:~170`) so a plan author can place a plain clickable external link inside a diagram label. Severity Low-Medium (needs a plan author or an agent that wrote the doc).
- Race claim: NOT a real race. `draw` (`mermaid-block.tsx:11-29`) calls `initialize` then `render` synchronously in one tick, every block passes the same current theme, and mermaid's `render` serialises through its own internal queue. A theme change cancels stale effects (`live` flag) and redraws every block. Worst case is a transient wrong-theme frame corrected by the next effect. A promise queue would add code for no observable gain.
- "`mermaid.render()` appends temporary DOM to `document.body` before sanitising": true (mermaid uses a temp `d<id>` element; the existing code removes it on failure, `:23-24`), but that DOM is built by mermaid with per-label `sanitizeText`, not from the raw source, so no pre-sanitise injection route was found. Treat as NOT-STATICALLY-VERIFIABLE; no action.
**Fix seam:** `web/src/plan/markdown/mermaid-block.tsx`, function `draw` (`:11-29`). After `mermaid.render`, parse `svg` with `new DOMParser().parseFromString(svg, "image/svg+xml")`, remove every `img`, `style`, `script`, `iframe`, `object` and `embed` element under `foreignObject`, replace each `a` by its text content, serialise with `XMLSerializer` and return that. No new dependency (`dompurify` is only a transitive dependency, do not import it). If the parse fails, return `{ error: true }`. Alternative rejected: `htmlLabels: false` changes layout of every diagram. Do not add the promise queue.
**Regression test:** `web/src/plan/markdown/mermaid-block.test.tsx` (hoisted `mermaid` mock with `render: vi.fn<...>`, `SOURCE`, `CodeBlock`). Add: `mermaid.render.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><img src="/api/x"/><a href="https://evil.example">t</a></div></foreignObject></svg>' })`; assert `.md-diagram` contains no `img` and no `a`, and still contains the text `t`. Fails today (both present).
**Files a fix touches:** `web/src/plan/markdown/mermaid-block.tsx`, `web/src/plan/markdown/mermaid-block.test.tsx`; `web/dist/` rebuild.
**Size / tier:** M (about 40 lines); judgment (security).
**Overlap:** CR-63 (same `web/src/plan/markdown/` package).

---

## CR-63 — Bidi/control character neutralisation gaps

**Verdict:** CONFIRMED for body Markdown, code blocks and the stage summary; REFUTED for `routes/stage.tsx:105`
**Evidence:**
- `web/src/plan/chat/visible-controls.ts:3-9` (`visibleControls`) replaces `\p{Cf}` with `⟨U+XXXX⟩`. It is applied to summary markdown (`web/src/plan/markdown/inline.tsx:219-250`, `renderSummaryInline`) and chat/file chips, but the general renderer `renderInline` (`inline.tsx:148-175`) passes `node.value` through `markText` unchanged for `text` (`:154`), `inlineCode` (via `CodeSpan`), `html` (`rawHtmlText`), image `alt` and link children. `web/src/plan/markdown/plain-code-block.tsx:32-34` renders `{value}` raw. `web/src/components/stage-detail.tsx:118-130` renders `{summary}` raw (plan-authored text; comment at `:113-117` says "untrusted plain text"). No `visibleControls` use exists anywhere under `web/src/components/` or `web/src/routes/`.
- `web/src/routes/stage.tsx:105` is `<code>{id}</code>` in `NotFound`, where `id` is the user's own URL segment; React escapes it, the user typed it, and it is not an untrusted-content channel. REFUTED as a defect; leave alone.
- Impact is visual spoofing only (no markup injection), as the review says.
**Fix seam (decision: narrow, not all of `\p{Cf}`):** applying `visibleControls` to all body text would rewrite legitimate text: ZWJ in emoji sequences (U+200D), ZWNJ in Persian and Indic scripts, soft hyphen, word joiner. Add to `web/src/plan/chat/visible-controls.ts` a second export `visibleBidi(text)` with the same `⟨U+XXXX⟩` format but matching only `/[؜‎‏‪-‮⁦-⁩]/gu`. Apply it at: `inline.tsx:154` (`text`), the `inlineCode` span, `rawHtmlText` input, image `alt` and the `case "image"` fallback (`:~170`); `plain-code-block.tsx:32-34` for the rendered `<code>` only (keep `CopyButton text={value}` raw so a copy returns the true bytes; add a one-line comment); `web/src/components/stage-detail.tsx:118-130` for `{summary}` (keep the existing summary-markdown path on full `visibleControls`). Do not touch `routes/stage.tsx`. Mind search marks: `markText(value, pattern)` should run on the transformed string.
**Regression test:** `web/src/plan/markdown/markdown.test.tsx` (uses `renderMarkdown(hostile)`, `linkNamed`, `container.textContent`; fixtures in `web/src/plan/markdown/fixtures/`). Add: markdown containing `a‮b`, inline code `` `c⁦d` ``, and a fenced block `e‮f`; assert `textContent` contains `⟨U+202E⟩`, `⟨U+2066⟩` and no raw `‮`; assert the copy button text is raw; assert a ZWJ emoji sequence `👨‍👩‍👧` is unchanged. For the summary: a test beside `web/src/components/stage-details.test.tsx` rendering `StageSummaryText` with `‮`.
**Files a fix touches:** `web/src/plan/chat/visible-controls.ts`, `web/src/plan/markdown/inline.tsx`, `web/src/plan/markdown/plain-code-block.tsx`, `web/src/components/stage-detail.tsx`, `web/src/plan/markdown/markdown.test.tsx`, `web/src/components/stage-details.test.tsx`.
**Size / tier:** M; needs-exploration (the narrow-set decision is settled above).
**Overlap:** CR-62 (same markdown package).

---

## CR-64 — Duplicate `cn` libraries and build-only packages in `dependencies`

**Verdict:** CONFIRMED, with the "Performance" part NARROWED
**Evidence:** `web/package.json` lists `"clsx"` (`:41`), `"cn": "0.5.2"` (`:42`), `"tailwind-merge"` (`:54`), and `"shadcn"`, `"tailwindcss"`, `"@tailwindcss/vite"`, `"tw-animate-css"` under `dependencies`. `cn` (the package, which itself exports `cn`, `clsx`, `twMerge`) is imported by 52 files and by `web/src/lib/utils.ts:1`. `web/src/aurora-ui/shared/lib/cn.ts:1-6` re-implements `cn` with `clsx` + `twMerge`; only three vendored files import it (`aurora-ui/feedback/BusyRoundel.tsx`, `EmptyState.tsx`, `HazardPanel.tsx`). `rg` finds no other `clsx`/`tailwind-merge` import. `web/src/lib/utils.ts` has no importer but `web/components.json:15` maps the shadcn `utils` alias to it, so keep it. The four build tools are used only at build time: `web/src/index.css:2,5,6` imports and `web/vite.config.ts:4-7`. Narrowing: the bundle is a static asset embedded in the loom binary, so `dependencies` vs `devDependencies` changes nothing at runtime; this is hygiene. The duplication adds a small amount of `tailwind-merge` code to the bundle through the three aurora files.
**Fix seam:** make `web/src/aurora-ui/shared/lib/cn.ts` `export { cn } from "cn";` (the aurora README, `web/src/aurora-ui/README.md:5-8`, lists two local changes to vendored files; add `shared/lib/cn.ts` to that sentence). Then run `bun remove clsx tailwind-merge` (confirm nothing else needs them; `class-variance-authority` does not). Move the build tools with `bun remove shadcn tailwindcss @tailwindcss/vite tw-animate-css` then `bun add -d shadcn tailwindcss@4.3.5 @tailwindcss/vite@4.3.5 tw-animate-css` (keep the pinned versions `tailwindcss 4.3.5`, `@tailwindcss/vite 4.3.5`, `shadcn 4.23.0`; use `bun add -d` with explicit versions). Never hand-edit the manifest or `bun.lock`.
**Regression test:** none useful beyond the gate: `bun run typecheck`, `bun run test` (existing aurora consumers: `web/src/plan/connection-dialog.tsx` renders `HazardPanel`), and a successful `vite build`. A grep check in review: no `clsx` or `tailwind-merge` import remains.
**Files a fix touches:** `web/package.json`, `web/bun.lock`, `web/src/aurora-ui/shared/lib/cn.ts`, `web/src/aurora-ui/README.md`; `web/dist/` rebuild.
**Size / tier:** S; mechanical.
**Overlap:** CR-13 (`web/package.json` scripts), CR-78 hygiene.

---

## CR-65 — first half: non-recursive `rerun-if-changed` on `web/dist/`

**Verdict:** REFUTED
**Evidence:** `loom/build/assets.rs:119-132` emits `cargo:rerun-if-changed=<web/dist>` (the directory). The Cargo reference states that when the path is a directory "it will scan the entire directory for any modifications" (the page does not say recursive explicitly, but it does not need to). Independently, `emit_web_assets` (`:134-148`) embeds each file with `include_bytes!`, and rustc records every `include_bytes!` file in the crate's dep-info, so an in-place change to `web/dist/assets/<file>.js` recompiles the crate regardless of the build script. Only added or removed files change the generated `web_assets.rs` table, and those alter the directory entries that the directory scan sees. The review's "in-place updates under `web/dist/assets/` are missed" has no failure mode. No change recommended.
**Fix seam:** none.
**Regression test:** none.
**Files a fix touches:** none.
**Size / tier:** n/a.
**Overlap:** CR-13.

## CR-65 — second half: stale reading view after "Save and close"

**Verdict:** CONFIRMED (transient, until a watcher push or reconnect)
**Evidence:**
- `web/src/plan/editor/edit-flow.ts:87-96` (`save`) dispatches the result and nothing else; `edit-session.ts:23-27` then navigates to `toAddress(doc.path)` (without `?edit=1`). `ReadingColumn` (`web/src/plan/files/reading-column.tsx:297-334`) stays mounted for the same path and keeps showing `openDocAtom.doc`, which still holds the pre-edit text; `editing` flips off via the query string only (`:307`).
- `web/src/plan/files/open-doc.ts:71-115` (`useStaleRefetch`; the review's `:43-69` is `useInitialFetch`, which does not rerun for the same path) refetches only when `openDocIsStaleAtom` (`files/atoms.ts:9-17`) sees `docChangedAtom[path] !== seenHash`. `docChangedAtom` is set only by the plan socket's `doc_changed` push (`web/src/plan/files/api.ts:62-63`, `:70-73`), which the server emits after its file watcher notices the write (`loom/src/plan_server/files/mod.rs:135-146`). Until then the reader sees the old text; if the socket is down it stays old.
- Precedent: `web/src/plan/trace/settle-flow.ts:116-121` (`reload`) sets `docChangedAtom` directly after its own write for exactly this reason.
**Fix seam:** export `markChanged` from `web/src/plan/files/api.ts:61-62` (currently a file-private `const markChanged = (store, path, hash)`). In `web/src/plan/editor/edit-flow.ts`, pass `store` (already obtained by `useEditFlow` at `:121`) into `useFlowActions` (`:75-81`) and in `save`'s `.then` (`:92-94`), after `dispatch({ type: "result", result })`, call `markChanged(store, path, result.value.hash)` when `result.ok && result.value.result === "saved"`. This makes `openDocIsStaleAtom` true, so `useStaleRefetch` refetches and replaces `openDocAtom`; when the watcher push arrives with the same hash nothing changes. It also fixes the same staleness for plain Ctrl-S saves that stay in edit mode. The edit page itself is unaffected: `useStaleSource` (`edit-body.tsx:104-107`) compares against the reducer's `openedHash`, which `saved` already set to the same hash.
**Regression test:** `web/src/plan/editor/edit-mode.test.tsx` (harness above; it already seeds `docChangedAtom` to `{}` in `beforeEach` `:121`): stub `fetch` to answer the save with `{result:"saved", hash:"sha256:new", validation:null}`, press Ctrl-S on the fake editor, then `await waitFor(() => expect(store.get(docChangedAtom)[doc.path]).toBe("sha256:new"))`. Fails today (stays `{}`).
**Files a fix touches:** `web/src/plan/files/api.ts`, `web/src/plan/editor/edit-flow.ts`, `web/src/plan/editor/edit-mode.test.tsx`.
**Size / tier:** S; mechanical.
**Overlap:** U1 (close path), U3 (`edit-flow.ts`), CR-78 untested components (this is the only test of that path since `ReadingColumn` is mocked in `web/src/plan/router.test.tsx:31`).

---

## CR-78 (fourth bullet) — web tsconfig, oxlint, ledger row, activity scroll, dialog footer, codemirror size, untested components

Each sub-claim separately.

### CR-78.a — `tsconfig.{app,node}.json` omit `"strict": true`
**Verdict:** REFUTED as a defect.
**Evidence:** `web/package.json:72` pins `typescript ~7.0.2` (`bun.lock:1387` `7.0.2`, installed `web/node_modules/typescript/package.json` 7.0.2). A probe with an empty-option config run through `web/node_modules/.bin/tsc` reported TS7006 (implicit any) and TS2322 (null to string), so strict is on by default in this compiler. `tsc --showConfig` does not list defaults, so it cannot show this.
**Fix seam:** none required. Optional: add `"strict": true` to `web/tsconfig.app.json` and `web/tsconfig.node.json` as documentation (one line each).
**Size / tier:** S (optional); mechanical. **Overlap:** none.

### CR-78.b — `.oxlintrc.json` omits `react/rules-of-hooks`
**Verdict:** CONFIRMED.
**Evidence:** `web/.oxlintrc.json` has no `plugins` and no hooks rule. A probe file with a conditional `useState` linted under the project config (oxlint 1.93.0) produced no finding; the same file with `{"plugins":["react"],"rules":{"react/rules-of-hooks":"error"}}` was flagged ("called conditionally"). Enabling the rule over `web/src` (same ignore patterns) yields 11 errors, all in test files (`*.test.tsx`: top-level calls of helpers named `use*` such as `useFakeClock` in `web/src/plan/chat/permission-fetch.test.tsx:7`, `useSettingsTestLifecycle`, `useContractFixtures`, and `useLocation` inside an anonymous function at `web/src/plan/diff/rendered-diff.test.tsx:236`), none in production code.
**Fix seam:** `web/.oxlintrc.json`: add `"plugins": ["react"]` plus `"react/rules-of-hooks": "error"`, and an `overrides` entry `{"files": ["**/*.test.tsx", "**/*.test.ts"], "rules": {"react/rules-of-hooks": "off"}}` (the existing overrides block at the end of the file shows the form). Do not rename the test helpers. Check that `plugins: ["react"]` does not newly enable the noisy `react/*` correctness rules seen in a trial run (they were warnings, and `lint` uses `--deny-warnings`): set `"categories": { "correctness": "off" }` only if the full `bun run lint` fails; verify with a focused `oxlint` run, not the suite.
**Regression test:** none (config); verify by `bun run lint` and the probe above.
**Files:** `web/.oxlintrc.json`. **Size / tier:** S; needs-exploration (plugin side effects). **Overlap:** none.

### CR-78.c — `ledger-row.tsx:42` `<tr onClick>` without keyboard parity
**Verdict:** NARROWED (keyboard path exists).
**Evidence:** `web/src/components/ledger-row.tsx:41` is the row `onClick={() => navigate(href)}` (citation off by one). The same row's stage cell renders `StageName` (`:125-140`) with a real `<Link to={href}>` whose click handler calls `stopPropagation`, so every row has a focusable keyboard route to the same destination. The row click is a pointer convenience; only the full-row target lacks keyboard activation. No fix needed unless the plan wants the row itself focusable.
**Fix seam:** none recommended. **Size / tier:** n/a. **Overlap:** none.

### CR-78.d — `.activity-scroll` missing `tabIndex={0}`
**Verdict:** NARROWED (low).
**Evidence:** `web/src/plan/header/activity.tsx:166` renders `<div className="activity-scroll">` with `overflow-y: auto` (`web/src/plan/header/activity.css:232-237`). Populated, it holds focusable rows (`activity-rows.tsx:139` `Link`, `:188` `button`), which keeps it keyboard-scrollable. Empty ("Nothing in progress.") has no focusables, but then nothing overflows. The case that matters is a long detail panel opened inside it with no focusable children. `web/src/plan/markdown/plain-code-block.tsx:32` already uses the project's `tabIndex={0}` for scrollers.
**Fix seam:** optional `tabIndex={0}` plus `role="region"` and an `aria-label` on `activity.tsx:166`; extend `web/src/plan/header/activity-popover.test.tsx:165-195` ("the popover's scrolling region") with `expect(scroll.tabIndex).toBe(0)`. **Size / tier:** S; mechanical. **Overlap:** none.

### CR-78.e — `DialogFooter` `flex-col-reverse` tab order
**Verdict:** CONFIRMED (narrow viewport, one consumer).
**Evidence:** `web/src/components/ui/dialog.tsx:106-111`: `flex flex-col-reverse ... sm:flex-row sm:justify-end`. Below the `sm` breakpoint the visual order reverses while DOM and tab order stay, so Tab runs bottom-to-top. Single consumer: `web/src/plan/run/abandon-dialog.tsx:146-159` (`ConfirmationActions`: Cancel first, then the destructive "Abandon run").
**Fix seam:** change `flex-col-reverse` to `flex-col` in `dialog.tsx:110`; the file is shadcn-generated (excluded from lint; format with `oxfmt` as the repo does). On mobile the order becomes Cancel above the destructive button, matching tab order. **Regression test:** none practical (class change); assert in a DOM test of `AbandonDialog` that the footer has no `flex-col-reverse` if desired. **Size / tier:** S; mechanical. **Overlap:** none.

### CR-78.f — `codemirror.ts` is 403 LOC
**Verdict:** CONFIRMED (limit 400, over by 3).
**Evidence:** `wc -l web/src/plan/editor/codemirror.ts` is 403. The file is a guarded lazy-chunk boundary (`web/scripts/bundle-rules.json` forbids it from both entry bundles), so any split must keep it a dynamic-import-only module.
**Fix seam:** extract a cohesive group of at least 10 lines into a sibling module under `web/src/plan/editor/` that `codemirror.ts` imports (statically), for example the theme or keymap helpers; run `node scripts/bundle-check.mjs` after a build to confirm the new module stays out of `index.html`/`plan.html` initial closures (it will, because only `codemirror.ts` imports it and `codemirror.ts` is lazy). Needs a look at the file to pick the seam. **Regression test:** existing `web/src/plan/editor/codemirror.test.ts`. **Size / tier:** S-M; needs-exploration. **Overlap:** CR-13 (bundle rules include this path).

### CR-78.g — untested `stage.tsx`, `reading-column.tsx`, `edit-body.tsx`, `validation-list.tsx`
**Verdict:** NARROWED (no dedicated test files; each has indirect coverage of varying depth).
**Evidence:**
- `web/src/routes/stage.tsx` (`StagePage`): rendered by `web/src/routes/terminal.test.tsx`; no stage-route test of its own (loading, not-found, summary).
- `web/src/plan/files/reading-column.tsx`: the only importer under test is `web/src/plan/router.test.tsx:31`, which replaces it with a mock, so the real component (load states, edit toggle, scroll restore, stale refetch) is effectively untested. This is the weakest of the four; CR-65's test and U1's close test both exercise its neighbours but not it.
- `web/src/plan/editor/edit-body.tsx`: heavily exercised through `edit-mode.test.tsx` and the editor contract tests.
- `web/src/plan/files/plan/validation-list.tsx`: asserted through `data-testid="validation-list"` in `web/src/plan/files/doc-heading-view.test.tsx:105-118` and `plan-view.test.tsx:178`.
**Fix seam:** add a focused test file for `files/reading-column.tsx` (mock `fetchDoc`, render `ReadingColumn` with and without `?edit=1`, assert the stale-refetch and the post-save refresh from CR-65); optional small `routes/stage.test.tsx`. **Size / tier:** M; needs-exploration. **Overlap:** CR-65 second half, U1.
