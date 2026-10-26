# chat-web C7: changed-since checkpoint

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/review/checkpoint.ts`; `src/plan/review/checkpoint.test.ts`
- May read: `src/plan/chat/storage.ts (unit C3: safeStorage, storageKey)`; `doc/plans/briefs/feat-planner-09-chat/chat-web/w6-review.md section checkpoint.ts`

## Entry points

export type Checkpoint = { commit: string | null; hashes: Record<string, string> }; export const checkpointAtom; export function changedSince(cp: Checkpoint, current: Record<string, string>): Array<{ path: string; change: "added" | "changed" | "removed" }>; export function markSeen(store, path: string, hash: string): void

## Steps

1. checkpointAtom is stored through safeStorage under storageKey(project_key, "checkpoint"), zod-validated on read.
2. changedSince compares hashes (pure); markSeen updates one path.
3. Tests: stored per project key, removed document listed, changedSince pure comparison.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/review/checkpoint.test.ts`

## Traps

The first-visit rule (set silently) is the UI worker job, not this file.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
