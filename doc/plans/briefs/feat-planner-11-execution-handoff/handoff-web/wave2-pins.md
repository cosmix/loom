# Wave 2 pins of stage `handoff-web`

Shared by units W1a, W1b, W1c, W2a, W2b, W2c. Work from `web/`. Parallel units compile together
only through the interfaces pinned here: import a sibling's file by its pinned path even when it
does not exist yet, and do not write it. No git, no `.loom` paths, no `oxfmt`, no TypeScript
parameter properties, no `dangerouslySetInnerHTML`; server text (log tails, run output, branch
names, plan paths, error messages) renders as text through React children or
`Untrusted` from `@/plan/chat/visible-controls`.

## Already written (read, import, do not edit)

- `src/plan/chat/pressure/use-pressure.ts`: `pressureRunAtom`, `pressureOptionsAtom`
  (`ApiResult<PressureOptions> | null`), `pressureRunWaitingAtom`, `fetchPressureOptions`,
  `startPressure(body: PressureStart)`, `stopPressure`, `confirmConfig`,
  `installPressureSubscription(store)`, `usePressureRun`, `usePressureOptions`.
- `src/plan/run/use-run.ts`: `runStateAtom`, `runStripAtom`, `runOutputAtom`
  (`Record<RunJob, string[]>`), `fetchRunState(store)`, `commitRun(store, body)`,
  `startRun(store, body)`, `abandonRun(store, confirmed_unmerged)`, `finishAbandon(store)`,
  `fetchDashboard(store)` (returns `{ url, tunnel? }`), `installRunSubscription(store)`.
- `src/plan/offers/actions-pressure.ts`: `pressureDialogAtom`, `registerPressureOfferAction(store)`.
- `src/plan/offers/actions-run.ts`: `runDialogAtom`, `registerRunOfferAction(store)`.
- Schemas and fixtures: `src/plan/api/schema-pressure.ts`, `schema-run.ts`,
  `src/plan/api/fixtures/{pressure-options,pressure-run,run-state,run-strip,run-output}.json`.
- Plan 2 helpers: `DisabledReason` (`@/plan/ui/disabled-reason`), `LoadState`
  (`@/plan/ui/load-state`), `authorBlockReasonAtom`, `isHolderAtom`, `serverInfoAtom`,
  `planSocketAtom` (`@/plan/state/atoms`), `Untrusted` (`@/plan/chat/visible-controls`),
  `toAddress` (`@/plan/files/address`), `BusyRoundel` (`@/aurora-ui/feedback/BusyRoundel`),
  `Dialog` (`@/components/ui/dialog`), diff links as in `src/plan/chat/turn-footer.tsx`
  (`openDiff` from `@/plan/diff/open-diff`).
- Frozen contracts (READ-ONLY, they must pass): `src/plan/run/contracts/abandon.contract.test.tsx`
  and `src/plan/chat/pressure/contracts/pressure.contract.test.tsx`. Read both before writing the
  component they render. Test style reference: the same files (Provider + createStore +
  MemoryRouter, `initClient(store)`, `hello` fixture for holder/read-only state, a `fetch` spy).

## Pinned exports (the files below are written by the unit named; import them by these paths)

| Path | Export | Unit |
| --- | --- | --- |
| `@/plan/chat/pressure/pressure-dialog` | `PressureDialog(): JSX.Element \| null`, no props; reads `pressureDialogAtom`, `usePressureOptions` | W1a |
| `@/plan/chat/pressure/pressure-header` | `PressureHeader({ session }: { session: SessionView })` | W1b2 |
| `@/plan/chat/pressure/codex-status` | `CodexStatusItem({ status, startedMs }: { status: CodexStatus; startedMs?: number })` | W1b1 |
| `@/plan/chat/pressure/steps` | `stepLabel(kind: PressureStepKind): string` (`/pressure`, `Codex`, `/address`); `stepForSession(run: PressureRunView \| null, sessionId: string): PressureStepView \| null`; `codexRunsAlone(run: PressureRunView \| null): boolean` (true only while no `ended`, the latest step is `codex` and its state is `running`); `endLine(end: PressureEnd): string` (`recorded` → "Pressure test recorded" plus ` · <line>` when `line`; `stopped` → "Stopped by <by>"; `failed` → "Step <step> failed: <reason>"; `interrupted` → "Interrupted") | W1b1 |
| `@/plan/run/run-dialog` | `RunDialog(): JSX.Element \| null`, no props; reads `runDialogAtom`, `runStateAtom` | W2a |
| `@/plan/run/run-output` | `RunOutput({ job }: { job: RunJob })`, reads `runOutputAtom` | W2a |
| `@/plan/run/dashboard-button` | `DashboardButton(): JSX.Element \| null` | W2a |
| `@/plan/run/abandon-dialog` | `AbandonDialog({ state }: { state: RunStateView })`, also the "Finish abandoning" control | W2b |
| `@/plan/run/run-strip` | `RunStrip({ compact }: { compact?: boolean })` | W2c |
| `@/plan/run/plan-run-actions` | `PlanRunActions({ path, state }: { path: string; state: PlanStateView })` (`PlanStateView` from `@/plan/api/schema-files`) | W2c |
| `@/plan/run/handoff-mounts` | `HandoffMounts(): JSX.Element` | W2c |

`HandoffMounts` is the single wiring point: in `useEffect` it calls `installPressureSubscription`,
`installRunSubscription`, `registerPressureOfferAction`, `registerRunOfferAction` with
`useStore()` and returns their cleanups, calls `void fetchRunState(store)` once, and renders
`<PressureDialog />` and `<RunDialog />`. It is mounted from `src/plan/header-extras.tsx`.

## Cross-cutting rules

- Holder-only write controls (Start, Stop, Commit, Abandon, Finish abandoning, Pressure-test,
  Start a run, I have checked these files) render as `<DisabledReason reason={...} onClick={...}>`
  with the reason from `authorBlockReasonAtom` (null for the holder); tests assert it with
  `toHaveAccessibleDescription`.
- Fetch states use `<LoadState/>`. Components that call the API take the jotai store from
  `useStore()`.
- A file stays under 400 lines, a function under 50; split a component file rather than exceed.
- Tests sit beside the component as `<name>.test.tsx`; vitest with Testing Library as the
  contract tests do. Do not run tests; at most one compile-only `node_modules/.bin/tsc --noEmit -p .`
  filtered to your files, and expect errors only from siblings not yet written.
