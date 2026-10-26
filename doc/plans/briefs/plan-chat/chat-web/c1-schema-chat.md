# chat-web C1: zod schema for the chat model

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/api/schema-chat.ts`; `src/plan/api/schema-chat.test.ts`
- May read: `../loom/src/plan_server/wire/chat.rs (plan 7)`; `doc/plans/briefs/plan-session-bridge/common.md section The chat model`; `src/plan/api/fixtures/chat-snapshot.json, chat-deltas.json, chat-commands.json, chat-frames.json`

## Entry points

Exports: chatFrameSchema, chatDeltaSchema, chatSnapshotSchema, chatCommandSchema, itemSchema, sessionViewSchema, attachmentSchema, attachmentInputSchema and their inferred types.

## Steps

1. Mirror the Rust types field for field: ChatFrame tagged frame, ChatDelta tagged op, Item tagged type, ChatCommand tagged command; Option fields nullable; times as strings.
2. Export the inferred types.
3. Tests: every entry of the four fixtures parses; every Item type, ToolStatus, PermissionStatus, DenyReason, NoticeKind and Origin value parses.

## Done

All fixture entries parse.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/api/schema-chat.test.ts`

## Traps

Attachment.path, AttachmentInput.path and rests_on entries are repo-relative (doc/...). Never edit a fixture.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
