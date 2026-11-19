# Unit web-schemas

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 1 and section 5 (fixture readers) of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/web.md`.

## Files you own (write only these)

- `web/src/plan/api/schema-chat.ts`
- `web/src/plan/api/schema-kickoff.ts`
- `web/src/plan/chat/commands.ts`
- `web/src/plan/api/schema-chat.test.ts`
- `web/src/plan/chat/commands.test.ts`
- `web/src/plan/chat/commands-set-aside.test.ts`
- `web/src/plan/chat/replay.test.tsx`
- `web/src/plan/header/use-activities.test.tsx`

## Entry points

`set_aside_kickoff` and `discard_kickoff` in `schema-chat.ts`; `kickoffDraftViewSchema` and `kickoffDraftDetailSchema` in `schema-kickoff.ts`; `setAsideKickoff` and `discardKickoff` in `commands.ts`.

## Done

The schemas and command helpers match the wire contract, the fixture-reader tests assert the new
fields, and `./node_modules/.bin/tsc -b --noEmit` from `web/` shows no error in your files.
Orchestrator proof: `./node_modules/.bin/vitest run src/plan/api src/plan/chat` from `web/`.

## Traps

The `wire` unit edits the JSON fixtures in this same wave: code to the contract, not to the fixture on disk. Callers of `discardKickoff` in `draft-view.tsx` belong to `web-drafts` and break until wave 2.
