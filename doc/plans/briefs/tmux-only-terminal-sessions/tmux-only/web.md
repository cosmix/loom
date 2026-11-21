# Brief: web (tmux-only stage)

You remove the terminal-backend selector and native-terminal hints from the dashboard and the Feature
Planner. The Rust side changes in parallel; the wire shapes below are pinned.

## Files you own

`web/src/**`, except `web/src/tmux-only.contract.test.tsx` (frozen contract file; do not edit it).
Do not build `web/dist`; integration-verify does.

## Pinned wire shapes

- Status stage summary: no `session_backend` field. Every live session is tmux.
- `GET /api/config`: no `terminal.backend` entry.
- `GET /api/run/state`: no `backends`, no `default_backend`.
- `POST /api/run/start` body: exactly `{ "plan": <path>, "clean": <bool> }`.

Do not touch Codex "native worker" code (`codex_jobs`, `codex.backend`, `lib/codex-jobs.ts`,
`stage-token-lanes.tsx`) or `plan.summary_backend`: those are other features that also use the
words native/backend. Check each `rg -n -i "native|backend" web/src` hit against this list.

## Steps

1. `src/api/schema.ts`: delete `sessionBackendSchema` (~61) and `session_backend` (~179); fix
   `schema.test.ts`. Remove `session_backend` keys from `src/api/fixtures/snapshot.json`, the
   `terminal.backend` entry (~49) from `src/api/fixtures/config.json`, and from `help-example.tsx`
   (~26), `plan/files/plan/stage-graph.tsx` (~65), `components/terminal/terminal-view-test-helpers.tsx`
   (~163), `api/token-schema.ts` if present, and any other fixture or test that sets it.
2. `components/terminal/terminal-glyph.tsx`: `terminalGate(terminals, stage)` returns
   `"Start the dashboard with loom status --web --terminals"` when `!terminals`, `"No live session"`
   when `!stage.session_alive`, else `null`. `terminalHistory` drops the native check. Update the doc
   comment and `terminal-glyph.test.tsx`.
3. `components/stage-modal.tsx` (~104, 367) and `routes/stage.tsx` (~78): delete the native flag and
   the `CopyCommand` for `loom run --backend tmux`. `components/stage-sections.tsx` (~52, 146): delete
   the backend row and its hint. Fix the related tests (`stage-modal.test.tsx`,
   `stage-modal-history.test.tsx`, `routes/terminal.test.tsx`, `terminal-view.test.tsx`,
   `history-view*`, `terminal-back.test.tsx`, `attention-panel.test.tsx`,
   `graph/stage-graph-keyboard.test.tsx`, `state/*.test.ts`, `lib/notify.test.ts`) only where they
   reference the removed field or text. Check `terminal-notice.tsx`, `mode-key.tsx`, `use-terminal.ts`,
   `history-view.tsx`, `routes/terminal.tsx` for native/backend wording.
4. Settings: `src/test/settings-kit.tsx` (~53) uses `["native","tmux"]` as its generic enum fixture;
   switch it to `plan.summary_backend` with variants `["claude","codex"]`, keeping validation,
   per-scope update and reset coverage in `settings-dialog.test.tsx` and `settings-model.test.ts`.
5. Planner Run: `plan/api/schema-run.ts` (~14): delete `backendSchema` and the state/request fields;
   `plan/api/fixtures/run-state.json` (~10): drop `backends`, `default_backend`.
   `plan/run/run-dialog.tsx` (~341): delete the backend radio group, the environment-based native
   offer and the selected-backend state; the start request posts `{plan, clean}`. Check
   `plan/run/plan-run-actions.tsx` and `plan/feature/run-panel.tsx` for a backend argument. Update
   `run-dialog.test.tsx`.
6. `components/help-topics.ts` and `help-visuals.tsx`: terminal help says sessions run in tmux,
   `loom attach [stage-id]` opens them, and `loom status --web --terminals` enables browser
   terminals; no native or `--backend` wording.
7. Run `cd web && ./node_modules/.bin/tsc -b --noEmit && ./node_modules/.bin/oxlint --deny-warnings && ./node_modules/.bin/oxfmt --check`
   and `./node_modules/.bin/vitest run src/components src/routes src/plan/run src/plan/api src/api`
   (from `web/`). Fix everything they report.

## Rules

- No literals `--backend native`, `--backend tmux`, `terminal.backend` or `session_backend` may remain
  in `web/src` outside the contract file; the stage greps for them.
- Keep accessibility labels and existing copy for the paths that remain.
- Report: files changed, tests changed, copy changed (old → new), and mistakes or surprises for
  `loom memory`.
