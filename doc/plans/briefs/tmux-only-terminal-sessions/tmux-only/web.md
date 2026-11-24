# Brief: web (tmux-only stage)

You remove the terminal-backend selector and native-terminal hints from the dashboard and the Feature
Planner. The Rust side changes in parallel; the wire shapes below are pinned.

## Files you own

`web/src/**` and `web/scripts/graph-keyboard-test.mjs`. Only `web/src/tmux-only.contract.test.tsx` is
frozen (do not edit it); every other `*.contract.test.ts(x)` / `*.contracts.test.tsx` is an earlier
plan's ordinary test. Do not build `web/dist`; the main agent rebuilds it after you return.

## Pinned wire shapes

- Status stage summary: no `session_backend` field. Every live session is tmux.
- `GET /api/config`: no `terminal.backend` entry.
- `GET /api/run/state`: no `backends`, no `default_backend`.
- `POST /api/run/start` body: exactly `{ "plan": <path>, "clean": <bool> }`.

Do not touch Codex "native worker" code (`codex_jobs`, `codex.backend`, `lib/codex-jobs.ts`,
`stage-token-lanes.tsx`) or `plan.summary_backend`: those are other features that also use the
words native/backend. Check each `rg -n -i "native|backend" web/src` hit against this list.

## Steps

1. `src/api/schema.ts`: delete `sessionBackendSchema` and the `session_backend` line of
   `stageSummarySchema`; fix `schema.test.ts`. `stageSummarySchema` is `.strict()`, so every fixture
   and literal parsed or typed as a stage summary must lose the key. Remove `session_backend` keys from
   `src/api/fixtures/snapshot.json` and `src/api/fixtures/plan-stats-snapshot.json` (nine keys each,
   including the `"native"` one), the `terminal.backend` entry from `src/api/fixtures/config.json`, and
   from `help-example.tsx`, `plan/files/plan/stage-graph.tsx`,
   `components/terminal/terminal-view-test-helpers.tsx`, `api/token-schema.ts` if present,
   `src/routes/terminal.contract.test.tsx` (delete the `session_backend` line),
   `src/token-insights.contracts.test.tsx` (delete `session_backend: "tmux"` from the returned
   literal), `web/scripts/graph-keyboard-test.mjs` (delete the line `stage.session_backend = "tmux";`),
   and any other fixture or test that sets it (`rg -l session_backend web`). Rust tests under
   `loom/src/commands/status/web/` read these JSON files with `include_str!`: remove exactly the key.
2. `components/terminal/terminal-glyph.tsx`: `terminalGate(terminals, stage)` returns
   `"Start the dashboard with loom status --web --terminals"` when `!terminals`, `"No live session"`
   when `!stage.session_alive`, else `null`. `terminalHistory` drops the native check. Update the doc
   comment and `terminal-glyph.test.tsx`.
3. `components/stage-modal.tsx` and `routes/stage.tsx`: delete the native flag and the `CopyCommand` for
   `loom run --backend tmux`. `components/stage-sections.tsx`: delete the `backend` row and its hint
   text in the hints map ("Where the session runs: a native terminal window or a tmux pane."). Fix the
   related tests (`stage-modal.test.tsx`, `stage-modal-history.test.tsx`, `routes/terminal.test.tsx`,
   `terminal-view.test.tsx`, `history-view*`, `terminal-back.test.tsx`, `attention-panel.test.tsx`,
   `graph/stage-graph-keyboard.test.tsx`, `state/*.test.ts`, `lib/notify.test.ts`) only where they
   reference the removed field or text. Check `terminal-notice.tsx`, `mode-key.tsx`, `use-terminal.ts`,
   `history-view.tsx`, `routes/terminal.tsx` for native/backend wording.
4. Settings: `src/test/settings-kit.tsx` uses `["native","tmux"]` as its generic enum fixture (the
   `terminal.backend` entry); switch it to `plan.summary_backend` with variants `["claude","codex"]`,
   keeping validation, per-scope update and reset coverage in `settings-dialog.test.tsx` and
   `settings-model.test.ts`.
5. Planner Run: `plan/api/schema-run.ts`: delete `backendSchema` and the state/request fields;
   `plan/api/fixtures/run-state.json`: drop `backends`, `default_backend`. The Rust test
   `plan_server::wire::tests_run` reads both files with `include_str!`: remove exactly `backends`,
   `default_backend` and `backendSchema`, change nothing else in them. `plan/run/run-dialog.tsx`:
   delete the backend radio group, the environment-based native offer and the selected-backend state;
   the start request posts `{plan, clean}`. `plan/run/use-run.test.ts` passes `backend` to `startRun`:
   drop it. Check `plan/run/plan-run-actions.tsx` and `plan/feature/run-panel.tsx` for a backend
   argument. Update `run-dialog.test.tsx`.
6. `components/help-topics.ts` and `help-visuals.tsx`: terminal help says sessions run in tmux,
   `loom attach [stage-id]` opens them, and `loom status --web --terminals` enables browser
   terminals; no native or `--backend` wording.
7. Run `cd web && ./node_modules/.bin/tsc -b --noEmit && ./node_modules/.bin/oxlint --deny-warnings && ./node_modules/.bin/oxfmt --check`
   and `./node_modules/.bin/vitest run src/api src/components src/lib src/routes src/state src/plan/run src/plan/api src/plan/files src/plan/feature src/plan/settings src/token-insights.contracts.test.tsx`
   (from `web/`), the stage's acceptance selection. Integration-verify runs the whole suite.
   Fix everything they report.

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- No literals `--backend native`, `--backend tmux`, `terminal.backend`, `session_backend` or
  `default_backend` may remain in `web/src` or `web/scripts`; the only exempt file is
  `web/src/tmux-only.contract.test.tsx`. The stage greps for them.
- Keep accessibility labels and existing copy for the paths that remain.
- Report: files changed, tests changed, copy changed (old -> new), and mistakes or surprises for
  `loom memory`.
