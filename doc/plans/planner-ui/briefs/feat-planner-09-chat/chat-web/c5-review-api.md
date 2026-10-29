# chat-web C5: review route client

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/review/api.ts`; `src/plan/review/api.test.ts`
- May read: `../loom/src/plan_server/wire/review.rs (merged by chat-server)`; `src/plan/api/fixtures/review-restore.json, review-compare.json, review-suggestion.json`; `src/plan/api/client.ts (plan 2 apiGet, apiPost)`

## Entry points

Exports zod schemas for every review body and postRestore, getTurnSections, getHashes, resolveRef, getCompare, getAtCommit, getBuilt, getOpenQuestions, getImplementedBy.

## Steps

1. Write the schemas field for field and the fetchers for the paths /api/restore (POST), /api/turn-sections, /api/hashes, /api/ref, /api/compare, /api/at-commit, /api/built, /api/open-questions, /api/implemented-by.
2. Tests: the three fixtures parse; each fetcher calls its path with the right query (stub fetch).

## Done

Tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/review/api.test.ts`

## Traps

Open-question counts on the WebSocket arrive as the review kind open_questions; this file only fetches; another worker subscribes.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
