# Unit web-size

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 2 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/web.md`.

## Files you own (write only these)

- `web/src/plan/new/kickoff-state.ts`
- `web/src/plan/new/kickoff-state.test.ts`
- `web/src/plan/new/kickoff-head.tsx`
- `web/src/plan/new/taking-shape.tsx`
- `web/src/plan/new/taking-shape.test.tsx`
- `web/src/plan/header/feature-switcher.tsx`
- `web/src/plan/header/feature-switcher.test.tsx`

## Entry points

`kindOf` in `kickoff-state.ts`; its callers at `kickoff-head.tsx`, `feature-switcher.tsx` and `taking-shape.tsx` (which reads `shape?.size` directly today). Wave 1 made `kickoffDraftViewSchema.size` non-nullable (`web/src/plan/api/schema-kickoff.ts`).

## Done

`kindOf` prefers the offer, every caller uses it, the tests cover a feature kickoff whose shape
says plan ("Write the vision", `confirm_kickoff` with `feature`), and
`./node_modules/.bin/tsc -b --noEmit` from `web/` shows no error in your files. Orchestrator
proof: `./node_modules/.bin/vitest run src/plan/new src/plan/header` from `web/`.

## Traps

Change the doc comment of `kindOf` to say the offer wins.
