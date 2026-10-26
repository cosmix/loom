# chat-web C6: tool row grouping

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/tool-group.ts`; `src/plan/chat/tool-group.test.ts`
- May read: `src/plan/api/schema-chat.ts (unit C1: Item type)`; `doc/plans/briefs/plan-chat/chat-web/w3-transcript.md section tool-group.ts`

## Entry points

export type ToolGroup = { kind: "group"; items: Item[]; label: string }; export function groupToolRows(items: Item[], turnEnded: boolean): Array<Item | ToolGroup>

## Steps

1. Only when turnEnded: three or more consecutive tool items with no text item between fold into one ToolGroup labelled Used <n> tools · edited <m> files (m counts distinct Edit or Write paths); tool items with status error or denied stay outside and break the run.
2. Tests: two rows no fold, running turn no fold, error in the middle, three rows fold with the right label.

## Done

Four tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/tool-group.test.ts`

## Traps

Nested subagent tool items (subagent set) are not counted in the run; leave them attached to their Agent row.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
