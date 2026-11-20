# E1: agent environment, auth probe, startup refusal, Remote Control, run guidance

Tier: sonnet. Read `doc/plans/briefs/open-issues-19-24/common.md` first (worker rules and pinned
interfaces); this brief adds only E1's part. Plan Decisions 8 (#19) and 12's `loom run` bullet (#20 C).
Line numbers below were read at `ff3fe947` and re-checked at `11859505` (PR #25, which this stage's
worktree includes: five commits on `ff3fe947`). `platform-portability` has merged before you start and
re-shaped `commands/run/mod.rs` (it deletes the PR's `objc_fork_safety` module and its call in
`execute_background`, and adds the daemon re-exec; the socket-path refusal sits in
`prepare_background_run`, right after `work_dir.load()?`, never in `run_startup_preflights`) and
`daemon/server/environment.rs` (adds `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE`, `RUST_LOG`). Anchor on the
symbols named here, never on the numbers.

## Files owned (write only these; all under `loom/`)

`src/process/environment.rs`, `src/process/mod.rs`,
`src/orchestrator/terminal/native/wrapper/script_text.rs`,
`src/orchestrator/terminal/native/wrapper.rs`,
`src/orchestrator/terminal/native/wrapper/tests_exec_env.rs` (new),
`src/claude.rs`, `src/claude/auth.rs` (new), `src/claude/auth_tests.rs` (new),
`src/commands/run/mod.rs`, `src/commands/run/auth_preflight.rs` (new),
`src/commands/run/guidance.rs` (new), `src/remote_control.rs`, `src/remote_control_tests.rs` (new),
`src/quota/credentials.rs`, `src/daemon/server/environment.rs`.
`src/orchestrator/terminal/native/mod.rs` is 397 lines: do not touch it. PR #25's test
`env_allowlist_forwards_user_and_logname` (`wrapper/tests.rs`, not yours) renders `env_allowlist()` under
`bash -c` and checks `USER`/`LOGNAME` are forwarded and `AWS_SECRET_ACCESS_KEY` is not: it keeps passing
with the generated list; leave the file unchanged. Read-only: `src/process/mod.rs`
`run_bounded` (:115) and `BoundedOutput` (:60); `src/claude/session.rs:87` `spawn_retrying_text_busy`.

## Pinned interfaces

You PROVIDE (quoted from common.md, "Pinned interfaces: session-auth-and-stalls"):

- "**Environment (E1, `loom/src/process/environment.rs`, re-exported from `loom/src/process/mod.rs`).**
  `pub const AGENT_SESSION_ENV_NAMES: &[&str]` (the wrapper's forwarding names, `USER` and `LOGNAME`
  included; `HOME` and `PATH` are handled separately); `pub fn agent_session_environment_from<I, K, V>(source: I)
  -> Vec<(OsString, OsString)>`; `pub fn apply_stage_environment_from<I, K, V>(command: &mut Command, source: I)`
  (today private; made pub). `STAGE_HOST_ENV_ALLOWLIST` gains `USER` and `LOGNAME`."
- "**Auth probe (E1, `loom/src/claude/auth.rs`, declared `pub mod auth;` in `loom/src/claude.rs`).**
  `#[derive(Debug, Clone, PartialEq, Eq)] pub enum AuthProbe { LoggedIn { method: String }, NotLoggedIn,
  Unknown(String) }`; `pub fn parse_auth_status(stdout: &str, exit_success: bool) -> AuthProbe`;
  `pub fn stage_auth_status(claude_path: &Path) -> AuthProbe` (runs `<claude_path> auth status --json` with
  `env_clear()` plus `agent_session_environment_from(std::env::vars_os())`, bounded at 30 s through
  `crate::process` bounded-run helpers; never logs `email`, `orgId`, `orgName`)."

`stage_auth_status` is a thin wrapper over a crate-private `stage_auth_status_from<I, K, V>(claude_path:
&Path, source: I) -> AuthProbe` (same bounds as `agent_session_environment_from`) that takes the
environment source; the wrapper passes `std::env::vars_os()`. Tests pass an explicit source and never
mutate a process variable.

E2 calls `stage_auth_status` from the daemon; the frozen contract file
`tests/session_auth_and_stalls_contracts.rs` calls `agent_session_environment_from`,
`apply_stage_environment_from`, `parse_auth_status`. You CONSUME nothing from E2 or E3.
You also add one unpinned helper, `pub fn operator_auth_status(claude_path: &Path) -> AuthProbe`, in
`claude/auth.rs` (same runner, inherited environment) for `auth_preflight`.

## Root cause (re-verified at 11859505, PR #25)

`USER`/`LOGNAME` are dropped at the host layer, so the claude CLI cannot find its macOS Keychain login by
`$USER`. PR #25 fixed only the wrapper's `env -i` list (item 2, now done in the literal); the host layer
(item 1) is E1's:

1. `STAGE_HOST_ENV_ALLOWLIST` (`process/environment.rs:14-77`) has neither name; `apply_stage_environment`
   (`:80`) `env_clear()`s and copies only that list. Callers: `native/spawner.rs:61`, `tmux/mod.rs:123,140`
   (every per-session tmux server, so `new-session` starts with a stripped environment), `verify/criteria/confine.rs:148`.
2. The wrapper's `env -i` list, `script_text.rs` `env_allowlist()` (the `for _loom_name in ...` loop), now
   carries `USER LOGNAME` (PR #25, with a doc paragraph on the Keychain reason). It is still a literal that
   can drift from the host list. On the spawner and tmux paths item 1 strips both names before the wrapper
   runs, so the wrapper's loop finds them unset: the PR's wrapper change alone does not fix those paths.

Loom's own Keychain probe (`remote_control.rs` `macos_keychain_has_credentials`, `:221`) runs in the daemon
environment, which keeps `USER` (`daemon/server/environment.rs:8-9`), so it passes while sessions fail.

## Tasks

### 1. `process/environment.rs`, `process/mod.rs`

- Add `pub const AGENT_SESSION_ENV_NAMES: &[&str]` with the names in the wrapper loop at `ff3fe947`, in
  this order, then `"USER"`, `"LOGNAME"` (the PR's literal places them after `SHELL`; the generated list
  puts them last, which the PR's test does not care about): `LANG LC_ALL LC_CTYPE TERM TERMINFO TERMINFO_DIRS COLORTERM
  TERM_PROGRAM SHELL DISPLAY WAYLAND_DISPLAY XAUTHORITY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR TMUX_TMPDIR
  TMUX TMUX_PANE TMPDIR SCCACHE_DIR SCCACHE_CACHE_SIZE`. Doc comment: the wrapper's `env -i` list is
  generated from it; `HOME` and `PATH` are handled separately; locations only, never credentials.
- Add `"USER"` and `"LOGNAME"` to `STAGE_HOST_ENV_ALLOWLIST` (after `"PATH"`, with a comment naming the
  Keychain lookup). The acceptance check `rg -q -F '"USER"' src/process/environment.rs` must pass.
- Make `apply_stage_environment_from` `pub`.
- `agent_session_environment_from` mirrors the bash loop exactly: first `("HOME", value or "")` (the wrapper
  writes `HOME=${HOME:-}`), then `("PATH", value if non-empty else "/usr/bin:/bin")`, then each
  `AGENT_SESSION_ENV_NAMES` entry present in `source` with a non-empty value, in `AGENT_SESSION_ENV_NAMES`
  order; drop everything else. Collect `source` into a map first (`K: Into<OsString>`).
- `process/mod.rs` (384 lines): extend the `pub use environment::...` line to export
  `agent_session_environment_from`, `apply_stage_environment`, `apply_stage_environment_from`,
  `AGENT_SESSION_ENV_NAMES`.

```rust
pub fn agent_session_environment_from<I, K, V>(source: I) -> Vec<(OsString, OsString)>
where I: IntoIterator<Item = (K, V)>, K: Into<OsString>, V: Into<OsString> { /* HOME, PATH, then names */ }
```

### 2. Wrapper list generated from the constant

`script_text.rs` `env_allowlist()` returns `String` (was `&'static str`): `format!` the same shell text with
the loop's name list built from `AGENT_SESSION_ENV_NAMES.join(" ")`, replacing the PR's literal list
(`USER LOGNAME` included). Keep the `_loom_env=( "HOME=..." "PATH=..." )` head, the `${!_loom_name}`
non-empty test and the doc comment, including the PR's USER/LOGNAME paragraph; rewrite its "Fully static — no
interpolation" sentence, which stops being true, and say the list is generated from the constant. The one
caller is `wrapper.rs` `build_wrapper_script` (`let env_allowlist = env_allowlist();`, interpolated as
`{env_allowlist}`): it compiles unchanged. The literal `AGENT_SESSION_ENV_NAMES` must appear in `script_text.rs` (wiring check).
Register the new test file at the bottom of `wrapper.rs` beside `mod tests;`:

```rust
#[cfg(test)]
mod tests_exec_env;
```

### 3. `claude/auth.rs` (+ `pub mod auth;` in `claude.rs`)

- `AuthProbe` as pinned. Private runner `run_auth_status(claude_path, env: Option<Vec<(OsString, OsString)>>)`:
  `Command::new(claude_path).args(["auth", "status", "--json"]).stdin(Stdio::null())`; for `stage_auth_status`
  `env_clear()` then `.envs(agent_session_environment_from(std::env::vars_os()))`; for
  `operator_auth_status` leave the environment inherited. Run through
  `crate::process::run_bounded(&mut cmd, Duration::from_secs(30))` (it sets piped stdout/stderr): `Completed`
  gives `parse_auth_status(stdout, status.success())`; `TimedOut` gives `Unknown("timed out after 30s")`; a spawn
  error gives `Unknown("could not run claude: <io error kind>")`.
- `parse_auth_status`: parse `serde_json::Value`; read ONLY `loggedIn` (bool) and `authMethod` (string, default
  `"unknown"`). `loggedIn: false` gives `NotLoggedIn` whatever `exit_success` is; `true` gives
  `LoggedIn { method }`; unparseable output or a missing `loggedIn` gives `Unknown` whatever `exit_success`
  is (a non-zero exit with unparseable stdout is `Unknown`, never `NotLoggedIn`).
- `stage_auth_status_from(claude_path, source)` holds the runner call for the stage probe:
  `run_auth_status(claude_path, Some(agent_session_environment_from(source)))`.
  `stage_auth_status(claude_path)` is `stage_auth_status_from(claude_path, std::env::vars_os())`.
- Unknown reasons are fixed strings or `serde_json::Error` display (carries line and column only). Never
  interpolate stdout, stderr or any JSON value other than `authMethod`: `email`, `orgId`, `orgName` must not
  reach a `Debug`, log line or error.
- Declare tests: `#[cfg(test)] #[path = "auth_tests.rs"] mod tests;` at the end of `auth.rs`.

### 4. `commands/run/auth_preflight.rs`, `guidance.rs`, `run/mod.rs`

```rust
pub(super) fn auth_problem(stage: &AuthProbe, operator: &AuthProbe) -> Option<String>;
pub(super) fn require_login_with(stage: AuthProbe, operator: impl FnOnce() -> AuthProbe) -> anyhow::Result<()>;
pub(super) fn require_stage_login(claude_path: &Path) -> anyhow::Result<()>;
```

- `auth_problem`: `None` unless `stage == NotLoggedIn`. Stage `NotLoggedIn` and operator `LoggedIn`: a message
  that the claude CLI is logged in for the operator's shell but not under the stage environment (the variables
  sessions receive are `loom::process::AGENT_SESSION_ENV_NAMES` plus `HOME` and `PATH`), so stage sessions would
  start "Not logged in". Stage `NotLoggedIn` with operator `NotLoggedIn` or `Unknown`: `claude is not logged in;
  run claude /login, then loom run again`.
- `require_login_with(stage, operator)` holds the decision, so a test drives it with explicit probes:
  `LoggedIn` is `Ok`; `Unknown(reason)` prints one `eprintln!` warning (`could not verify the claude login
  for stage sessions: {reason}`) and is `Ok`; `NotLoggedIn` calls the `operator` closure (only then) and
  `bail!`s with `auth_problem(...)`.
- `require_stage_login(claude_path)` is its production wrapper:
  `require_login_with(crate::remote_control::cached_stage_auth(claude_path).clone(), ||
  operator_auth_status(claude_path))`. It reads the probe through remote_control's memoized
  `cached_stage_auth` (Task 5; make that fn `pub(crate)`), which seeds its `OnceLock` here, so
  `loom run` runs the stage probe once and the Remote Control preflight after it reuses the result.
- In `run_startup_preflights`, inside the existing `if let Ok(claude_path) = crate::claude::find_claude_path()`
  block, call `auth_preflight::require_stage_login(&claude_path)?;` BEFORE
  `crate::remote_control::run_startup_preflight(...)`. A missing claude binary skips it. Add `mod auth_preflight;`
  and `mod guidance;`. Update the function's doc comment to name the new refusal.
- `guidance.rs`: `pub(super) fn write_follow_guidance(out: &mut impl std::io::Write, work_dir: &Path) ->
  std::io::Result<()>` writes three lines in the style of the existing `println!("  {}  Monitor progress", ...)`
  (colored command, two spaces, description): `loom status --live`, `loom status --web`, and the absolute
  `work_dir.join("orchestrator.log")` as the daemon log. In `execute_background` replace the two
  `println!("  {}  Check status" / "Monitor progress", "loom status".cyan())` lines (already-running path and
  fresh-start path) with `guidance::write_follow_guidance(&mut std::io::stdout(), work_dir.root())?;`. Delete PR #25's
  `fn print_log_location(work_dir: &WorkDir)` (it prints `Failures and stalls are logged to <abs log path>`)
  and its call just before the fresh-start `Monitor progress` line: `write_follow_guidance` prints the same
  absolute path. Keep `print_stop_guidance()`. Both flags exist (`cli/types.rs` `Status { live, .. }`, `cli/types_status_web.rs`).

### 5. `remote_control.rs` (598 lines) and `quota/credentials.rs`

- Delete `DISQUALIFYING_ENV_VARS`, `keychain_probe_argv`, `macos_keychain_has_credentials`, and the
  credentials-file and env heuristics inside `remote_control_eligible`. `rg -q -F macos_keychain_has_credentials src` must find nothing.
- New `fn eligibility_from(probe: &AuthProbe) -> Result<()>`: `Ok` only for `LoggedIn` with method `"claude.ai"`;
  `LoggedIn` with another method: `claude is logged in with <method>, but Remote Control requires claude.ai login`;
  `NotLoggedIn`: `claude is not logged in under the stage environment`; `Unknown(r)`: `could not determine the
  claude login (<r>)`.
- `remote_control_eligible(claude_path: &Path) -> Result<()>` gains the path parameter (its only caller is
  `preflight`, which has it) and returns `eligibility_from(cached_stage_auth(claude_path))`, where
  `cached_stage_auth` is a `static OnceLock<AuthProbe>` memoizing `crate::claude::auth::stage_auth_status`
  (copy `cached_named_arg_supported`'s shape; `pub(crate)`, because `auth_preflight::require_stage_login`
  reads the probe through it). The literal `stage_auth_status(` must appear in `remote_control.rs`.
- Rewrite the module doc: its `preflight()` bullet says "a version probe with an auth-eligibility
  heuristic" and the `preflight` fn doc says the same. After this change there is no heuristic: eligibility
  is the verdict of `claude auth status --json` under the stage environment (`eligibility_from`). Say that,
  and drop the claim that the setup is checked by anything else.
- Move the inline `mod tests` (from `#[cfg(test)] mod tests {`, ~`:396` to the end) into `src/remote_control_tests.rs`,
  dedenting one level (module body becomes the file body, `use super::*;` and `use serial_test::serial;` stay)
  and replace it with `#[cfg(test)] #[path = "remote_control_tests.rs"] mod tests;`. Keep every assertion line
  verbatim. DELETE only these two tests, because their subjects are gone, and list both in your report so the
  orchestrator disputes the resulting test-integrity event: `keychain_probe_argv_is_exact` and
  `eligible_rejects_disqualifying_env_var`. Add tests for `eligibility_from`: `claude.ai` is `Ok`; `console`,
  `NotLoggedIn` and `Unknown` are `Err`, and the `Err` text never contains an `email`-shaped value.
  `remote_control.rs` must end under 400 lines.
- `quota/credentials.rs:21`: rewrite the doc sentence naming `remote_control::keychain_probe_argv` so it no
  longer references the deleted function (keep the `-w` rationale).

### 6. `daemon/server/environment.rs`

Add one test in its inline `tests` module: every name in `crate::process::AGENT_SESSION_ENV_NAMES` passes the
predicate `capture_from` filters with (`is_allowed(OsStr::new(name))` at `ff3fe947`; if
`platform-portability` renamed it, use whatever `capture_from` calls). It passes because that stage added `SCCACHE_DIR` and `SCCACHE_CACHE_SIZE`; do not edit the allowlist yourself.

## Tests to write (exact paths)

- `process::environment::tests` (inline, append): `agent_session_environment_keeps_user_logname_home_and_path`,
  `agent_session_environment_defaults_path_and_skips_empty_values`,
  `stage_host_layer_keeps_user_and_logname` (Command `/usr/bin/env` as the existing tests do; canary
  `GITHUB_TOKEN` absent).
- `orchestrator::terminal::native::wrapper::tests_exec_env::wrapper_exec_environment_keeps_user_and_drops_secrets`
  (exact path; acceptance runs it `--exact`). Use `super::*`, `create_wrapper_script(work, "loom-env-test",
  "env-stage", "session-env-1", "env", None, SessionType::Stage, 100_000)` (as `wrapper/tests.rs` does),
  then run the written script with `Command::new("bash").arg(path).env_clear()` plus `HOME=<tempdir>`,
  `PATH=/usr/bin:/bin`, `USER=alice`, `LOGNAME=alice`, `GITHUB_TOKEN=canary`. Run it through `bash <path>`, not by
  exec of the script, so ETXTBSY cannot occur. Assert stdout has lines `USER=alice` and `LOGNAME=alice`, and
  contains neither `GITHUB_TOKEN` nor `canary`.
- `claude::auth::tests` in `claude/auth_tests.rs`: `probe_output_never_carries_identity` (exact name; parse the
  logged-in shape with `email`, `orgId`, `orgName` and assert the `Debug` string of the probe contains none of
  those values; also feed malformed JSON containing the email and assert the `Unknown` text does not),
  `logged_in_status_parses_to_logged_in` (keys `analyticsDisabled, apiProvider, authMethod, configDirectory,
  email, loggedIn, orgId, orgName, projectsDirectory, subscriptionType`; `loggedIn: true`, `authMethod:
  "claude.ai"`), `logged_out_status_is_not_logged_in_whatever_the_exit` (the captured JSON
  `{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty", "analyticsDisabled": false,
  "projectsDirectory": "/home/u/.claude/projects", "configDirectory": "/home/u/.claude"}` with `exit_success`
  true and false), `unparseable_output_is_unknown` (exact name; asserts `Unknown` for the same unparseable stdout with
  `exit_success` true AND with it false), and `stage_probe_sees_user` (no `#[serial]`, no process variable
  touched): write a fake claude script (`#!/bin/sh`, prints `{"loggedIn": true, "authMethod": "claude.ai"}` and
  exits 0 only when `$USER` is non-empty, else prints the logged-out JSON and exits 1) into a `TempDir`,
  `chmod 0o755`, call `stage_auth_status_from(&script, [("HOME", <tempdir>), ("PATH", "/usr/bin:/bin"),
  ("USER", "alice")])` and assert `LoggedIn`, then call it again with the same source minus `USER` and assert
  `NotLoggedIn`. Never run the real `claude`.
- `commands::run::auth_preflight::tests` (inline): `auth_problem` for every pair (stage `LoggedIn` and `Unknown`
  with any operator: `None`; stage `NotLoggedIn` with each of the three operator verdicts: `Some`, with the
  `LoggedIn` case naming the stage environment and the other two naming `claude /login`);
  `not_logged_in_refuses_the_run` (exact name; acceptance runs it `--exact`: `require_login_with(
  AuthProbe::NotLoggedIn, || AuthProbe::NotLoggedIn)` is an `Err` whose text names `claude /login`);
  `require_stage_login_warns_and_passes_when_claude_cannot_run` (path `/nonexistent/claude-xyz` gives `Ok`).
- `commands::run::guidance::tests` (inline): `follow_guidance_names_status_live_status_web_and_the_log` (a
  `Vec<u8>` writer, a `TempDir` work dir; contains `loom status --live`, `loom status --web`, `<dir>/orchestrator.log`).
- `remote_control::tests` (moved file): the `eligibility_from` tests above.

## Patterns to copy

- `process/environment.rs:109-135` tests (spawn `/usr/bin/env`, read stdout) for the env tests.
- `remote_control.rs` `cached_named_arg_supported` (OnceLock memo) for `cached_stage_auth`.
- `commands/run/git_preflight.rs` for a refusing preflight module with an inline `tests`.
- DO NOT copy: the old `remote_control_eligible` reading `std::env::var_os`, `~/.claude/.credentials.json` or
  `security`; they answer for the daemon's environment, which is the bug.

## Traps (knowledge, quoted)

- Fake-claude scripts: "the classic Linux ETXTBSY fork/exec race ... a test that passes alone and fails only under
  `--all-targets`, with error code 26 naming the just-written executable, is this race"
  (`mistakes/test-concurrency-and-fixtures.md`). `#[serial]` does not stop non-serial tests forking. In
  `stage_probe_sees_user` retry the call up to 50 times with a 10 ms sleep while the result is `Unknown(_)`, then assert.
- "every external command issued from the poll loop goes through `process::run_bounded`"
  (`mistakes/sessions-and-liveness.md`): E2 calls your `stage_auth_status` from the orchestrator loop, so keep
  the 30 s bound and never use `Command::output()`.
- Tests never touch the real `HOME`, the live `.loom/work`, or the real `claude` (`mistakes/live-state-pollution.md`).
- `STAGE_HOST_ENV_ALLOWLIST` also governs plan-authored confined criteria (`verify/criteria/confine.rs:148`);
  `USER`/`LOGNAME` are not secrets, so that is intended. Do NOT add `ANTHROPIC_API_KEY`, `CLAUDE_CONFIG_DIR` or proxy variables to `AGENT_SESSION_ENV_NAMES` (plan non-goal).
- `commands/run/tests/confinement.rs` drives `run_startup_preflights` and expects the confinement refusal first;
  keep `require_confinement` the first call. Do not move or touch the socket-path refusal
  `prepare_background_run` carries (W1's); only `run_startup_preflights` and `execute_background` are yours.
- Never print `email`, `orgId` or `orgName`; do not add a `Serialize` derive to `AuthProbe`.

## The one check you may run (once)

`cd loom && cargo test --lib claude::auth::tests:: 2>&1 | tail -30` (the crate may not compile mid-wave: if it
fails on another worker's symbol, say so and stop). No `cargo fmt`, no clippy.

## Report

Files changed; the one check and its result; the two deleted tests (names) for the integrity dispute;
ledgered units you shrank (`remote_control.rs` should leave the ledger); that `print_log_location` is gone; every deviation from the pinned
interfaces (the extra `operator_auth_status`, `stage_auth_status_from`, `require_login_with`, the
`remote_control_eligible(claude_path)` parameter, `cached_stage_auth` made `pub(crate)`); surprises.
