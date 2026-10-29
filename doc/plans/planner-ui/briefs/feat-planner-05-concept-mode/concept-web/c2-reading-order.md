# concept-web C2: reading order

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/reading-order.ts`; `src/plan/concept/reading-order.test.ts`
- May read: `src/plan/api/schema-concept.ts (unit C1)`; `src/plan/concept/types.ts (unit C7)`; `doc/plans/briefs/feat-planner-05-concept-mode/concept-web/w1-state.md section reading-order.ts`

## Entry points

export function readingOrder(page: ConceptPage, open: OpenRun[], passages: Record<string, string[]>): FocusTarget[] where passages maps a sentence id to the passage keys path#anchor it opened.

## Steps

1. Sentences in document order; inside an opened sentence its run (level-1 sentences in citation order, or its passages as one target each), recursively; a folded run never appears.
2. Write tests: closed page, one open level-0 run, nested level-1 run with two passages, folded run never listed.

## Done

Four tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/concept/reading-order.test.ts`

## Traps

A sentence opens only if an OpenRun with its sentenceId exists; level-0 sentences citing a Source open straight to passages.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
