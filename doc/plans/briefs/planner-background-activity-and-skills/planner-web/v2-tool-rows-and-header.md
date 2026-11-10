# V2: background tool rows, `Task` nesting and the header's background kind (step A6, Decision A-D5)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run in wave 2, after V1 returned (`backgroundWord` in `src/plan/chat/background-fold.ts`) and while nobody else edits the chat.

## Files owned

`src/plan/chat/tool-group.ts`, `src/plan/chat/tool-row.tsx`, `src/plan/chat/transcript.tsx`, `src/plan/chat/spawn-tool.ts` (new), `src/plan/chat/tool-background.test.tsx` (new), `src/plan/header/use-activities.ts`, `src/plan/header/activity.tsx`, `src/plan/header/activity-background.test.tsx` (new), `src/plan/ui/activity-glyphs.tsx`.

Frozen, read only: `src/plan/chat/background.contracts.test.tsx`, whose contract `a running background row stays outside the tool fold` is yours to pass.

## What exists (verified 2026-11-10; anchor by text)

- `chat/tool-group.ts:44-60` `groupToolRows(items)` folds every run of plain tool rows (`subagent === null`, not `error` or `denied`) into one group, drops Skill rows, keeps nested subagent tools outside for the Agent row; the fold renders as `<details className="chat-tool-fold" open={turn.open}>` in `transcript.tsx` (closed when the turn is not open).
- `chat/tool-row.tsx:21-30` `TOOL_GLYPHS` (no `Task`), `:36-42` `STATUS_WORD` over `ToolItem["status"]`, `:45-53` `ToolStatus`, and the literal `item.name === "Agent"` at `:177` and `:201`; `transcript.tsx:36` filters Agent rows the same way. The server treats `Task` as the legacy name of `Agent` (`loom/src/plan_session/transcript/tools.rs:24`).
- `transcript.tsx` is at the 400-line cap and was being edited in the operator's checkout when the plan was written: your edits there are net-neutral or move code out.
- `ui/activity-glyphs.tsx:9-17` `ActivityKind`, `:19-28` `ACTIVITY_KINDS`, `:30-` `ACTIVITY_LABELS`, `:147-` `ACTIVITY_TONES`; `header/activity.tsx:44-52` `PillGlyph` draws the roundel for progress kinds (`isProgressKind`), `:98-103` the pill label "`N` in progress: …"; `header/use-activities.ts:188` `useActivities()` builds the rows.
- `backgroundEntryAtom(toolUseId)` and `chatBackgroundAtom` in `src/plan/chat/background-tasks.ts` (merged), `backgroundWord(state)` in `background-fold.ts` (V1).

## Steps

1. **`spawn-tool.ts`**: `isSpawnTool(name)` is true for `Agent` and `Task`. Replace the literal `"Agent"` at `transcript.tsx:36` and `tool-row.tsx:177,201` with it; add `Task: Bot` to `TOOL_GLYPHS`. A `Task` row nests its children like an `Agent` row.
2. **Rows read their entry.** A tool row reads `backgroundEntryAtom(item.tool_use_id)` (one selector per row, so a 5 s refresh re-renders only the rows whose entry changed). With an unended entry the row shows `backgroundWord(entry.state)` (`running` for running and tool-wait, `stalled?`) in place of `item.status`'s word, with the marks rule's glyph for a state word; with an ended entry it shows `done`, `failed` or `stopped`. Extend `ToolStatus` and its word map for these words without changing the existing five.
3. **The fold.** `groupToolRows(items, background)` takes the chat's background tasks and keeps a row outside the fold while the entry of its `tool_use_id` is unended (an Agent row always launches a background task; a Bash row does when its result named a task id). Once the entry ends the row folds as before. Every caller passes the tasks (`rg -n 'groupToolRows' src/plan`).
4. **Header.** `"background"` joins `ActivityKind`, `ACTIVITY_KINDS` (after `"waiting"`), `ACTIVITY_LABELS` ("background tasks") and `ACTIVITY_TONES` (the waiting tone); it is neither a progress kind (so `PillGlyph` draws the dot `ActivityGlyph`, never the roundel) nor a state-word kind. `useActivities` adds one row per session with at least one unended task, in any session state, labelled "N background tasks" (`plural`). The pill's "`N` in progress" count leaves background rows out. Check every `Record<ActivityKind, …>` and switch over the kinds (`rg -n 'ActivityKind' src/plan`) and give each a case.
5. **Tests** (new files): `tool-background.test.tsx` (a background row goes from running to done and stays out of the fold while it runs; a `stopped` and a `failed` shell row show their words; a `Task` row nests under its parent; a Bash row without a task id folds as today); `activity-background.test.tsx` (the header `background` row appears with an unended task and clears when the last ends, draws the dot and not the roundel, and is not counted in "N in progress"). Run `node_modules/.bin/vitest run src/plan/chat/tool-background.test.tsx src/plan/chat/background.contracts.test.tsx src/plan/chat/transcript src/plan/chat/tool src/plan/header src/plan/help`.

## Done when

The fold contract and your tests pass, the existing `src/plan/chat` and `src/plan/header` tests pass, and `transcript.tsx` is at most 400 lines.

## Traps

- `item.status` keeps its meaning for every row without a background entry; the table's word wins only where an entry exists.
- A state word takes the two-arrow `ArrowSpinner` through `StateMark`, an item the dot (`conventions/plan-web.md`, "State word marks and item marks"); a background row's running word is a state word.
- The header kind is not "in progress": the user is not waiting on the main agent.
