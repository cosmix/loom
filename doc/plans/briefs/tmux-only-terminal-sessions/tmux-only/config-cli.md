# Brief: config-cli (tmux-only stage)

You remove every way to select a terminal backend: the `[terminal]` section, the `terminal.backend`
key, the `--backend` flag on `loom init` / `loom run`, the init prompt, the run-path backend
resolution, the daemon's terminal forwarding, and the key's dashboard Settings entry. Read the
PUBLIC SURFACE block of the `tmux-only` stage description in
`doc/plans/IN_PROGRESS-PLAN-tmux-only-terminal-sessions.md` (the name `loom run` gives the plan in
stage worktrees) first; other workers code against it in parallel.

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
   `terminal_backend_set()` and the doc references to them (the doc of `context_ceiling_tokens_set`
   points at `terminal_backend_set`; repoint it to `context_ceiling_tokens_set` itself). Intra-doc
   links to `read_terminal_config` in `user_config/mod.rs` and `user_config/workspace.rs` go too
   (rustdoc gate). `parse.rs`: delete `get_backend` and the `terminal_backend:` line in
   `parse_document`. `keys.rs`: delete the `terminal.backend` KeySpec. `render.rs`: drop
   `terminal.backend` from the value table and the `[terminal]` block from the TOML printer, so
   `--print` has no empty terminal group. `workspace.rs`: drop the `terminal.backend` mapping and doc
   mentions. `value.rs` and user_config tests: remove terminal cases; where a test used
   `terminal.backend` as its example enum key, switch to another real enum key (`plan.summary_backend`:
   `claude`/`codex`) so the same behaviour stays covered. A leftover `[terminal]` table must parse and
   be ignored: `parse_document` reads per key, so deleting the reader is enough; add no code for it.
2. **Project config.** Delete `fs/work_dir/config_sections/terminal_config.rs` and its `tests.rs`, the
   `mod` line and re-exports (`read_terminal_config`, `write_terminal_config`) in `config_sections.rs`
   and `fs/work_dir.rs`, and the terminal tests in `fs/work_dir/tests.rs`
   (`write_then_read_terminal_config_round_trip`, `read_terminal_config_defaults_when_section_missing`,
   `global_config_tier_terminal_backend_falls_through_to_user_config`, and the test whose comment says
   it "must fall through to the user config's terminal.backend"). Reword the doc comments naming
   `read_terminal_config` in `fs/work_dir/config_sections/models_config.rs` and `config_sections.rs`.
   Keep `stale_project_execution_section_is_harmless`.
3. **`loom config`.** `commands/config/**` (TUI, `--list`, `--print`, `-k`, completion of keys): no
   terminal row or group. TUI tests using `terminal.backend` (cycling, quit, rendering, save, scope)
   switch to `plan.summary_backend` with the same assertions. Writing `terminal.backend` now fails as
   any unknown key does.
4. **CLI and init signatures.** `cli/types.rs` (init) and `cli/types_runtime.rs` (`RunArgs`): delete the
   `--backend` options. Pinned signatures: `init::execute(plan_path: Option<PathBuf>, clean: bool,
   no_repair: bool)`; `run::execute` and `run::execute_background` lose only their `backend`
   parameter; `initialize_with_plan` (`commands/init/plan_setup.rs`) loses its `terminal_backend`
   parameter. Update the callers: `cli/dispatch.rs`, `commands/init/execute.rs` (drop
   `resolve_backend_choice`), `commands/init/execute/tests.rs`, `commands/init/tests.rs`, and
   `commands/init/execute_contract_tests.rs` (an earlier plan's contract file, an ordinary test now:
   drop the argument only). Reword the help strings saying "tmux backend only" in `cli/types.rs`
   (attach) and `cli/types_status_web.rs`, and the help prose naming `[terminal]` in
   `cli/types_config.rs`. `loom ask --backend` is a different option: leave it.
5. **Init.** Delete `commands/init/backend.rs` and `backend_tests.rs` and their `mod` lines.
   `plan_setup.rs`: delete the `[terminal]` write and backend parameter. Add the non-fatal advisory
   `pub(crate) fn tmux_advisory(tmux_available: impl Fn() -> bool) -> Option<String>` there:
   `Some(format!("warning: {TMUX_MISSING_MESSAGE}"))` when `require_tmux(tmux_available)` is Err, else
   `None` (the plan's PUBLIC SURFACE pins this name). `initialize_with_plan` calls it with
   `|| which::which("tmux").is_ok()`, prints the result to stderr and continues, so init still exits
   0 (a `reachable` check walks from `initialize_with_plan` to `tmux_advisory`). Add a unit test for
   both probe values.
6. **Run.** `commands/run/mod.rs`: delete `resolve_backend_flag` and `resolve_backend_flag_with_probe`,
   the daemon-restart check for a backend change and the persistence. `TMUX_MISSING_MESSAGE` is
   already imported there for `resolve_backend_flag_with_probe`; after the deletion the preflight is a
   `require_tmux(|| which::which("tmux").is_ok())?` call (path `crate::orchestrator::terminal::require_tmux`).
   Background: inside `prepare_background_run`, which runs before `DaemonServer::start`. Foreground: in
   `run/foreground.rs::execute`, before the orchestrator is built; the literal text `require_tmux(`
   must appear in foreground.rs (a wiring check greps for it). `run/guidance.rs`: drop backend
   guidance. Keep a unit test that the preflight returns the message with a false probe (factor a
   small fn taking the probe if needed).
7. **Completions.** `completions/**` and its dynamic tests: no `--backend` suggestions for init/run;
   `ask --backend` completion stays.
8. **Daemon.** `daemon/server/launch.rs`: delete the `detect_terminal` import and call and the
   `terminal` parameter and `LOOM_TERMINAL` env of `daemon_command`; new signature
   `daemon_command(exe, work_root, config, env)`. `environment.rs`: `LOOM_CONTROL_ALLOWLIST` keeps only
   `LOOM_HOOKS_DIR`. Keep the literal `LOOM_TERMINAL` out of every non-contract file (a stage gate
   greps for it): in the `environment.rs` tests delete the `("LOOM_TERMINAL", "kitty")` fixture entry
   and its `contains` assertion; in `launch/tests.rs` delete the `LOOM_TERMINAL` assertion. The frozen
   contract `daemon_child_never_receives_a_terminal_hint` covers non-capture. Leave `TERM`,
   `TERMINFO*` and notification-related variables alone. If `HOST_ENV_ALLOWLIST` already lists
   `BUN_INSTALL_CACHE_DIR`, `npm_config_cache`, `UV_CACHE_DIR`, `GOCACHE`, `LOOM_BUILD_SEED` (another
   plan merged first), keep them.
9. **Dashboard config API.** `commands/status/web/config_api*`: `GET /api/config` follows the
   registry, so the key disappears; a POST naming it gets the existing invalid-key 400 with no write.
   Rewrite tests that used `terminal.backend` as the generic enum fixture (`tests.rs`,
   `tests/resolution.rs`, `tests/typed_values.rs`, `tests/updates.rs`, `request_lifecycle_tests.rs`,
   `tests/config_api.rs`) onto `plan.summary_backend`, keeping validation, per-scope update and reset
   coverage; rename a retargeted test whose name contains `terminal_backend` (for example
   `a_malformed_project_terminal_backend_fails_to_resolve`). Add one test that a POST for
   `terminal.backend` returns 400 and leaves the files unchanged.
10. `tests/e2e/daemon_config/**`: remove the `LOOM_TERMINAL` guard in `stale_project_execution.rs`, the
    `read_terminal_config` comment in `mod.rs`, and reword the header comment in
    `stale_project_execution.rs` that says "`loom init --backend native`".

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- No hidden flags, no inert-key readers, no retirement notices: clean removal.
- The stage greps for these literals outside the contract files: `--backend native`, `--backend tmux`,
  `terminal.backend` (the POST-400 test in `config_api` tests names the key via a `const` built with
  `concat!("terminal", ".backend")`), `LOOM_TERMINAL`, `terminal_backend` (also
  inside test function names), `resolve_backend_flag`, `write_terminal_config`, `read_terminal_config`.
  Ratatui's `terminal.backend_mut()` / `terminal.backend()` are not hits and stay.
- Keep the `mod contract_tests;` lines the contract session added in `cli/mod.rs`,
  `user_config/mod.rs` and `daemon/server/launch.rs`.
- The commit hook runs rustdoc with warnings denied: an intra-doc link (`` [`name`] ``) to a deleted or
  private item fails it. In doc comments name removed or private items in plain backticks.
- Report every ledger row (`loom/maintainability-baseline.txt`) whose file or function you shrank
  (rows for `src/fs/work_dir.rs`, `src/fs/work_dir/tests.rs` and `initialize_with_plan` shrink);
  compile-sweep lowers them.
- Files under 400 lines, functions under 50. Do not run the full suite.
- Report: deleted items, changed signatures, tests deleted or retargeted, edits needed outside your
  territory, and mistakes or surprises for `loom memory`.
