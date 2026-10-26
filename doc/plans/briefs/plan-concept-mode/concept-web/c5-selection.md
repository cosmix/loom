# concept-web C5: selection provenance

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/selection.ts`; `src/plan/concept/selection.test.ts`
- May read: `src/plan/api/schema-concept.ts (unit C1)`; `the plan 3 AttachmentDraft type in src/plan/attachments/`; `doc/plans/briefs/plan-concept-mode/concept-web/w1-state.md section selection.ts`

## Entry points

export function attachmentFromSelection(sel: Selection, page: ConceptPage): AttachmentDraft | null; export function attachmentsFromSelection(sel: Selection, page: ConceptPage): AttachmentDraft[]

## Steps

1. Read the elements the selection covers through their data attributes (data-sentence-id, data-summary, data-passage="path#anchor", data-line).
2. Summary arm: { kind: "summary", summary: <summary id>, sentence: <sentence id>, text, restsOn: <that sentence rests_on, repo-relative>, hash: page.file_hash }. Source arm: { kind: "source", path: <repo-relative path>, anchor, text, lines, stageId: null, hash: <the passage DocView hash, read from data-hash on the passage block> }.
3. Tests in jsdom fragments: summary provenance, passage provenance with lines, spanning levels gives one draft per part.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/concept/selection.test.ts`

## Traps

The reading column (another worker) sets data-hash on each passage block; state that attribute name in a doc comment so the two match.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
