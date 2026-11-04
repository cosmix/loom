# H1 — limits in a footer, statistics out of the popover (issue 13)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first, D15 in particular.

## Footer

- `web/src/plan/shell.tsx` (`PlanShell`): render `<PlanFooter />` from a new `web/src/plan/shell-footer.tsx` after `<main>`, as a `footer` (landmark `contentinfo`).
- Data: the quota from the summary panel feed, `GET /api/summaries/panel` plus the `summary_panel` socket frame (`summaries/state.ts:65,113`; `panel.quota`, the dashboard's `quotaSnapshotSchema`).
  - Today `LivePill` in `header/activity.tsx` owns `useSummaryFeed`, so the feed runs only while the pill is mounted. Move that ownership to the shell, so the footer and the pill read one feed. The pill keeps its auto-open behaviour (knowledge: planner-web-shell "Activity pill and popover").
  - The pill's Retry button calls the feed's `reload` (`StatusContent` gets `{load, reload}` from `LivePill`). Once the shell owns the hook, publish `reload` through a shell-level atom or context that the pill reads, so Retry keeps working; test it.
  - Keep `useSummaryFeed`'s one subscription. Do not start a second fetch. `SummaryStatus` (`summaries/summary-status.tsx`) also calls `useSummaryFeed` but has no planner importer outside tests: leave it as it is (frozen `summaries/contracts.test.tsx` renders it).
- Draw `QuotaMeters` from `@/components/quota-meters` as the dashboard footer does (`web/src/routes/shell.tsx`), with `quotaLeft={[]}` because the planner has no `quota_left` data. Plan code may import it (`web/scripts/bundle-rules.json` forbids only terminal and editor chunks in `plan.html`).
- The element is `<footer className="plan-footer">` after `<main>`; V4 styles `.plan-footer`.
- Unavailable: for each provider (claude, codex) that `providerRows(quota)` (`web/src/lib/quota.ts`) drops, write `<Provider> limits unavailable` in `--type-fine` `--muted-foreground`. When both report, no note renders (stage contract `footer-hides-the-note-when-limits-exist`). The dashboard hides such a provider silently; the operator asked for the word.
- Read-only servers and the sign-in notice: the footer renders nothing while the plan shell shows only the sign-in notice (frozen `plan-shell.contracts.test.tsx` "plan shell shows only the sign-in notice after a 401").
- V4 restyles the footer's spacing with the header. You own its markup and data.

## Statistics out of the popover

- `web/src/plan/summaries/summary-panel.tsx`: export `RebuildActivity` (private today; it draws "Running rebuild", "Queue", "Failed rebuilds" and the start error). `SummaryPanel` keeps composing it and keeps everything else, its quota meters included: the frozen `summaries/contracts.test.tsx` asserts the token totals and the claude 5h, claude 7d and codex 7d meter images on `SummaryPanel`. Do not remove them.
- `web/src/plan/summaries/summary-status.tsx` `StatusContent` stops rendering `<SummaryPanel …>` (a plan `after_stage` greps for it) and renders the `read_only` text, `RebuildActivity` and the question instead. The popover's rebuild row so keeps its queue, errors, start error and question (frozen `header/activity-fixes.contracts.test.tsx`; `activity-popover.test.tsx` expects the region "Queue") and loses the totals and the meters.
- Settings: add a "Summaries" section in `web/src/plan/settings/settings-sections.tsx` that renders `<SummaryPanel …>` whole (last rebuild, token totals, terminal claims, limits) from the same feed (a plan wiring check greps `<SummaryPanel` there). Follow the existing section pattern and `settings/settings.css`.
- Exceptions: you do not own `ui/DESIGN.md`. Report any deliberate exception as `exception: <surface>: <rule>: <why>`.

## Tests

- The frozen stage contracts `footer-shows-limits-and-unavailable`, `footer-hides-the-note-when-limits-exist` and `popover-has-no-limits-or-token-totals`.
- Unit tests: the feed runs once with both footer and pill mounted; the pill's Retry reloads the feed; Settings shows the totals, as `it("Settings shows the summary totals")` in `settings/settings-dialog.test.tsx`: open the real settings dialog (not `SummaryPanel` mounted alone) with the panel feed answering `api/fixtures/summary-panel.json`, open its Summaries section, and find the region "Token totals" and a claude quota meter. A plan `after_stage` greps the name; the frozen `summaries/contracts.test.tsx` mounts `SummaryPanel` directly and cannot prove Settings mounts it.

## Run once at the end

`node_modules/.bin/vitest run src/plan/summaries src/plan/settings src/plan/header/activity src/plan/shell` from `web/`.
