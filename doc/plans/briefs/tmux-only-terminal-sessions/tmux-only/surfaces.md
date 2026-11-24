# Brief: surfaces (tmux-only stage)

You remove native-lane branches from the operator-facing Rust surfaces: the planner run API and its
child processes, `loom attach`, the status payload and merge-attention text, the browser terminal
resolver, and merge-resolver stop/launch. Read the PUBLIC SURFACE block of the `tmux-only` stage
description in `doc/plans/IN_PROGRESS-PLAN-tmux-only-terminal-sessions.md` (the name `loom run` gives
the plan in stage worktrees) first; other workers code against it in parallel.

## Files you own

`loom/src/plan_server/run/**`, `loom/src/plan_server/wire/run.rs`, `loom/src/plan_server/wire/tests_run.rs`,
`loom/src/plan_server/router.rs`, `loom/src/commands/attach/**`, `loom/src/status/**`,
`loom/src/commands/status/web/terminal/**`, `loom/src/commands/status/web/model_tests_stages.rs`,
`loom/src/commands/status/ui/**`, `loom/src/commands/status/render/**`,
`loom/src/commands/stage/merge_resolver.rs`, `loom/src/orchestrator/core/merge_handler/resolver_stop.rs`,
`loom/src/orchestrator/signals/merge_launch.rs`, `loom/src/orchestrator/core/completion_handler.rs`,
`loom/src/orchestrator/core/mod.rs`.

`status/data/stopped_contract_tests.rs` is an earlier plan's contract file and an ordinary test now:
drop its `backend` setup lines, change no assertion. The `mod run_contract_tests;` line the contract
session added to `plan_server/wire/mod.rs` stays; you do not own that file, do not edit it.

The crate will not compile until the compile-sweep worker runs after you. Judge your work by
`cargo check --manifest-path loom/Cargo.toml --all-targets` showing no error in a file you own (log
to `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/surf-check.log`, `rg` it for your paths).

## Names other workers delete

`SessionBackendKind`, `TerminalConfig`, `Session.backend`, `Session.native_window_title`,
`read_terminal_config`, `orchestrator::terminal::native::*` (now `terminal::launch::*`),
`SessionBackend::from_config` (now infallible `SessionBackend::new(work_dir)`),
`SessionBackend::resolve_lane`,
`NativeBackend::window_title_and_pid_key` (now `terminal::launch::tracking_and_pid_key`).

## Steps

1. **Planner run wire** (`plan_server/wire/run.rs`): delete `enum Backend`,
   `RunStateView.backends`, `RunStateView.default_backend` and `RunStartRequest.backend`; the request
   is `{ plan: String, clean: bool }` with `#[serde(deny_unknown_fields)]` (the convention of
   `plan_server/wire/server.rs`), so a body naming `backend` fails to deserialize. In
   `plan_server/run/` add a route test (beside the existing start tests in `tests_route.rs`) that
   POSTs `{"plan":<a plan>,"clean":false,"backend":"tmux"}` to `/api/run/start` and asserts a 4xx
   response and that no run job began (the jobs guard is still free: a valid start right after is
   not answered `busy`). Update `tests_run.rs`: it reads
   `web/src/plan/api/fixtures/run-state.json` and `web/src/plan/api/schema-run.ts` with `include_str!`.
   Delete the `Backend` part of `backend_job_and_stream_values_are_in_schema` and the `Backend` import.
   `run_state_fixture_matches_serde_output` passes once the web worker has removed `backends` and
   `default_backend` from that fixture; it may fail in your isolated check until then.
2. **Planner run** (`plan_server/run/`): `state.rs`: delete `backends()` and the native offer; delete
   `EnvView` only if nothing else uses it (`rg -n EnvView loom/src`). `start.rs`: `loom init <plan>
   [--clean]` with no `--backend`; keep init-then-run and run-only resume and their clean/abandon
   protections; update `start/tests.rs`, `start/tests_refusal.rs`, `state/orphaned.rs`,
   `state/tests_workspace.rs`. `dashboard.rs`: always pass `--terminals` to the dashboard child; drop
   the config read; keep holder-only token delivery, redaction and read-only/SSH behaviour; rename the
   test `child_arguments_follow_host_and_terminal_backend` (in `dashboard/tests.rs`) to cover host only
   and assert `--terminals` is always present. `router.rs` has no backend mention (verify with
   `rg -in backend loom/src/plan_server/router.rs`): no edit.
3. **Attach** (`commands/attach/`): discovery no longer filters on backend (every session is tmux);
   `no_live_sessions_message` loses its backend parameter and the native tip; `diagnose.rs` loses native
   branches. Keep endpoint-not-ready ("still spawning, or just ended") distinct from no-session.
   Update `attach/tests.rs`.
4. **Status payload** (`status/`): delete `StageSummary.session_backend` (field in `status/data/mod.rs`,
   producer in `status/data/collector.rs`) and the native handling in `stopped.rs`: saved history is
   offered only when the transcript file exists, as today. `attention_model/merge.rs` `resolver_view`
   today returns `None` when `stage.session_backend` is `None`; new rule: return `Some(<the tmux attach
   wording>)` when `stage.session_alive`, else `None`; delete the native-window branch. Retarget the
   cases in `attention_model_guidance_tests.rs` from `session_backend: Some(Tmux)` to
   `session_alive: true`; delete the native-window cases. Update the other tests naming the field
   (`blocked_guidance_tests.rs`, `data/collector_resolver_tests.rs`, `data/inventory_tests.rs`),
   `commands/status/render/*tests.rs`, `commands/status/ui/tui/app.rs` and
   `commands/status/web/model_tests_stages.rs`.
5. **Browser terminal** (`commands/status/web/terminal/resolve.rs`): delete the native refusal and its
   "run with --backend tmux" text; a stage with no live session or no ready endpoint keeps its
   existing distinct refusals. Update `tests_history.rs`, `tests_route.rs`.
6. **Merge resolver**: pin `write_ahead_merge_session(session: &mut Session, stage_id: &str, work_dir:
   &Path)` in `orchestrator/signals/merge_launch.rs` with its current return type: the `lane`
   parameter is removed (verify the current signature and keep the parameter types as they are).
   Update its callers in your files: `commands/stage/merge_resolver.rs` (including its test; also
   `SessionBackend::new`, drop the native match arm) and the tests in `merge_launch.rs`.
   In `merge_resolver.rs`, the manual branch publishes a merge snapshot, writes the signal and records
   the write-ahead session before it reaches the facade, so a missing tmux would leave launch state
   behind and record a crashed resolver. Build the `SessionBackend` in the caller of
   `spawn_resolver_session` and pass it in; make `backend.ensure_tmux()?` the first statement of
   `spawn_resolver_session`, which runs only after the `DaemonManaged` and `AlreadyRunning` returns and
   before `publish_merge_snapshot` (the stage's wiring check greps `ensure_tmux(` in this file). Add a
   test with `SessionBackend::with_tmux_probe(work_dir, || false)`: the call returns Err whose chain
   contains `TMUX_MISSING_MESSAGE`, and the stage's session field, the signals directory and the merge
   snapshot are unchanged; calling it a second time gives the same Err and still changes nothing;
   `core/merge_handler/resolver_spawn.rs` is compile-sweep's. In `merge_launch.rs`
   `publish_merge_session` delete `current.backend = spawned.backend`. In `resolver_stop.rs` delete
   `session.backend = self.backend.resolve_lane();`, remove the lane parameter of `teardown_proves_gone`
   and its `Native => false` arm so the tmux behaviour (the kill must succeed) is unconditional.
   `orchestrator/core/completion_handler.rs`, `orchestrator/core/mod.rs`: remove `SessionBackendKind`
   uses, keeping tmux behaviour.
7. Check whether any Rust test reads `web/src/api/fixtures/*.json` or `web/src/plan/api/fixtures/*.json`
   (`rg -n "fixtures/(snapshot|config|run-state|plan-stats-snapshot)" loom`); if so, report it: the web
   worker edits those fixtures to drop `session_backend`, `backends` and `default_backend`.

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- No compatibility tolerance for the old `backend` request field. Clean removal.
- The stage greps for these literals outside the contract files: `--backend native`, `--backend tmux`,
  `terminal.backend`, `session_backend`, `default_backend`, `SessionBackendKind`; assert on the new
  text instead.
- The commit hook runs rustdoc with warnings denied: an intra-doc link (`` [`name`] ``) to a deleted or
  private item fails it. In doc comments name removed or private items in plain backticks.
- Files under 400 lines, functions under 50. Do not run the full suite.
- Report: deleted fields and types, changed messages (quote old and new), tests changed, edits needed
  outside your territory, and mistakes or surprises for `loom memory`.
