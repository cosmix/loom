# Brief: terminal-core (tmux-only stage)

You remove the native terminal lane from the orchestrator and move the launch code it shared with
tmux into a module named for what it does. Read the PUBLIC SURFACE block of the `tmux-only` stage
description in `doc/plans/IN_PROGRESS-PLAN-tmux-only-terminal-sessions.md` (the name `loom run` gives
the plan in stage worktrees) first: it is the interface other
workers code against in parallel, so match it exactly.

## Files you own

`loom/src/orchestrator/terminal/**`, `loom/src/orchestrator/mod.rs`, `loom/src/orchestrator/spawner.rs`,
`loom/src/orchestrator/notify.rs`, `loom/src/models/session/**`, `loom/src/orchestrator/orphan_candidates.rs`,
`loom/src/orchestrator/session_registry.rs`, `loom/src/orchestrator/session_registry/identityless.rs`,
`loom/src/orchestrator/core/orchestrator.rs`, `loom/tests/e2e/tmux_backend.rs`.

`terminal/tmux/history_contract_tests.rs` is an earlier plan's contract file inside your territory
and is an ordinary test now: apply the renames, drop its `backend` setup, change no assertion.
`loom/src/orchestrator/terminal/contract_tests.rs` is THIS stage's frozen contract: never edit it, and
keep the `mod contract_tests;` line the contract session added to `terminal/mod.rs`.

Do not edit other files. The crate does not compile until compile-sweep runs after you: judge your
work by `cargo check --manifest-path loom/Cargo.toml --all-targets` (log to
`${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/core-check.log`) showing no error in a file you own.

## Steps

1. **Rename the module.** `mv loom/src/orchestrator/terminal/native loom/src/orchestrator/terminal/launch`
   (a plain filesystem move: subagents run no git commands; the main agent stages the rename).
   Inside it, rename `launch.rs` to `prepare.rs` and the inner directory `launch/` (command.rs,
   host.rs, tests_host.rs: all three move) to `prepare/`, so there is no `launch::launch` (clippy
   `module_inception`). `launch/mod.rs` declares `mod prepare;` and keeps re-exporting
   `prepare_session_launch`, `record_daemon_binary`, `run_host_facts`. `#[path = "tests_*.rs"]`
   attributes resolve beside the declaring file and need no edit when files move together. Fix every
   other `mod` and `super::launch::` reference. Keep all other file names (capsule, session_settings,
   wrapper, build_cache, pid_guard, pid_tracking, session_log and every `tests_*` file). Behaviour must
   not change: same tracking key, PID-file path, session id, environment allowlists, wrapper text.
2. **Delete native-only code:** `launch/detection.rs`, `launch/window_identity.rs`,
   `launch/window_ops.rs`, `launch/window_ops/`, `terminal/emulator.rs`, `terminal/emulator/tests.rs`,
   `NativeBackend` and its helpers (`close_window_for_terminal`, `window_exists_for_terminal`,
   `with_terminal`) in `launch/mod.rs`, and the re-exports of `detect_terminal`, `spawn_in_terminal`,
   the window functions and `TerminalEmulator` in `terminal/mod.rs`, plus the `pub use` of
   `NativeBackend` in `orchestrator/mod.rs`.
   BEFORE deleting `emulator.rs`: `orchestrator/notify.rs` imports
   `terminal::emulator::escape_applescript_string` (the only consumer outside the terminal tree;
   desktop notifications stay). Move that function into `notify.rs` as a private fn, move its tests
   (`test_escape_applescript_string` in `emulator/tests.rs`) into notify's tests with assertions
   unchanged, and delete the import.
   In `launch/tests.rs` (today `native/tests.rs`) delete only `test_backend`, the `kill_session_*`
   tests and `test_native_backend_creation`. KEEP the key tests (`window_title_and_pid_key_*`,
   `pid_key_distinct_per_session_for_same_stage`, `prefix_sharing_stage_ids_get_distinct_titles`)
   rewritten to call `tracking_and_pid_key`, assertions unchanged: they guard "identical values". Keep
   the `build_claude_command_*` tests and `write_test_pid_identity` (53 files, 109 references at the
   base tree; re-measure with `rg -c write_test_pid_identity loom/src loom/tests`).
3. **PID wait.** Move `await_session_pid` and `finalize_pid_identity` from `launch/spawner.rs` into a
   new `launch/pid_wait.rs`; drop the `fallback_pid` parameter (on no PID it returns the existing
   `Err("No PID discovered for session (pid_key=...)")`) and the AppleScript timing comments, keep the
   delay/retry constants' values. Delete `spawner.rs` (`spawn_in_terminal`, `spawn_reaper_thread`).
   Re-export `pub(crate) use pid_wait::await_session_pid;`. The one remaining caller is
   `await_tmux_session_pid` in `tmux/mod.rs`: drop its trailing `None` and reword its doc comment,
   which describes the native lane.
4. **Tracking key.** Replace `NativeBackend::window_title_and_pid_key` with
   `pub(crate) fn tracking_and_pid_key(session: &Session) -> Option<(String, String)>` in
   `launch/mod.rs`, same body (tracking key, else `loom-<stage_id>`; pid key `<tracking_key>-<session.id>`).
   Update: `tmux/viewer.rs` (import and title helper), `tmux/socket.rs` (test calling
   `super::super::native::NativeBackend::window_title_and_pid_key`), `tmux/tests_spawn.rs`,
   `session_registry/identityless.rs` (import, both calls), `launch/pid_guard.rs` (imports
   `NativeBackend` from `super`, calls it in production and test code: use `super::tracking_and_pid_key`),
   and doc comments naming the old function. `tmux/backend.rs` and `tmux/mod.rs` need doc-comment
   edits only (they link `native::NativeBackend::new`, `::spawn_session`, `::spawn_main_repo_session`).
   Keep `build_claude_command`; reword its doc "shared by all native spawn sites" to the tmux spawn.
   Production callers outside your files (`orchestrator/core/launch_completion.rs`,
   `orchestrator/monitor/direct_worker.rs`, `orchestrator/monitor/running_command.rs`,
   `commands/clean/sessions.rs`, `daemon/server/peer_identity.rs`) belong to compile-sweep.
5. **SessionBackend** (`terminal/backend.rs`). Keep it a thin facade over `TmuxBackend`: fields
   `work_dir`, `tmux`, `tmux_available: fn() -> bool`. `pub fn new(work_dir: PathBuf) -> Self`
   replaces `from_config` (reads no config). `#[cfg(test)] pub(crate) fn with_tmux_probe(work_dir,
   tmux_available: fn() -> bool) -> Self`. Delete `configured_kind`, `native`, `lazy_native`,
   `resolve_lane`, `native_lane`, `spawn_native_lane`. `resolve_lane` callers: `commands/stage/merge_resolver.rs`
   and `core/merge_handler/resolver_stop.rs` are the surfaces worker's; `core/session_lifecycle.rs`,
   `core/stage_preflight.rs`, `core/merge_handler/resolver_spawn.rs` are compile-sweep's.
   `TMUX_MISSING_MESSAGE` already exists here with the old text (`commands/run/mod.rs` imports it):
   REPLACE its text with `"Loom needs tmux on PATH to start sessions. Install tmux and retry."`, do not
   add a second constant. Add `pub fn require_tmux(tmux_available: impl Fn() -> bool) ->
   anyhow::Result<()>` returning `Err(anyhow!(TMUX_MISSING_MESSAGE))` (Display equals the constant).
   Re-export both from `terminal/mod.rs`. Add `pub fn ensure_tmux(&self) -> anyhow::Result<()>`
   returning `require_tmux(self.tmux_available)`: the surfaces worker calls it from the manual merge
   resolver before that path publishes anything.
   Keep ONE private seam `fn dispatch_spawn(&self, session: Session, spawn_tmux: impl
   FnOnce(&TmuxBackend, Session) -> Result<Session>) -> Result<Session>`: it calls
   `self.ensure_tmux()?` FIRST, before anything writes a wrapper or PID file, then the
   closure wrapped in the existing `tmux spawn failed for session '<id>'` context. ALL FIVE spawn
   methods go through it (the frozen contract calls all five with a false probe and asserts no file was
   written). `kill_session`, `release_ended_session` (keep `end_tmux_server` with its one retry and
   message), `is_session_alive` call the tmux lane unconditionally. Keep `merge_resolver_worktree`.
   Rewrite the module docs of `backend.rs` and `terminal/mod.rs`.
   Rewrite `backend/tests.rs`: delete native and config-resolution cases (including
   `resolve_lane_never_depends_on_tmux_availability`); keep and adapt the tmux-missing,
   spawn-failure-context and `end_tmux_server` cases with `with_tmux_probe`. The spawn-failure test
   injects a failing closure through `dispatch_spawn`; the missing-tmux tests assert `to_string()`
   equals `TMUX_MISSING_MESSAGE`.
6. **Session model** (`models/session/`). Delete `SessionBackendKind`, its `Display`, `TerminalConfig`,
   `Session.backend`, `Session.native_window_title`, and their re-exports in `mod.rs`; fix
   `methods.rs`. `Session` has no `deny_unknown_fields`, so older records with `backend:` or
   `native_window_title:` keys still load; add no reader for them.
7. **Consumers you own.**
   - `session_tail.rs`: delete the `match session.backend` native log-tail branch and its test (the one
     setting `SessionBackendKind::Native`); the tmux behaviour stays.
   - `orphan_candidates.rs`: delete the `backend` field of `OrphanEvidence` and the native inference
     from an absent socket; in `session_registry.rs` delete `session.backend = evidence.backend` in
     `adopt_orphan`. An adopted orphan with no socket is still recorded; keep the owner/tracking-key
     and PID checks.
   - `session_registry.rs` `LivenessProbe::resolve` returns
     `Self::Backend(SessionBackend::new(work_dir.to_path_buf()))`: delete the `from_config` error
     branch, its `DEGRADED` log and the eager-native comment. Delete the `PidOnly` variant and its
     `is_alive` arm: the tmux lane's `is_session_alive` is `pid_only_is_alive`, so the Backend probe
     answers the same, and a variant built only by tests fails clippy dead_code. Keep the
     `#[cfg(test)] Error` variant. Rewrite the enum and `resolve` doc comments.
     (`orchestrator/tests_session_registry.rs` is compile-sweep's.)
   - `orchestrator/core/orchestrator.rs`: the `SessionBackend::from_config(..)?` call in
     `Orchestrator::new` becomes `Arc::new(SessionBackend::new(config.work_dir.clone()))`.
   - `orchestrator/spawner.rs` module doc: drop the `NativeBackend` reference.
   - `tests/e2e/tmux_backend.rs`: reword the comments naming `window_title_and_pid_key` and
     `native/launch.rs` to `launch::tracking_and_pid_key` and `prepare.rs`.
8. Delete every other reader or writer of `session.backend` in your files: `session.backend =
   SessionBackendKind::Tmux` in the tmux lane; the Tmux guard in `tmux/viewer.rs`
   `load_live_tmux_session`; the guard in `tmux/fullscreen.rs` `is_sampled` and the test helper that
   assigns the field; the `backend` field of the fixture in `tmux/tests_discovery.rs` plus its test
   `live_tmux_sessions_skips_native_backend_sessions`; the uses in `tmux/tests_history_support.rs`.
9. Remove comments in your files that describe a native lane, `[terminal]` config or `--backend`.

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- Preserve the tmux failure semantics: one server/socket per session, bounded control commands,
  post-spawn readiness probe, verified PID liveness, teardown after a partial spawn
  (`launch::cleanup_stage_files`), scrollback capture before kill. No fallback may run a second agent.
- The commit hook runs rustdoc with warnings denied: an intra-doc link (`` [`name`] ``) to a deleted or
  private item fails it. In doc comments name removed or private items in plain backticks.
- Report every ledger row (`loom/maintainability-baseline.txt`) whose file or function you shrank;
  compile-sweep lowers them.
- No compatibility readers, no hidden aliases. Files under 400 lines, functions under 50.
- Do not run the full test suite.
- Report: files moved/deleted, public names changed (old -> new), tests deleted and why, edits still
  needed outside your territory, mistakes or surprises for `loom memory`.
