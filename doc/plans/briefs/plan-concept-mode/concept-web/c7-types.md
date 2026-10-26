# concept-web C7: shared view-state types

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/types.ts`; `src/plan/concept/types.test.ts`
- May read: `src/plan/api/schema-concept.ts (unit C1 in the same wave; import SourceAnchor from it)`

## Entry points

Exports: type OpenRun = { level: 0 | 1 | 2; sentenceId: string; restsOn: SourceAnchorRef[]; parent: string | null }; type SourceAnchorRef = { path: string; anchor: string; title?: string }; type FoldNote = { paragraphOf: string; text: string; path: string }; type FocusTarget = { kind: "sentence" | "passage"; id: string; level: 0 | 1 | 2; runOf: string | null }.

## Steps

1. Write the four types exactly as listed, with a doc comment on each saying which module produces it (open-state, reading-order, view-state).
2. Export nothing else. In types.test.ts use vitest expectTypeOf to pin each shape (for example expectTypeOf<OpenRun["level"]>().toEqualTypeOf<0 | 1 | 2>()), one it() per type.

## Done

The types compile and match the listed shapes exactly.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run --typecheck src/plan/concept/types.test.ts`

## Traps

open-state.ts re-exports OpenRun and FoldNote from this file for the frozen contracts; keep the names exactly.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
