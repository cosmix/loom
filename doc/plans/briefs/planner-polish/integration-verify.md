# Integration verify: the 15 polish issues, checked in the running app

Read `common.md` first. You are the last gate, and passing tests do not prove the operator's complaint is gone. Check each issue in the running app.

## Order

Your session starts in `loom/`. "From the worktree root" means `cd "$(git rev-parse --show-toplevel)"` first.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`. The binary embeds `web/dist`.
2. **Diagnostic gate and preflight.**
   - Run `loom check integration-verify`. Fix every failure through an engineer of the cheapest capable tier (haiku for a one-line value, sonnet for code, opus only after a failed sonnet attempt), rebuild after each fix (step 1), and re-check only the failed criteria with `--criterion <n>`.
   - Then the browser preflight, from the worktree root: `scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/preflight" /`. If it fails, file `loom stage block integration-verify "<preflight output>"`.
3. **Journeys.** `scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"`, without `--strict`. List every PENDING id in the walk.
4. **Screens.** `scripts/plan-screens.sh --worktree-doc --themes=ledger,aubergine,graphite loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens"` over these routes:
   - `/`
   - `/f/feat-planner`
   - `/f/feat-planner/product`
   - `/f/feat-planner/metaplan`
   - `/f/feat-planner/engineering/<a plan slug>`
   - `/f/feat-planner/pressure`, which must render the not-found state.

   Some states the script cannot reach. Shoot these with a throwaway Playwright driver under `$LOOM_SCRATCH_DIR`, in one Bash call with its own server start (plan Environment inventory). Where a session cannot start, stub `WebSocket` in `addInitScript` and replay frames from `web/src/plan/api/fixtures/chat-snapshot.json`.
   - An open chat with a running turn (Stop beside Send) and after Stop (one notice).
   - The composer with two and with five Add to chat references plus a file chip (issue 15).
   - The open chat menu.
   - The Settings Summaries section.
   - The footer at 1440px beside the dashboard footer at 1440px. Start the dashboard as earlier planner plans did, or compare against `web/src/routes/shell.tsx` values.
   - A feature page at 1280px (the panel shows) and at 1180px (the sheet button shows).
   - The activity popover during a summary rebuild (the roundel).
5. **Issue walk.** Judge each item yourself against the shots and the code. For each of 1-15, write one line: "fixed", "not fixed" or "could not check here", with the evidence (a shot path, a test name or a `file:line`).
   - Record each line with `loom memory note "issue-walk #N: <verdict>: <evidence>"`, citing the gate the plan's "Issue to gate map" names.
   - An item you cannot show fixed is a defect to fix now. A visual defect goes to a fresh fable visual worker for its area (shell or chat sidebar) (`loom-senior-software-engineer`, model fable, loading `frontend-design:frontend-design` first) under common.md's loop. A Settings statistics defect goes to opus at effort medium with the same skill. Anything else goes to the cheapest capable engineer.
6. **Final build and gate.** After the last fix, rebuild (step 1) and run the full `loom check integration-verify` once more. That run is the acceptance.
7. **Review.** After the final gate, spawn parallel `loom-code-reviewer` subagents:
   - (a) **Summaries.** The metaplan id through validate, store, evaluate, brief, seal, engine, spawner, routes, page key, body and key. That it is in no overview scope. That the engineering overview's metaplan stand-in is unchanged. The prompts agree with the code.
   - (b) **Chat.** The interrupt notice rule and its test. The collapse removal (no orphaned atom, rail CSS or storage key). The menu's Escape and outside-click handling and its focus return. Stop's placement against the help scene pins.
   - (c) **Accessibility and design.** The roundel's `aria-hidden` wrapper and no doubled live regions. Reduced motion. The 13px floor. Graphite contrast. The 390px no-sideways-scroll. DESIGN.md matches the CSS.
   - (d) **Dead code.** `pressure-page.tsx` and its helpers, `pressureOffer` if unused, the optional-pressure `markOf` branch, `CollapsedRail` CSS, `panel-collapsed`, the `QuotaMeters` import in `summary-panel.tsx`, and `ActivityGlyph` icon imports the roundel replaced.

   Fix or dispute every finding. The review gate refuses completion when the tree changed after its round, so a fix after a round needs step 6 again and another round.
8. **Commit.** The code fixes first, then `web/dist` last as `chore(web): rebuild the planner bundle`.

## Issue walk evidence to look for

| # | Evidence |
| --- | --- |
| 1 | Footer shot at 1440px: Claude and Codex in one row, matching the dashboard; wiring `display: grid` on `.plan-footer`; frozen footer contracts. |
| 2 | Contract `plan-rows-carry-the-pressure-record`; spine shot with badges; `/f/feat-planner/pressure` not found. |
| 3 | Contract `stop-then-interrupt-is-one-notice`; N1's unit test; shot after Stop. |
| 4 | Contract `stop-sits-in-the-action-row`; composer and head shots at 1440px and 390px; the run header on one line. |
| 5 | Open-menu shot; C1's Escape and outside-click test; frozen `chat-fixes` menu contract. |
| 6 | Contract `feature-opens-on-product-overview`; the engineering overview rebuild is an operator check. |
| 7 | Contracts `metaplan-summary-is-expected` and `metaplan-route-is-a-summary`; a built metaplan summary on screen is an operator check after a rebuild. |
| 8 | Contract `chat-panel-has-no-collapse`; shots at 1280px and 1180px. |
| 9 | Contract `progress-uses-the-roundel`; activity popover shot; B1's wrapped messages. |
| 10 | Shots of the spine and chat; after_stage type checks; DESIGN.md "Type". |
| 11 | Contract `graphite-accent-is-bright`; Graphite shots. |
| 12 | Settings shot in Ledger and Graphite at 1440px and 390px. |
| 13 | Contract `settings-shows-no-limits`. |
| 14 | Help topics read in the dialog; `doc/product/feat-planner` diff. |
| 15 | Composer shots with two and five reference chips and a file chip at 1440px and 390px, in Ledger and Graphite; one row anatomy, no sideways scroll, the field stays visible; the kickoff on `/new` reads the same. |

Record the operator checks below with `loom memory note "issue-walk open: …"`.

## What the operator checks after the run

Report these as open, never as passed:

- Rebuild summaries. Each feature metaplan gains `doc/loom/summaries/<feature>/metaplan/<stem>.md`. The engineering overview is rebuilt, and no sentence of it opens to a single member sentence.
- A real chat: press Stop during a turn and see one notice.
