# Web C3: Sparkline and HeatStrip components (codex unit)

Do not run git. Do not touch any path under .loom. Do not run oxfmt. Read doc/plans/briefs/token-flow/common.md section 2. Read doc/loom/knowledge/conventions/web-dashboard-typography.md for the styling rules: colours only through existing CSS tokens such as var(--tone-executing) or currentColor, and no new uppercase mono labels.

## Files

- Write: web/src/components/sparkline.tsx and web/src/components/sparkline.test.tsx.
- Read only: web/src/components/context-meter.tsx (component and className conventions, the cn helper).

## Surface (pinned)

    export function Sparkline(props: { values: readonly number[]; width: number; height: number; label: string; className?: string })
    export function HeatStrip(props: { values: readonly number[]; max: number; label: string; className?: string })

Leave the return types inferred: React 19's types declare `JSX` only inside the React namespace, so a bare `JSX.Element` annotation fails typecheck with "Cannot find namespace 'JSX'".

## Rules

- Sparkline renders one svg with role="img", aria-label=label and viewBox "0 0 width height".
  - It draws an area path (fill currentColor, fill-opacity 0.2) and a line path (stroke currentColor, stroke-width 1.25, fill none).
  - Points are scaled to the maximum of values; with all zeros or no values the line runs along the bottom edge.
  - It ends with a circle of r 1.75 at the last point.
- HeatStrip renders a div with role="img", aria-label=label and a CSS grid of one cell per value (grid-auto-flow column).
  - Each cell is a span whose background is color-mix(in oklch, var(--tone-executing) P%, transparent), with P = 18 + 82 * value / max, rounded.
  - A zero value is transparent.
  - Each cell carries data-level 0 to 4: 0 for zero, otherwise ceil(4 * value / max).

## Tests (sparkline.test.tsx, @testing-library/react)

- The aria-label is present.
- The path d strings contain one point per value.
- An all-zero input renders without NaN in any attribute.
- HeatStrip renders as many cells as values.
- A zero cell has data-level 0 and the max cell has data-level 4.

Done when `bun run test src/components/sparkline` passes from web/. Write erasable TypeScript only: no parameter properties, enums or namespaces (tsconfig sets erasableSyntaxOnly; vitest accepts code that `tsc -b` rejects with TS1294). The orchestrator runs `bun run typecheck` after your wave.
