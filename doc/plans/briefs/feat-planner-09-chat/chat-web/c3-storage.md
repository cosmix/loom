# chat-web C3: safe browser storage

Codex unit (gpt-6.1-sol, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/storage.ts`; `src/plan/chat/storage.test.ts`
- May read: `src/plan/state/atoms.ts (plan 2: serverInfoAtom with project_key)`

## Entry points

export const safeStorage (a jotai createJSONStorage over localStorage whose getItem and setItem catch every error); export const storageFullAtom: PrimitiveAtom<boolean>; export function storageKey(projectKey: string, name: string): string returning loom-plan:<projectKey>:<name>

## Steps

1. setItem catches a QuotaExceededError (or any error) and sets storageFullAtom true through the default store; a later successful write sets it false; getItem returns the initial value on any error.
2. Tests: quota error on write sets storageFull, a later success clears it, read error returns the initial value, keys carry the project key.

## Done

Four tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/storage.test.ts`

## Traps

Mock localStorage.setItem to throw a DOMException named QuotaExceededError.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
