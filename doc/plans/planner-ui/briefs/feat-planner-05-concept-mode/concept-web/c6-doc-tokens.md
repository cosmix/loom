# concept-web C6: document colour token test

Codex unit (gpt-6.1-terra, effort high). Working dir: web/.

## Files

- Owns: `src/plan/concept/doc-tokens.test.ts`
- May read: `src/index.css`; `src/aurora-ui/shared/styles/tokens.css`

## Entry points

A vitest file only; no production code.

## Steps

1. Read src/index.css as text with node:fs; parse the eight --doc-1 .. --doc-8 OKLCH values in the :root block and in the .dark block; read --background the same way from tokens.css.
2. Convert OKLCH to sRGB and to relative luminance in the test (no dependency) and assert: each token defined in :root and .dark; no hue in 50-90; contrast at least 3:1 against --background in both themes.

## Done

The file compiles. It is red until the reading-column worker adds the tokens, so the orchestrator runs it after that worker.

Proof (the orchestrator runs it after you): `node_modules/.bin/vitest run src/plan/concept/doc-tokens.test.ts`

## Traps

Do not edit index.css. OKLCH values look like oklch(0.5 0.15 252); a hue may be given with deg.

Rules: erasable TypeScript only (no parameter properties, enums or namespaces; tsconfig sets
erasableSyntaxOnly). Import z only from @/api/zod. No dangerouslySetInnerHTML. Do not run oxfmt;
the orchestrator formats after you. Keep the file under 360 lines. Your own check is
node_modules/.bin/tsc -b --noEmit from web/. Do not run git. Do not touch any dot-loom or dot-work
path.
