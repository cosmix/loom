# Brief: web-dialog (locked plan overview dialog)

Tier: `loom-software-engineer` (sonnet). Wave 1. Read `doc/plans/briefs/open-github-issues/common.md` first, then open `doc/plans/briefs/open-github-issues/plan-overview/mockup.html` in full: it is the approved design (layout A). Its numbers are example data.

## Owns

`web/src/components/plan-overview/**` (new), `web/src/routes/shell.tsx`, `web/src/api/schema.ts`, `web/src/api/schema.test.ts`.

Frozen, read-only: `web/src/components/plan-overview/plan-overview.contract.test.tsx`.

## Wire (pinned; the Rust record in `loom/src/plan_stats/record.rs`)

`status.plan_stats?: PlanStats` with `schema_version`, `plan_id`, `plan_name`, `started_at`, `ended_at`, `wall_secs`, `execution_secs`, `totals {input, cache_write, cache_read, output}`, `stages[] {id, name, stage_type, status, started_at?, completed_at?, execution_secs, model?, effort?, phases[] {phase: "contract"|"executing"|"adjudication"|"retry"|"waiting", started_at, ended_at?}, tokens}`, `models[] {model, effort?, lane: "claude"|"codex", agents, tokens}`. Stage summaries also gain `phases` and `completed_at`. Add `planStatsSchema` (`.strict()`) and the stage fields in `schema.ts`; `statusDataSchema` gains `plan_stats` optional.

## Build

- `PlanOverviewDialog({ record })` in `plan-overview-dialog.tsx`, using `Dialog`/`DialogContent` from `web/src/components/ui/dialog.tsx`: `open` always true, no `onOpenChange`, `showCloseButton={false}`, `onEscapeKeyDown` and `onInteractOutside` call `preventDefault()`, `role="alertdialog"` (precedent: `web/src/plan/connection-dialog.tsx:14-20`).
- Sections in mockup order: header (eyebrow "Plan complete", plan name, lock note naming `loom init` and `loom clean`, a pill with completed and skipped counts); five totals (wall time with clock range, stage execution with the parallel factor = execution / wall, fresh tokens = input + cache write + output, cache hit = cache read / (cache read + input + cache write), all tokens); token mix bar with legend; timeline as inline SVG, one row per stage, phase segments coloured contract / executing / adjudication / retry, the gaps hatched as waiting, skipped stages as a dashed stub, hour ticks, duration labels, a native `<title>` per segment; models table with the colour key beside its heading (input, cache write, output) and `modelWithEffort`, lane chip, agents, fresh, cache read, output, share bar; footer naming `<loom home>/stats/plans/<file>` (from `plan_id` and `started_at` the same way Rust `file_name` builds it) and a Copy summary button (`navigator.clipboard.writeText` in the click handler, rejection shows "Copy failed").
- Colours only from theme tokens; add `--phase-contract` (magenta, hue about 340), `--phase-executing` (green, hue about 150), `--phase-adjudication` (blue, hue about 255) and `--phase-retry` (orange to amber) tokens in the component stylesheet for light and every dark variant, as in the mockup. The four phases must stay far apart in hue: the operator rejected an adjudication teal beside the executing green as indistinguishable. Both themes legible; keyboard focus visible; reduced motion respected.
- Mount in `routes/shell.tsx` beside the other global overlays (:36-52): render `<PlanOverviewDialog record={status.plan_stats} />` when `status?.plan_stats` exists.
- Split into small files (timeline, totals, models, footer); each under 400 lines.

## Tests

Component tests for totals arithmetic, timeline segment geometry (one scale; skipped stage stub), legend presence, copy summary, and the shell mounting the dialog when `plan_stats` is present and not otherwise.

## Done

`cd web && ./node_modules/.bin/vitest run src/components/plan-overview src/api src/routes` passes (schema fixture tests may wait for the `wire` worker's fixture; say so), `./node_modules/.bin/tsc -b --noEmit` and `./node_modules/.bin/oxlint --deny-warnings` clean.

## Traps

- Do not mount it inside `routes/overview.tsx` (sidebar); the operator rejected the sidebar.
- The frozen contract presses Escape and clicks the overlay; both must leave the dialog open.
