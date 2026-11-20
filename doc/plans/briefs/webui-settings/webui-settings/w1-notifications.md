# W1 — desktop notifications core

Lane: codex `gpt-5.6-terra`, effort `xhigh`. Three units, run in order, one forward each. Your task
text names ONE unit: do only the section headed with that unit id and stop at its own "Done when"
line. Earlier units are already on disk; later units are not yours.

You own these files and no others:

```text
web/src/lib/notify.ts              (new)
web/src/lib/notify.test.ts         (new)
web/src/state/notifications.ts     (new)
web/src/state/notifications.test.ts (new)
web/src/state/apply.ts             (edit)
web/src/state/apply.test.ts        (edit — add three cases, keep the existing one)
web/src/main.tsx                   (edit — one import, one call; unit w1c)
```

Read-only, for context: `web/src/api/schema.ts`, `web/src/lib/activity.ts`, `web/src/lib/levels.ts`,
`web/src/state/atoms.ts`, `web/src/state/store.ts`, `web/src/api/ws.ts`,
`web/src/api/fixtures/snapshot.json`, `web/src/aurora-ui/shared/atoms/theme.ts`.

Do not run `git` at all. Do not touch `web/dist`, `web/index.html`, `src/index.css`, or any file not
listed above.

Lint is `oxlint --deny-warnings` with one project rule (`web/.oxlintrc.json`): `zod` may be imported
only through `@/api/zod`. You need no `z` at all — import `snapshotSchema` and the types from
`@/api/schema`.

## What already exists, and why the design is a diff

The dashboard's wire is ONE shape: a whole snapshot, not a stream of events. `snapshotSchema`
(`web/src/api/schema.ts:168-181`) is used for both the `/ws` frame and the `/api/status` GET, and it
carries no message-type discriminator. `applySnapshot` (`web/src/state/apply.ts:7-18`) replaces
`snapshotAtom` wholesale. So a notification has to be derived by comparing the previous snapshot with
the next one.

That diff already has a precedent in this repo: `appendTransitions` in `web/src/lib/activity.ts:41-67`,
called from `applySnapshot` with both snapshots in hand, plus `statusesById` at `:24-32`. Read both
before writing anything — `notify.ts` is the same shape of module.

Two facts from that precedent that bind you:

- `previous === null` is a BASELINE, not a burst. The first frame after the page opens must produce
  no events at all, or opening the dashboard on a run that is already blocked fires a notification
  for every stage at once.
- Iterate stages through `orderStages` (`web/src/lib/levels.ts:47`), so simultaneous transitions come
  out in a stable order rather than in filesystem order. `orderStages` returns `OrderedStage[]`, which
  is `{ stage, level }` pairs and NOT stages, so the loop header destructures exactly as
  `activity.ts:57` does:

  ```ts
  for (const { stage } of orderStages(next.status.stages)) {
  ```

There is no "the agent asked a question" field anywhere on the wire. A question from Claude Code
surfaces only as the stage's `status` flipping to `waiting-for-input`, which the server then also
publishes as an entry in the snapshot's `attention` array with `label: "NEEDS INPUT"`. Consume that
array rather than re-deriving a classification, but know what it is:

- It holds at most ONE entry per stage, and `entry.id` is always a stage id.
- The label is NOT a function of stage status. The server picks the first of three sources
  (`loom/src/commands/status/render/attention_model.rs:36-45`): a cleanup warning
  (`CLEANUP FAILED`), then a completion blocker (`COMPLETION PENDING`, `COMPLETION BLOCKED`,
  `WRITER UNCONFIRMED`), and only then the status table (`BLOCKED`, `MERGE CONFLICT`,
  `ACCEPTANCE FAILED`, `MERGE ERROR`, `NEEDS REVIEW`, `NEEDS INPUT`, `ADJUDICATING`).
- `COMPLETION PENDING` sits on a stage that is merely `executing` while loom watches a first
  completion failure. Nobody is being asked for anything, so it must NOT notify. Every other label
  needs a human, including `COMPLETION BLOCKED` and `WRITER UNCONFIRMED`, which replace `NEEDS REVIEW`
  on a `needs-human-review` stage that carries a blocker.
- Labels are static strings and hints carry no counters or durations, so an `id:label` key is stable
  from frame to frame.

Three lifecycle facts are settled; do not "fix" any of them:

- A reconnect does NOT re-baseline. `snapshotAtom` survives an outage and `ws.ts` never refetches
  `/api/status` on reconnect, so the first frame back is a real catch-up diff. That is intended. Do
  not reset the baseline anywhere.
- Every open tab runs its own socket and notifies independently. Cross-tab de-duplication is the
  browser's `tag` replacement, which is why every event key must be stable and is passed as `tag`.
- A preference changed in one tab MUST reach every other open tab's notifier, whether or not that tab
  has its settings dialog open. jotai's `atomWithStorage` listens for the `storage` event only while
  the atom is MOUNTED (`baseAtom.onMount` subscribes, `web/node_modules/jotai/esm/vanilla/utils.mjs:455-460`;
  the listener is removed on unsubscribe, `:426-440`), and `store.get` never mounts it. The dialog's
  `useAtom` mounts it only while the dialog is open, so without an app-lifetime subscriber a tab whose
  settings are closed would keep notifying after the user opted out elsewhere.
  `watchNotificationPreference` (w1b) plus its call in `web/src/main.tsx` (w1c) is that subscriber.

## Unit w1a-notify-lib — `web/src/lib/notify.ts` + `web/src/lib/notify.test.ts`

### Step 1 — the module

Exact public surface. Do not rename, do not add exports:

```ts
import type { Snapshot } from "@/api/schema";

export interface NotifyEvent {
  /** Stable de-dupe key; also used as the Notification `tag`. */
  key: string;
  title: string;
  body: string;
}

/** Key of the run-finished event; the delivery cap never drops this one. */
export const RUN_FINISHED_KEY = "run-finished";

export function notifiableEvents(previous: Snapshot | null, next: Snapshot): NotifyEvent[];
```

`notifiableEvents` is PURE — no `window`, no atoms, no side effects. It applies three rules in this
order, then de-dupes by `key` keeping the first occurrence, then returns. The run-finished event, when
present, is therefore always the LAST element.

Rule 0 — baseline. `previous === null` returns `[]`.

Rule 1 — attention arrivals. Build a `Set<string>` of `` `${entry.id}:${entry.label}` `` over
`previous.attention`. For every entry of `next.attention` whose label is not `"COMPLETION PENDING"`
and whose key is NOT in that set, emit:

```text
key:   `attention:${entry.id}:${entry.label}`
title: `loom — ${entry.label.toLowerCase()}`
body:  `${entry.name} (${entry.id})`
```

Hold the excluded label in a module-local constant, `const INFORMATIONAL_LABEL = "COMPLETION PENDING";`.

Key on id AND label, not on id alone: one stage's label changes under it (a blocker arrives, or
`WRITER UNCONFIRMED` becomes `COMPLETION BLOCKED`), and each such arrival asks the human for something
different. This is the same key `attention-panel.tsx:55` uses for its card.

The body uses `entry.name`, never `entry.hint`. The hint is a CLI command such as
`loom stage resume client`; this notification serves a browser that may be on another machine.

Rule 2 — handoff. Build the previous statuses with the same `Map<string, StageStatus>` shape as
`activity.ts:24-32` (first occurrence per id wins). For each `{ stage }` of
`orderStages(next.status.stages)` whose previous status differs from its new one AND whose new status
is exactly `"needs-handoff"`, emit:

```text
key:   `handoff:${stage.id}`
title: "loom — needs handoff"
body:  `${stage.name} (${stage.id}) needs a handoff`
```

Only `needs-handoff`. Every other attention-worthy status (`blocked`, `merge-conflict`,
`completed-with-failures`, `merge-blocked`, `needs-human-review`, `waiting-for-input`,
`needs-adjudication`) already arrives through rule 1, and emitting it twice would double-notify.
`needs-handoff` is the one the server leaves out of `attention` (`attention_model.rs:135`).

Rule 3 — run finished. A module-local predicate decides whether a snapshot is settled:

```ts
function settled(snapshot: Snapshot): boolean {
  const { stages, merge } = snapshot.status;
  return (
    stages.length > 0 &&
    merge.conflicts.length === 0 &&
    stages.every(
      (stage) => stage.status === "skipped" || (stage.status === "completed" && stage.merged),
    )
  );
}
```

Emit exactly one event when `!settled(previous) && settled(next)`:

```text
key:   RUN_FINISHED_KEY
title: "loom — run finished"
body:  `${next.status.plan_name ?? "the run"} finished; ${mergedCount} stages merged`
```

where `mergedCount` is the number of stages in `next.status.stages` with `merged === true`.

Every `stage_type` is treated alike — `knowledge`, `integration-verify` and `knowledge-distill`
stages are merged by the same handler as `standard` ones and carry `merged: true` when done
(`loom/src/orchestrator/core/completion_handler.rs:150-153` sets it on a `Knowledge` stage), and
loom's own end-of-run check, `all_stages_merged` (`loom/src/fs/plan_lifecycle.rs:91`), requires
`merged` on every stage with no type exception. Do not special-case any type.

The predicate is over the whole stage set of each frame, so it needs no intermediate frame: a run
whose last stage goes from `executing` straight to `completed` + `merged: true` between two frames,
with `merge.pending` empty in both, still announces. A stage in any other status (`blocked`,
`merge-conflict`, `merge-blocked`, `completed-with-failures`, …) or `completed` with `merged: false`
keeps the run unsettled — the latter covers merge states the web never sees, because the server's
merge-status warnings (missing branch, unknown state) are dropped from the web payload
(`loom/src/commands/status/data/collector.rs:274-282`).

DO NOT decide this from `merge.pending`. `merge.pending` holds completed-but-unmerged stages only
(`loom/src/git/merge/status.rs:163-178` skips every stage that is not `completed`), so it goes
non-empty and back to empty once per stage, all through the run. A rule keyed on it announces
"run finished" after every single merge. `merged` is a per-stage boolean on `stageSummarySchema`
(`schema.ts:95`); `plan_name` is `string | null` on `statusDataSchema`. The `!settled(previous)` term
is what makes the rule edge-triggered, and the key is a constant so that any accidental repeat
replaces the first OS notification through its `tag` instead of stacking.

### Step 2 — the tests

Build snapshots by parsing the real fixture, never by hand-writing one: a `StageSummary` has about
thirty required fields and `snapshotSchema` is `.strict()`. The precedent is
`web/src/state/apply.test.ts:9`, which parses `src/api/fixtures/snapshot.json` through
`snapshotSchema`. Do the same, then `structuredClone` the result and edit the clone per case.

Know the fixture's starting state, because several cases depend on it: 7 stages, none of them
`waiting-for-input` or `needs-handoff`, every stage `merged: false`; `attention` has three entries
(`ACCEPTANCE FAILED` on `client`, `MERGE CONFLICT` on `docs`, `NEEDS REVIEW` on `integration-verify`);
and `merge` is `{ merged: [], pending: ["docs"], conflicts: ["docs"] }` — CONFLICTS IS NOT EMPTY. A
"settled" frame therefore needs every stage set to `completed` with `merged: true` AND
`merge.conflicts` cleared. Write one small helper in the test file that returns such a clone.

Cases, each its own `it`. Case 7's name must contain the exact words `mid-run merge` — a plan gate
greps for them:

1. a first frame (`previous === null`) yields no events
2. an attention entry present in both frames yields no event
3. an attention entry only in `next` yields one event whose title contains the lower-cased label,
   whose body contains the stage name and the stage id, and whose body does NOT contain the hint
4. the same stage id re-entering attention under a DIFFERENT label yields a fresh event
5. a new attention entry labelled `COMPLETION PENDING` yields no event; the same entry relabelled
   `COMPLETION BLOCKED` in the following frame yields one
6. a stage flipping to `needs-handoff` yields one event; the same stage still `needs-handoff` in the
   following frame yields none
7. `mid-run merge`: previous has `merge.pending: ["a"]`, next has `merge.pending: []` and
   `merge.conflicts: []`, while at least one stage is still `executing` — NO run-finished event
8. previous unsettled (one stage `completed` but `merged: false`), next settled — exactly one event
   with key `RUN_FINISHED_KEY`, it is the LAST element, and its body carries the merged count
9. settled in both frames yields no run-finished event
10. every stage `completed` and `merged` but `merge.conflicts` non-empty yields no run-finished event
11. the final stage completes between frames with `merge.pending: []` and `merge.conflicts: []` in
    BOTH: previous has one stage `executing` and every other stage `completed` + `merged: true`; next
    has that stage `completed` + `merged: true` too — exactly one run-finished event
12. one stage in each of `blocked`, `merge-conflict`, `merge-blocked` and `completed-with-failures`
    (one `it` per status, or `it.each`), every other stage `completed` + `merged: true`, conflicts
    empty, previous unsettled — no run-finished event
13. every stage `completed`, one of them `merged: false`, `merge.pending: []` and conflicts empty (a
    merge state the web cannot see) — no run-finished event
14. a stage of `stage_type: "knowledge-distill"` that is the last to reach `completed` +
    `merged: true` triggers the run-finished event exactly as a `standard` one does

### Done when (w1a)

`bun run --cwd web test src/lib/notify.test.ts` passes. Run it once if you want; nothing else.

## Unit w1b-notify-state — `web/src/state/notifications.ts` + `web/src/state/notifications.test.ts`

### Step 1 — the module

```ts
import { atomWithStorage } from "jotai/utils";
import type { Snapshot } from "@/api/schema";
import { notifiableEvents, RUN_FINISHED_KEY } from "@/lib/notify";
import type { Store } from "@/state/store";

export const NOTIFICATIONS_STORAGE_KEY = "loom:notifications";

export const notificationsEnabledAtom = atomWithStorage<boolean>(
  NOTIFICATIONS_STORAGE_KEY,
  false,
  undefined,
  { getOnInit: true },
);

export type NotificationSupport = "unsupported" | "insecure" | "default" | "granted" | "denied";

export function notificationSupport(): NotificationSupport;

export async function requestNotificationPermission(): Promise<NotificationSupport>;

export function deliverNotifications(store: Store, previous: Snapshot | null, next: Snapshot): void;

/** Keeps the preference atom mounted for the app's lifetime, so a `storage` event from another tab
 *  updates it even while the settings dialog is closed. Returns the unsubscribe. */
export function watchNotificationPreference(store: Store): () => void;
```

`watchNotificationPreference(store)` is exactly `return store.sub(notificationsEnabledAtom, () => {});`
— `store.sub` mounts the atom, and mounting is what attaches jotai's `storage` listener. It must not
run at module scope or on import: w1c calls it once from `web/src/main.tsx`.

`atomWithStorage` with `{ getOnInit: true }` is exactly how the three theme atoms persist
(`web/src/aurora-ui/shared/atoms/theme.ts:35-54`) — match that call shape. The default is `false`:
notifications are opt-in, because turning them on triggers a browser permission prompt.

`notificationSupport()`, in this order:

1. `!("Notification" in window)` → `"unsupported"`
2. `window.isSecureContext !== true` → `"insecure"`
3. otherwise `window.Notification.permission` (`"default" | "granted" | "denied"`)

There is no `typeof window === "undefined"` guard: this is a browser-only SPA with no SSR, and under
vitest's jsdom environment `window` is the global, so that branch is unreachable.

The `insecure` case is real and is a documented way to run this dashboard: `loom status --web --host
<lan-ip>` serves plain HTTP off loopback (`loom/src/cli/types_status_web.rs:20-25`), and the
Notifications API is only available in a secure context — `https`, `localhost`, or `127.0.0.0/8`. A
LAN IP over `http` is none of those. Treat it as a normal state with an explanation, never as an
error.

`requestNotificationPermission()`: if `notificationSupport()` is not `"default"`, return it unchanged.
Otherwise `await window.Notification.requestPermission()` and return the result. It must only ever be
called from a click handler — W3 does that; your job is just not to call it at module scope or on
import.

`deliverNotifications(store, previous, next)`:

1. return immediately if `store.get(notificationsEnabledAtom) !== true`
2. return immediately if `notificationSupport() !== "granted"`
3. `const events = notifiableEvents(previous, next)`; split it into `finished` (the events whose key is
   `RUN_FINISHED_KEY`) and `rest` (all others). Deliver
   `[...rest.slice(0, MAX_NOTIFICATIONS_PER_FRAME), ...finished]`, with
   `const MAX_NOTIFICATIONS_PER_FRAME = 5` module-local. One frame of a wide DAG can flip a dozen
   stages at once, and a dozen simultaneous OS notifications is a wall. The run-finished event is
   NEVER cut by the cap: it is always last in the array, so a plain `slice(0, 5)` would drop it first.
   Events the cap cuts are gone for good — the next frame's `previous` already contains them. That is
   accepted; do not queue them.
4. for each event, inside a `try`/`catch` that swallows the error (a comment-only `catch` block, the
   shape `web/src/api/config.ts:93-95` already uses):
   `new window.Notification(event.title, { body: event.body, tag: event.key })`, then set
   `notification.onclick = () => { window.focus(); notification.close(); };`

`tag` makes the browser coalesce a repeat of the same key rather than stacking it, across tabs too.
The whole loop is wrapped defensively because the constructor throws a `TypeError` on mobile browsers,
which support notifications only through a service worker.

### Step 2 — the tests

Stub the API; jsdom 30 implements neither `Notification` nor `isSecureContext` (`window.isSecureContext`
is `undefined` there), so nothing fires unless you stub both. Use `vi.stubGlobal` for a class that
records its constructor arguments AND its instances, with a `close` method that records calls, a static
`permission` and a static `requestPermission`. Set `window.isSecureContext` explicitly with
`Object.defineProperty(window, "isSecureContext", { configurable: true, value: true })`. Restore with
`vi.unstubAllGlobals()` in `afterEach`.

Drive the atom with `store.set(notificationsEnabledAtom, true)` on a fresh `createStore()` (from
`jotai/vanilla`) per case — NEVER by writing `localStorage`. `getOnInit` reads storage once, when the
module is evaluated (`web/node_modules/jotai/esm/vanilla/utils.mjs:449-453`), so a later
`localStorage.setItem` does not change an unmounted atom and the case would silently test the wrong
state.

Cases:

1. the atom `false` → nothing constructed, even with events available
2. atom `true` and permission `"granted"` → one construction per event; the first carries the expected
   `title`, `options.body` and `options.tag`; the constructed instance has a callable `onclick`, and
   invoking it calls the stub's `close`
3. atom `true` and permission `"denied"` → nothing constructed
4. `Notification` absent from `window` → nothing constructed and nothing thrown
5. `isSecureContext` false → `notificationSupport()` is `"insecure"` and nothing is constructed
6. seven new attention entries in one frame → exactly five constructions
7. seven new attention entries AND the run settling in the same frame → exactly six constructions,
   and the LAST one's `tag` is `RUN_FINISHED_KEY`
8. a constructor that throws → `deliverNotifications` still returns normally
9. `another tab` (the name must contain those exact words — a plan gate greps for them): on a fresh
   store, call `watchNotificationPreference(store)`; then simulate another tab turning the preference
   ON with `localStorage.setItem("loom:notifications", "true")` followed by
   `window.dispatchEvent(new StorageEvent("storage", { key: "loom:notifications", newValue: "true", storageArea: window.localStorage }))`;
   `deliverNotifications` on a frame with one new attention entry constructs once. Then simulate the
   other tab turning it OFF the same way with `"false"`; the next such delivery constructs nothing
   more. This is the only case that writes `localStorage` directly, because the storage event is the
   behaviour under test; the rule above (drive the atom with `store.set`) still holds for cases 1-8.
10. after calling the disposer `watchNotificationPreference` returned, a `storage` event setting
    `"true"` leaves `store.get(notificationsEnabledAtom)` unchanged

### Done when (w1b)

`bun run --cwd web test src/state/notifications.test.ts` passes. Run it once if you want; nothing else.

## Unit w1c-apply-wiring — `web/src/state/apply.ts` + `web/src/state/apply.test.ts` + `web/src/main.tsx`

### Step 1 — the call site

`applySnapshot` (`web/src/state/apply.ts:7-18`) is the single funnel every production snapshot passes
through: `receiveSnapshot` in `ws.ts` calls it for the WebSocket frame and for the `/api/status`
bootstrap alike. (`web/src/components/terminal/terminal-view-test-helpers.tsx` also calls it, from
tests; the atom defaults to `false`, so your call is inert there. Leave that file alone.)

First, `web/src/main.tsx`: import `watchNotificationPreference` from `@/state/notifications` and call
`watchNotificationPreference(store);` on the line after `connectStatusSocket(store);`, under the same
comment block — extend that comment by one sentence saying the preference subscription is held for
the app's lifetime for the same reason. Discard the returned disposer exactly as the socket handle is
discarded there; do not move either call into an effect. Change nothing else in `main.tsx`.

Then, in `applySnapshot`, add ONE call as the LAST statement of the function, after BOTH `store.set`
lines:

```ts
deliverNotifications(store, previous, next);
```

`previous` is already bound at the top of the function. Keep the out-of-order guard exactly as it is —
a stale frame must `return` before notifying, or a late-arriving frame re-fires events. A plan gate
checks that the call comes after `store.set(snapshotAtom`. Add the import and one short sentence to the
function's doc comment saying that desktop notifications are raised from here because this is where
both snapshots are in scope. Change nothing else in the file.

### Step 2 — the tests

Add THREE `it` cases to `web/src/state/apply.test.ts`. Keep the existing `describe` and its one `it`
exactly as they are — you are appending, not rewriting. The file currently imports only `describe`,
`expect` and `it` from `vitest`; add `afterEach` and `vi`. A plan gate counts at least four `it(` calls
in this file.

Every case stubs `Notification` as in w1b with permission `"granted"`, sets
`Object.defineProperty(window, "isSecureContext", { configurable: true, value: true })` — without it
jsdom's `undefined` makes `notificationSupport()` return `"insecure"` and nothing is ever constructed —
uses a fresh `createStore()`, and calls `store.set(notificationsEnabledAtom, true)`.

1. a baseline frame, then a frame with a later `generated_at` carrying one new attention entry →
   the stub was constructed exactly once. This is the proof that the notifier is reachable from the
   real snapshot path, not merely defined.
2. a baseline frame, then a frame with an EARLIER `generated_at` carrying a new attention entry →
   constructed zero times (the stale-frame guard returns before notifying)
3. a single first frame that already has attention entries → constructed zero times (baseline)

### Done when (w1c)

`web/src/main.tsx` calls `watchNotificationPreference(store)` (a plan wiring check greps for it), and
`bun run --cwd web test src/lib/notify.test.ts src/state/notifications.test.ts src/state/apply.test.ts`
passes. Run that one command once if you want it.

## Report

The orchestrator runs typecheck, lint, format and the full suite; you do not. Report files changed,
anything you assumed, anything unresolved.
