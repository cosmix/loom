# Codex units of stage `handoff-web`

IMPLEMENTER units (`loom-codex-forwarder`), not the product's Codex `$pressure` step. Each:
`--model gpt-6.1-terra --effort high`, FOREGROUND, Bash timeout 600000 ms, only its files, no git,
no tests run (the orchestrator runs them), no dot-loom path, no `oxfmt` (the orchestrator formats),
no TypeScript parameter properties (`erasableSyntaxOnly`). Run from `web/`. They run first, before
W1 and W2, which import them.

- **CW1** owns `src/plan/chat/pressure/use-pressure.ts` and `src/plan/chat/pressure/use-pressure.test.ts`:
  jotai atom `pressureRunAtom` (latest `PressureRunView`), `fetchPressureOptions()`,
  `startPressure(body: PressureStart)`, `stopPressure()`, `confirmConfig()` through plan 2's
  `apiGet`/`apiPost` (`web/src/plan/api/client.ts`), the socket subscription
  `subscribe("pressure_run", ...)`, and the resync refetch through
  `store.get(planSocketAtom).onResync(handler)` (`@/plan/state/atoms`); schemas from
  `src/plan/api/schema-pressure.ts` (stage `app-handoff`). Tests: the atom updates on a frame; a
  resync refetches options. Proof: `node_modules/.bin/vitest run src/plan/chat/pressure/use-pressure.test.ts`.
- **CW2** owns `src/plan/run/use-run.ts` and `src/plan/run/use-run.test.ts`: `runStateAtom`,
  `runStripAtom`, `runOutputAtom` (lines per job, capped at 5,000, oldest dropped), fetchers and
  posts for `/api/run/state`, `/api/run/commit`, `/api/run/start`, `/api/run/abandon`
  (`{ confirmed_unmerged }`), `/api/run/finish-abandon`, `/api/run/dashboard`, subscriptions to
  `run_state`, `run_output`, `run_strip`, and the resync refetch of `/api/run/state`. Tests: the
  5,000-line cap; the abandon body carries the list given. Proof: `node_modules/.bin/vitest run src/plan/run/use-run.test.ts`.
- **CW3** owns `src/plan/offers/actions-pressure.ts` and `src/plan/offers/actions-pressure.test.ts`:
  `registerOfferAction("pressure_test", handler)` from plan 2's `src/plan/offers/actions.ts`,
  opening W1's dialog through an atom `pressureDialogAtom` (`{ open: boolean, plan: string | null }`,
  defined in this file) with the offer's repo-relative `target`. Test: running the offer sets the
  atom. Proof: `node_modules/.bin/vitest run src/plan/offers/actions-pressure.test.ts`.
- **CW4** owns `src/plan/offers/actions-run.ts` and `src/plan/offers/actions-run.test.ts`: the same
  for `start_run` with `runDialogAtom` (`{ open: boolean, plan: string | null }`). Proof:
  `node_modules/.bin/vitest run src/plan/offers/actions-run.test.ts`.
