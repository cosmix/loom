# F2 — transport, emulator adapter, hook, schema (codex `gpt-5.6-sol`, effort xhigh)

You own the non-visual half of the terminal view: installing xterm.js, the emulator adapter that
hides it behind one interface, the WebSocket client with its close-code state machine, the React
hook the view renders against, and the schema change. F1 renders; F3 tests the components. Your
own unit tests cover the transport and the adapter's pure parts.

## Files

| Own (write) | Read only |
| --- | --- |
| `web/package.json`, `web/bun.lock` (ONLY via `bun add`, task 1) | `web/src/api/ws.ts` (the socket pattern: `SocketDeps` :6-12, superseded-socket guard :87-90, `BACKOFF_MS` :14) |
| `web/src/api/terminal.ts`, `web/src/api/terminal.test.ts` | `web/src/api/ws.test.ts` (how a fake `WebSocket` is injected) |
| `web/src/components/terminal/emulator.ts`, `emulator.test.ts` | `web/src/state/atoms.ts` (`snapshotAtom`, `selectStage`) |
| `web/src/components/terminal/use-terminal.ts` | `web/src/api/fixtures/snapshot.json` (S2 adds `"terminals": false`) |
| `web/src/api/schema.ts`, `web/src/api/schema.test.ts` | |
| `web/README.md` (one paragraph under Views) | |

## Task 1 — install (first, once)

From `web/`: `bun add @xterm/xterm@6.0.0 @xterm/addon-fit@0.11.0 @xterm/addon-webgl@0.19.0
@xterm/addon-unicode11@0.9.0` (versions read from the npm registry on 2026-09-08; all four ship
their own `typings/*.d.ts`; none of the addons declares `peerDependencies`, so nothing checks the
xterm-6 pairing — install those four exact versions, never a range). Then `bun install
--frozen-lockfile` must succeed. Never hand-edit `package.json`. You share `web/` with F1, who
runs at the same time: run `bun add` ONCE at the start and then no other `bun install`/`bun add`
(two concurrent bun processes over one `node_modules` corrupt it).

## Task 2 — schema

`web/src/api/schema.ts::snapshotSchema` (:143-155, `.strict()`, so a missing schema field makes
zod REJECT every frame and the page renders blank): add `terminals: z.boolean()` after `source`.
`schema.test.ts`: the fixture parses and `terminals` is `false`; a frame without `terminals`
fails to parse.

## Task 3 — emulator adapter (`emulator.ts`)

```ts
export interface EmulatorSize { cols: number; rows: number }
export interface Emulator {
  open(host: HTMLElement): void;
  write(data: Uint8Array): void;
  /** Keystrokes from the user (already encoded by xterm). */
  onData(handler: (data: string) => void): () => void;
  /** Recompute cols/rows from the host box; null before open. `FitAddon.fit()` returns void
   *  (addon-fit typings:33) — call it, then read `terminal.cols`/`terminal.rows`. */
  fit(): EmulatorSize | null;
  focus(): void;
  /** view = stdin disabled, hollow bar cursor, no blink; control = block cursor, blink. */
  setReadOnly(readOnly: boolean): void;
  dispose(): void;
}
export type EmulatorFactory = () => Promise<Emulator>;
/** Lazily imports @xterm/* so the main bundle does not carry them. */
export const createXtermEmulator: EmulatorFactory;
/** The dark-well palette xterm needs as concrete colours. Pure; tested. */
export function terminalTheme(): ITheme-shaped object (type it locally; do not import xterm types at module top level)
```

`createXtermEmulator` does `await Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit"),
import("@xterm/addon-webgl"), import("@xterm/addon-unicode11")])` inside the factory so Vite emits
a separate chunk; build with `fontFamily: '"IBM Plex Mono", monospace'`, `fontSize: 13`,
`lineHeight: 1.4`, `allowProposedApi: true`, `cursorStyle: "bar"`, `scrollback: 2000`,
`theme: terminalTheme()` (`allowProposedApi: true` is REQUIRED: `IUnicodeHandling` is marked
experimental in `xterm.d.ts:1884`); load unicode11 and set `unicode.activeVersion = "11"`; load
fit; try the WebGL addon and on `onContextLoss` dispose it (xterm falls back to its DOM renderer
by itself — there is no canvas addon in xterm 6). The palette is
hex, dark-well only (xterm cannot parse oklch): background `#1c1f27`, foreground `#e3e5ea`,
cursor `#8fb4f0`, selectionBackground `#8fb4f04d`, black `#1c1f27`, red `#e07a6f`, green
`#7fc79a`, yellow `#d9b96a`, blue `#8fb4f0`, magenta `#c59bd9`, cyan `#7fc3cf`, white `#c8cbd3`,
bright variants one step lighter. Record the mapping to the dashboard's `--tone-*` tokens in a
comment (blue = executing, green = completed, red = blocked, yellow = warning, cyan = queued).

## Task 4 — transport (`terminal.ts`)

```ts
export type TerminalMode = "view" | "control";
export type TerminalPhase = "connecting" | "waiting" | "live" | "ended" | "dropped" | "refused";
export interface TerminalState { phase: TerminalPhase; mode: TerminalMode; reason?: string; since: number }
export interface TerminalDeps { WebSocket?: typeof WebSocket; location?: { protocol: string; host: string }; setTimeout?; clearTimeout?; now?: () => number }
export class TerminalConnection {
  constructor(stageId: string, mode: TerminalMode, sink: (bytes: Uint8Array) => void, onState: (state: TerminalState) => void, deps?: TerminalDeps)
  /** Keystrokes. MUST go out as a BINARY frame: `this.socket.send(new TextEncoder().encode(data))`. */
  send(data: string): void;          // ignored in view mode
  resize(size: EmulatorSize): void;  // text frame {"resize":{"cols","rows"}}
  retry(): void;                     // reconnect now, resetting attempts
  close(): void;                     // idempotent; cancels timers
}
export const RETRY_WAITING_MS = 2000;
export const RETRY_DROPPED_MS = 1000;
```

URL: `${ws|wss}://${host}/ws/terminal/${encodeURIComponent(stageId)}/${mode}`, `binaryType =
"arraybuffer"`. Frames: binary → `sink(new Uint8Array(data))`; text frames are ignored (reserved).

**`send` encodes before it sends.** `WebSocket.send` accepts a `string` and then transmits a TEXT
frame (`lib.dom.d.ts`: `send(data: string | ArrayBufferLike | Blob | ArrayBufferView)`), and the
server parses text frames ONLY as resize JSON — every keystroke would be dropped with no error
anywhere on either side, and no view-mode test would notice. So `send(data)` is
`this.socket.send(new TextEncoder().encode(data))`, always, and `resize` stays the one text frame
(`JSON.stringify({ resize: { cols, rows } })`). Test both directions explicitly: assert the fake
socket received a `Uint8Array` whose bytes equal the UTF-8 encoding of the input, for an ASCII
payload AND for a non-ASCII one (e.g. `"é"` → `[0xc3, 0xa9]`, two bytes, not one).

State machine on `onclose`:

- code 1000 → `ended`, reason from the frame.
- 4004 → `refused` with reason; no retry (no such stage).
- 4008 → `refused` with reason; no retry (native backend, unrenderable identifier — permanent).
- 4009 → `waiting` with reason; schedule `openSocket` after `RETRY_WAITING_MS`, indefinitely while
  not closed (no live session yet, still spawning, endpoint not ready — all transient).
- 1001 → `dropped`, reason `server stopping`; no automatic retry.
- a close (1006) BEFORE the socket ever opened → `refused` immediately, no retry, reason
  `handshake refused`. The server's pre-accept refusals (404 terminals disabled, 403 origin, 401
  cookie) all reach the browser as a bare `error` + `close` with no status, so this is the only
  signal the page gets; F1 turns it into a message using `snapshot.terminals` (no new wire field).
- anything else (1006 etc.) after a successful open → first time: retry once after
  `RETRY_DROPPED_MS`; second consecutive failure → `dropped`.

`onopen` → `live` (attempts reset). Every handler checks the socket is still the current one
(`ws.ts:87-90`). `since` is the time of the last PHASE CHANGE, not of the last frame — pin the
meaning in a doc comment (see mistakes/web-dashboard-server "since" lesson).

`terminal.test.ts` with an injected fake `WebSocket` (mirror `ws.test.ts`'s fake). Two of these
carry acceptance lines and must be NAMED so `vitest -t` finds them: a test whose name contains
**`close code table`** — a table driving `[1000, 1001, 4004, 4008, 4009, 1006]` to their phases and
to whether a retry was scheduled, the mirror of `refusal_close_code_table` on the Rust side — and
one whose name contains **`encodes keystrokes as binary`** (the ASCII + non-ASCII byte assertions
above). Plus: live on open; waiting retries after 2000 ms via the injected timers; the second 1006
lands in dropped; a 1006 before any open lands in refused with no retry scheduled; `send` in view
mode sends nothing; `resize` sends the exact JSON; `close` cancels a pending retry; a superseded
socket's late close is ignored.

## Task 5 — hook (`use-terminal.ts`)

```ts
export function useTerminal(stageId: string, mode: TerminalMode, factory: EmulatorFactory = createXtermEmulator, deps: TerminalDeps = {}): {
  hostRef: RefObject<HTMLDivElement | null>; state: TerminalState; size: EmulatorSize | null; retry: () => void; focus: () => void;
}
```

Effect keyed on `[stageId, mode]`. `factory()` is ASYNCHRONOUS (it dynamically imports four
packages), so every action after the `await` needs a cancel guard — without one, unmounting or
switching stage/mode before the import resolves lets the stale continuation mount xterm into a
detached node and open a socket that nothing will ever close:

```ts
useEffect(() => {
  let cancelled = false;
  let emulator: Emulator | undefined;
  let connection: TerminalConnection | undefined;
  let observer: ResizeObserver | undefined;
  let fitTimer: ReturnType<typeof setTimeout> | undefined;

  factory().then(
    (created) => {
      if (cancelled || !hostRef.current) { created.dispose(); return; }   // late arrival: dispose it
      emulator = created;
      // ... open, setReadOnly, connect, observe
    },
    (error: unknown) => {
      if (cancelled) return;
      setState({ phase: "refused", mode, reason: "terminal renderer failed to load", since: now() });
    },
  );

  return () => {
    cancelled = true;
    if (fitTimer !== undefined) clearTimeout(fitTimer);   // the 100 ms debounce, always cancelled
    observer?.disconnect();
    connection?.close();
    emulator?.dispose();
  };
}, [stageId, mode]);
```

Four things that block, individually, are defects: a post-`await` action with no `cancelled`
check; a late emulator that is not disposed; an uncancelled debounce timer; a rejected `factory()`
left as an unhandled rejection with the UI stuck at `connecting`. Pass the sink as
`(bytes) => emulator.write(bytes)`, never the bare `emulator.write` method — this codebase has
already had real-browser "Illegal invocation" failures from unbound host methods and binds them
explicitly (`ws.ts:40-50`); an unbound adapter method is the same class of bug.

The connection is `new TerminalConnection(stageId, mode, (bytes) => emulator.write(bytes),
setState, deps)`; `onData` feeds `connection.send`; a `ResizeObserver` on the host calls `fit()`
debounced 100 ms and sends `resize` when cols/rows changed (and once right after `live`). On
`ended`/`dropped`/`refused` the emulator stays mounted so the last screen is visible.

Two tests prove this lifecycle and F3 writes them in `terminal-view.test.tsx` (you own no test
file for the hook): a factory whose promise is resolved manually AFTER unmount disposes the late
emulator and opens no socket — its name must contain **`unmount before the emulator resolves`**,
because an acceptance line runs `vitest -t` on exactly that — and a rejected factory landing in
`refused` with a reason and scheduling nothing. Leave the hook's `factory`/`deps` parameters
injectable so F3 can drive both without xterm; say so in your report. Re-export F1's entire contract from `use-terminal.ts`
verbatim, so F1 has one import site:
`export type { TerminalMode, TerminalPhase, TerminalState, TerminalDeps } from "@/api/terminal";`
and `export type { Emulator, EmulatorFactory, EmulatorSize } from "./emulator";`
(`verbatimModuleSyntax` requires the `export type` form). Do not rename any of them.

## Task 6 — README

`web/README.md`, under `## Views`: one paragraph naming the terminal view, that it needs
`loom status --web --terminals` and the tmux backend, view/control, and the `@xterm/*` chunk
being lazy-loaded.

## Constraints

- Do NOT run `bun run check`, `bun run typecheck`, `bun run lint` or `bun run test`: they are
  project-wide (`web/package.json`), F1 is writing into `web/src/` at the same time, and the
  orchestrator runs them after both of you return. Your only permitted verification is the single
  vitest command in "Done means", run ONCE; `bun run format` on your own files ONCE if needed.
  `verbatimModuleSyntax` → `import type`. No `strict` in the tsconfig chain, so add explicit null
  checks yourself where xterm's types return `undefined`.
- `web/tsconfig.app.json:24` sets `erasableSyntaxOnly: true`: NO parameter properties
  (`constructor(private x: T)`), NO `enum`, NO `namespace` — declare fields explicitly and assign
  in the constructor body. `noUnusedLocals`/`noUnusedParameters` (:22-23) are on, so every import
  and parameter must be used. `types` is `["vite/client"]` only — no `@types/node` globals, so
  type timer handles as `ReturnType<typeof setTimeout>` the way `ws.ts` does.
- Do not import `@xterm/*` at module top level anywhere; the dynamic import is the point.
- No `TODO`/`FIXME`. Never run `git`.
- `loom memory note` anything the installed xterm 6 API names differently from this brief.

## Done means

`cd web && bunx vitest run src/api/terminal.test.ts src/api/schema.test.ts
src/components/terminal/emulator.test.ts` passes (run ONCE) and reports at least TEN tests (the
suite baseline is 188 and the plan gates on ≥ 200; F3 supplies twelve more), `rg -q
'"@xterm/xterm"' web/package.json`, and the hook's exported types match the block above verbatim.
