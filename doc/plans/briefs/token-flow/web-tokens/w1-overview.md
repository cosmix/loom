# Web W1: overview surfaces (Claude, sonnet)

Read `doc/plans/briefs/token-flow/common.md`, then `doc/loom/knowledge/conventions/web-dashboard-typography.md`, then the approved mockup <https://claude.ai/artifact/BFDRMiWwtsv653hqsMPvED> (Mockups A1 and A2; amber outlines mark the additions). Wave 1 has already landed:

- `api/schema.ts`: `TokenFlow` and the other types, with `status.tokens` optional;
- `lib/tokens.ts`: `fresh`, `compactTokens`, `cacheHitPercent`, `stageFlow`, `modelRates`;
- `components/sparkline.tsx`: `Sparkline`, `HeatStrip`.

Use them and do not edit those files. Everything that exists today stays where it is; you only add. Paths are under `web/src/`.

Read-only and frozen: `src/token-flow.contracts.test.tsx`. Its contracts render the real routes from `src/api/fixtures/snapshot.json` and expect: the region named "Token flow" with "43.2k" on `/`; the ledger header order context, tokens, time; the text "43.2k/m" in the server row's `data-label="tokens"` cell on `/ledger` and in the server graph card on `/`; and "43.2k", "2.23M" and "93%" inside the `<header>` (role banner). Render rates as `{compactTokens(fresh(rate))}/m` and the rail's cache hit as `{Math.round(cacheHitPercent)}%`.

## Files you own

- `components/header.tsx`, `components/header-lines.tsx`, `header.css`
- `components/token-flow-panel.tsx` (new), `components/token-flow-panel.test.tsx` (new)
- `routes/overview.tsx`, `routes/overview.test.tsx`, `routes/ledger.tsx`, `routes/ledger.test.tsx`
- `components/ledger-table.tsx`, `components/ledger-row.tsx`
- `components/graph/stage-node.tsx`, `components/graph/stage-node.test.tsx`, `components/graph/stage-graph.tsx`, `components/graph/stage-graph.test.ts`
- `lib/graph.ts`, `lib/graph.test.ts`
- `components/legend-dialog.tsx`

## Changes

1. **TOKENS rail in the header.**
   - `header-lines.tsx`: a `TokenLine` component reads `snapshotAtom`. It renders nothing when `status.tokens` is absent.
   - Otherwise it renders a rail titled "tokens", styled like `StatRail`, with four cells:
     - "{compactTokens(fresh(plan.rate))}" labelled "fresh / min";
     - a `Sparkline` of the last 30 values of `plan.series`, labelled "last 15 min";
     - "{compactTokens(fresh(plan.total))}" labelled "plan total";
     - "{cacheHitPercent rounded}%" labelled "cache hit".
   - `StatCell.count` is a number, so give `TokenLine` its own cell markup, matching `StatCell`'s classes.
   - `header.tsx`: mount `<TokenLine/>` after `<MergeLine/>` in the same flex row.
2. **Token flow panel** (`token-flow-panel.tsx`): a `<section aria-label="Token flow">` with an eyebrow h2 "Token flow" and a right-aligned "5 min avg". It renders nothing when `status.tokens` is absent. It contains:
   - the plan fresh rate, large, in mono, with "fresh / min";
   - an SVG stacked-area chart of each stage's last 60 series values, stacked in `stages` order:
     - each stage coloured from a fixed palette of the existing `--tone-*` tokens, cycled;
     - three y ticks;
     - x labels "−30m", "−15m", "now";
     - all text in theme tokens;
   - a legend of stage ids;
   - a mix bar and a label/value grid of fresh input, cache write, output and cache read per minute (`plan.rate`), plus cache hit;
   - per-model rates from `modelRates`.

   Mount it in `routes/overview.tsx`'s aside between `AttentionPanel` and `ActivityPanel`. In `routes/ledger.tsx`, mount it as its own full-width row between `<LedgerTable />` and the panel grid. Do not put it inside that grid: it is `lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]` with exactly two children (`AttentionPanel`, `ActivityPanel`), and a third child would sit beside Attention and push Activity onto a new row.
3. **Ledger column.** In `ledger-table.tsx`, `COLUMNS` gains "tokens" between "context" and "time"; fix the "eight-column" comment. `ledger-row.tsx` adds `<TableCell data-label="tokens">` between the context and time cells:
   - an executing stage with a flow: `<Sparkline>` of the last 20 series values plus "{compactTokens(fresh(rate))}/m";
   - a completed stage with a flow: "{compactTokens(fresh(total))} total", dimmed;
   - otherwise "—", dimmed.
4. **Graph node token line.**
   - `lib/graph.ts`: add `NODE_TOKENS_HEIGHT = 18`. The node `Footer` is `flex flex-col gap-0.5 leading-4`, so each extra line is 16 px plus a 2 px gap, which is why `NODE_METER_HEIGHT` is 18; a 16 would leave layout and card 2 px apart.
   - `nodeHeight(stage, hasTokens)`: the footer now shows when `hasFooter(stage) || hasTokens`, so it adds `NODE_FOOTER_HEIGHT` in that case (not only when `hasFooter`), plus `NODE_TOKENS_HEIGHT` when `hasTokens`.
   - Thread a per-stage `hasTokens`, true when `stageFlow(flow, stage.id)` exists, from `stage-graph.tsx` into the dagre layout and into `StageNodeData`. Layout and component must agree; the comment in `lib/graph.ts` says so. Key the layout `useMemo` (`stage-graph.tsx`, `layoutStages(stages)`) on a string of the ids of stages that have a flow, not on the flow object: every status frame carries a new object, which would re-run the dagre layout each second.
   - `stage-node.tsx` renders `Footer` when `hasFooter(stage) || hasTokens`, and Footer renders a third line when `hasTokens`:
     - executing: a `Sparkline` of the last 20 series values plus "{rate}/m";
     - otherwise: "{total} total", dimmed.
5. **Legend** (`components/legend-dialog.tsx`): after the "context" entry add a "tokens" entry, "fresh tokens per minute (input + cache writes + output), 5 min average; the 5h time left is at the last 30 min average", in the same `dt`/`dd` markup.
6. **Tests.** Update every test the new column or height breaks by fixing its numbers, never by deleting assertions. Add focused tests:
   - `TokenLine` absent without tokens;
   - the panel shows "43.2k" from the fixture;
   - `nodeHeight` grows by 18 with tokens, and by `NODE_FOOTER_HEIGHT + 18` for a stage without a footer;
   - the ledger header order.

## Check

From `web/`: `bun run typecheck`, `bun run lint`, `bun run format` then `bun run format:check`, and `bun run test`.
