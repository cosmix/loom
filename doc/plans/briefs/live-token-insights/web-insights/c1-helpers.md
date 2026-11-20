# web-insights C1: series and model helpers (codex unit)

Files you write: `web/src/lib/tokens.ts` and `web/src/lib/tokens.test.ts`. Read anything else; write nothing else. Do not run git, do not touch any `.loom` path, do not run oxfmt. Write erasable TypeScript only (no enums, no parameter properties, no namespaces). Your check, from `web/`: `bunx vitest run src/lib` and `bunx tsc -b --noEmit`.

Anchors: `ModelUsage`, `AgentFlow`, `TokenUsage` types exported from `web/src/api/schema.ts` (`ModelUsage = { model: string; effort: string | null; total: TokenUsage }`); the existing helpers in `web/src/lib/tokens.ts` (`fresh`, `maxOf`, `padLeft`, `lastValues`, `modelRates`); the fixture numbers in `doc/plans/briefs/live-token-insights/common.md` section 5.

## Steps

1. Add to `tokens.ts`, each with a `///` doc comment like the existing ones:
   - `export interface ModelShare { model: string; fresh: number; share: number }` and `export function modelShares(models: readonly ModelUsage[]): ModelShare[]`: group by `model`, summing `fresh(total)` over efforts; `share` is the group's fresh over the sum of all fresh (0 to 1); order by fresh descending, then model ascending; `[]` when the list is empty or the fresh sum is 0.
   - `export interface EffortTotals { effort: string | null; fresh: number; cacheRead: number }`, `export interface ModelGroup { model: string; fresh: number; cacheRead: number; efforts: EffortTotals[] }` and `export function modelGroups(models: readonly ModelUsage[]): ModelGroup[]`: the same grouping and order as `modelShares`, with each model's efforts in their input order, `cacheRead` from `total.cache_read`.
   - `export function rollingRate(series: readonly number[], bucketSecs: number, window = 10): number[]`: element `i` is the sum of `series[max(0, i - window + 1) .. i]` times 60 divided by `window * bucketSecs` (tokens per minute over a full window). Same length as `series`. Use a running sum, not a nested loop.
   - `export function cumulativeSeries(total: number, series: readonly number[]): number[]`: element `i` is `total` minus the sum of `series[i + 1 ..]`, never below 0. The last element equals `total`.
   - `export function carryForward(values: ReadonlyArray<number | null>): number[]`: each `null` takes the previous value; leading `null`s are 0.
   - `export function liveCounts(agents: readonly AgentFlow[], length: number, window = 4): number[]`: left-pad each agent's `series` to `length` with `padLeft`; element `i` counts the agents with any value above 0 in `[max(0, i - window + 1), i]`.
2. Tests in `tokens.test.ts` (append; keep its existing tests untouched): with the four fixture models, `modelShares` gives sonnet 1110000 (share ≈ 0.4978), opus 740000 (≈ 0.3318), gpt-6.1-sol 380000, in that order, and `modelGroups` gives opus efforts `xhigh` (560000, cacheRead 7000000) then `high` (180000, 2000000); an empty list and an all-zero list give `[]`; `rollingRate([30, 30, ..., 30] x 12, 30)` ends with 60 and starts with 6; `cumulativeSeries(100, [10, 20, 30])` is `[50, 70, 100]`; `cumulativeSeries(10, [20, 30])` never goes below 0; `carryForward([null, 5, null, 7])` is `[0, 5, 5, 7]`; `liveCounts` with one agent whose padded series has a single non-zero at index 3 of 8 gives `[0, 0, 0, 1, 1, 1, 1, 0]`; a 200000-value series does not throw in `rollingRate` and `cumulativeSeries`.
3. Never spread a series into `Math.max`/`Math.min` arguments; loop (see `maxOf`).

Done means both commands above pass.
