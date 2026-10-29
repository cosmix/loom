# W1: the pressure dialog and the pressure-run panel

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

LANE: codex units write `use-pressure.ts` (CW1) and `src/plan/offers/actions-pressure.ts` (CW3, with `pressureDialogAtom`) before you start; see `doc/plans/briefs/feat-planner-11-execution-handoff/handoff-web/codex-units.md`. Import them; do not write those files.

Stage `handoff-web` of `doc/plans/PLAN-feat-planner-11-execution-handoff.md`. Tier: codex sol/high (`loom-codex-forwarder`). Load `frontend-design:frontend-design` only for layout questions the
existing components do not answer; reuse the dashboard's and plan 9's components first. Never
run git, never spawn subagents. Orchestrator proof after the worker returns, from `web/`:
`node_modules/.bin/vitest run src/plan/chat/pressure`. Frozen contract files under
`src/plan/chat/pressure/contracts/` are READ-ONLY.

## Files you own (relative to `web/`)

- `src/plan/chat/pressure/**` (new: `pressure-dialog.tsx`, `pressure-header.tsx`,
  `codex-status.tsx`, their `*.test.tsx`), except `contracts/` and CW1's `use-pressure.ts`
  and `use-pressure.test.ts`
- `src/plan/chat/run-header.tsx` (one branch: `session.kind === "pressure"` renders
  `<PressureHeader/>`; plan 9 owns the rest of the file)
- `src/plan/chat/session-divider.tsx` (one branch: between two pressure-step sessions, render
  the step divider naming the step and model, mapped from `PressureRunView.steps[].session_id`)
- `src/plan/chat/composer-gate.ts` (one reason: during the Codex-only part of a step,
  `"Codex is reviewing; send when the next Claude step starts"`, section 08)
- (`src/plan/offers/actions-pressure.ts` is CW3's; you render the dialog from its `pressureDialogAtom`)
- `src/plan/files/plan/plan-actions-extras.tsx` and `src/plan/concept/page-header-extras.tsx`: one
  line each mounting `<PlanRunActions/>` (W2 writes that component; you add only the mount line)

## Spec

`doc/product/section_11.md` lines 21-56 (where offered, the dialog), 106-148 (what the chat
shows, stopping); `doc/product/section_08.md` lines 387-414 ("Pressure runs").

## Data

Zod schemas and fixtures from stage `app-handoff`: `src/plan/api/schema-pressure.ts`
(`pressureOptionsSchema`, `pressureRunSchema`), fixtures `src/plan/api/fixtures/pressure-*.json`.
Routes: `GET /api/pressure/options`, `POST /api/pressure/start` (body `PressureStart`: `plan`,
`rounds`, six slots), `POST /api/pressure/stop`. Live updates: plan 2's socket
`subscribe("pressure_run", handler)`. `use-pressure.ts` keeps the latest `PressureRunView` in a
jotai atom `pressureRunAtom` and exposes `startPressure`, `stopPressure`, `fetchPressureOptions`
through plan 2's `apiGet`/`apiPost`.

## The dialog (`pressure-dialog.tsx`)

Opened from the checklist offer, from a written plan's view (file mode and concept mode, through
W2's `<PlanRunActions/>`), with the plan preselected. Fields: plan (select of `options.plans`),
rounds (number 1..10, default 2), three model rows (`/pressure` Claude model and effort, Codex
model and effort, `/address` Claude model and effort), each defaulting to `options.defaults`
and choosing only from the vocabularies in `options`. Text: "This takes a while, edits the
plan, and writes planning-omissions.md at the repository root." When an authoring chat is open
(plan 9's session atom says so): "The open chat ends first, with its closing write." When
`options.available` is false, the Start button is disabled and the reason
(`options.disabled_reason`) is shown beside it. Errors from `POST /api/pressure/start` show the
server's message. Read-only browsers never see the dialog's Start.

## The panel (`pressure-header.tsx`, `codex-status.tsx`)

- Header (section 08 "Run header"): "Pressure test · `<plan file>` · round <r> of <n> · <step>"
  (`/pressure`, `Codex`, `/address`), elapsed time from `started_ms` (ticking each second while
  running), token total. The state line above it stays plan 9's.
- Step dividers naming the step and model, rendered by your branch in plan 9's
  `session-divider.tsx` (`<SessionDivider previous next />`: a step boundary is a session
  boundary in plan 7's `ChatSnapshot`); each Claude step's turns are plan 9's normal rendering of
  that step's session (`steps[].session_id`); each step's footer lists `edits`
  with plan 9's Diff links and its tokens.
- Codex status item: the model, elapsed time with `BusyRoundel` while running, then "Review
  written" with a link opening the report in file mode, or "No review"; on failure the
  `log_tail` as monospace text (rendered as text, never HTML).
- Stop for the whole run: plan 9's `stop-control.tsx` with
  `confirm="Stop the pressure test? The plan keeps the edits made so far, and no record is written."`;
  no End chat while a run lasts.
- The end line from `ended`: `recorded` → "Pressure test recorded" (with `line`); `stopped` →
  "Stopped by <by>"; `failed` → "Step <step> failed: <reason>"; `interrupted` → "Interrupted".
- Read-only browsers: no composer, no Stop.

## Tests

Component tests on the fixtures: header text for round 2 of 2 at the codex step; Codex item in
each state; Stop shows the confirmation; composer gate returns the Codex reason only while the
codex step runs alone; the dialog disables Start with each `disabled_reason`; untrusted text
(a `log_tail` containing `<img src=x onerror=alert(1)>`) renders as text. Also: the dialog
prefills rounds 2 and every slot from `options.defaults`; it shows the "takes a while" text and,
with an authoring session open, the "The open chat ends first" line; the `pressure_test` offer
action registered in `actions-pressure.ts` opens the dialog with the offer's plan preselected
(the offer's `target` and `options.plans` are both repo-relative `doc/...`: compare for
equality); the Codex item links to `toAddress(report_path)` from `@/plan/files/address`
(`toAddress` accepts a repo-relative path and builds the doc-relative `/files/` address) when the review is written; a read-only
browser sees neither the composer nor Stop; each `ended` kind's end line.

## Added (fix round)

- Resync: on plan 2's socket handle `store.get(planSocketAtom).onResync(handler)` (from
  `@/plan/state/atoms`; returns an unsubscribe; fires on every `hello` after the page's first), refetch `/api/pressure/options` and
  re-read the latest `pressure_run` (it is re-sent by the server's `on_open`; if not, keep the
  last view and mark it "reconnected; waiting for the run's next update"). Test: a resync after a
  missed `pressure_run` shows the new step.
- Disabled controls: every write control a browser may not use (Start in the dialog, Stop,
  Pressure-test) renders as `<DisabledReason reason={string | null} onClick={fn}>{label}</DisabledReason>` from
  `@/plan/ui/disabled-reason` (null reason = a normal button; with a reason it stays focusable,
  `aria-disabled`, ignores the click, and shows the reason as text and through `aria-describedby`)
  (plan 2), the reason from `authorBlockReasonAtom` (holder or read-only) or
  `options.disabled_reason`;
  tests assert the reason with `toHaveAccessibleDescription`.
- Configuration notice: when `ended.reason` starts with "Codex changed session configuration",
  the end line lists the files and offers "I have checked these files" (holder only) which posts
  `POST /api/pressure/confirm-config`; test it.
