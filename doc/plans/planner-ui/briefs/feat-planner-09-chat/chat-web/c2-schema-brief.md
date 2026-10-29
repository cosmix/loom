# chat-web C2: zod schema for the session brief

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/api/schema-session-brief.ts`; `src/plan/api/schema-session-brief.test.ts`
- May read: `src/plan/api/fixtures/session-brief.json (plan 8)`

## Entry points

Exports: sessionBriefSchema and type SessionBriefView = { session_id, label, brief, model, effort, display_name }, plus getSessionBrief(id) using plan 2 apiGet on /api/session/brief?session=<id>.

## Steps

1. Write the schema and the fetcher.
2. Test: the fixture parses; a 404 answer resolves to { ok: false, status: 404 } through apiGet.

## Done

Two tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/api/schema-session-brief.test.ts`

## Traps

Stub fetch in the test; apiGet already maps statuses.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
