# planner-web: shared brief

Every worker of stage `planner-web` reads this file in full, then its own brief. The stage's `working_dir` is `web/`; paths below are relative to `web/` unless they start with `../` or `doc/`.

## Where things are

- **The plan.** `doc/plans/IN_PROGRESS-PLAN-planner-background-activity-and-skills.md` once `loom run` has started (`PLAN-…` before): Decision A-D5 (the idle sign, the list, the header kind, the tool rows, `Task` nesting), Decision B-D2's "Web gate" and B-D5 (the composer), Steps A6, B3 and B5. Where this file and the plan differ on a name, this file wins.
- **The design.** `src/plan/ui/DESIGN.md` is the design source: one surface level and no box in a box (`## Surfaces and borders`), the state-word and item marks rule (a mark before a state word is `ArrowSpinner` through `StateMark`; a mark before an item is the dot `ActivityGlyph`), the tone tokens, the chat's motions (nothing moves when nothing works). `doc/loom/knowledge/conventions/plan-web.md` holds the test rules ("Chat Panel and Review UI Rules", "UI Repair Test Rules", "Planner Polish Rules", "State word marks and item marks", "Accessible Names Trim Each Span"). Read both before writing a component or a test.
- **What the server stages merged.**
  - `src/plan/api/fixtures/background.json`: one `background` frame for session `00000000-0000-4000-8000-000000000001` (the session of `chat-snapshot.json`) with five tasks: agent `a-run` running (tool_use_id `toolu_bg1`, tokens), shell `bshell01` running (adopted, no tool_use_id), agent `a-quiet` stalled (`toolu_bg2`), agent `a-done` done (`toolu_bg3`, report "DONE"), shell `bshell02` failed (`toolu_bg4`). Three are unended.
  - `src/plan/api/schema-background.ts` (`BackgroundTaskView`, `BackgroundFrame`), `backgroundAtom` in `src/plan/state/atoms.ts` (set by the socket, reset on hello), and `src/plan/chat/background-tasks.ts`: `chatBackgroundAtom` (the chat session's tasks; a frame for another session gives `[]`), `isUnended(state)` (`running`, `tool-wait`, `stalled`), `unendedBackgroundCountAtom`, `backgroundEntryAtom(toolUseId)` (one cached selector per tool row). Restore already reads the count. Build on these; do not change them.
  - `src/plan/api/schema-session-commands.ts` (`CommandEntry`, `CommandsView`, the `commands` frame), `src/plan/api/fixtures/session-commands.json` (a `ready` view, generation 1: `simplify` bundled, `loom-plan:args` plugin with alias `args`, `probe-personal` personal, `docs` account, `context` and `compact` built-in run-with-handling, `clear` terminal-only "Changes the session outside the planner" with aliases `reset` and `new`, `usage` terminal-only "Opens a terminal dialog" with aliases `cost` and `stats`, `effort` terminal-only "Saves your default for new sessions", `help` terminal-only "Opens a terminal dialog"), and `src/plan/api/fixtures/slash-vectors.json` (`{tables: {ready, loading, unavailable}, cases: [{text, table, expect}]}`, `expect` `"ok"` or the server's refusal reason).
  - The user item's `command: {name, args} | null` field (`schema-chat.ts`); the server sets it on the user item of a prompt-type command.
  - `GET /api/session/commands?session=<id>` answers a `CommandsView` (`loading` with the native rows for no session or an unknown one).

## Shared names (settled)

| Name | File | Owner |
| --- | --- | --- |
| `BackgroundLine` (component: the idle line and its disclosure button) | `src/plan/chat/background-line.tsx` | V1 |
| `BackgroundList` (component: the task list) | `src/plan/chat/background-list.tsx` | V1 |
| `foldBackground(tasks)` (ended agents folded into "N earlier agents") | `src/plan/chat/background-fold.ts` | V1 |
| `backgroundWord(state)` (`running` for running and tool-wait, `stalled?`, `done`, `failed`, `stopped`) | `src/plan/chat/background-fold.ts` | V1 |
| `isSpawnTool(name)` (`Agent` or `Task`) | `src/plan/chat/spawn-tool.ts` | V2 |
| `groupToolRows(items, background)` | `src/plan/chat/tool-group.ts` | V2 |
| `"background"` `ActivityKind` | `src/plan/ui/activity-glyphs.tsx` | V2 |
| `CommandMenu` (component: the listbox) | `src/plan/chat/command-menu.tsx` | V3 |
| `commandsAtom` (per-session: loading, ready or unavailable) | `src/plan/state/commands-atom.ts` | V3 |
| `messageProblem(text, attachments, commands)` | `src/plan/chat/message-problem.ts` | V3 |

V2 imports `backgroundWord` from V1's `background-fold.ts` (signature `(state: BackgroundTaskView["state"]) => string`), so V2 runs after V1 returned. Waves: V1 and V3 together; then V2; then V4, which renders all three in the help scenes.

## Rules for every worker

- Write only your brief's files. Frozen and read-only: `src/plan/chat/background.contracts.test.tsx` and `src/plan/chat/commands.contracts.test.tsx` (this stage's five contracts, `loom stage contracts show planner-web`), `src/plan/chat/hearth.contracts.test.tsx` (an earlier plan's contracts, which must stay green unchanged: idle with no task draws nothing and announces nothing, and the new element avoids `.chat-state-row`, `.aurora-roundel` and the text "Waiting for you").
- Visual work (V1, V3) loads the `frontend-design:frontend-design` skill before any edit; DESIGN.md and this brief win over the skill's defaults.
- Server text is untrusted: every label, agent type, model, description, argument hint, reason and report preview renders through `Untrusted` (`src/plan/chat/visible-controls.ts`). Build invisible characters in tests with `String.fromCodePoint`.
- Tests: no jest-dom (assert with Testing Library queries and plain matchers); `.ts` test files hold no JSX; a test that needs the desktop panel stubs `matchMedia` true for every `min-width` query; chat frames are seeded as raw `{ type: "chat", frame }` JSON through the `FakeSocket` seam in `planSocketAtom`, and a `background` or `commands` frame through the same handler, never by writing the atom; a test file stays under 400 lines.
- Run only your own tests: `node_modules/.bin/vitest run <your files and the consumers your brief names>`. You cannot run `bun run build`, `cargo build` or `tsc` (the subagent guard); the main agent type-checks, builds and shoots.
- Size caps: 400 lines per file. `src/plan/chat/transcript.tsx` is at the cap and was being edited in the operator's checkout when the plan was written: edits there are net-neutral or move code out.
- MEMORY: record mistakes, decisions and surprises at once with `loom memory note` / `loom memory decision`. Never Claude Code auto-memory, never `loom knowledge`.
- Report: files changed, tests run and their result, anything unresolved. Do not run git.
