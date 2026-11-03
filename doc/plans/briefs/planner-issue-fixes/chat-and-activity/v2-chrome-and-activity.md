# V2: chat rail, composer, logo, activity glyph, activity popover (#7, #11, #12, #13, #14, #17, #6 web)

Tier: opus at effort medium (`loom-senior-software-engineer`, spawned with `effort: medium`). This is visual work.

**First action, before reading code:** invoke the `frontend-design:frontend-design` skill with the Skill tool. Then read `../common.md` (its "Design direction" section is the brief for the skill) and `web/src/plan/ui/DESIGN.md`. Where the skill's general advice and DESIGN.md disagree, DESIGN.md wins.

The values below are starting points that satisfy DESIGN.md. Refine them where the result reads better, within DESIGN.md. The operator-fixed requirements are marked **fixed** and do not move.

## #7: the collapsed chat rail

`web/src/plan/chat/chat-state-glyph.tsx` `ChatGlyph` ends with `return <BusyRoundel busy={false} ... size={14} />;`. That line draws the aurora roundel for an idle chat and for no chat, in the collapsed rail and the sheet trigger.

- **Fixed:** no roundel. An idle chat, or no chat, shows a plain chat glyph.
- Starting value: `<span role="img" aria-label={label} className="inline-flex"><MessageSquare size={16} aria-hidden="true" /></span>`, with `MessageSquare` from `lucide-react`.
- The permission, failed, ended and busy branches stay (the busy branch already draws an `ActivityGlyph`).
- Remove the `BusyRoundel` import: the stage's `after_stage` check requires `chat-state-glyph.tsx` not to name it.
- No test in `chat/panel.test.tsx` or `chat/panel-header.test.tsx` asserts the idle roundel today; check again after the change. Keep the `.aurora-roundel` rules in `chat/chat.css`: the tool row still draws the roundel (`tool-row.test.tsx`), and `panel.test.tsx` pins the reduced-motion rule.

## #17: the composer is a visible input

`web/src/plan/chat/chat-composer.css` `.chat-composer-box` has `border: 0; background: transparent`. On the `--card` panel it is invisible.

- **Fixed:** the composer reads as an input at a glance in Ledger and Aubergine.
- Starting values:
  - `width: 100%; margin-inline: 0; border: 1px solid var(--border); background: var(--background);`;
  - focus: the existing outline plus `border-color: var(--ring)`.
- Rewrite the header comment's "The text box has no box of its own."
- Do not touch `chat/chat.css`: `chat/panel-a11y.test.tsx` and `chat/panel.test.tsx` pin it. No `box-shadow`, `opacity` or `--tone-` in the composer.
- Frozen contract "the composer has a visible border" reads `chat-composer.css` raw: the `.chat-composer-box` rule declares `border: 1px solid var(--<token>)` and no `border: 0`.

## #11: the logo matches the dashboard

The dashboard (`web/src/components/header.tsx:37-40`) draws `<Logo className="h-8 w-auto sm:h-10 lg:h-14" />`.

- **Fixed:** the planner logo is identical: 32px, 40px from 640px, 56px from 1024px.
- `web/src/plan/header/header.css`:
  - `.plan-logo` (~L30-34) is 26px today;
  - an override at ~L221 sets 22px below 480px: delete it;
  - the app bar's `min-height: 60px` (~L21) grows to fit the 56px logo at 1024px and up, with balanced vertical space.
- Check that the switcher, pill and controls stay centred on the logo's centre line.
- `header/header.test.tsx` pins no height today.
- Set the sizes in `header/header.css` with `px` heights and `px` media queries: the frozen contract "the planner logo matches the dashboard's sizes" reads that file raw and expects `.plan-logo` at 32px, 40px inside `(min-width: 640px)` and 56px inside `(min-width: 1024px)`, with no 22px or 26px rule left. The `max-width: 479.98px` block also sets the app bar's `min-height: 52px`; a 32px logo plus the 8px paddings fits it.

## #12: rotating arrows mean running

`web/src/plan/ui/activity-glyphs.tsx` `case "running"` draws a pulse ring plus `<Play />`. `summaries` already spins `<RefreshCw className="activity-glyph__motion" />` through a rule scoped to `.activity-glyph[data-kind="summaries"] > .activity-glyph__motion` (`ui/activity-glyphs.css` ~L19-20; keyframes near the end of the file, ~L152).

- **Fixed:** running is shown by rotating arrows. No play icon anywhere in the planner: the stage's `after_stage` check is `! rg -q '\bPlay\b' src/plan`.
- `case "running"` returns `<RefreshCw className="activity-glyph__motion" />`.
- `ui/activity-glyphs.test.tsx` has `it.each(EXPECTED_KINDS)("defines and uses CSS keyframes for %s")` (~L121-122), and `EXPECTED_KINDS` includes `running`: it requires both `@keyframes activity-glyph-running {` and `animation: activity-glyph-running ` in the CSS. Keep that name: rewrite `@keyframes activity-glyph-running` as a rotation and add `.activity-glyph[data-kind="running"] > .activity-glyph__motion { animation: activity-glyph-running <duration> linear infinite; }`. Delete only the pulse span, `.activity-glyph__pulse` and its rules.
- Reduced motion stops it.
- `ui/glyphs.tsx` maps `running: Play` (~L22, ~L58): map it to `RefreshCw`. `PLAN_STATE_GLYPHS.running` then draws the same icon as `SUMMARY_STATE_GLYPHS.rebuilding`; `ui/glyphs.test.tsx` checks only DOC_KIND distinctness, so that is allowed. Say so in the map's comment.
- Update `ui/activity-glyphs.test.tsx` (~L71-74 pin the pulse; ~L88 matches `.activity-glyph__pulse` in the CSS).
- Tests outside `ui/` pin the play icon, and you own them for this change: `files/doc-heading-view-parts.test.tsx` (~L60 `["running", "accent", "svg.lucide-play"]`), `files/tree-entry.test.tsx` (~L60 `["running", "play"]`) and `concept/page-heading.test.tsx` (~L94, ~L107 `"play"`). They become `lucide-refresh-cw` / `refresh-cw`.

## #14: each activity appears once

`header/activity.tsx` lists activities from `useActivities()` (`header/use-activities.ts`). When the daemon runs, `deriveActivities` pushes a run row titled "Plan running" (`deriveActivities`, ~L109-131). The footer (`Footer`, ~L157-165) also mounts `<RunStrip />` (`run/run-strip.tsx`), a second row for the same run. A summary rebuild doubles up the same way: the list row plus `SummaryStatusRow` (`summaries/summary-status.tsx`).

- **Fixed:** one row per thing in progress, and the footer holds controls only.
- **Run:** keep the list entry, which drives the pill. Its subject (`runSubject`) already names the plan file. Add RunStrip's other facts as text: "M of N stages merged" once a total exists, "n need attention" (a `--tone-warning` glyph plus a word) when n > 0, and the dashboard link when `dashboard_url` is set. No progress bar. `Activity` (`use-activities.ts`, `{ id, kind, title, subject, since }`) has no slot for the extras and `DashboardButton` is a component that fetches on mount, so render the run's extras in the list by `kind === "running"` (or add an optional `extra?: ReactNode` to `Activity`). Remove `<RunStrip />` from the footer and delete `run-strip.tsx` and `run-strip.test.tsx`: `activity.tsx` is its only mount (the `RunStrip` wire type in `api/schema-run.ts` stays). The stage's `after_stage` check is `! rg -q '<RunStrip\b' src/plan`.
- **A stopped daemon (settled).** `useActivities` lists a run only while `daemon_running` (or the fallback workspace is running), and RunStrip was the only place a plan whose daemon stopped kept its plan name, merged count, attention count and dashboard link. That must stay reachable. When `runStripAtom` names a plan and the run is not in the activity list, the footer shows one quiet control line: the plan file name, "M of N stages merged" once a total exists, "n need attention" when n > 0, and the dashboard link when `dashboard_url` is set. It is a control, not an activity: it does not light the pill, count in the "In progress" list or draw the running glyph, and it never says "daemon stopped" (the frozen `header/shell.contracts.test.tsx` asserts the banner never does). While the run is in the list, that footer line is absent, so exactly one run representation is visible at any time. Rewrite `header/activity.test.tsx`'s stopped-daemon case (~L148-158) to this rule. Write the named test `a stopped run keeps one dashboard line and no activity` in `activity.test.tsx` (the stage's `after_stage` checks the name): a running strip shows the plan once in the list and no footer line; switching the same atom to `daemon_running: false, attention: 2` shows the plan once in the footer with "2 need attention" and the dashboard link, and the list labelled "In progress" holds no run item.
- **Summaries (settled).** `SummaryStatusRow` is the expander that mounts `StatusContent`, which renders the rebuild question ("Rebuild all?", `rebuild-question.tsx`) and `SummaryPanel` (queue ids, failed rebuilds, `start_error`; `summary-panel.tsx` ~L107-141). B2's waiting press relies on that panel to say why a press waits behind a running session, so the detail must stay one click away while a rebuild runs. While a rebuild runs, the summary list row is the only summary line, and it is a disclosure button (`aria-expanded`) that opens the same `StatusContent` below it; `SummaryStatusRow` drops its own title and status line then. With no rebuild running, `SummaryStatusRow` stays as today. Keep one expanded state across the switch (lift it to the popover or key it so it survives `running` starting and ending), and keep the question auto-expanding the detail whenever `summaryQuestionAtom` holds a question. `summary-status.test.tsx` pins the row's accessible name ("Summary rebuilds: Rebuilding 1 of 3") at ~L144, ~L156 and ~L369: update those. Write the named test `a running rebuild's row opens its queue and errors` in `activity.test.tsx` (checked by `after_stage`): a real `ActivityPill` with a running rebuild, a queued pressed id and a `start_error`; the popover shows exactly one summary line; activating it shows the queued id and the error; a pending question stays reachable.
- `runSubject` (`use-activities.ts` ~L137-141): drop the `?? 0` (~L140). With no strip or no total, the subject is the plan name alone. B3 fixes the merged count on the server.
- `feature/run-scope.ts:122` keeps its wording.
- Frozen contract "a running plan appears once in the activity popover": inside the "In progress" dialog exactly one element names `PLAN-x.md`, "2 of 6 stages merged" appears once, the "In progress" list holds one listitem, and neither "loom run" nor a progressbar remains.

## #13: the popover looks designed

Today: `PopoverContent` `w-[min(360px,86vw)] rounded-[12px] ... p-1.5` (`activity.tsx` ~L135-152, width class ~L141); `.activity-row`, a 26px tile, body and time grid (`header/activity.css` ~L77-100). It stays a `PopoverContent` (`data-slot="popover-content"`): `web/scripts/plan-journeys-kit.mjs` dismisses auto-opened popovers by that attribute.

- **Fixed:** every row shares one anatomy and one text column. The footer's controls align to that column.
- Starting design:
  - a 20px glyph column, a `--type-ui` title, one `.plan-fine` subject line with an ellipsis, and the elapsed time right-aligned in tabular figures;
  - rows separated by space, with one `--hairline` between the list and the footer;
  - footer controls as quiet links or quiet `.plan-button`s, one line each;
  - the empty state on the same column;
  - width `min(380px, 92vw)`.
- At 390px nothing scrolls sideways.
- O1 owns the "Changed since you looked" footer row and works in parallel with you. **Fixed seam:** keep the class names `activity-row`, `activity-row__body`, `activity-row__title`, `activity-row__sub` and `activity-row__time`, and the export `ActivityTile({ kind?, paused?, children })` from `ui/activity-glyphs.tsx`, with their current meaning. O1 uses only those. Restyle them freely.
- `header/shell-contracts-kit.tsx` feeds the frozen `header/shell.contracts.test.tsx`: keep its exports (`desktopMedia`, `demoChecklist`, `demoStore`, `renderAt`) and their behaviour.

## Files you own

- Chat: `web/src/plan/chat/chat-state-glyph.tsx`, `chat/panel.test.tsx`, `chat/panel-header.test.tsx`, `chat/chat-composer.css`.
- Header: `web/src/plan/header/header.css`, `header/header.test.tsx`, `header/activity.tsx`, `header/activity.css`, `header/activity.test.tsx`, `header/use-activities.ts`, `header/use-activities.test.tsx`, `header/shell-contracts-kit.tsx`.
- UI: `web/src/plan/ui/activity-glyphs.tsx`, `ui/activity-glyphs.css`, `ui/activity-glyphs.test.tsx`, `ui/glyphs.tsx`.
- Run: `web/src/plan/run/run-strip.tsx`, `run/run-strip.test.tsx`.
- `web/src/plan/feature/run-scope.ts`.
- Summaries: `web/src/plan/summaries/summary-status.tsx`, `summaries/summary-status.test.tsx`, `summaries/rebuild-question.tsx`, `summaries/rebuild-question.test.tsx` (stage 2 moved the approval notice out of the key; do not assume its old DOM).
- Play-icon pins: `web/src/plan/files/doc-heading-view-parts.test.tsx`, `files/tree-entry.test.tsx`, `concept/page-heading.test.tsx`.

## Contracts (frozen before you start; make them pass)

- `web/src/plan/header/activity-fixes.contracts.test.tsx`: "a running plan appears once in the activity popover" and "the planner logo matches the dashboard's sizes".
- `web/src/plan/chat/chat-fixes.contracts.test.tsx`: "the composer has a visible border" (the file's other two tests are C1's).

## Proof

Run once at the end: `node_modules/.bin/vitest run src/plan` from `web/`. Glyphs and the activity row reach `src/plan/{home,new,files,concept}` (for example `new/taking-shape.test.tsx` pins `.activity-glyph[data-kind="drafting"]` and `concept/document-key.test.tsx` pins `.lucide-refresh-cw.activity-glyph__motion`). You cannot build or take screenshots: the main agent builds, shoots, and sends a refinement round with the shots if anything misses. In your report, list each design decision you made beyond the starting values and why.
