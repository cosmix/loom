# W4: the web wire for commands and the user item's chip field

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run alone in wave 4, after W2 (the `command` field on `Item::User`) and W3 (`session-commands.json`, `slash-vectors.json`, `PlanMessage::Commands`) returned. Paths are from the repository root; run commands from `web/`.

## Files owned

`web/src/plan/api/schema.ts`, `schema-chat.ts`, `schema-session-commands.ts` (new), `schema.test.ts`, `schema-chat.test.ts`, `web/src/plan/chat/**` (only the `command: null` additions below), `web/src/plan/help/chat-scene.tsx`.

Read only: `session-commands.json`, `slash-vectors.json` (W3's) and the chat fixtures (W2 added `"command": null` to their user items); Rust compares all of them. Every other web file is read only too.

## What exists (verified 2026-11-10)

- `schema-chat.ts:132-139` the strict user-item schema `{type: "user", id, text, attachments, delivery, origin: originSchema.nullable()}`; the chat fixtures hold user items (`chat-snapshot.json` four, `chat-deltas.json` one). The Rust parity test (`loom/src/plan_server/wire/tests_chat.rs`) loads them, so after the field is added each user item needs `"command": null` (or a populated value) exactly as the Rust serializer writes it.
- Typed user-item literals (`type: "user"`) in `web/src/plan`: `help/chat-scene.tsx`, `chat/chips-test-kit.tsx`, `chat/state-line.test.tsx`, `chat/model-commands.test.ts`, `chat/model.test.ts`, `chat/resend-control.tsx`, `chat/chips.tsx`, `chat/start-chat-test-kit.tsx`, `chat/model-file-send.test.ts`, `chat/chips-marks.test.tsx`, `chat/model-identity.test.ts`, `chat/composer-delivery.test.tsx`, `chat/resend-once.test.tsx`, `chat/pressure/seams.test.tsx` (`rg -ln 'type: "user"' web/src/plan` lists them; recount when you start).
- `schema.ts:31-54` `PLAN_MESSAGE_TYPES`, `:118-131` `planMessageSchema`.

## Steps

1. **`schema-session-commands.ts`**: `commandEntrySchema` (strict: `name`, `aliases` array, `description`, `argument_hint`, `source` enum `built-in | bundled | plugin | personal | account | skill`, `class` enum `run | run-with-handling | terminal-only`, `reason` nullable), `commandsViewSchema` (strict: `state` enum `loading | ready | unavailable`, `generation` nullable `u64`, `reason` nullable, `entries`), `commandsMessageSchemas` (`type: "commands"`, `session`, and the view's fields), and the exported types `CommandEntry`, `CommandsView`.
2. **`schema.ts`**: `"commands"` in `PLAN_MESSAGE_TYPES`; spread `commandsMessageSchemas` into `planMessageSchema`.
3. **`schema-chat.ts`**: `commandRefSchema = z.strictObject({ name: z.string(), args: z.string() })` and `command: commandRefSchema.nullable()` on the user item. Keep the file under 400 lines (347 today).
4. **Fixtures**: W2 already gave every user item of `chat-snapshot.json` and `chat-deltas.json` its `"command": null`; check that the schema parses them.
5. **Literals**: add `command: null` to every typed user-item literal; change nothing else in those files.
6. **Tests**: `schema.test.ts` parses `session-commands.json` under `commandsViewSchema` and a `{type: "commands", session, ...view}` frame under `planMessageSchema`, each rejecting an extra field; `schema-chat.test.ts` parses a user item with a populated `command` and rejects one without the key.
7. **Run** `node_modules/.bin/vitest run src/plan/api src/plan/chat src/plan/help` once ready. You cannot run `tsc` or `bun run build` (the subagent guard); the main agent type-checks.

## Done when

The schemas parse both fixtures unchanged, `rg -n 'type: "user"' web/src/plan` finds no literal without `command`, and the tests above pass.

## Traps

- A help scene parses its literals at module load: a missing key fails every suite that imports the help footer.
- The composer, the menu and the chip rendering are `planner-web`'s: add no UI here.
