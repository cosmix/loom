# Brief: surfaces (tmux-only stage)

You remove native-lane branches from the operator-facing Rust surfaces: the planner run API and its
child processes, `loom attach`, the status payload and merge-attention text, the browser terminal
resolver, and merge-resolver stop/launch. Read the PUBLIC SURFACE block of the `tmux-only` stage
description in `doc/plans/PLAN-tmux-only-terminal-sessions.md` first; other workers code against it
in parallel.

## Files you own

`loom/src/plan_server/run/**`, `loom/src/plan_server/wire/run.rs`, `loom/src/plan_server/wire/tests_run.rs`,
`loom/src/plan_server/router.rs`, `loom/src/commands/attach/**`, `loom/src/status/**`,
`loom/src/commands/status/web/terminal/**`, `loom/src/commands/status/web/model_tests_stages.rs`,
`loom/src/commands/status/ui/**`, `loom/src/commands/status/render/**`,
`loom/src/commands/stage/merge_resolver.rs`, `loom/src/orchestrator/core/merge_handler/resolver_stop.rs`,
`loom/src/orchestrator/signals/merge_launch.rs`, `loom/src/orchestrator/core/completion_handler.rs`,
`loom/src/orchestrator/core/mod.rs`.

The crate will not compile until the compile-sweep worker runs after you. Judge your work by
`cargo check --manifest-path loom/Cargo.toml --all-targets` showing no error in a file you own (log
to `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/surf-check.log`, `rg` it for your paths).

## Names other workers delete

`SessionBackendKind`, `TerminalConfig`, `Session.backend`, `Session.native_window_title`,
`read_terminal_config`, `orchestrator::terminal::native::*` (now `terminal::launch::*`),
`SessionBackend::from_config` (now infallible `SessionBackend::new(work_dir)`),
`NativeBackend::window_title_and_pid_key` (now `terminal::launch::tracking_and_pid_key`).

## Steps

1. **Planner run wire** (`plan_server/wire/run.rs`): delete `enum Backend`,
   `RunStateView.backends`, `RunStateView.default_backend` and `RunStartRequest.backend`; the request
   is `{ plan: String, clean: bool }`. Update `tests_run.rs`.
2. **Planner run** (`plan_server/run/`): `state.rs` (~16, 90-105): delete `backends()` and the native
   offer; delete `EnvView` only if nothing else uses it (`rg -n EnvView loom/src`). `start.rs`
   (~200-225): `loom init <plan> [--clean]` with no `--backend`; keep init-then-run and run-only
   resume and their clean/abandon protections; update `start/tests.rs`, `start/tests_refusal.rs`,
   `state/orphaned.rs`, `state/tests_workspace.rs`. `dashboard.rs` (~12, 78-84): always pass
   `--terminals` to the dashboard child; drop the config read; keep holder-only token delivery,
   redaction and read-only/SSH behaviour; rename the test
   `child_arguments_follow_host_and_terminal_backend` to cover host only and assert `--terminals` is
   always present. `router.rs`: drop backend mentions.
3. **Attach** (`commands/attach/`): discovery no longer filters on backend (every session is tmux);
   `no_live_sessions_message` loses its backend parameter and the native tip (~242, 271);
   `diagnose.rs` loses native branches. Keep endpoint-not-ready ("still spawning, or just ended")
   distinct from no-session. Update `attach/tests.rs` (~45).
4. **Status payload** (`status/`): delete `StageSummary.session_backend` (`data/mod.rs:229`,
   `data/collector.rs:226`) and `stopped.rs` native handling (~175): saved history is offered only
   when the transcript file exists, as today. `attention_model/merge.rs:54-60`: the merge-attention
   text loses its native branch; keep the tmux attach wording. Update guidance, attention and data
   tests (`attention_model_guidance_tests.rs`, `blocked_guidance_tests.rs`, `data/*_tests.rs`) and
   `commands/status/render/*tests.rs`, `commands/status/ui/tui/app.rs`,
   `commands/status/web/model_tests_stages.rs`.
5. **Browser terminal** (`commands/status/web/terminal/resolve.rs`): delete the native refusal and its
   "run with --backend tmux" text; a stage with no live session or no ready endpoint keeps its
   existing distinct refusals. Update `tests_history.rs`, `tests_route.rs`.
6. **Merge resolver**: `commands/stage/merge_resolver.rs:127,194`: `SessionBackend::new`, drop the
   native match arm; `orchestrator/core/merge_handler/resolver_stop.rs:256`: drop `Native => false`
   so the tmux behaviour is unconditional; `orchestrator/signals/merge_launch.rs`: drop setting the
   backend on the write-ahead record; `orchestrator/core/completion_handler.rs`,
   `orchestrator/core/mod.rs`: remove `SessionBackendKind` uses, keeping tmux behaviour.
7. Check whether any Rust test reads `web/src/api/fixtures/*.json` or `web/src/plan/api/fixtures/*.json`
   (`rg -n "fixtures/(snapshot|config|run-state)" loom`); if so, report it: the web worker edits those
   fixtures to drop `session_backend`, `backends` and `default_backend`.

## Rules

- No compatibility tolerance for the old `backend` request field. Clean removal.
- No assertions containing the literals `--backend native`, `--backend tmux` or `terminal.backend`
  (the stage greps for them); assert on the new text instead.
- Files under 400 lines, functions under 50. Do not run the full suite.
- Report: deleted fields and types, changed messages (quote old and new), tests changed, edits needed
  outside your territory, and mistakes or surprises for `loom memory`.
