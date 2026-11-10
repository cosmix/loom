# V1: the idle line, the task list, the fold, the kickoff head and DESIGN.md (step A6, Decision A-D5)

Read `common.md` in this directory first. You are a `loom-senior-software-engineer` running on the fable model: this is new visual design inside the planner's design system. Load the `frontend-design:frontend-design` skill before your first edit. You run in wave 1 beside V3 (the composer and its menu: disjoint files).

## Files owned

`src/plan/chat/background-line.tsx`, `background-list.tsx`, `background-fold.ts`, `background.css` (all new), `background-line.test.tsx`, `background-list.test.tsx` (new), `src/plan/chat/panel-head.tsx`, `src/plan/chat/chat-surface.css`, `src/plan/chat/state-line.tsx`, `src/plan/new/kickoff-head.tsx`, `src/plan/new/kickoff-head-background.test.tsx` (new), `src/plan/ui/DESIGN.md`, `scripts/plan-chat-screens.mjs` (new), `../scripts/plan-screens.sh` (one flag).

Frozen, read only: `src/plan/chat/background.contracts.test.tsx` (contracts `idle chat shows the background line from a socket frame` and `background list rows show label controls as visible marks`; a third, about the tool fold, is V2's to pass), `src/plan/chat/hearth.contracts.test.tsx`.

## What exists (verified 2026-11-10; anchor by text)

- `chat/panel-head.tsx:61-68`: `.chat-head-sub` holds `<StateLine />` and the place span, joined by " · " when `hasStateWords(session)`.
- `chat/chat-surface.css:112-121` `.chat-head-sub` (`overflow: hidden`, `text-overflow: ellipsis`, `white-space: nowrap`, `--muted-foreground`, `--type-ui`); `:123-125` hides the line when it has neither `.chat-state-row` nor `.chat-head-place` (`:has()`).
- `chat/state-line.tsx:11-23` `VISIBLE_WORDS` (idle has none) and `hasStateWords`; its live region announces state words only, and an idle chat announces nothing.
- `new/kickoff-head.tsx:22-48` keeps its own `STATE_WORDS` and `StateWords`; `KickoffHead` is at `:57`.
- `ui/DESIGN.md`: the kickoff-conversation exception under `## Surfaces and borders` ("the panel head's state words at `--type-ui`, idle saying nothing") and, under `## Chat`, the bullet that ends "The head draws no busy sign and no state glyph but the permission warning; its sub-line is the state's words, then the place the chat concerns." The user's "idle says nothing" rule is `doc/loom/knowledge/mistakes/planner-visual-feedback-shortfall.md` (contract `idle-state-line-is-empty`).
- `LiveElapsed` (`chat/pressure/elapsed.tsx:11`) runs one 1 s interval per instance; `useNow` (`web/src/components/hooks/use-now.ts:4`) is the shared clock; `plural` (`ui/plural.ts`), `compactTokens` (`@/lib/tokens`), `StateMark` and `ActivityGlyph` (`ui/activity-glyphs.tsx`).
- `scripts/plan-screens.sh:41` runs `web/scripts/plan-screens.mjs` with `<url> <out-dir> <route>...` (`:87`); `plan-screens.mjs` launches `playwright-core` and adds only the theme init script. No script feeds chat frames (`conventions/plan-web.md`).

## Steps

1. **`background-fold.ts`**: `backgroundWord(state)` (`common.md`) and `foldBackground(tasks)`, which keeps every unended task and every ended shell as a row and folds ended agents into one "N earlier agents" entry (A-D5: a planner fold keyed on table state; the dashboard's `EarlierAgents` is private and folds on a zero series, so it is not reused).
2. **`BackgroundLine`** (`background-line.tsx`): reads `unendedBackgroundCountAtom` and the panel session's state. Only when the session is idle and the count is at least one, it renders one nowrap span "Waiting on N background tasks" (`task` for one, `plural`) and a disclosure button whose accessible name is "Background tasks", `aria-expanded` and `aria-controls` naming the list. Otherwise it renders nothing at all. Its class is new (`.chat-background-line`); it never uses `.chat-state-row`, `.aurora-roundel` or the words "Waiting for you".
3. **`panel-head.tsx`**: render `<BackgroundLine />` inside `.chat-head-sub` after `<StateLine />`, joined to the place by the same " · " rule; render `<BackgroundList />` as a sibling after `.chat-head-sub` (the sub-line clips its content). **`chat-surface.css`**: the `:has()` rule counts `.chat-background-line` as content. **`state-line.tsx`**: `hasStateWords` counts the line, so the place's separator appears.
4. **`BackgroundList`** (`background-list.tsx`, styles in `background.css`): open while the disclosure is pressed. One row per folded entry: a glyph per the marks rule (`StateMark` before a state word), the state word, the label through `Untrusted`, and the elapsed time since `started_at` from one shared `useNow` for the whole list (never one interval per row); agent rows add the model, `compactTokens(total)` and `compactTokens(tokens_per_min)/m` when they are set. Rows use the planner's list styles (hairline-divided list on the panel, no box in a box, no heat strip, no cluster box); it is not a Radix Popover (it takes focus). Escape closes it and returns focus to the disclosure.
5. **Live region.** The line is announced once when the first task starts while idle and once when the last one ends ("Background tasks finished"), through a `role="status"` element (an `aria-live` element inside page content leaks past a modal's `hideOthers`, `conventions/plan-web.md`); per-row changes are not announced.
6. **Kickoff head.** `KickoffHead` renders the same `<BackgroundLine />` (and list) beside its state words.
7. **`DESIGN.md`**: rewrite the two "idle says nothing" statements to say that an idle chat says nothing unless background tasks it waits on exist, when it shows the one line "Waiting on N background tasks" and its list (the reversal of the user's rule applies only while tasks exist); add the list's rules (one glyph and state word per row, the shared clock, the fold). Change nothing else.
8. **Screens driver.** `scripts/plan-chat-screens.mjs` takes the arguments `plan-screens.mjs` takes, stubs `WebSocket` in `page.addInitScript` and replays hello (`src/plan/api/fixtures/hello.json`), the chat snapshot with its session set idle, `background.json` and a `commands` frame of `session-commands.json`, then shoots, at 1440x900 and 390x844 in each theme given: the idle line, the open list, a running Agent tool row outside the fold, and the composer with "/" typed. Import `playwright-core` the way `plan-screens.mjs` does. `../scripts/plan-screens.sh` gains `--driver=<path>` (default `web/scripts/plan-screens.mjs`), passed through like the other flags; keep it `shellcheck`-clean and its usage comment current. You cannot run it (no build in a subagent): report its exact command for the main agent.
9. **Tests** (new files): `background-line.test.tsx` (idle with tasks shows the line and the button and announces once; working with tasks shows nothing; idle with only ended tasks shows nothing; a frame fed through the socket handler); `background-list.test.tsx` (ended agents fold into "N earlier agents"; a stopped and a failed shell show their words; elapsed time comes from one clock); `kickoff-head-background.test.tsx` (the kickoff head shows the line). Run `node_modules/.bin/vitest run src/plan/chat/background src/plan/chat/hearth.contracts.test.tsx src/plan/chat/state-line.test.tsx src/plan/new src/plan/help/chat-scene.test.tsx`.

## Done when

The two frozen contracts you own and your tests pass; `hearth.contracts.test.tsx` and `state-line.test.tsx` pass unchanged; the driver and the flag exist; DESIGN.md states the new rule.

## Traps

- Idle with an empty table must render no element at all, or the hearth contracts and the `:has()` rule change.
- Tokens are optional per task: a shell and a fresh agent have `null` figures; show nothing for them, never "0".
- Accessible names trim each span: put the spaces between number and words in text nodes (`conventions/plan-web.md`, "Accessible Names Trim Each Span").
