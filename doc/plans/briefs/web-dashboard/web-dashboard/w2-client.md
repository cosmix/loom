# W2 — TypeScript data layer: WebSocket client, Jotai state, pure formatters, tests

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Runs in parallel with W1 and W3 after W0 returned.
Do not run `git` at all. Do not touch `.loom/`. Never edit `web/src/api/schema.ts` or the
fixture (W0's; if the shape is wrong, report it — the orchestrator re-spawns W0).

Read `doc/plans/PLAN-web-dashboard.md` § "Design decisions" first. The Rust sources named below
are the SEMANTICS you are porting: read them in full (they are short) and port line by line.

## Files you own (write)

- `web/src/api/ws.ts`, `web/src/api/ws.test.ts`, `web/src/api/schema.test.ts`
- `web/src/state/store.ts`, `web/src/state/atoms.ts`, `web/src/state/apply.ts`, `web/src/state/atoms.test.ts`
- `web/src/lib/format.ts`, `web/src/lib/levels.ts`, `web/src/lib/activity.ts`, `web/src/lib/format.test.ts`, `web/src/lib/levels.test.ts`, `web/src/lib/activity.test.ts`
- `web/src/test/setup.ts`

Read-only: `web/src/api/schema.ts`, `web/src/api/fixtures/snapshot.json`, `web/vite.config.ts`;
Rust semantics: `loom/src/utils.rs:38-46` (`format_elapsed`),
`loom/src/commands/status/ui/tui/ledger/cells.rs:42-345`,
`loom/src/orchestrator/monitor/context.rs:25-36` (context bands),
`loom/src/commands/status/ui/tui/state.rs:48-165` (ordering + activity log),
`loom/src/plan/graph/levels.rs` (`compute_level`, `compute_all_levels`),
`loom/src/models/stage/types.rs:986-1096` (icon/label/colour per status),
`loom/src/commands/status/ui/tui/ledger/legend.rs:14-66`,
`loom/src/commands/status/render/attention_model.rs:96-111` (`failure_label`),
`loom/src/commands/status/ui/tui/ledger/header.rs:80-104,173-200` (progress percentage, daemon spans).

W3 (the design worker) imports ONLY the exports pinned below, by these exact names and
signatures. Do not rename, do not add default exports.

## `web/src/test/setup.ts`

```ts
import "@testing-library/react";   // registers nothing else; jsdom comes from vite.config.ts test.environment
```

If `@testing-library/react` needs `afterEach(cleanup)` under vitest without globals, add
`import { cleanup } from "@testing-library/react"; import { afterEach } from "vitest"; afterEach(cleanup);`.

## `web/src/state/store.ts`

```ts
import { createStore } from "jotai/vanilla";
export const store = createStore();
export type Store = ReturnType<typeof createStore>;
```

## `web/src/state/atoms.ts`

```ts
import { atom } from "jotai/vanilla";
import type { Alert, Attention, Snapshot, StageStatus, StageSummary } from "@/api/schema";
import { orderStages, type OrderedStage } from "@/lib/levels";

export type ConnectionPhase = "connecting" | "live" | "reconnecting" | "error";
export interface ConnectionState { phase: ConnectionPhase; since: number; message?: string }
export interface ActivityEntry { at: number; stageId: string; status: StageStatus; message: string }

export const snapshotAtom = atom<Snapshot | null>(null);
export const connectionAtom = atom<ConnectionState>({ phase: "connecting", since: 0 });
export const activityLogAtom = atom<ActivityEntry[]>([]);           // oldest first, at most 20
export const orderedStagesAtom = atom<OrderedStage[]>((get) => { const s = get(snapshotAtom); return s ? orderStages(s.status.stages) : []; });
export const attentionAtom = atom<Attention[]>((get) => get(snapshotAtom)?.attention ?? []);
export const alertsAtom = atom<Alert[]>((get) => get(snapshotAtom)?.alerts ?? []);
export function selectStage(snapshot: Snapshot | null, id: string): StageSummary | undefined
```

## `web/src/state/apply.ts`

```ts
import type { Snapshot } from "@/api/schema";
import type { Store } from "@/state/store";
/// Store the frame and append the activity transitions it implies (previous snapshot vs next).
export function applySnapshot(store: Store, next: Snapshot, now: number = Date.now()): void
```

Reads `snapshotAtom` (previous), sets it to `next`, then
`store.set(activityLogAtom, appendTransitions(store.get(activityLogAtom), prev, next, now))`.

## `web/src/lib/levels.ts` — port of `plan/graph/levels.rs` + `state.rs:59-82`

```ts
export interface OrderedStage { stage: StageSummary; level: number }
/// level = 0 without dependencies, else 1 + max(level of each dependency present in `stages`); a dependency that is not in the list contributes 0; a cycle contributes 0 for the stage that closes it.
export function computeLevels(stages: readonly StageSummary[]): Map<string, number>
/// Deduplicate by id (first wins), then sort by level ascending, then id ascending (plain string comparison, like Rust's `String::cmp`).
export function orderStages(stages: readonly StageSummary[]): OrderedStage[]
```

## `web/src/lib/activity.ts` — port of `state.rs:111-165`

```ts
export const MAX_ACTIVITY_ENTRIES = 20;
/// Append one entry per stage whose status differs from `prev` (or is new when prev is null): executing → "<id> started", completed → "<id> completed", blocked → "<id> blocked", queued → "<id> ready", needs-handoff → "<id> needs handoff"; every other status records nothing but still counts as the new previous status. Keep the newest 20, oldest first.
export function appendTransitions(log: readonly ActivityEntry[], prev: Snapshot | null, next: Snapshot, now: number): ActivityEntry[]
```

The Rust log compares against the last SEEN status per stage, not the last LOGGED one; a
stage going executing → waiting-for-input → executing logs "started" twice. Mirror that: derive
the previous status map from `prev.status.stages` (the last applied snapshot).

## `web/src/lib/format.ts` — pure ports of the TUI cell logic

```ts
export type Tone = "executing" | "completed" | "blocked" | "pending" | "queued" | "warning" | "merged" | "dimmed" | "neutral";
export interface StateMeta { icon: string; label: string; tone: Tone; bold: boolean }

/// `30s`, `1m30s`, `1h1m` — utils.rs:38-46, the same integer arithmetic (Math.trunc, not Math.floor).
export function formatElapsed(seconds: number): string
/// Icon (the exact Unicode glyph the TUI uses), short label and tone per status — types.rs:986-1096. Order and glyphs: waiting-for-deps ○ Waiting pending; queued ▶ Queued queued; executing ● Executing executing; waiting-for-input ? Input warning(bold); blocked ✗ Blocked blocked; completed ✓ Completed completed; needs-handoff ⟳ Handoff warning; skipped ⊘ Skipped dimmed; merge-conflict ⚡ Conflict warning; completed-with-failures ⚠ Failed blocked; merge-blocked ⊗ MergeBlk blocked; needs-human-review ⏸ Review warning(not bold); needs-adjudication ⚖ Adjudicate warning. (The TUI colours Input and Review magenta; map both to "warning".)
export function stateMeta(status: StageStatus): StateMeta
/// The 13 legend rows in the TUI's order with the TUI's exact text — legend.rs:14-66.
export const LEGEND: ReadonlyArray<{ status: StageStatus; meaning: string }>
/// attention_model.rs:96-111
export function failureLabel(type: FailureType): string
/// cells.rs:52-92 + 216-289. Returns null for statuses the TUI leaves blank. `held` prefixes "held · ". Working with a tool: "working · <tool>"; idle/stale: "<prefix> <formatElapsed(staleness)>" when staleness is known; blocked: "<failureLabel or error> <retry>/<max or 3>"; completed-with-failures: "failed <retry>/<max or 3>"; needs-adjudication: "dispute <n> · judge <none|working|stale>" (working when heartbeat ≤ 300 s); executing + incoherence → "incoherent".
export function activityText(stage: StageSummary): { text: string; tone: Tone } | null
/// cells.rs:94-116 + context.rs:25-36. null unless status is executing | waiting-for-input | needs-handoff and both tokens and a non-zero ceiling exist. percent rounds half away from zero like Rust's f64::round; filled = floor(ratio*5) clamped to 0..5; health: red at ratio ≥ 0.90, yellow at ≥ 0.60, else green.
export function contextUsage(stage: StageSummary): { tokens: number; ceiling: number; percent: number; filled: number; health: "green" | "yellow" | "red" } | null
/// cells.rs:118-131 without width truncation: { model, execution: ["sonnet", "gpt-5.6-terra"] }.
export function modelsOf(stage: StageSummary): { model: string; execution: string[] }
/// cells.rs:301-316: null for waiting-for-deps | queued; otherwise formatElapsed(execution_secs ?? elapsed_secs) or "" when both are null.
export function timeText(stage: StageSummary): string | null
/// cells.rs:318-345: completed+cleanup_warning → "cleanup!" warning; completed (non-knowledge) merged → "merged" merged; completed (non-knowledge) unmerged → "unmerged" warning; merge-conflict → "conflict" warning; merge-blocked → "error" blocked; else null.
export function mergeText(stage: StageSummary): { text: string; tone: Tone } | null
/// header.rs:197-202: completed*100 + total/2, integer-divided by total; 0 when total is 0.
export function progressPercent(completed: number, total: number): number
/// header.rs:173-188 + DaemonState: running/unreachable with age ≥ 60 → { text: "loop stalled <age>s", tone: "warning" }; running/unreachable with an age → { text: "daemon running", detail: "tick <age>s ago", tone: "completed" }; age null → detail "tick unknown"; process-only → { text: "daemon process alive, socket missing", tone: "warning" }; not-running → { text: "daemon stopped", tone: "dimmed" }.
export function daemonLine(daemon: DaemonState, tickAgeSecs: number | null): { text: string; detail?: string; tone: Tone }
/// header.rs:106-135 counts: executing, queued, waiting (waiting-for-deps), attention (= attention.length), done (completed).
export function summaryCounts(stages: readonly StageSummary[], attentionCount: number): { executing: number; queued: number; waiting: number; attention: number; done: number }
```

## `web/src/api/ws.ts`

```ts
import type { Store } from "@/state/store";
export interface SocketDeps { WebSocket?: typeof WebSocket; fetch?: typeof fetch; location?: { protocol: string; host: string }; setTimeout?: typeof setTimeout; clearTimeout?: typeof clearTimeout }
/// Fetch /api/status once, open ws(s)://<host>/ws, apply every valid frame with applySnapshot, and reconnect with backoff (1 s, 2 s, 4 s, 8 s, then 10 s; reset on open). Returns a disposer that closes the socket and stops reconnecting.
export function connectStatusSocket(store: Store, deps: SocketDeps = {}): () => void
```

Behaviour:

- `connectionAtom` transitions: `connecting` at start → `live` on `open` → `reconnecting` on
  `close`/`error` (with `message` = the close reason or "socket error") → `live` again on the next
  `open`. A frame that fails `snapshotSchema.safeParse` sets `{ phase: "error", message: <first zod issue path + message> }`
  and logs `console.error("dashboard: bad frame", issues)` but keeps the socket open (the next
  frame may parse).
- URL: build it with exactly this template literal, `` `${scheme}://${host}/ws` `` where
  `scheme` is `"wss"` for `https:` and `"ws"` otherwise (a wiring check greps for `${host}/ws`
  in this file); the snapshot fetch is `/api/status`
  with `{ cache: "no-store" }`; a fetch failure is logged and ignored (the socket delivers the
  first frame anyway).
- `deps` exist for tests: default to the globals.

## Tests (vitest, `*.test.ts` next to the module)

- `schema.test.ts` — the fixture parses (`snapshotSchema.parse(fixture)`), has 7 stages, and
  `stageStatusSchema.options.length === 13`; a fixture copy with `status.stages[0].status = "bogus"`
  fails; a copy without `cleanup_warning` keys still parses.
- `levels.test.ts` — from the fixture: knowledge-bootstrap 0; server, client, docs 1; design 2;
  integration-verify 3; knowledge-distill 4; `orderStages` yields ids in the order
  `knowledge-bootstrap, client, docs, server, design, integration-verify, knowledge-distill`
  (level, then id); a self-cycle `a → a` gives 0; a missing dependency gives 0.
- `activity.test.ts` — applying the fixture to an empty log yields exactly two entries,
  `knowledge-bootstrap completed` and `server started` (the other five statuses in the fixture
  are not in the Rust table: `completed-with-failures`, `waiting-for-deps`, `merge-conflict`,
  `needs-human-review` log nothing); a second snapshot where `design` becomes `queued` appends
  `design ready`; unchanged statuses append nothing; 25 transitions keep the newest 20.
- `format.test.ts` — `formatElapsed(30) === "30s"`, `(90) === "1m30s"`, `(3660) === "1h1m"`;
  `contextUsage(server)` → percent 39, filled 1, green (312000/800000 = 0.39; floor(1.95) = 1);
  a 0.60 ratio is yellow and a 0.90 ratio red; `activityText(server)` → "working · Bash";
  `activityText(client)` → "failed 1/3"; `activityText(integration-verify)` → "held · awaiting you";
  `mergeText(docs)` → conflict; `timeText(design) === null`; `progressPercent(1, 7) === 14`
  and `progressPercent(2, 3) === 67`; `daemonLine("running", 75).text === "loop stalled 75s"`;
  `stateMeta("merge-conflict").icon === "⚡"`; `LEGEND.length === 13`.
- `atoms.test.ts` — `applySnapshot(store, fixture)` sets `snapshotAtom`, `orderedStagesAtom`
  has 7 rows, `attentionAtom` has 3 entries (client, docs, integration-verify), `activityLogAtom`
  has 2 entries.
- `ws.test.ts` — a fake `WebSocket` class (constructor records the URL, exposes `onopen`,
  `onmessage`, `onclose`, `onerror`, `close()`), a fake `fetch` returning the fixture: after
  `connectStatusSocket(store, deps)`, `connectionAtom` is `connecting`; firing `onopen` → `live`;
  `onmessage({ data: JSON.stringify(fixture) })` → `snapshotAtom` set; a bad frame → `error`
  phase and the previous snapshot kept; `onclose` → `reconnecting` and a new socket is created
  after the first backoff (use a fake `setTimeout` that records delays: 1000, 2000, 4000, 8000,
  10000, 10000); the disposer closes the socket and cancels the pending timer. The URL built from
  `{ protocol: "http:", host: "127.0.0.1:7373" }` is `ws://127.0.0.1:7373/ws`.

## Done means

- Your one check: `cd web && bunx vitest run` — all of the above green. Do not run typecheck,
  lint, format or build (the orchestrator does). Run `bunx oxfmt` once over your files before
  returning so the format gate does not fail on them.
- Report: files created, the vitest summary line, and every place the Rust semantics forced a
  choice this brief did not spell out (with the file:line you followed).
