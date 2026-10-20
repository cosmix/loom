# F1 — terminal view: the window cut into the ledger (fable, `frontend-design` skill)

You own the visual work: the terminal view inside the stage dialog and on its own page, the four
states it can be in, the entry glyph on cards and rows, and the CSS. F2 owns the transport, the
xterm adapter and the hook you render against; F3 writes the component tests after you both
return. Load `frontend-design:frontend-design` first.

The approved mock is `doc/plans/briefs/web-terminal/web-terminal/mockup.html` — open it in a
browser (or read it) and match it: it is the design, not a suggestion. Its state switcher is
mock-only chrome; everything else is what ships.

## The concept

Everything on the dashboard is paper describing the agent; the terminal is the one place you see
the agent itself, so it reads as a pane of glass cut into the card with something live behind it.
The well is always dark, in both themes (`oklch(0.17 0.012 255)` light, `oklch(0.13 0.012 255)`
dark): a lightbox set into paper. The glass is the mode: viewing shows a diagonal sheen, a hollow
cursor and a hairline frame; taking control lifts the sheen over 300 ms, fills the cursor, and
lights the frame in the stage's `--tone` as a 1 px ring plus a 2 px bar along the bottom edge.

## Files

| Own (write) | Read only |
| --- | --- |
| `web/src/components/terminal/terminal-view.tsx` | `web/src/components/terminal/use-terminal.ts`, `emulator.ts`, `web/src/api/terminal.ts` (F2 — interfaces below) |
| `web/src/components/terminal/terminal-glyph.tsx` | `web/src/components/stage-detail.tsx`, `stage-heading.tsx` (`StateLine`), `state-badge.tsx` (`toneClass`), `copy-command.tsx` (`CopyCommand`), `ui/kbd.tsx`, `ui/button.tsx` |
| `web/src/components/terminal/terminal.css` | `web/src/aurora-ui/shared/styles/tokens.css` (semantic tokens on `:root` and `.dark`) |
| `web/src/components/stage-modal.tsx` | `web/src/graph.css` (`.stage-node`, `.stage-open`, `.thread-chip`) |
| `web/src/components/graph/stage-node.tsx`, `graph/context.tsx`, `graph/stage-graph.tsx` | `web/src/lib/format.ts` (`stateMeta`) |
| `web/src/components/ledger-row.tsx` | |
| `web/src/routes/terminal.tsx`, `web/src/router.tsx` | |
| `web/src/routes/stage.tsx` (ONLY to `export` its `NotFound`) | |
| `web/src/index.css` (two `@import` lines only, placed after line 15) | |

Do NOT run `bun add` (F2 installs `@xterm/*`) and do not write tests (F3). Do not edit
`web/src/api/schema.ts` (F2 adds `terminals: z.boolean()` to `snapshotSchema`; read
`snapshot.terminals` as if it were already there).

## F2's interfaces (code against these exactly)

```ts
// web/src/components/terminal/use-terminal.ts
export type TerminalMode = "view" | "control";
export type TerminalPhase = "connecting" | "waiting" | "live" | "ended" | "dropped" | "refused";
export interface TerminalState { phase: TerminalPhase; mode: TerminalMode; reason?: string; since: number }
export interface EmulatorSize { cols: number; rows: number }
export type EmulatorFactory = () => Promise<Emulator>;   // from ./emulator.ts
export interface TerminalDeps { WebSocket?: typeof WebSocket; /* timers, location — see F2 */ }
export function useTerminal(stageId: string, mode: TerminalMode, factory?: EmulatorFactory, deps?: TerminalDeps): {
  hostRef: RefObject<HTMLDivElement | null>;  // attach to the well's inner div
  state: TerminalState;
  size: EmulatorSize | null;                    // current cols×rows after fit
  retry: () => void;                            // reconnect now (Reconnect / Retry buttons)
  focus: () => void;                            // focus the emulator (call when entering control)
}
```

`use-terminal.ts` re-exports every type above (`TerminalMode`, `TerminalPhase`, `TerminalState`,
`TerminalDeps`, `Emulator`, `EmulatorFactory`, `EmulatorSize`), so import all of them from
`./use-terminal` and nowhere else.

The hook opens the socket while mounted, reconnects on its own when the server answers "not yet"
(`waiting`, every 2 s), and tears down on unmount or when `stageId`/`mode` change. Export the
exact component shape so F3 and `routes/terminal.tsx` agree:
`export interface TerminalViewProps { stage: StageSummary; frame: "dialog" | "page"; factory?:
EmulatorFactory; deps?: TerminalDeps }` and `export function TerminalView(props:
TerminalViewProps): ReactElement`. `factory` and `deps` pass straight through to the hook: F3's
tests inject a fake emulator and a fake socket through them.

## Behaviour

**URL.** The dialog already lives at `?stage=<id>`. Add `STAGE_VIEW_PARAM = "view"`; `?stage=<id>
&view=terminal` shows the terminal view. Export `useOpenTerminal(): (id: string) => void` next to
`useOpenStage` (sets both params). Back returns to details, back again closes. Mode (view/control)
is component state, never in the URL: a reload always lands in view.

**Dialog sizing.** In terminal view `DialogContent` takes `sm:max-w-[min(96vw,1280px)]` and a
fixed height `h-[min(86dvh,860px)]`, grid rows `auto 1fr auto`; the details view is unchanged.

**Header.** Left: eyebrow `Terminal`, name in the dialog-title size, id in mono, then the
existing `StateLine` (badge, roundel, time) and the worktree/branch line. Right: a two-segment
`radiogroup` `View` / `Take control` built from the badge shapes; under it the connection word in
mono with the `dot-live` pulse: `connecting`, `viewing`, `controlling`, `ended`, `waiting`,
`dropped`. Hovering `Take control` shows a tooltip: `Keystrokes reach a running autonomous agent`.
The dialog's close button stays where it is.

**Well.** `<div class="terminal-well" data-mode={mode} data-phase={state.phase}>` — two
attributes, not one: `data-mode` is only `view`/`control`, `data-phase` is the six
`TerminalPhase` values (the mock's single `data-mode` on `.dialog` with five values,
`mockup.html:286-351`, cannot represent "control AND ended"). Port the mock's
`[data-mode="ended"]`/`[data-mode="waiting"]` selectors (`mockup.html:298-329`) to
`[data-phase="…"]`; keep `[data-mode="view"|"control"]` for the sheen, cursor and frame. Mirror
both attributes on the dialog content so the mock's `.seg`/`.hint` selectors keep working. The
well holds the sheen, the bottom bar, the `hostRef` div (xterm mounts here), the corner stamp, and
the waiting block. Import `@xterm/xterm/css/xterm.css` and then `./components/terminal/terminal.css`
as lines 16-17 of `web/src/index.css`, immediately after `@import "./graph.css";` (:15) and BEFORE
the first rule at `:root` (:32) — CSS discards an `@import` that follows any rule, and a misplaced
one still satisfies the plan's `xterm\.css` wiring grep while the terminal renders unstyled.
terminal.css comes after xterm.css so the well palette wins. (F2 installs the package; the line
resolves once `bun install` has run.)

**Modes and keys.**

- view: `useTerminal(id, "view")`; sheen on; `Esc` returns to details (the dialog's
  `onEscapeKeyDown` calls `preventDefault` and clears the `view` param).
- control: `useTerminal(id, "control")`; sheen lifts; call `focus()`; `onEscapeKeyDown`,
  `onInteractOutside` and `onPointerDownOutside` all `preventDefault` — every key including Esc
  goes to the agent, and a stray click outside must not drop the session. Leaving is the close
  button or the browser back button; the footer says so. Radix's focus trap
  (`@radix-ui/react-focus-scope/dist/index.mjs:107-132`, mounted `trapped` by
  `DialogPrimitive.Content`) hijacks Tab/Shift+Tab and does NOT honour `defaultPrevented`, so
  xterm never sees them and shell completion silently does nothing.

  The fix is a **bubble-phase `onKeyDown`** on the terminal well — NEVER `onKeyDownCapture`.
  Radix's handler is an ordinary bubble handler on the FocusScope div
  (`index.mjs:107,131`: `onKeyDown: handleKeyDown`), and xterm's own listener sits on its
  `<textarea class="xterm-helper-textarea">`, i.e. on the TARGET. Capture runs root→target, so a
  capture handler on an ancestor that calls `stopPropagation()` stops the key BEFORE xterm ever
  sees it — it would suppress exactly the thing it is meant to protect, and a test that only
  checks "focus did not move" passes anyway. Bubble runs target→root: xterm's listener fires
  first, then the well's handler stops the event before it reaches the dialog content. So: while
  `mode === "control"` and the event target is inside `hostRef.current`, call
  `event.stopPropagation()` in `onKeyDown`.

  Esc is unaffected either way: DismissableLayer listens on the DOCUMENT in CAPTURE phase
  (`@radix-ui/react-dismissable-layer/dist/index.mjs:105`), so it always runs first, and control
  mode's `onEscapeKeyDown` + `preventDefault` is what stops the dismiss. Keep it.

  F3 proves this behaviourally: Tab in control mode must leave focus inside the well AND reach
  the transport (the fake socket receives the encoded byte). An acceptance line runs
  `vitest -t "tab in control mode"`, and another asserts `onKeyDownCapture` appears NOWHERE in
  `terminal-view.tsx`.
- `t` in the details view opens the terminal view when the glyph condition (below) holds and the
  event target is not an input/textarea/contenteditable. `shell.tsx` is not yours: copy the
  four-line `isTyping` guard from `web/src/routes/shell.tsx:55-58` into `stage-modal.tsx` rather
  than exporting it from there. (The existing global `?` handler at `shell.tsx:28-33` uses the
  same guard, and xterm's `<textarea class="xterm-helper-textarea">` satisfies it, so `?` typed in
  control mode does not open the legend — no change needed there.)
- `refused` covers BOTH permanent close codes: 4004 (no such stage) and 4008 (native backend,
  unrenderable identifier). 4009 is the retryable one and renders as `waiting`, never as
  `refused` — so the native-backend case really does land in the refused view the paragraph below
  promises, which it could not under a two-code scheme.
- `refused` with reason `handshake refused` (F2 reports this when the socket closes before it
  ever opened — the server's 404/403/401 refusals carry no status into the browser): render, in
  the well, `Start the dashboard with loom status --web --terminals` when `snapshot.terminals` is
  false, else `Dashboard cookie missing — open the tokenized URL printed when the dashboard
  started`. Other `refused` reasons render `state.reason` as they are.

**Lifecycle rendering.**

- `connecting`: well empty, connection word `connecting`.
- `waiting`: the empty well with one centred mono line `waiting for the session's tmux server`
  and the pulse, then `state.reason` (the server's wording) and `retrying in 2s`.
- `live`: the screen. Footer left in view: `Esc back to details · screen <cols> × <rows>`; in
  control: `every key reaches the agent · close with ✕ or the back button` in the executing tone.
- `ended`: the emulator stays mounted (last screen kept), `filter: grayscale(0.6) opacity(0.85)`
  over 600 ms, stamp top-right `ended` in the warning tone; when the snapshot later shows
  `session_alive` for this stage again, the stamp gains a `Reconnect` button calling `retry()`.
- `dropped`: stamp `connection dropped` with `Retry`.
- `refused`: the well shows `state.reason` centred (unknown stage → 4004; native backend or an
  unrenderable identifier → 4008); segmented control disabled. Nothing here retries.
- The `waiting` block above is where every 4009 lands: no live session yet, still spawning,
  endpoint not ready. Its reason text comes from the server; the retry cadence is F2's 2 s.

**Footer right.** `CopyCommand` with `loom attach <stage-id>`; `open in a tab` linking to
`/terminal/<id>` (encode with the same `encodeURIComponent` `web/src/components/stage-href.ts`
uses).

**Entry points.** `TerminalGlyph({ stage, onOpen })`: a literal `>_` in mono at the eyebrow size
tinted with the stage's tone, `dot-live`-style pulse, `aria-label="open terminal for <name>"`.
Shown only when `snapshot.terminals && stage.session_backend === "tmux" && stage.session_alive`.
Place it in the card header after the badge (`stage-node.tsx:71-98`, class `nodrag` like the
open button) and in the ledger name cell next to the held/incoherent badges
(`ledger-row.tsx:103-120`, with `event.stopPropagation()` like the links there). Extend
`GraphActions` with `openTerminal: (id: string) => void` (`graph/context.tsx`) and provide it
from `stage-graph.tsx` where `open` is provided. The dialog header's `Terminal` button is the
primary entry: enabled under the same condition; otherwise disabled with a tooltip reason —
`Start the dashboard with loom status --web --terminals` / `This stage's session runs in a native
terminal window. Terminals need loom run --backend tmux.` / `No live session`.

**Full page.** `routes/terminal.tsx` at `terminal/:stageId`: the same `TerminalView` (with
`frame: "page"`) in a page frame with the back link pattern of `routes/stage.tsx`; unknown stage
renders that route's `NotFound` — it is module-private at `stage.tsx:45`, so `export` it there
(a one-word edit; `routes/stage.tsx` has no test of its own) rather than duplicating it.

## Look

- Type: IBM Plex Mono 13 px / 1.4 for the emulator (F2 sets the emulator font; you set the well's
  fallback and the header/footer type in Inter as the rest of the sheet).
- Palette: fixed, since the well is single-theme: ground `oklch(0.17 0.012 255)` /
  `.dark oklch(0.13 0.012 255)`, rule `oklch(1 0 0 / 0.08)`, sheen `linear-gradient(115deg,
  transparent 30%, oklch(1 0 0 / 0.06) 46%, transparent 58%)` (`0.045` alpha in dark). Frame and
  cursor use `var(--tone-executing)`.
- Motion: dialog keeps its 100 ms zoom; the well fades in and header items stagger 40 ms (the
  graph's `--i` idea); sheen lift 300 ms; grayscale drift 600 ms. All disabled under
  `prefers-reduced-motion` alongside the existing `.row-wash`/`.dot-live` rule in `index.css`.
- Everything through tokens (`--popover`, `--muted`, `--hairline`, `--tone-*`); never a literal
  colour outside `terminal.css`'s well palette.

## Constraints

- Match the existing code's shape: functional components, `cn` from `"cn"`, `jotai/react`
  `useAtomValue`, `lucide-react` icons, `import type` under `verbatimModuleSyntax`.
- You share `web/` with F2, who runs at the same time and installs `@xterm/*`. Do NOT run
  `bun run lint`, `bun run format:check`, `bun run typecheck` or `bun run check` — all are
  project-wide and will trip over F2's in-flight files; the orchestrator runs them after both of
  you return. You may run `bun run format` on your OWN files ONCE (oxlint defaults, oxfmt
  defaults apply to what you write).
- `web/tsconfig.app.json` sets `erasableSyntaxOnly` (no `enum`, no parameter properties),
  `noUnusedLocals`/`noUnusedParameters`, and has no `@types/node` globals.
- No `TODO`/`FIXME`. Never run `git`, never `bun add`, never `bun install`.
- Record design decisions you made beyond this brief with `loom memory decision`.

## Done means

Your files exist and type-check against F2's interfaces. If F2's files are not in the tree yet,
do not write a stand-in `use-terminal.ts`; code against the block above and report the missing
import, and the orchestrator integrates. Report: files changed, any deviation from the mock and
why.
