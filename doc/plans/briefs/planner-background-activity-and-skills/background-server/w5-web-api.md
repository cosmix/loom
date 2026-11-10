# W5: web wire, chat selectors and the `SessionView` change (step A5, web side)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run alone in wave 4, after W4 wrote `web/src/plan/api/fixtures/background.json` and the Rust side dropped `SessionView.subagents_running`. Decisions A-D3 ("Restore") and A-D4 ("`SessionView` change", "Fixtures and schema lists", "Web") of the plan are the specification. All paths below are from the repository root; run commands from `web/`.

## Files owned

`web/src/plan/api/schema.ts`, `schema-chat.ts`, `schema-background.ts` (new), `schema.test.ts`, `socket.ts`, `socket-background.test.ts` (new); `web/src/plan/state/atoms.ts`; `web/src/plan/chat/background-tasks.ts` (new), `chat/background-tasks.test.ts` (new), `chat/restore.tsx`, `chat/restore.test.tsx`, `chat/state-line-kit.tsx`; `web/src/plan/header/use-activities-test-kit.ts`; `web/src/plan/help/chat-scene.tsx`, `help/getting-around-scene.tsx`, `help/pressure-scene.tsx`.

Read only: `web/src/plan/api/fixtures/background.json` (W4's) and the three chat fixtures (W3 removed their `subagents_running` keys; the Rust tests compare them), every other web file.

## What exists (verified 2026-11-10)

- `schema-chat.ts:99` `subagents_running: u32` in the strict `sessionViewSchema`; the chat fixtures carry the key at `chat-snapshot.json:15,487`, six places in `chat-deltas.json` and `chat-frames.json:17`. The Rust parity test (`loom/src/plan_server/wire/tests_chat.rs`) loads all three, so each must match the Rust serializer exactly after the key is gone.
- Typed `SessionView` literals that set the key: `chat/state-line-kit.tsx:27`, `header/use-activities-test-kit.ts:22`, `help/chat-scene.tsx:54`, `help/getting-around-scene.tsx:58`, `help/pressure-scene.tsx:27`, and `chat/restore.test.tsx:50,154`.
- `chat/restore.tsx:33-42` `useAvailable` blocks Restore while `session.subagents_running > 0`.
- `schema.ts:31-54` `PLAN_MESSAGE_TYPES`; `:118-131` `planMessageSchema` spreads each module's message schemas (`kickoffMessageSchemas` and the rest). A tag missing from `PLAN_MESSAGE_TYPES` makes the Rust tag-parity test fail and the socket drop the frame (`doc/loom/knowledge/conventions/plan-web.md`, "New PlanMessage Variants Need Both Schema Arms").
- `socket.ts` `applyHello` resets `kickoffAtom` and `draftTeamAtom` (`:133-134`); `apply` (`:146-`) switches on `message.type` and stores `draft_team` without its tag (`:166-169`).
- `chat/model.ts:25` `sessionAtom`; `web/src/state/atoms.ts:45` builds a per-id selector with `selectAtom` from `jotai/vanilla/utils`.
- `api/socket.test.ts` has the `FakeSocket` pattern and the "stores draft team frames without the type tag" test (`:307`); `api/schema.test.ts:34-49` lists fixtures that must parse and reject extra fields.

## Steps

1. **Remove `subagents_running`** from `sessionViewSchema` and the typed literals above (W3 already removed it from the three chat fixtures).
2. **`schema-background.ts`**: `backgroundTaskViewSchema` (strict; every key of `common.md`'s `BackgroundTaskView` required, the optional ones `.nullable()`; `kind` the enum `agent | shell`, `state` the enum `running | tool-wait | stalled | done | failed | stopped`, counts as the `u64` helper `schema-chat.ts` uses) and `backgroundMessageSchemas = [z.strictObject({ type: z.literal("background"), session: z.string(), tasks: z.array(backgroundTaskViewSchema) })]`, with exported types `BackgroundTaskView` and `BackgroundFrame` (`{ session, tasks }`).
3. **`schema.ts`**: `"background"` in `PLAN_MESSAGE_TYPES` and `...backgroundMessageSchemas` in `planMessageSchema`.
4. **`state/atoms.ts`**: `backgroundAtom = atom<BackgroundFrame | null>(null)`. **`socket.ts`**: reset it to `null` in `applyHello` beside `draftTeamAtom`; in `apply`, store a `background` frame without its tag.
5. **`chat/background-tasks.ts`**: `chatBackgroundAtom` (the tasks of `backgroundAtom` when its `session` equals `sessionAtom`'s id, else an empty array), `isUnended(state)` (`running`, `tool-wait`, `stalled`), `unendedBackgroundCountAtom`, and `backgroundEntryAtom(toolUseId)`, a `selectAtom` over `chatBackgroundAtom` that returns the task with that `tool_use_id` or `null`, compared by value, cached in a module `Map` so each tool row gets one atom.
6. **`chat/restore.tsx`**: `useAvailable` reads `unendedBackgroundCountAtom > 0` where it read `subagents_running > 0` (A-D3: every unended task blocks Restore, adopted shells and `stalled` included). Keep the file under 400 lines (383 today). **`chat/restore.test.tsx`**: the case at `:154` sets an unended background task through `backgroundAtom` (session id matching) instead of `subagents_running: 1`; add a case where only an adopted shell is running and Restore is still hidden; drop the key from `:50`.
7. **Tests.**
   - `schema.test.ts`: `background.json` parses under `planMessageSchema` and rejects an extra field (add it to the `fixtures` list).
   - `socket-background.test.ts` (new, the `FakeSocket` pattern of `socket.test.ts`): a `background` frame fed through the socket's message handler (`onmessage` with the fixture's JSON, never a direct atom write) leaves `backgroundAtom` holding its tasks; a second hello resets it to `null`.
   - `chat/background-tasks.test.ts` (new): with the chat snapshot's session in `snapshotAtom`, `chatBackgroundAtom` holds the fixture's five tasks and `unendedBackgroundCountAtom` is 3; a frame for another session id gives `[]` and 0; `backgroundEntryAtom("toolu_bg1")` returns the running agent and keeps its identity across an unrelated frame change.
8. **Run** `node_modules/.bin/vitest run src/plan/api src/plan/state src/plan/chat/background-tasks.test.ts src/plan/chat/restore.test.tsx src/plan/help src/plan/header` once ready. You cannot run `bun run build` or `tsc` (the subagent guard); the main agent type-checks.

## Done when

`rg -n 'subagents_running' web/src` finds nothing, the tests above pass, and the strict schema parses `background.json` unchanged.

## Traps

- Test files named `.ts` cannot hold JSX; use `createElement` (`conventions/plan-web.md`, "Concept Mode Test Rules").
- Build invisible characters with `String.fromCodePoint`, never escapes the formatter rewrites.
- A help scene parses `sessionViewSchema` at module load: a literal with a stale key fails every suite that imports the help footer.
