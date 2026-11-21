# Brief: compile-sweep (tmux-only stage)

Three workers have deleted the native terminal lane (terminal-core, config-cli, surfaces) and have
returned. The crate does not compile yet: about 80 files, mostly tests, still use the removed names.
You make `loom` build, test and lint cleanly. Read the PUBLIC SURFACE block of the `tmux-only` stage
description in `doc/plans/PLAN-tmux-only-terminal-sessions.md` first.

## Files you own

Every file under `loom/src` and `loom/tests` (the other Rust workers are done), plus
`loom/maintainability-baseline.txt`. Do not edit `web/`, docs, or any `*contract_tests.rs` file
(frozen contracts).

## Renames and deletions to apply

| Old | New |
| --- | --- |
| `orchestrator::terminal::native::<item>` | `orchestrator::terminal::launch::<item>` (same item name) |
| `NativeBackend::window_title_and_pid_key(s)` | `terminal::launch::tracking_and_pid_key(s)` |
| `SessionBackend::from_config(dir)?` / `.ok()` / `.context(..)?` / `match` | `SessionBackend::new(dir)` (infallible; delete the error handling) |
| `backend: SessionBackendKind::Tmux` in a `Session` literal, `session.backend = ...` | delete the line |
| `native_window_title: ...` | delete |
| `write_terminal_config(..)` / `TerminalConfig { .. }` test setup | delete the setup |
| `EnvVarGuard::set("LOOM_TERMINAL", ..)` and manual `LOOM_TERMINAL` save/restore helpers with their comments | delete (`LOOM_TERMINAL_RECOVERY_HELPER` in `tests/terminal_recovery.rs` is unrelated: keep it) |
| `NativeBackend`, `TerminalEmulator`, `detect_terminal`, `spawn_in_terminal`, window functions | gone; a test that exists only to exercise them is deleted |

Known sites: `commands/sessions.rs:129`, `commands/stage/state.rs:197`,
`commands/stage/state/loop_recovery/mod.rs:50`, `orchestrator/tests_session_registry.rs` (native-eager
cases: delete those; keep the PID-only liveness case, rewritten without config),
`orchestrator/merge_lifecycle/test_support.rs:6-25`, `session_registry/identityless_tests.rs`,
`orchestrator/core/{merge_handler_attempt,crash_handler_identity,spool_drain}_tests.rs`,
`tests/integration/relay_*e2e.rs`, `tests/host_verification_e2e.rs`, and every file
`rg -l "terminal::native|SessionBackendKind|native_window_title|LOOM_TERMINAL\b|from_config|session_backend" loom/src loom/tests`
still lists (check that each `from_config` hit is `SessionBackend`'s; other types have their own).

## Steps

1. `cargo check --manifest-path loom/Cargo.toml --all-targets > "${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/sweep.log" 2>&1; echo "exit=$?"`,
   then `rg -n -m 40 "^error" -A 6` the log. Fix, repeat until it compiles.
2. `loom/maintainability-baseline.txt`: delete the four entries for deleted files
   (`terminal/emulator.rs build_command`, three `terminal/native/detection.rs`). Add none. Run
   `cargo test --manifest-path loom/Cargo.toml --test maintainability`; if a file you edited now
   exceeds 400 lines or a function 50, split it instead of adding an entry.
3. `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings` clean, including
   unused imports and dead code the deletions orphaned (delete the orphan; never `#[allow]`).
4. `cargo fmt --manifest-path loom/Cargo.toml --all`.
5. Run `cargo test --manifest-path loom/Cargo.toml --lib -- orchestrator::terminal:: orchestrator::session_registry user_config:: commands::config:: commands::run:: commands::init:: commands::attach:: daemon::server:: plan_server::run plan_server::wire:: status:: cli::`
   (logged as above). Fix failures caused by the removal; a test asserting native behaviour is
   deleted, one asserting tmux behaviour is fixed. Contract tests in `*contract_tests.rs` must pass
   unchanged; if one cannot, report it with the output and do not edit it.
6. `rg -q` the stage's two negative-grep acceptance commands (in the plan YAML) and clear every hit in
   `loom/`.

## Rules

- Mechanical edits only; when a fix needs a design choice the pinned interface does not settle,
  report it instead of choosing.
- Never change a test's assertion to make it pass unless the assertion was about the native lane.
- Report: counts of files touched, tests deleted (name and reason), any behaviour change you made,
  and mistakes or surprises for `loom memory`.
