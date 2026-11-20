# W3 — Visual design and React components: routes, panels, SVG logo, theme

Tier: fable, spawned as `loom-senior-software-engineer` with `model: "fable"`. Runs in parallel
with W1 and W2 after W0 returned. Before writing anything, load
`Skill(skill="frontend-design:frontend-design")` and
`Skill(skill="loom-skills", args="loom-react loom-typescript loom-accessibility")`.

You design AND build the page. The design is the code: there is no separate mockup step. The
brief below fixes WHAT the page shows and the data it reads; HOW it looks is yours, under the
frontend-design skill's standards. The target is a dashboard people leave open on a second
monitor for hours: calm, legible at a glance, alive when something changes, never noisy.

## Files you own (write)

- `web/src/main.tsx` (W0 wrote its pinned form; keep the same imports, adjust freely)
- `web/src/router.tsx`, `web/src/routes/shell.tsx`, `web/src/routes/ledger.tsx`, `web/src/routes/stage.tsx`, `web/src/routes/error.tsx`, `web/src/routes/ledger.test.tsx`
- `web/src/components/*.tsx` — at least: `logo.tsx`, `header.tsx`, `connection-badge.tsx`, `alerts-band.tsx`, `ledger-table.tsx`, `state-badge.tsx`, `context-meter.tsx`, `attention-panel.tsx`, `activity-panel.tsx`, `legend-dialog.tsx`
- `web/src/components/ui/**` — the ten shadcn components W0 added; add more with `bunx shadcn@4.21.0 add -y <name>` when you need them
- `web/src/index.css` — theme tokens (keep W0's font imports and the three Tailwind `source`/`@source` lines verbatim; they are pinned by acceptance)
- `web/index.html` (title `loom`, favicon link, `<div id="root">`), `web/public/favicon.svg`
- `web/README.md`

Read-only: `web/src/api/schema.ts` (types), `web/src/api/fixtures/snapshot.json` (realistic
data for your test and for judging the design), W2's exports (pinned below), and the TUI
sources that define the semantics you are re-rendering:
`loom/src/commands/status/ui/tui/ledger/{cells.rs,header.rs,panels.rs,legend.rs,layout.rs}`,
`loom/src/commands/status/ui/tui/state.rs:86-165`, `loom/src/lib.rs:36-39` (the logo).

Do not edit `web/src/api/**`, `web/src/state/**`, `web/src/lib/format.ts`, `levels.ts`,
`activity.ts`, `vite.config.ts`, `package.json` scripts, tsconfigs, or anything under `loom/`.
Never touch `.loom/`. Never run `git`.

## Contracts you build on (W2 writes these in parallel; import them exactly as named)

```ts
// @/state/store
export const store; export type Store;
// @/state/atoms
export const snapshotAtom: Atom<Snapshot | null>;
export const connectionAtom: Atom<{ phase: "connecting" | "live" | "reconnecting" | "error"; since: number; message?: string }>;
export const activityLogAtom: Atom<{ at: number; stageId: string; status: StageStatus; message: string }[]>;
export const orderedStagesAtom: Atom<{ stage: StageSummary; level: number }[]>;
export const attentionAtom: Atom<Attention[]>;  export const alertsAtom: Atom<Alert[]>;
export function selectStage(snapshot: Snapshot | null, id: string): StageSummary | undefined;
// @/state/apply
export function applySnapshot(store: Store, next: Snapshot, now?: number): void;   // use in your test
// @/lib/format
export type Tone = "executing" | "completed" | "blocked" | "pending" | "queued" | "warning" | "merged" | "dimmed" | "neutral";
export function formatElapsed(seconds: number): string;
export function stateMeta(status: StageStatus): { icon: string; label: string; tone: Tone; bold: boolean };
export const LEGEND: ReadonlyArray<{ status: StageStatus; meaning: string }>;
export function failureLabel(type: FailureType): string;
export function activityText(stage: StageSummary): { text: string; tone: Tone } | null;
export function contextUsage(stage: StageSummary): { tokens: number; ceiling: number; percent: number; filled: number; health: "green" | "yellow" | "red" } | null;
export function modelsOf(stage: StageSummary): { model: string; execution: string[] };
export function timeText(stage: StageSummary): string | null;
export function mergeText(stage: StageSummary): { text: string; tone: Tone } | null;
export function progressPercent(completed: number, total: number): number;
export function daemonLine(daemon: DaemonState, tickAgeSecs: number | null): { text: string; detail?: string; tone: Tone };
export function summaryCounts(stages: readonly StageSummary[], attentionCount: number): { executing: number; queued: number; waiting: number; attention: number; done: number };
```

All display semantics (what an activity cell says, when a context meter shows, what "merged"
means) come from these functions. Do not re-derive them in components; if a formatter is
missing something you need, compute it locally in the component and say so in your report.

## What the page shows (functional spec — every item is required)

Route `/` (the ledger):

1. **Header**: the SVG logo; the plan name (`status.plan_name`, or a dimmed "(no plan name)");
   the daemon line (`daemonLine`) with a tone; the connection badge (live / reconnecting /
   connecting / error, with the message on hover, and the time since the last frame); the
   progress line "N of M stages complete" with a bar and `progressPercent`; the summary counts
   (executing · queued · waiting · need attention · done, each with its state glyph); the merge
   line "merged N · unmerged N · conflicts N".
2. **Alerts band** (`alertsAtom`): one row per alert, severity-toned (info / warning /
   critical); hidden when empty.
3. **Attention panel** (`attentionAtom`): one card per entry: label (BLOCKED, MERGE CONFLICT,
   ACCEPTANCE FAILED, MERGE ERROR, NEEDS REVIEW, NEEDS INPUT, ADJUDICATING, CLEANUP FAILED), the
   stage name and id, the `hint` as a copyable command (mono), the failure label, the evidence
   lines in a scrollable mono block (this is untrusted text: render as text nodes only),
   `review_reason`, `cleanup_warning`, and for `has_human_review_choices` the three commands the
   TUI shows (`loom stage human-review <id> --approve`, `--force-finish`, `--reject --reason "..."`;
   read `panels.rs:168-170`). Adjudicating entries show dispute count and judge heartbeat age.
   Hidden when empty.
4. **Ledger table** (`ledger-table.tsx` exports `export function LedgerTable(...)`, and
   `routes/ledger.tsx` renders `<LedgerTable />` — a wiring check greps for that name;
   data from `orderedStagesAtom`): the TUI's eight columns — STATE (glyph + label,
   `stateMeta`), STAGE (name, id in mono, indented or otherwise marked by `level`), DEPENDS ON
   (ids as chips linking to `/stages/:id`), MODELS (`modelsOf`: the orchestrator model, then the
   execution models), ACTIVITY (`activityText`), CONTEXT (`contextUsage`: a five-segment meter
   plus percent, health-toned; empty when null), TIME (`timeText`), MERGE (`mergeText`). Each row
   links to `/stages/:id`. Rows with `held` and `incoherence` are visibly marked. The table must
   stay legible from ~900 px up and degrade sensibly below (columns collapse into the row, never
   a horizontal page scroll).
5. **Activity panel** (`activityLogAtom`): the last transitions, newest at the bottom or top
   (pick one and keep it), each with its state glyph and relative time.
6. **Legend** (`legend-dialog.tsx`): a shadcn Dialog listing the 13 states (`LEGEND`, with
   glyph, label, meaning) and the activity meanings (working / idle / stale / orphaned / crashed).
   Opened by a header button and by the `?` key when no input is focused; closed by Escape.
7. **Empty and degraded states**: no snapshot yet (skeleton, not a spinner); a snapshot with
   zero stages ("no stages in this workspace"); `daemon` not running (the header says so, the
   page keeps rendering the last file snapshot); connection lost (badge turns to reconnecting,
   the data stays, nothing flashes).

Route `/stages/:stageId` (stage detail): everything on the row plus every remaining
`StageSummary` field, grouped: identity (name, id, type, model, execution models), graph
(dependencies as links, level), timing (elapsed, execution), context (tokens / ceiling with the
meter), session (pid, alive, backend, session type, last tool, last activity, staleness),
retries (retry_count / max_retries), adjudication (dispute_count, judge_heartbeat_secs),
merge (merged, base_branch, base_merged_from, cleanup_warning), review_reason, incoherence, and
the full `failure_info` (type, detected_at, evidence as a mono block). Unknown id → a not-found
message with a link back. A back link to `/`.

`routes/error.tsx`: the router `errorElement`; check `isRouteErrorResponse` first, then
`Error`, per the loom-react skill's ErrorBoundary rule.

## The logo — `components/logo.tsx`

Recreate `crate::LOGO` (`loom/src/lib.rs:36-39`) as an inline SVG:

```text
   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴
```

It spells "loom" in box-drawing strokes: a tall `l` (rows 1-4, columns 4-6), two `o`s and an
`m`. Draw it on a 14-column by 4-row grid where each character cell is one unit wide and two
units tall, with stroked `<path>`s (`stroke="currentColor"`, `fill="none"`, square line caps
where a box corner meets, round where a stroke ends) so it inherits the text colour. Export
`export function Logo(props: { className?: string; title?: string })` with `role="img"` and an
`aria-label` of "loom". Use it in the header at text size and in `public/favicon.svg` (a
standalone copy of the same paths). Keep the glyph faithful: someone who knows the terminal
logo must recognise it instantly.

## Typography, components, theme

- Inter Variable for everything textual; IBM Plex Mono for stage ids, commands, hints, numbers
  in the table, evidence, and the logo's neighbours. W0 set `--font-sans` and `--font-mono` in
  `index.css`; use Tailwind's `font-sans` / `font-mono`.
- shadcn/ui (style `radix-nova`) for every interactive or structural primitive: Table, Badge,
  Card, Dialog, Tooltip, ScrollArea, Separator, Progress, Button, Kbd are installed. Compose
  them; do not hand-roll a dialog or tooltip.
- Light and dark, following `prefers-color-scheme`, both finished to the same standard. State tones map to the
  TUI's meaning consistently in both: executing blue, completed green, blocked red, queued cyan,
  waiting/pending gray, warning amber, merged a softer green, dimmed muted. Define the tone
  colours once as CSS variables in `index.css` and read them through a small `toneClass(tone)`
  helper in `state-badge.tsx`; never scatter literal colours.
- Motion: only where it carries information (a row whose state just changed, the connection
  badge), short, and respecting `prefers-reduced-motion`.
- Accessibility: every glyph has a text label (visually or via `aria-label`); table headers are
  real `<th>`; the legend dialog traps focus; the `?` shortcut is documented in the legend;
  colour is never the only carrier of a state (the glyph and label always accompany it).

## `routes/ledger.test.tsx`

With `@testing-library/react`: build a fresh `createStore()`, `applySnapshot(store, fixture)`,
render `<Provider store={store}><RouterProvider router={createMemoryRouter(routes, { initialEntries: ["/"] })} /></Provider>`
(export your route objects from `router.tsx` as `routes` so the test can build a memory router),
then assert: the plan name "Web Dashboard Fixture" is in the document; all seven stage ids are
present; the attention labels "ACCEPTANCE FAILED", "MERGE CONFLICT", "NEEDS REVIEW" are present;
the logo has `aria-label="loom"`. A second test renders `/stages/server` and asserts the tool
"Bash" and the model "opus" appear.

## `web/README.md`

Short: what the app is; `loom status --web` then `bun run dev` for live development (the Vite
proxy targets port 7373); `bun run check`; `bun run build` writes `dist/`, which is COMMITTED
and embedded by `loom/build.rs` — rebuild and commit `dist/` with every change to `src/`.

## Done means

- The page is finished, not a scaffold: every item in the functional spec is rendered from the
  atoms, and the fixture renders without console errors.
- Your one permitted check, run once at the end: `cd web && bun run format && bun run lint`
  (format writes; lint must be clean under `--deny-warnings`). Do not run typecheck, tests or
  the build; the orchestrator runs them and comes back with a fresh brief if something at the
  W2/W3 seam does not type-check.
- Report: the component tree (one line per file), the shadcn components you added beyond the
  ten, the design decisions that shape the page (three to six sentences), and anything the
  functional spec made you compute locally because a formatter was missing.
