# Brief: compile-sweep (tmux-only stage)

Three workers have deleted the native terminal lane (terminal-core, config-cli, surfaces) and have
returned. The crate does not compile yet: over 100 files outside the first-wave territories, mostly
tests, still use the removed names. You make `loom` build, test and lint cleanly. Read the PUBLIC
SURFACE block of the `tmux-only` stage description in
`doc/plans/IN_PROGRESS-PLAN-tmux-only-terminal-sessions.md` (the name `loom run` gives the plan in stage
worktrees) first.

## Files you own

Every file under `loom/src` and `loom/tests` except `loom/tests/installer_tmux_advisory.rs` (the docs
worker's), plus `loom/maintainability-baseline.txt`. Do not edit `web/` or docs.

Frozen for this stage are only its own contract files: `orchestrator/terminal/contract_tests.rs`,
`user_config/contract_tests.rs`, `daemon/server/launch/contract_tests.rs`,
`plan_server/wire/run_contract_tests.rs`, `cli/contract_tests.rs`. Never edit them; keep their `mod`
lines. Contract files of earlier plans are ordinary tests and are yours where a first-wave worker left
them: `orchestrator/core/{dead_worker,block,spawn_publication}_contract_tests.rs`,
`commands/hook/relay/provenance_contract_tests.rs`,
`orchestrator/terminal/tmux/history_contract_tests.rs`, `status/data/stopped_contract_tests.rs`,
`commands/init/execute_contract_tests.rs`: rename imports, delete removed-name setup lines and
arguments, change no assertion. Never edit `loom/tests/fixtures/**` (frozen replay inputs; the stage's
gates skip that directory).

## Renames and deletions to apply

| Old | New |
| --- | --- |
| `orchestrator::terminal::native::<item>` | `orchestrator::terminal::launch::<item>` (same item name) |
| `NativeBackend::window_title_and_pid_key(s)` | `terminal::launch::tracking_and_pid_key(s)` |
| `SessionBackend::from_config(dir)?` / `.ok()` / `.context(..)?` / `match` | `SessionBackend::new(dir)` (infallible; delete the error handling) |
| `self.backend.resolve_lane()` assignments and arguments | delete (`core/session_lifecycle.rs`, `core/stage_preflight.rs`); in `core/merge_handler/resolver_spawn.rs` the call becomes `write_ahead_merge_session(session, stage_id, work_dir)`, the lane parameter is gone |
| `backend: SessionBackendKind::Tmux` in a `Session` literal, `session.backend = ...` | delete the line |
| `native_window_title: ...` | delete |
| `write_terminal_config(..)` / `TerminalConfig { .. }` test setup | delete the setup |
| `EnvVarGuard::set("LOOM_TERMINAL", ..)` and manual `LOOM_TERMINAL` save/restore helpers with their comments | no replacement: the default lane was already tmux and these tests spawn nothing; delete each save/restore helper pair with its call sites (`LOOM_TERMINAL_RECOVERY_HELPER` in `tests/terminal_recovery.rs` is unrelated: keep it) |
| `LivenessProbe::PidOnly` in `orchestrator/tests_session_registry.rs` | `LivenessProbe::Backend(SessionBackend::new(<work dir>))`, assertions unchanged; delete the native-configured cases; rename `the_pid_only_probe_answers_liveness_with_no_terminal_backend_at_all` so it does not contain `terminal_backend` |
| `OsRuntime.backend: Option<SessionBackend>` in `commands/stage/state/loop_recovery/mod.rs` | `SessionBackend`; `is_alive` returns `self.backend.is_session_alive(&session)`; delete the `None` arm |
| `init::execute(plan, clean, backend, no_repair)` callers | drop the third argument |
| `"backend": "tmux"` in the request body in `loom/tests/plan_handoff_operator.rs` | delete the key |
| `NativeBackend`, `TerminalEmulator`, `detect_terminal`, `spawn_in_terminal`, window functions | gone; a test that exists only to exercise them is deleted |

Known sites: `commands/sessions.rs`, `commands/stage/state.rs`,
`commands/stage/state/loop_recovery/mod.rs`, `orchestrator/tests_session_registry.rs`,
`orchestrator/merge_lifecycle/test_support.rs`, `session_registry/identityless_tests.rs`,
`orchestrator/core/{merge_handler_attempt,crash_handler_identity,spool_drain}_tests.rs`,
`orchestrator/core/contract_budget_tests.rs`, `orchestrator/core/event_handler/contract_phase_tests.rs`,
`orchestrator/core/event_handler/contract_phase.rs`, `tests/integration/relay_*e2e.rs`,
`tests/host_verification_e2e.rs`, `tests/e2e/web_terminal.rs`, `tests/pressure_pipeline_contracts.rs`
(delete the test `builtin_backend_default_is_tmux` and its import), `commands/run/checks.rs` and
`commands/status.rs` (`sccache_status_line` import path), the production callers of the key function
(`orchestrator/core/launch_completion.rs`, `orchestrator/monitor/direct_worker.rs`,
`orchestrator/monitor/running_command.rs`, `commands/clean/sessions.rs`,
`daemon/server/peer_identity.rs`), and every file
`rg -l "terminal::native|SessionBackendKind|native_window_title|LOOM_TERMINAL\b|from_config|session_backend|resolve_lane|window_title_and_pid_key|write_terminal_config|terminal_backend|resolve_backend_flag|default_backend" loom/src loom/tests`
still lists (check that each `from_config` hit is `SessionBackend`'s; other types have their own).

## Steps

1. `cargo check --manifest-path loom/Cargo.toml --all-targets > "${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/sweep.log" 2>&1; echo "exit=$?"`,
   then `rg -n -m 40 "^error" -A 6` the log. Fix, repeat until it compiles.
2. Ledger (`loom/maintainability-baseline.txt`): delete the four rows for deleted files
   (`terminal/emulator.rs build_command`, three `terminal/native/detection.rs`). Then run
   `LOOM_LEDGER_TIGHTEN=1 cargo test --manifest-path loom/Cargo.toml --test maintainability` once (it
   removes stale rows and lowers rows of files and functions that shrank: the ledger is exact in both
   directions and the commit hook would otherwise rewrite it after the final review), then rerun it
   without the variable; it must pass. Never raise or add a row; split a file or function you grew
   past a limit. List every changed row in the report.
3. `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings` clean, including
   unused imports and dead code the deletions orphaned (delete the orphan; never `#[allow]`). Rustdoc
   gate: `RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps`
   must be clean (the commit hook runs it); fix intra-doc links to removed or private items by writing
   plain backticks.
4. `cargo fmt --manifest-path loom/Cargo.toml --all`.
5. Run each `cargo test` line of the stage's `acceptance:` in the plan YAML (the `--lib` filter line and
   the `--test e2e` and `--test pressure_pipeline_contracts` lines), logged as above. The
   `tests/integration` and `tests/host_verification_e2e.rs` edits are compile-checked by clippy and run
   in integration-verify. Fix failures caused by the removal; a
   test asserting native behaviour is deleted, one asserting tmux behaviour is fixed. This stage's five
   contract files must pass unchanged; if one cannot, report it with the output and do not edit it.
6. The stage has four negative-grep acceptance commands (in the plan YAML): run each with `rg -q` and
   clear every hit in `loom/`. Ratatui calls `terminal.backend_mut()` / `terminal.backend()` are not
   hits. Reword comments in `loom/src` and `loom/tests` that still name `terminal/native/`,
   `terminal::native` or `native::launch::` to the new path (`terminal/launch/`, `launch::prepare::`).

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- Mechanical edits only; when a fix needs a design choice the pinned interface does not settle,
  report it instead of choosing.
- Never change a test's assertion to make it pass unless the assertion was about the native lane.
- Report: counts of files touched, tests deleted (name and reason), changed ledger rows, any behaviour
  change you made, and mistakes or surprises for `loom memory`.
