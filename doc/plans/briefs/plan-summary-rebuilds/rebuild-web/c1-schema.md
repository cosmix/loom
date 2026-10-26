# C1: summary zod schemas

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-rebuilds/common.md` first.

## Files owned

`web/src/plan/api/schema-summaries.ts`, `web/src/plan/api/schema-summaries.test.ts`.

## Files to read

`doc/plans/briefs/plan-summary-rebuilds/common.md` section "Wire", the fixtures `web/src/plan/api/fixtures/summary-panel.json`, `summary-question.json`, `summary-turn-cost.json` (stage rebuild-engine wrote them), plan 2 merged `web/src/plan/api/schema.ts` (the pattern), the dashboard quota schema under `web/src/api/` (reuse it for the quota field).

## Entry points

Exported zod schemas and inferred types: `summaryPanelSchema`, `summaryQuestionSchema`, `summaryTurnCostSchema`, and message schemas for the types summary_panel, summary_question, summary_question_closed, summary_turn_cost.

## Steps

1. Write `schema-summaries.ts`.
2. Write `schema-summaries.test.ts` parsing each fixture with its schema and rejecting one mutated fixture each.
3. Run `bun run typecheck` from web/.

## Done when

The orchestrator runs `bun run test` and `bun run typecheck` from web/ and both pass.

## Traps

Token fields are optional numbers: absent means unknown, never 0.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Do not run oxfmt; the orchestrator formats after you. No TypeScript parameter properties
  (erasableSyntaxOnly). Import z only from `@/api/zod`.
- Touch no file outside the Files owned list.
