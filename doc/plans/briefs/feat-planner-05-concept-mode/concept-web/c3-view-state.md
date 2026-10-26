# concept-web C3: view state atoms

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/view-state.ts`; `src/plan/concept/view-state.test.ts`
- May read: `src/plan/concept/types.ts (unit C7)`; `doc/plans/briefs/feat-planner-05-concept-mode/concept-web/w1-state.md section view-state.ts`

## Entry points

export const openRunsAtom: (pageKey: string) => PrimitiveAtom<OpenRun[]>; export const notesAtom: (pageKey: string) => PrimitiveAtom<FoldNote[]>; export function clearNotesIn(store, pageKey: string, paragraphOf: string): void

## Steps

1. openRunsAtom is a jotai atomFamily mirrored to sessionStorage under loom-plan:concept:<pageKey> as JSON; every read and write sits in try/catch and a failure keeps the memory value; reads validate with zod and drop bad data.
2. notesAtom is memory only; clearNotesIn removes the notes of one paragraph.
3. Tests: storage failure keeps memory, bad stored data dropped, clearNotesIn removes the paragraph notes, a remount reads the stored runs.

## Done

Four tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/concept/view-state.test.ts`

## Traps

jsdom sessionStorage persists across renders in one test file; clear it in beforeEach.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
