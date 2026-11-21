# Brief: config-cli (tmux-only stage)

You remove every way to select a terminal backend: the `[terminal]` section, the `terminal.backend`
key, the `--backend` flag on `loom init` / `loom run`, the init prompt, the run-path backend
resolution, the daemon's terminal forwarding, and the key's dashboard Settings entry. Read the
PUBLIC SURFACE block of the `tmux-only` stage description in
`doc/plans/PLAN-tmux-only-terminal-sessions.md` first; other workers code against it in parallel.

## Files you own

`loom/src/user_config/**`, `loom/src/fs/work_dir.rs`, `loom/src/fs/work_dir/**`,
`loom/src/commands/config/**`, `loom/src/cli/**`, `loom/src/commands/init/**`,
`loom/src/commands/run/**`, `loom/src/completions/**`, `loom/src/daemon/server/launch.rs`,
`loom/src/daemon/server/launch/**`, `loom/src/daemon/server/environment.rs`,
`loom/src/commands/status/web/config_api.rs`, `loom/src/commands/status/web/config_api/**`,
`loom/src/commands/status/web/tests/config_api.rs`, `loom/tests/e2e/daemon_config/**`.

The crate will not compile until the compile-sweep worker runs after you. Judge your work by
`cargo check --manifest-path loom/Cargo.toml --all-targets` showing no error in a file you own
(log it to `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/cfg-check.log` and `rg` it for your paths).

## Names other workers delete (do not reference them)

`SessionBackendKind`, `TerminalConfig`, `Session.backend`, `orchestrator::terminal::native::*`
(now `terminal::launch::*`), `detect_terminal`. Use
`crate::orchestrator::terminal::{require_tmux, TMUX_MISSING_MESSAGE}` (terminal-core adds them).

## Steps

1. **User config.** `user_config/mod.rs`: delete the `terminal_backend` field, `terminal_backend()`,
   `terminal_backend_set()` and the doc references to them (line ~280 points another accessor's
   docs at `terminal_backend_set`; repoint to `context_ceiling_tokens_set` itself). `parse.rs`:
   delete `get_backend`. `keys.rs`: delete the `terminal.backend` KeySpec (line ~72). `render.rs`:
   drop `terminal.backend` from the value table (~39) and the `[terminal]` block from the TOML printer
   (~60-72), so `--print` has no empty terminal group. `workspace.rs`: drop the `terminal.backend`
   mapping (~190) and doc mentions. `value.rs` and user_config tests: remove terminal cases; where a
   test used `terminal.backend` as its example enum key, switch to another real enum key
   (`plan.summary_backend`: `claude`/`codex`) so the same behaviour stays covered. A leftover
   `[terminal]` table must parse and be ignored: `parse_document` reads per key, so deleting the
   reader is enough; add no code for it.
2. **Project config.** Delete `fs/work_dir/config_sections/terminal_config.rs` and its `tests.rs`,
   the `mod` line and re-exports in `config_sections.rs` and `fs/work_dir.rs` (lines 13, 16), and the
   terminal tests in `fs/work_dir/tests.rs` (round trip ~295, defaults ~307, user fallthrough ~479,
   and ~497). Keep `stale_project_execution_section_is_harmless`.
3. **`loom config`.** `commands/config/**` (TUI, `--list`, `--print`, `-k`, completion of keys): no
   terminal row or group. TUI tests using `terminal.backend` (cycling, quit, rendering, save, scope)
   switch to `plan.summary_backend` with the same assertions. Writing `terminal.backend` now fails as
   any unknown key does.
4. **CLI.** `cli/types.rs` (init) and `cli/types_runtime.rs` (`RunArgs`): delete the `--backend`
   options; fix dispatch. `loom ask --backend` (`cli/types_config.rs` or wherever it lives) is a
   different option: leave it.
5. **Init.** Delete `commands/init/backend.rs` and `backend_tests.rs` and their `mod` lines.
   `plan_setup.rs`: delete the `[terminal]` write and backend parameter. Keep a non-fatal advisory:
   when `require_tmux(|| which::which("tmux").is_ok())` is `Err`, print `warning: <message>` to
   stderr and continue. Fix `commands/init/tests.rs` (~57).
6. **Run.** `commands/run/mod.rs`: delete `resolve_backend_flag` and `resolve_backend_flag_with_probe`
   (~200-320), the daemon-restart check for a backend change and the persistence; both
   `execute_background` and `run/foreground.rs::execute` call
   `crate::orchestrator::terminal::require_tmux(|| which::which("tmux").is_ok())?` before starting
   the daemon or orchestrator. `run/guidance.rs`: drop backend guidance. Keep a unit test that the
   preflight returns the message with a false probe (factor a small fn taking the probe if needed).
7. **Completions.** `completions/**` and its dynamic tests: no `--backend` suggestions for init/run;
   `ask --backend` completion stays.
8. **Daemon.** `daemon/server/launch.rs`: delete the `detect_terminal()` call (~93) and the
   `terminal` parameter and `LOOM_TERMINAL` env of `daemon_command` (~118-145); new signature
   `daemon_command(exe, work_root, config, env)`. `environment.rs:79`: `LOOM_CONTROL_ALLOWLIST` keeps
   only `LOOM_HOOKS_DIR`; update its test (~154,178) to assert `LOOM_TERMINAL` is NOT captured. Fix
   `launch/tests.rs` (~200). Leave `TERM`, `TERMINFO*` and notification-related variables alone.
9. **Dashboard config API.** `commands/status/web/config_api*`: `GET /api/config` follows the
   registry, so the key disappears; a POST naming it gets the existing invalid-key 400 with no write.
   Rewrite tests that used `terminal.backend` as the generic enum fixture (`tests.rs`,
   `tests/resolution.rs`, `tests/typed_values.rs`, `tests/updates.rs`, `request_lifecycle_tests.rs`,
   `tests/config_api.rs`) onto `plan.summary_backend`, keeping validation, per-scope update and reset
   coverage. Add one test that a POST for `terminal.backend` returns 400 and leaves the files unchanged.
10. `tests/e2e/daemon_config/**`: remove the `LOOM_TERMINAL` guard in `stale_project_execution.rs`
    (~42) and the `read_terminal_config` comment in `mod.rs` (~51).

## Rules

- No hidden flags, no inert-key readers, no retirement notices: clean removal.
- Do not write test assertions containing the literals `--backend native`, `--backend tmux` or
  `terminal.backend` outside the POST-400 test; the stage greps for them (the 400 test lives in
  `config_api` tests; name the key via a `const` built with `concat!("terminal", ".backend")`).
- Files under 400 lines, functions under 50. Do not run the full suite.
- Report: deleted items, changed signatures, tests deleted or retargeted, edits needed outside your
  territory, and mistakes or surprises for `loom memory`.
