# W2: the run dialog, Abandon run, the run strip, Open dashboard, plan run actions

LANE: codex units write `use-run.ts` (CW2) and `src/plan/offers/actions-run.ts` (CW4, with `runDialogAtom`) before you start; see `doc/plans/briefs/feat-planner-11-execution-handoff/handoff-web/codex-units.md`. Import them; do not write those files.

Stage `handoff-web` of `doc/plans/PLAN-feat-planner-11-execution-handoff.md`. Tier: sonnet
(`loom-software-engineer`). Never run git, never spawn subagents. One narrow check at the end,
from `web/`: `node_modules/.bin/vitest run src/plan/run`. Frozen contract files under
`src/plan/run/contracts/` are READ-ONLY.

## Files you own (relative to `web/`)

- `src/plan/run/**` (new: `run-dialog.tsx`, `abandon-dialog.tsx`, `run-output.tsx`,
  `run-strip.tsx`, `dashboard-button.tsx`, `plan-run-actions.tsx`, `*.test.tsx`),
  except `contracts/` and CW2's `use-run.ts` and `use-run.test.ts`
- `src/plan/header-extras.tsx` (one line mounting `<RunStrip/>`)
- (`src/plan/offers/actions-run.ts` is CW4's; you render the dialog from its `runDialogAtom`)

W1 mounts `<PlanRunActions/>` in the plan-view and concept slots; you write the component.

## Spec

`doc/product/section_11.md` lines 200-388 ("Starting a run" to "After plans complete").

## Data

Schemas and fixtures from stage `app-handoff`: `src/plan/api/schema-run.ts` (`runStateSchema`,
`runStripSchema`, `runOutputSchema`), fixtures `src/plan/api/fixtures/run-*.json`. Routes:
`GET /api/run/state`, `POST /api/run/commit` (`{ paths, message }`), `POST /api/run/start`
(`{ plan, backend, clean }`), `POST /api/run/abandon` (`{ confirmed_unmerged }`, the shown branch list), `POST
/api/run/dashboard` (returns `{ url, tunnel }`). Live: `subscribe("run_state" | "run_output" |
"run_strip")`. `use-run.ts`: atoms `runStateAtom`, `runStripAtom`, `runOutputAtom` (lines per
job, capped at 5,000 lines per job, oldest dropped).

## Run dialog (`run-dialog.tsx`)

In order (section 11 "The run dialog"): the plan (select of `plans`, `preselected` first,
unlisted plans marked); the branch ("merges into <branch>"); uncommitted changes in two lists:
`doc_changes` with a checkbox each (all checked), a message field prefilled
`docs(plan): planning documents for <plan file name>` and "Commit changes under doc/";
`blocking_changes` listed with "Commit or stash these first; the app does not touch files
outside doc/."; the backend (radio from `backends`, `default_backend` selected); the `--clean`
notice when `clean_notice` or the chosen plan differs from an initialized one. Start is
enabled only when `blocking_changes` is empty and `doc_changes` is empty (committed) and no job
is `busy`. Output of each job streams into `<RunOutput/>` (monospace, text only, auto-scroll
unless the user scrolled up). The workspace states' offers (Start, Start the run, Resume, Start
the next plan) label the primary button by `offer`; `none_running` shows "The plan is running"
with Open dashboard and no Start.

## Abandon (`abandon-dialog.tsx`)

Shown beside Resume and Open dashboard when `abandon` is present, holder only, not in read-only
mode. First confirmation names what is lost (section 11 "First confirmation"), lists
`merged_stages` (work stays on the base branch) and `unmerged_stages`; the button reads
"Abandon run". When `unmerged_branches` is non-empty, a second separate confirmation lists each
branch with its commit count ahead of `base_branch` and the button "Delete N branches with
unmerged commits"; declining cancels the whole abandon and says "You can merge or push those
branches first and come back." The request sends `confirmed_unmerged`: exactly the `unmerged_branches` list the second
confirmation showed (empty when none was shown), only after the confirmations; a 409
`unmerged_branches` reply reopens the second confirmation with the server's fresh list, which the
user must confirm again.

## Run strip and dashboard

`run-strip.tsx` in the header while `runStrip.plan` is set: "<plan> · daemon running|stopped ·
<merged>/<total> merged · <attention> need attention" and Open dashboard. `dashboard-button.tsx`
fetches the URL with `POST /api/run/dashboard` when the strip or the dialog renders, and again
on each `run_strip` change, then renders a plain `<a href={url} target="_blank" rel="noopener
noreferrer">Open dashboard</a>` (no `window.open` after an awaited request: popup blockers stop
it); over SSH shows the tunnel hint text. The holder's response carries the token; nobody else's
does.

## Plan run actions (`plan-run-actions.tsx`)

`<PlanRunActions path state />`: a written plan gets "Pressure-test" (opens W1's dialog with the
plan) and "Start a run" (opens the run dialog with the plan); a done plan gets "Review what was
built" and "Start a new metaplan round", dispatched through plan 2's offer registry as the
`refine_spec` and `new_metaplan_round` offers (plan 9 registered them). Running and retired
plans get none (they are locked).

## Tests

Component tests on the fixtures: each workspace state's primary button; Start disabled with a
blocking change and with uncommitted doc changes; the commit message prefill; the abandon flow
with and without unmerged branches (declining the second confirmation sends nothing); the strip
text; untrusted output lines render as text. Also: `none_running` shows no Start and shows Open
dashboard; Native is not offered when `backends` lacks it; the `start_run` offer action opens
the dialog; `PlanRunActions` shows Pressure-test and Start a run for a written plan, Review what
was built and Start a new metaplan round for a done plan, and nothing for running and retired
plans; Abandon is hidden for a non-holder and in read-only mode.

## Added (fix round)

- Resync: on plan 2's `store.get(planSocketAtom).onResync(handler)` (from `@/plan/state/atoms`;
  unsubscribe on unmount), refetch `/api/run/state`; test that a
  resync after a missed `run_state` shows the new state.
- Compact strip: plan 2's `HeaderExtras({ compact: boolean })` renders `<RunStrip compact={compact} />`;
  the compact form renders a
  status glyph plus the attention count with the full text as its accessible name.
- Disabled controls: Start, Commit, Abandon, Finish abandoning, Pressure-test and Start a run
  render as `<DisabledReason reason={string | null} onClick={fn}>{label}</DisabledReason>` from
  `@/plan/ui/disabled-reason` (null reason = a normal button; with a reason it stays focusable,
  `aria-disabled`, ignores the click, and shows the reason as text and through `aria-describedby`) for
  non-holders and in read-only mode, the reason from `authorBlockReasonAtom`; fetch states use
  plan 2's `<LoadState/>` from `@/plan/ui/load-state`; tests assert it with `toHaveAccessibleDescription`.
- Finish abandoning: when `finish_abandon` is set, the dialog shows "This plan's run was
  removed but the plan was not retired" and a holder-only "Finish abandoning" button posting
  `POST /api/run/finish-abandon`; test it.
- Tests: the abandon request body equals the shown branch list; the dashboard control is an
  anchor with `rel="noopener noreferrer"`; a viewer's anchor has no `token=`.
