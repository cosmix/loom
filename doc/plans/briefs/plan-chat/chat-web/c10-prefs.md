# chat-web C10: panel preferences

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/prefs.ts`; `src/plan/chat/prefs.test.ts`
- May read: `src/plan/chat/storage.ts (unit C3)`

## Entry points

export const panelWidthAtom (rem, clamped 20-40, default 26); export const panelCollapsedAtom (boolean, default false)

## Steps

1. Both atoms use atomWithStorage with safeStorage under storageKey(project_key, "panel-width") and storageKey(project_key, "panel-collapsed"); the width setter clamps to 20-40.
2. Tests: width and collapsed persist in localStorage, width clamps to 20-40, a write failure keeps memory.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/prefs.test.ts`

## Traps

Desktop notification opt-in reuses web/src/state/notifications.ts; do not add a third atom for it.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
