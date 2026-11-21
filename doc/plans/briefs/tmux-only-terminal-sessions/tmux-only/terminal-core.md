# Brief: terminal-core (tmux-only stage)

You remove the native terminal lane from the orchestrator and move the launch code it shared with
tmux into a module named for what it does. Read the PUBLIC SURFACE block of the `tmux-only` stage
description in `doc/plans/PLAN-tmux-only-terminal-sessions.md` first: it is the interface other
workers code against in parallel, so match it exactly.

## Files you own

`loom/src/orchestrator/terminal/**`, `loom/src/orchestrator/mod.rs`, `loom/src/orchestrator/spawner.rs`,
`loom/src/models/session/**`, `loom/src/orchestrator/orphan_candidates.rs`,
`loom/src/orchestrator/session_registry.rs`, `loom/src/orchestrator/session_registry/identityless.rs`,
`loom/src/orchestrator/core/orchestrator.rs`, `loom/tests/e2e/tmux_backend.rs`.

Do not edit other files. The crate will not compile until the compile-sweep worker runs after you:
other files still import `terminal::native::` and set `Session.backend`. Judge your work by
`cargo check --manifest-path loom/Cargo.toml --all-targets 2>&1` showing no error located in a file
you own (log to `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/core-check.log`, then `rg` it for your paths).

## Steps

1. **Rename the module.** `git mv loom/src/orchestrator/terminal/native loom/src/orchestrator/terminal/launch`.
   Inside it, rename `launch.rs` to `prepare.rs` and the directory `launch/` (host.rs, tests_host.rs)
   to `prepare/` so there is no `launch::launch` (clippy `module_inception`). Fix every `mod`,
   `super::launch::`, `#[path]`-free reference inside the module. Keep all other file names
   (capsule, session_settings, wrapper, build_cache, pid_guard, pid_tracking, session_log and their
   tests, including every `tests_capsule_*`, `tests_confinement_*`, `tests_launch*`,
   `tests_session_settings`, `tests_wrapper_env`). Their behaviour must not change: same tracking key,
   PID-file path, session id, environment allowlists and wrapper text.
2. **Delete native-only code:** `launch/detection.rs`, `launch/window_identity.rs`,
   `launch/window_ops.rs`, `launch/window_ops/` (macos.rs, test_commands.rs, tests.rs),
   `terminal/emulator.rs`, `terminal/emulator/tests.rs`, `NativeBackend` and its helpers
   (`close_window_for_terminal`, `window_exists_for_terminal`, `with_terminal`) in `launch/mod.rs`,
   and the re-exports of `detect_terminal`, `spawn_in_terminal`, the window functions and
   `TerminalEmulator` in `terminal/mod.rs` and `orchestrator/mod.rs` (line 56 re-exports
   `NativeBackend`). In `launch/tests.rs` delete tests of `NativeBackend`; keep
   `write_test_pid_identity` (36 call sites use `launch::write_test_pid_identity` after the sweep) and
   every launch/capsule test.
3. **PID wait.** Move `await_session_pid` and `finalize_pid_identity` from `launch/spawner.rs` into a
   new `launch/pid_wait.rs`, drop the `fallback_pid` parameter (the tmux lane passes `None`; on no
   PID it returns the existing `Err("No PID discovered for session (pid_key=...)")`), drop the
   AppleScript timing comments but keep the delay/retry constants' values. Delete `spawner.rs`
   (`spawn_in_terminal`, `spawn_reaper_thread`). Re-export `pub(crate) use pid_wait::await_session_pid;`.
4. **Tracking key.** Replace `NativeBackend::window_title_and_pid_key` with
   `pub(crate) fn tracking_and_pid_key(session: &Session) -> Option<(String, String)>` in
   `launch/mod.rs`, same body (tracking key, else `loom-<stage_id>`; pid key `<tracking_key>-<session.id>`).
   Update `tmux/backend.rs`, `tmux/mod.rs`, `tmux/viewer.rs:20,190`, `tmux/socket.rs:330`,
   `tmux/tests_spawn.rs:338`, `session_registry/identityless.rs:27,52,86` and the doc comments that
   mention the old name. Keep `build_claude_command` in `launch/mod.rs` unchanged; reword its doc
   "shared by all native spawn sites" to the tmux spawn.
5. **SessionBackend** (`terminal/backend.rs`). Keep the type as a thin facade over `TmuxBackend`:
   fields `work_dir`, `tmux`, `tmux_available: fn() -> bool`. `pub fn new(work_dir: PathBuf) -> Self`
   replaces `from_config` (reads no config). `#[cfg(test)] pub(crate) fn with_tmux_probe(work_dir,
   tmux_available: fn() -> bool) -> Self`. Delete `configured_kind`, `native`, `lazy_native`,
   `resolve_lane`, `native_lane`, `spawn_native_lane`, `dispatch_spawn`. Add
   `pub const TMUX_MISSING_MESSAGE: &str = "Loom needs tmux on PATH to start sessions. Install tmux and retry.";`
   and `pub fn require_tmux(tmux_available: impl Fn() -> bool) -> anyhow::Result<()>` returning
   `Err(anyhow!(TMUX_MISSING_MESSAGE))` (Display must equal the constant exactly). Re-export
   `require_tmux` and `TMUX_MISSING_MESSAGE` from `terminal/mod.rs`. Each spawn method calls
   `require_tmux(self.tmux_available)` FIRST, before anything writes a wrapper or PID file, then calls
   the tmux method with `.with_context(|| format!("tmux spawn failed for session '{id}'"))` as today.
   `kill_session`, `release_ended_session` (keep `end_tmux_server` with its one retry and message),
   `is_session_alive` call the tmux lane unconditionally. Keep `merge_resolver_worktree`. Update the
   module docs. Rewrite `backend/tests.rs`: delete native and config-resolution cases; keep and adapt
   the tmux-missing, spawn-failure-context and `end_tmux_server` cases, using `with_tmux_probe`.
6. **Session model** (`models/session/`). Delete `SessionBackendKind`, its `Display`, `TerminalConfig`,
   `Session.backend`, `Session.native_window_title`, and their re-exports in `mod.rs`; fix
   `methods.rs`. `Session` has no `deny_unknown_fields`, so older records with `backend:` or
   `native_window_title:` keys still load; do not add any reader for them.
7. **Consumers you own.** `session_tail.rs`: delete the native log-tail branch and its test at :98;
   the tmux behaviour stays. `orphan_candidates.rs:55`: delete the native inference from an absent
   socket; an adopted orphan is a tmux session, and keep the owner/tracking-key and PID checks.
   `session_registry.rs:72-90`: `SessionBackend::new` cannot fail, so delete the from_config error
   branch and its comment about eager native construction. `orchestrator/core/orchestrator.rs:185`:
   `Arc::new(SessionBackend::new(config.work_dir.clone()))`. `orchestrator/spawner.rs` module doc:
   drop the `NativeBackend` reference. `tests/e2e/tmux_backend.rs:164,305`: reword comments to
   `launch::tracking_and_pid_key` and `prepare.rs`.
8. Delete any `session.backend = SessionBackendKind::Tmux` assignments in the tmux lane.
9. In your files, remove comments that describe a native lane, `[terminal]` config or `--backend`.

## Rules

- Preserve the tmux failure semantics: one server/socket per session, bounded control commands,
  post-spawn readiness probe, verified PID liveness, teardown after a partial spawn
  (`launch::cleanup_stage_files`), scrollback capture before kill. No fallback may run a second agent.
- No compatibility readers, no hidden aliases. Files under 400 lines, functions under 50.
- Do not run the full test suite. When the crate compiles after the sweep, the orchestrator runs
  `cargo test --lib orchestrator::terminal::`.
- Report: files moved/deleted, every public name that changed (old → new), tests deleted and why,
  anything outside your territory that still needs an edit, and any mistake or surprise for
  `loom memory`.
