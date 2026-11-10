# V4: the help scenes, tips and legend

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run alone in wave 3, after V1, V2 and V3 returned: the background line and list, the background tool rows and header kind, and the command menu exist.

## Files owned

`src/plan/help/chat-scene.tsx`, `src/plan/help/getting-around-scene.tsx`, `src/plan/help/pressure-scene.tsx`, `src/plan/help/topics.ts`, `src/plan/help/legend-entries.tsx`, `src/plan/help/chat-scene.test.tsx`.

## What exists (verified 2026-11-10)

- `help/chat-scene.tsx` builds a sample chat (its `SessionView` literal near `:54`; the plan's step A6 names its line `:241` as one to update); `help/getting-around-scene.tsx` and `help/pressure-scene.tsx` build session views too; `help/topics.ts` holds `CHAT_TIPS`; `help/legend-entries.tsx` lists the glyphs the legend explains. Help scenes parse their literals with the strict schemas at module load, and selector contracts of help scenes pin their structure (`conventions/plan-web.md`, "UI Repair Test Rules": `:scope > h3`, `section[aria-labelledby]`).

## Steps

1. **Chat scene.** Show one idle chat waiting on background tasks: seed `backgroundAtom` (the scene's own store, never a live socket) with two unended tasks of `background.json`'s shapes, so the scene draws the line, and a running Agent row outside the tool fold. Keep the scene's existing sections and structure.
2. **Tips** (`CHAT_TIPS`): one tip for the background line ("An idle chat that waits on background agents or shells says so in its head; open the list to see each task.") and one for the command menu, naming its keys: `/` as the first character opens the list, Up and Down move, Enter or Tab insert, Escape closes; terminal-only commands are listed but disabled, each with its reason.
3. **Legend** (`legend-entries.tsx`): the background row's dot in the header, and the state words a background tool row shows (`running`, `stalled?`, `done`, `failed`, `stopped`).
4. **Getting around and pressure scenes**: only what their literals need to keep parsing; no new content.
5. **Tests**: `chat-scene.test.tsx` gains a case that the scene shows "Waiting on 2 background tasks" and that the tips name the menu keys. Run `node_modules/.bin/vitest run src/plan/help`.

## Done when

The help tests pass, including the existing selector tests unchanged, and the scenes start no fetch and open no socket.

## Traps

- A help scene is read by people learning the app: say what each thing does in plain words, one sentence each.
- The reading help scene's tests pin Jotai internals (`conventions/plan-web.md`, "Dependency upgrade compatibility"); seed atoms the way the existing scenes do.
