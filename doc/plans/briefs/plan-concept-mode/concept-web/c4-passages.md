# concept-web C4: passage cache

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/passages.ts`; `src/plan/concept/passages.test.ts`
- May read: `src/plan/files/address.ts (plan 3: docRel)`; `the plan 3 DocView zod schema (find it with loom map --find-all docViewSchema)`; `src/plan/api/client.ts (plan 2: apiGet)`

## Entry points

export function passageOf(doc: DocView, anchor: string): { title: string; anchor: string; startLine: number; endLine: number; markdown: string; hash: string } | null; export const docViewAtom: (repoPath: string) => Atom<Promise<DocView>>

## Steps

1. docViewAtom fetches apiGet("/api/doc?path=" + encodeURIComponent(docRel(repoPath)), schema) once per path and exposes a refresh function refreshDoc(store, repoPath).
2. passageOf cuts the lines of doc.text for the section whose anchor matches (the preamble "" runs from line 1 to the first section start); hash is doc.hash plus "#" plus the anchor.
3. Tests: nested section, preamble, missing anchor gives null.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/concept/passages.test.ts`

## Traps

Wire paths are repo-relative (doc/...); the request parameter is doc-relative, so always pass through docRel.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
