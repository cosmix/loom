# concept-web C1: zod schema for concept pages

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/api/schema-concept.ts`; `src/plan/api/schema-concept.test.ts`
- May read: `../loom/src/plan_server/wire/concept.rs (the Rust types, merged by the concept-data stage)`; `src/plan/api/fixtures/concept-overview.json`; `src/plan/api/fixtures/concept-summary.json`; `src/plan/api/schema.ts (plan 2 style)`

## Entry points

Exports: conceptPageSchema and the inferred types ConceptPage, KeyChip, DocLink, Block, Sentence, Mark, Citation, SourceAnchor, SummaryView, OutOfDate, ImplementsView, PlanLink, EmptyState, Progress, ConceptChanged, plus conceptChangedSchema.

## Steps

1. Write zod schemas that mirror the Rust types field for field (snake_case keys, discriminated unions on kind for Block, Citation and ImplementsView, nullable for Option).
2. Export the inferred types.
3. In the test file, parse both fixtures with conceptPageSchema and assert success, and assert that every reason string (source_changed, rebuilding, awaiting_approval, cannot_rebuild, failed) and every empty kind parses.

## Done

Both fixtures parse.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/api/schema-concept.test.ts`

## Traps

Use top-level it() calls only when a test is a contract; here ordinary describe blocks are fine. Paths in the fixtures are repo-relative (doc/...).

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
