# chat-web C9: composer draft

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/drafts.ts`; `src/plan/chat/drafts.test.ts`
- May read: `src/plan/chat/storage.ts (unit C3)`; `src/plan/api/schema-chat.ts (unit C1)`; `the plan 3 AttachmentDraft type`

## Entry points

export const draftAtom: WritableAtom<{ text: string; attachments: AttachmentDraft[] }>

## Steps

1. draftAtom uses atomWithStorage with safeStorage under storageKey(project_key, "draft"), validated with zod on read (bad data dropped).
2. Tests: storage failure keeps memory, bad stored draft dropped, draft survives a remount.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/drafts.test.ts`

## Traps

project_key comes from plan 2 serverInfoAtom; until it loads use the key name without a project and migrate nothing.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
