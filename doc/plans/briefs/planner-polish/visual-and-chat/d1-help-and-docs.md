# D1: help and the product spec follow the change (issue 14)

Tier: sonnet (`loom-software-engineer`). Runs after V1, V3, C1 and V2 have returned and the main agent has merged their reports. Read `common.md` (the issue list and D1-D14), then the stage's change list, which the main agent passes you (`git diff --stat` of the stage so far, plus each worker's report summary).

## Help (`web/src/plan/help/`)

`help/topics.ts`, `pressure-scene.tsx` and `getting-around-scene.tsx` (with their tests) state the new truth:
- **Getting around:**
  - a feature opens on its product overview (D4);
  - the spine has five groups and no Pressure test row;
  - each plan row shows "Pressure-tested <date>" or "Not pressure-tested" (D5);
  - the metaplan opens as a summary with Source one click away (D3);
  - the footer shows the Claude and Codex limits;
  - the roundel means work in progress (D10).
- **Chat and sessions:**
  - the panel is always shown on wide screens and is a sheet behind the Chat button on narrow ones (D7);
  - Stop sits beside Send while a turn runs (D8);
  - the menu holds End chat.
- **Pressure tests and runs:** a test starts from the plan page's Pressure test button or the spine's next action; there is no Pressure page (D6).
- **Settings:** the Summaries group shows the rebuild statistics; the limits are in the footer (D14).
- Keep `help/topics.test.tsx`'s topic ids and titles (`["pressure","Pressure tests and runs"]`). Add or change only notes and tips.
- Do not touch `chat-scene.tsx` (V3's) or `legend-entries.tsx` (V1's).

## Product spec (`doc/product/feat-planner/`)

Rewrite to the new truth only the passages these change. `rg -n` each claim first:
- `section_05.md`: "The metaplan has none" (around lines 22 and 50) → each metaplan has a document summary of its own, which no overview cites (D2). Landing on the product overview (D4).
- `section_06.md`: the spine's groups and plan rows (D5, D6), and the metaplan page (D3).
- `section_08.md`: the collapse to a 2.5rem rail (around lines 46-48, 61) → always shown at 1280px and wider, a sheet below (D7). The composer's Stop (D8), the menu (D9), the type sizes (around line 106, D11).
- `section_11.md`: the Pressure page and its row → the plan row badge and the plan page's button (D6).
- `index.md`: any vision sentence these contradict.

State current truth only: no "was", "used to" or "previously".

## Run once at the end

`node_modules/.bin/vitest run src/plan/help`, from `web/`.
