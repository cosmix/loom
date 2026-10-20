# F3 — component tests and integration glue (codex `gpt-5.6-terra`, effort xhigh)

Wave 2: you start after F1 (view, glyph, dialog, routes) and F2 (transport, emulator, hook, schema)
are both in the tree. Read both briefs in this directory and the code they produced. You own the
tests that prove the view behaves, and the small fixes where F1's rendering and F2's interfaces
do not meet. You may make MINIMAL wiring-only edits to F1's and F2's files under `web/src/` —
a prop, an export, a test id — never under `web/src/aurora-ui/**` or `web/src/components/ui/**`.
Never restyle, re-lay-out or rewrite an F1 component to make a test pass: F1 is a fable design
matched to an approved mock, and a codex rewrite of it is invisible to the gate. When a test and
the rendering disagree beyond a wiring fix, report the mismatch, quoting file and assertion.

## Files

| Own (write) | Read only |
| --- | --- |
| `web/src/components/terminal/terminal-view.test.tsx` | `web/src/routes/shell.test.tsx:18-29` (the provider/router render helper to copy) |
| `web/src/components/terminal/terminal-glyph.test.tsx` | `web/src/api/ws.test.ts` (fake WebSocket) |
| `web/src/components/stage-modal.test.tsx` | `web/src/test/setup.ts` (existing jsdom stubs) |
| `web/src/routes/terminal.test.tsx` | |
| glue edits in `web/src/**` as above | |

## Test doubles

Never mount real xterm in jsdom. Build `fakeEmulator()` returning an `Emulator` that records
`open`/`write`/`setReadOnly`/`focus`/`dispose` calls and lets the test fire `onData`; pass it as
the `factory` argument (`useTerminal(stageId, mode, factory, deps)`). Build `fakeSocketClass()`
the way `ws.test.ts` does, exposing the last instance so a test can fire `onopen`, `onmessage`
with an `ArrayBuffer`, and `onclose` with a `code`/`reason`. `TerminalView`'s props are
`TerminalViewProps { stage: StageSummary; frame: "dialog" | "page"; factory?: EmulatorFactory;
deps?: TerminalDeps }` (exported from `terminal-view.tsx`) — if F1's component lacks `factory`/
`deps`, add them (a two-line change that threads into `useTerminal`). The well carries
`data-mode` (`view`/`control`) and `data-phase` (the six phases) as two separate attributes.
`web/tsconfig.app.json` has `erasableSyntaxOnly`, `noUnusedLocals`/`noUnusedParameters` and no
`@types/node` globals.

Render helper: copy `renderShell` from `shell.test.tsx`, with the fixture snapshot (parse
`@/api/fixtures/snapshot.json`, then set `terminals: true` and, on the stage under test,
`session_backend: "tmux"`, `session_alive: true`).

## Tests

`terminal-view.test.tsx`

- opens in view mode: `setReadOnly(true)` called, connection word reads `viewing` after `onopen`.
- Esc in view mode clears the `view` param (dialog shows details again).
- clicking `Take control`: `setReadOnly(false)`, `focus()` called, the well has `data-mode="control"`.
- Esc in control mode does NOT close or leave the terminal view.
- in control mode a pointer-down outside the dialog does NOT close it (`onInteractOutside` /
  `onPointerDownOutside` are prevented); in view mode it does.
- **Name this one so it contains `tab in control mode`** (an acceptance line runs `vitest -t "tab
  in control mode"`): Tab in control mode must do BOTH things — leave `document.activeElement`
  inside the well (Radix's focus trap would otherwise move it) AND reach the transport, i.e. the
  fake emulator's `onData` fires and the fake socket receives the encoded bytes. Asserting only
  the focus half is worthless: the broken `onKeyDownCapture` implementation passes it while
  silently swallowing every key before xterm sees it. If `terminal-view.tsx` still uses
  `onKeyDownCapture`, change it to a bubble-phase `onKeyDown` — that IS a wiring fix, not a
  restyle — and say so in your report.
- **Name this one so it contains `unmount before the emulator resolves`** (acceptance line): with
  a factory whose promise you resolve MANUALLY, unmount the component first, then resolve; assert
  the late emulator's `dispose` was called, `open` was not, and no socket was constructed. A
  second test: a factory that REJECTS lands the view in `refused` with a reason and schedules no
  retry.
- the 100 ms fit debounce is cancelled on unmount: with fake timers, unmount while a resize is
  pending and assert no `fit`/`resize` runs afterwards.
- `onclose` code table, one assertion per code: 4009 → the waiting block shows the reason text and
  a retry is scheduled; 1000 → the `ended` stamp; 4004 → the refused text, no retry; **4008 (a
  native-backend session) → the refused text, no retry** — this is the case a two-code scheme got
  backwards, so assert the absence of the retry explicitly; 1006 twice (after an open) → `Retry`
  present, clicking it opens a new socket; 1006 BEFORE any open with `terminals: false` in the
  store → the `--terminals` message, and with `terminals: true` → the cookie message.
- Reconnect appears on `ended` once the snapshot shows the session alive again (set the store).
- footer shows `loom attach <id>` and the `open in a tab` link with the encoded href.

`terminal-glyph.test.tsx`

- rendered on the graph card and the ledger row only when `terminals && session_backend ===
  "tmux" && session_alive`; each of the three flags false → absent.
- clicking it opens `?stage=<id>&view=terminal` (assert the router's search params); on the ledger
  row the click must not also navigate to `/stages/<id>`.

`stage-modal.test.tsx`

- `?stage=<id>&view=terminal` renders the terminal header (eyebrow `Terminal`) and the wide
  dialog class; `?stage=<id>` renders details; the `t` key in details opens the terminal view
  when the glyph condition holds and does nothing when it does not.
- the disabled `Terminal` button carries the native-backend reason when `session_backend` is
  `"native"`, and the `--terminals` reason when `terminals` is false.

`routes/terminal.test.tsx`

- `/terminal/<id>` renders the view in a page frame; an unknown id renders `No such stage`.

## Glue

After the tests, run `cd web && bun run check` ONCE. Fix type errors between F1 and F2 (a renamed
export, a missing optional prop), lint findings in F1's/F2's files, and formatting. Do NOT change
behaviour to make a test pass; if a test reveals a real defect in F1's or F2's code, fix the
defect and say so in your report, naming file and line.

## Constraints

- `verbatimModuleSyntax` → `import type`. Testing Library queries by role/label, not by class.
- No `TODO`/`FIXME`. Never run `git`, never `bun add`.
- `loom memory note` any jsdom stub you had to add to `web/src/test/setup.ts` and why.

## Done means

`bun run check` passes from `web/`, and `bunx vitest run` reports at least twelve new tests across
the four files above. Each of the four files is an acceptance line in the plan
(`bunx vitest run <file>` must print `Tests N passed` with N ≥ 1 and no failures), so every file
must exist with at least one passing test.
