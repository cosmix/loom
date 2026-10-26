# W1 (web): summary panel, budget question, rebuild buttons, turn rows

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md`
("Wire") first. Stage `rebuild-engine` is merged: the fixtures `web/src/plan/api/fixtures/summary-*.json`
exist and are pinned by Rust parity tests.

## Files you own

`web/src/plan/summaries/{summary-panel,summary-status,rebuild-question,approval-notice,rebuild-button}.tsx`,
three of their test files, `web/src/plan/summaries/state.ts`, and one line each in `web/src/plan/header-extras.tsx`,
`web/src/plan/chat/rebuilt-summary-extras.tsx`, `web/src/plan/concept/page-header-extras.tsx`,
`web/src/plan/concept/key-row-extras.tsx`. The frozen
contract file `web/src/plan/summaries/contracts.test.tsx` is not yours to edit.

Codex unit C1 writes `schema-summaries.ts` and its test from the Schema section below, and C2
writes `summary-tokens.tsx` and its test; import from them.

## Schema

`schema-summaries.ts`: zod for `SummaryPanel`, `SummaryQuestion`, `SummaryTurnCost` and the four
WS message types, importing `z` from `@/api/zod` (the CSP-safe jitless build; never from `zod`
directly). `schema-summaries.test.ts` parses every `summary-*.json` fixture. Quota: reuse the
dashboard's quota zod schema from `web/src/api/` rather than redefining it.

## Components (`web/src/plan/summaries/`)

- `summary-panel.tsx` `<SummaryPanel panel nowSecs />`: the running rebuild ("Rebuilding 2 of 3",
  ids listed), the queue, failed ids with their error, the last rebuild's cost (fresh tokens,
  cache reads, output; an absent field shows "unknown", never 0), today and 7-day totals, and
  `<QuotaMeters snapshot={panel.quota} nowSecs={nowSecs} />` from `@/components/quota-meters`
  (`web/src/components/quota-meters.tsx:40`). Read-only: the reason line instead of controls.
- `rebuild-question.tsx` `<RebuildQuestion question canAnswer onAnswer />`: count, the ids, the
  estimate labelled "estimate" with its basis ("from the last builds", "scaled from past builds",
  "starting figure"), buttons "Rebuild all" and "Not now", disabled with a reason when
  `!canAnswer` (section 04: holder, or anyone while authoring is free: `canAuthorAtom`).
- `rebuild-button.tsx` `<RebuildButton ids />`: `apiPost("/api/summaries/rebuild", { ids })`. A
  browser that may not rebuild (another holds authoring, or server read-only) sees plan 2's
  `<DisabledReason reason={string | null} onClick={fn}>{label}</DisabledReason>`
  (`web/src/plan/ui/disabled-reason.tsx`; null reason = enabled; otherwise focusable, `aria-disabled`,
  reason as visible text and `aria-describedby`) with the reason from `authorBlockReasonAtom`; the same component
  wraps the question's and the approval notice's buttons and the terminal-claim Release buttons.
- `approval-notice.tsx` `<ApprovalNotice question canAnswer onAnswer />`: section 05's "Rebuild
  above the budget waiting for approval" notice: the stale count and, when `canAnswer`, an approve
  button (Rebuild all); read-only browsers see the text only.
- `summary-tokens.tsx` `<SummaryTokens tokens />`: the token figure that ends one rebuilt-summary
  row of a turn footer (plan 9 renders the rows from `turn.summaries_rebuilding`), "tokens unknown"
  for null.
- `summary-status.tsx` `<SummaryStatus compact? />`: the header control (idle / "Rebuilding n of m" /
  "n out of date"), opening a popover with `<SummaryPanel/>` and, when a question is open,
  `<RebuildQuestion/>`. `compact` (plan 2's `HeaderExtras({ compact })` passes it on) renders a glyph plus
  the count with an accessible name naming the state (contract `compact-header-control`).
  An arriving question opens the popover by itself (every browser sees it, section 04) WITHOUT moving
  focus (`onOpenAutoFocus` prevented, no `autoFocus` inside) and announces itself in an
  `aria-live="polite"` region; answering needs a click or explicit focus (contract
  `arriving-question-keeps-focus`). State: a jotai atom fed by `subscribe("summary_panel", ...)`,
  `subscribe("summary_question", ...)`, `subscribe("summary_question_closed", ...)` from
  `web/src/plan/api/socket.ts`, seeded by `apiGet("/api/summaries/panel")` and RESEEDED on plan 2's
  socket resync (`store.get(planSocketAtom).onResync(fn) -> unsubscribe`, `planSocketAtom` from
  `@/plan/state/atoms`, fired on every `hello` after the page's first;
  contract `panel-reseeds-on-resync`). The seed's loading and failure states use plan 2's
  `<LoadState state={"loading" | "empty" | "not_found" | "error"} title? message? action?={{ label, onClick }} />`
  (`web/src/plan/ui/load-state.tsx`): `error` with a Retry action (contract `panel-load-error-shown`).
- The panel lists `terminal_claims` with a Release button each (`POST /api/summaries/release`).

Mounts: `<SummaryStatus />` in `header-extras.tsx`; `<SummaryTokens tokens={...} />` in plan 9's
`chat/rebuilt-summary-extras.tsx` (`<RebuiltSummaryExtras id turn />`), looked up from the
`summary_turn_cost` messages by session, turn and id (an atom map); `<RebuildButton ids={[page.summary_id]} />`
in `concept/page-header-extras.tsx`; `<ApprovalNotice/>` (while a question is open) in plan 5's
`concept/key-row-extras.tsx` (`<KeyRowExtras page />`, after the document key and Fold all), the
place section 05 names for the notice.
Use the dashboard's tokens and components (`BusyRoundel`, `Stamp`) and the typography
convention (`doc/loom/knowledge/conventions/web-dashboard-typography.md`): no coloured left borders.
The popover and the question fit a 390px viewport (full-width below `lg`, 16px gutter, no sideways
scroll); integration-verify's layout reviewer checks it in the built bundle.

## Tests

Component tests beside each component for the states above (vitest + @testing-library/react, as
the existing `web/src/components/*.test.tsx` do).

## Proof

From `web/`: `bun run typecheck`, `bun run lint`, `bun run format:check`, `bun run test`,
`bun run build`.
