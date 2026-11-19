# Worker W: the browser side

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` first, especially the wire
contract. Worker S updates the shared fixtures (`web/src/plan/api/fixtures/chat-commands.json`,
`web/src/plan/api/fixtures/kickoff-drafts.json`) to that contract; you change the zod schemas
and every consumer. Do not edit the fixtures.

Territory brief for three codex units: `web-schemas` (section 1 and the fixture-reader tests),
`web-size` (section 2) and `web-drafts` (sections 3 and 4 with their tests). Each unit owns only
the files its unit brief lists. Units compile with `./node_modules/.bin/tsc -b --noEmit` from
`web/`; the orchestrator runs lint, format and `vitest` after the wave.

## 1. Schemas and commands

- `web/src/plan/api/schema-chat.ts`: `set_aside_kickoff` gains `now: z.boolean()`,
  `discard_kickoff` gains `delete_files: z.boolean()`.
- `web/src/plan/api/schema-kickoff.ts`: `kickoffDraftViewSchema.size` is `kickoffSizeSchema`
  (not nullable); `kickoffDraftDetailSchema` gains `written: z.array(z.string())` and
  `committed: z.boolean()`.
- `web/src/plan/chat/commands.ts`: `setAsideKickoff(store, session, now = false)` and
  `discardKickoff(store, session, deleteFiles: boolean)`. Keep `set_aside_kickoff` in the list of
  commands that return the unsent queue (`markInterruptPending`).

## 2. The size is the offer's

`kindOf` (`web/src/plan/new/kickoff-state.ts`) prefers the offer: `kickoff` → `feature`,
`kickoff-small` → `plan`, and only when the offer is neither does it fall back to the shape's
size. Change its doc comment to say so. Callers: `web/src/plan/new/kickoff-head.tsx:65`,
`web/src/plan/header/feature-switcher.tsx:61`; and `web/src/plan/new/taking-shape.tsx:42`, which
today reads `shape?.size` directly and must use `kindOf(shape?.size ?? null, session?.offer ?? null)`
for the sections row, the foot and the size passed to `WriteKickoff`. Update
`kickoff-state` tests and the taking-shape, kickoff-head and feature-switcher tests: a feature
kickoff whose shape says `plan` shows "Write the vision" and sends `confirm_kickoff` with
`feature`.

## 3. Set aside that visibly works (`web/src/plan/new/draft-view.tsx` `OpenDraftNotice`)

- While the running chat is this draft's session and its state is `ending`
  (`sessionAtom`, states in `schema-chat.ts`), the notice reads `<name> is being set aside…`,
  "Set it aside" is disabled with the reason `Setting aside`, and a **Stop now** button sends
  `setAsideKickoff(store, session, true)`.
- Otherwise it shows Continue and Set it aside as today; Set it aside sends `now: false`.
- When the draft turns not live, the notice disappears (it already keys off `live`); make sure a
  test proves that with a store update rather than a remount.

## 4. Delete (`DraftActions` in `draft-view.tsx`)

- With `detail.written` empty: Discard as today, sending `delete_files: false`.
- With `detail.written` non-empty: the quiet button reads **Delete**. The in-page confirm (no
  browser dialog) reads "Delete this draft and the files it wrote?", lists every path in
  `written` in monospace, adds "Some of these files are committed; git history still has them."
  when `committed` is true, and offers **Delete for good** (sends `delete_files: true`) and
  **Keep it**. A refusal's text shows in `CommandStatus` as today.
- `DraftBody`: `draft.size` is never null now; drop `KIND_UNDECIDED` if nothing else uses it.
  `web/src/plan/home/drafts-shelf.tsx`: drop any null-size branch the same way.

## 5. Tests

Extend `web/src/plan/new/draft-view.test.tsx`, `web/src/plan/chat/commands-set-aside.test.ts`,
`web/src/plan/home/drafts-shelf.test.tsx`, `web/src/plan/new/kickoff-start.test.tsx` and the
fixture readers `web/src/plan/api/schema-chat.test.ts`, `web/src/plan/chat/commands.test.ts`,
`web/src/plan/chat/replay.test.tsx` and `web/src/plan/header/use-activities.test.tsx`.
Cover: Stop now sends `now: true` while ending; Delete lists the written paths and the committed
note and sends `delete_files: true`; Discard sends `delete_files: false`.

## Files

`web/src/plan/api/schema-chat.ts`, `web/src/plan/api/schema-kickoff.ts`,
`web/src/plan/api/schema-chat.test.ts`, `web/src/plan/chat/commands.test.ts`,
`web/src/plan/chat/replay.test.tsx`, `web/src/plan/header/use-activities.test.tsx`, `web/src/plan/chat/commands.ts`,
`web/src/plan/chat/commands-set-aside.test.ts`, `web/src/plan/new/kickoff-state.ts`, `web/src/plan/new/kickoff-state.test.ts`,
`web/src/plan/new/kickoff-head.tsx`, `web/src/plan/new/taking-shape.tsx`, `web/src/plan/new/taking-shape.test.tsx`,
`web/src/plan/new/draft-view.tsx`, `web/src/plan/new/draft-view.test.tsx`,
`web/src/plan/new/kickoff-start.test.tsx`, `web/src/plan/header/feature-switcher.tsx`,
`web/src/plan/header/feature-switcher.test.tsx`, `web/src/plan/home/drafts-shelf.tsx`, `web/src/plan/home/drafts-shelf.test.tsx`.
