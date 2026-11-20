# web-insights W2: stage dialog chip trends and the terminal back button

Tier: haiku (`loom-software-engineer`, model haiku). Read `doc/plans/briefs/live-token-insights/common.md` section 6 first. Paths are relative to `web/`. Two independent parts; do part A, then part B.

You own: `src/components/stage-tokens.tsx`, `src/components/stage-tokens.test.tsx`, `src/components/terminal/terminal-view.tsx`, `src/components/terminal/terminal-view.test.tsx`, `src/components/terminal/history-view.tsx`, `src/components/terminal/history-view.test.tsx`, `src/components/stage-modal.tsx`, `src/components/stage-modal.test.tsx`. Never touch `src/token-insights.contracts.test.tsx` (frozen contracts).

## Part A: a trend under each token chip (`stage-tokens.tsx`)

1. Import `Sparkline` from `@/components/sparkline` and `rollingRate`, `cumulativeSeries`, `carryForward`, `liveCounts`, `lastValues` from `@/lib/tokens` (C1 added the first four; read their doc comments).
2. `const TREND_POINTS = 60;`. `Chip` becomes `Chip({ value, label, trend }: { value: string; label: string; trend: { values: number[]; label: string } })` and renders, after the label span, `<Sparkline values={trend.values} width={96} height={20} label={trend.label} className="text-(--tone-executing)" />`.
3. `StageTokens` needs `flow.bucket_secs` from the whole flow, so read the flow once (`const tokens = useAtomValue(snapshotAtom)?.status.tokens; const flow = stageFlow(tokens, stageId);`) and pass to the four chips, in their current order:
   - fresh rate: `lastValues(rollingRate(flow.series, tokens.bucket_secs), TREND_POINTS)`, label `fresh rate, last 30 min`
   - fresh so far: `lastValues(cumulativeSeries(fresh(flow.total), flow.series), TREND_POINTS)`, label `fresh so far, last 30 min`
   - cache hit: `lastValues(carryForward(flow.hit_series), TREND_POINTS)`, label `cache hit per 30 s, last 30 min`
   - agents live: `lastValues(liveCounts(flow.agents, flow.series.length), TREND_POINTS)`, label `agents live, last 30 min`
   Keep `StageTokens` under 50 lines (put the four trend computations in a `chipTrends(tokens, flow)` helper if needed).
4. `stage-tokens.test.tsx`: add a test that each of the four labels is found exactly once as role `img` inside the region `stage tokens`. The existing tests (chip texts `43.2k/m`, `2.23M`, `93.1%`, `3 of 3`; lanes) must still pass unchanged: a sparkline's label is an `aria-label`, not text.

## Part B: the back button (`terminal-view.tsx`, `history-view.tsx`, `stage-modal.tsx`)

1. `terminal-view.tsx`: add `onBack?: () => void` to `TerminalViewProps`; export `function BackButton({ onBack }: { onBack: () => void })` rendering `<button type="button" aria-label="Back to details" title="Back to details (Esc)" onClick={onBack} className="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"><ArrowLeftIcon className="size-4" /></button>` (lucide-react). Pass `onBack` from `TerminalView` to `Head`. In `Head` (`:136-172`) replace the eyebrow span `<span className="eyebrow rise" style={rise(0)}>Terminal</span>` with `<div className="rise flex items-center gap-2" style={rise(0)}>{onBack && <BackButton onBack={onBack} />}<span className="eyebrow">Terminal</span></div>`. The file is 341 lines; stay under 400.
2. `history-view.tsx`: add `onBack?: () => void` to `HistoryViewProps`. `HistoryView` (`:49-143`) is about 95 lines: move its `<header>` into `function HistoryHead({ stage, frame, onBack })` and its `<footer>` into `function HistoryFoot({ stage, session, frame })` in the same file, so `HistoryView` stays under 50 lines, and give `HistoryHead` the same eyebrow row with `BackButton` (eyebrow text `Terminal history`).
3. `stage-modal.tsx`: pass `onBack={toDetails}` to both `<HistoryView ...>` (`:173-179`) and `<TerminalView ...>` (`:183-188`). Update the doc comment at `:79-84` to say the back arrow returns to the details in either mode, like Esc does in view mode.
4. Tests:
   - `stage-modal.test.tsx`: from `?stage=<id>&view=terminal` (use the file's `renderModal`), clicking the button named `Back to details` leaves the router location with `stage=<id>` and without `view`, and the details render again; the same after switching to control mode first if the file already has a helper that does so, otherwise the view-mode case only; and from a stage whose history view shows (see the file's history tests), the button returns to the details too.
   - `terminal-view.test.tsx`: no `Back to details` button when `onBack` is not given (page frame); with `onBack` given, clicking it calls it once.
   - `history-view.test.tsx`: the same two cases for `HistoryView`.

## Proof

From `web/`: `bun run typecheck`, `bun run lint`, `bunx vitest run src/components`. Report counts. Do not run `bun run build`.
