# C2: the per-row token figure

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-rebuilds/common.md` first.

## Files owned

`web/src/plan/summaries/summary-tokens.tsx`, `web/src/plan/summaries/summary-tokens.test.tsx`.

## Files to read

`doc/plans/briefs/plan-summary-rebuilds/rebuild-web/w1-panel.md` (the summary-tokens item), the frozen contract `web/src/plan/summaries/contracts.test.tsx` (turn-rows case), an existing small component and its test in `web/src/components/` (the pattern).

## Entry points

`export function SummaryTokens({ tokens }: { tokens: number | null })`.

## Steps

1. Write `summary-tokens.tsx`: a number renders with thousands separators and the word tokens; null renders the text tokens unknown.
2. Write `summary-tokens.test.tsx` for both cases.
3. Run `bun run typecheck` from web/.

## Done when

The orchestrator runs `bun run test` from web/ and it passes.

## Traps

Use the dashboard typography tokens; no new colours.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Do not run oxfmt; the orchestrator formats after you. No TypeScript parameter properties
  (erasableSyntaxOnly). Import z only from `@/api/zod`.
- Touch no file outside the Files owned list.
