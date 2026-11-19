# Unit web-drafts

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
sections 3, 4 and the draft tests of section 5 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/web.md`.

## Files you own (write only these)

- `web/src/plan/new/draft-view.tsx`
- `web/src/plan/new/draft-view.test.tsx`
- `web/src/plan/new/kickoff-start.test.tsx`
- `web/src/plan/home/drafts-shelf.tsx`
- `web/src/plan/home/drafts-shelf.test.tsx`

## Entry points

`OpenDraftNotice`, `DraftActions`, `DraftBody` and `useDraftCommand` in `draft-view.tsx`; `sessionAtom` and the session states in `web/src/plan/api/schema-chat.ts`. Wave 1 changed `setAsideKickoff(store, session, now = false)` and `discardKickoff(store, session, deleteFiles)` in `web/src/plan/chat/commands.ts` and the detail schema in `schema-kickoff.ts`; read them on disk.

## Done

The notice shows "being set aside…" with Stop now while ending; Delete lists `written`, the
committed note, Delete for good and Keep it; Discard sends `delete_files: false`; the null-size
branches are gone; the tests cover each; `./node_modules/.bin/tsc -b --noEmit` from `web/` is
clean, since you are the last web unit. Orchestrator proof:
`./node_modules/.bin/vitest run src/plan/new src/plan/home` from `web/`.

## Traps

No browser dialogs (`alert`, `confirm`, `prompt`): the confirm is in the page. Paths render in monospace. Every button keeps an accessible name; the disabled Set it aside carries its reason through `DisabledReason` as today. The acceptance check greps `setAsideKickoff(store, draft.session, true)` in draft-view.tsx: write that call literally.
