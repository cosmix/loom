# chat-web C4: visible control characters

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/chat/visible-controls.ts`; `src/plan/chat/visible-controls.test.ts`
- May read: `doc/plans/briefs/feat-planner-09-chat/common.md section Fix-round rules, bullet Untrusted text, visibly`

## Entry points

export function visibleControls(text: string): string; export function Untrusted(props: { text: string }): JSX.Element (a span with style unicode-bidi plaintext and direction ltr around visibleControls(text))

## Steps

1. Replace every Unicode Cf character (bidi controls U+202A-202E and U+2066-2069, U+200B-200F, U+061C, U+FEFF, and any other \p{Cf}) with the marker ⟨U+XXXX⟩ (four or more uppercase hex digits).
2. Tests: every listed character escapes; plain text unchanged; Untrusted renders no raw U+202E in its text content.

## Done

Three tests pass.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/chat/visible-controls.test.ts`

## Traps

Use a regex with the u flag and \p{Cf}; iterate code points, not UTF-16 units.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
