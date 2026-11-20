# W1: Shared foreground Claude session driver

Plan: `doc/plans/PLAN-knowledge-bootstrap-command.md`. Worktree root is the repo root; the crate is
`loom/`. Run cargo from `loom/`.

## Goal

Move the interactive foreground-session driver out of `loom pressure` into a crate-level module so
`loom knowledge bootstrap` (built by W3 in wave 2) can reuse it. Pressure's behaviour must stay
the same: same argv, same env, same teardown. The one settled change is marker precedence (below).

## Files you own (exclusive)

- `loom/src/claude.rs` (add one `mod` line and one `pub(crate) use` line)
- `loom/src/claude/session.rs` (new)
- `loom/src/commands/pressure/spawn.rs`
- `loom/src/commands/pressure/paths.rs`
- `loom/src/commands/pressure/mod.rs` (import fixes only)
- `loom/src/commands/pressure/tests.rs` (import fixes plus the two test moves named below)

Read-only: `loom/src/lib.rs` (`pub mod claude;` is at line 5; do not change it).

## What moves

From `loom/src/commands/pressure/spawn.rs`:

- `ExitAction` (lines 14-23), `ClaudeOutcome` (25-34), `AGENT_TEAMS_ENV` (37),
  `POLL_INTERVAL_MS` (40), `TERM_GRACE_MS` (42), `classify_exit` (101-103),
  `classify_code` (106-113), `send_sigterm` (116-120), `terminate_idle_session` (180-196),
  and the body of `run_claude_foreground` (135-173).

From `loom/src/commands/pressure/paths.rs`:

- `ensure_marker_dir` (lines 131-139). `delete_file` (142-148) STAYS in `paths.rs`, because
  `pressure/mod.rs:260` still uses it for non-marker files. Give `session.rs` its own private
  `remove_if_exists(path) -> Result<()>` with the same body. That duplicates 6 lines; the
  alternative is a cross-module dependency from `claude` onto `commands::pressure`, which is worse.

## New API in `loom/src/claude/session.rs`

```rust
pub(crate) enum ExitAction { Continue, Abort, Warn }          // unchanged doc comments
pub(crate) enum ClaudeOutcome { Completed, Exited(ExitStatus) } // unchanged doc comments
pub(crate) const AGENT_TEAMS_ENV: &str = "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS";
pub(crate) fn classify_exit(status: ExitStatus) -> ExitAction;
fn classify_code(code: Option<i32>) -> ExitAction;   // private: only classify_exit calls it
fn send_sigterm(pid: u32);                          // private: only terminate_idle_session calls it
fn spawn_retrying_text_busy(command: &mut Command) -> std::io::Result<Child>; // private

/// Spawn `claude_path` with `args` in the foreground (inherited stdin/stdout/stderr,
/// `AGENT_TEAMS_ENV=1`, cwd = `cwd`), clear any stale `marker` first (creating its
/// parent dir), and return `Completed` once `marker` appears (after SIGTERM then
/// SIGKILL teardown). If the child has already exited, return `Completed` when the
/// marker exists and `Exited(status)` otherwise. Deletes the marker before returning.
pub(crate) fn run_foreground(
    claude_path: &Path,
    cwd: &Path,
    args: &[String],
    marker: &Path,
) -> Result<ClaudeOutcome>;
```

`POLL_INTERVAL_MS`, `TERM_GRACE_MS`, `terminate_idle_session`, `ensure_marker_dir`,
`remove_if_exists`, `classify_code`, `send_sigterm` and `spawn_retrying_text_busy` stay private
to `session.rs`. After the move `send_sigterm`'s only caller is `terminate_idle_session`
(spawn.rs:181) and `classify_code`'s only non-test caller is `classify_exit` (spawn.rs:102).
Re-exporting either would fail `cargo clippy --all-targets -- -D warnings` with an unused import.

`run_foreground` spawns through a private `spawn_retrying_text_busy(&mut Command)` in
`session.rs`, a copy of `loom/src/quota/codex.rs:172-186` (5 attempts, 20 ms sleep on
`raw_os_error() == Some(libc::ETXTBSY)`; `libc` is already a dependency). Today
`run_claude_foreground` spawns once (spawn.rs:156). The retry is the settled fix for the
fork/exec text-busy race (`mistakes/test-concurrency-and-fixtures.md:43-45`) and stays in
production code, because W3's integration test execs a freshly written fake `claude` script
through this driver.

**Marker precedence (settled, plan "Marker precedence" row).** Today the loop checks
`child.try_wait()` first (spawn.rs:158-162) and returns `Exited` even when the marker is already
on disk, then deletes it (:171). A child that runs `touch marker; exit 0` within one poll interval
therefore loses its completion. In `run_foreground`, when `try_wait` returns `Some(status)`,
break with `Completed` if `marker.exists()`, else `Exited(status)`; no teardown is needed because
the child is gone. The marker wins over any exit code, because touching it is the session's final
action. Keep the stale-marker clear before the spawn unchanged.

In `loom/src/claude.rs`, below the imports, add:

```rust
mod session;
pub(crate) use session::{classify_exit, run_foreground, ClaudeOutcome, ExitAction, AGENT_TEAMS_ENV};
```

`src/claude.rs` plus `src/claude/session.rs` is valid Rust 2018+ module layout. Do NOT
rename `claude.rs` to `claude/mod.rs`.

Update the `claude.rs` module doc (line 1) to: `//! Shared Claude binary resolution and
foreground-session driving.`

## Pressure after the move

- `spawn.rs` keeps `completion_instruction`, `claude_args`, `codex_args`, `print_log_tail`,
  `spawn_codex_background`, `wait_codex`, `should_stop`, `claude_should_stop`, `TAIL_BYTES`.
- `spawn.rs` keeps `pub(super) fn run_claude_foreground(claude_path, repo_root, slash, marker,
  model, effort)` with the SAME signature as a thin wrapper:

  ```rust
  let args = claude_args(slash, marker, model, effort);
  crate::claude::run_foreground(claude_path, repo_root, &args, marker)
  ```

  Write the call fully qualified as `crate::claude::run_foreground(`. A plan `wiring` check
  greps for exactly that text in `spawn.rs`.
- `spawn.rs` imports `ClaudeOutcome`, `ExitAction` and `classify_exit` from `crate::claude`
  where it still uses them (`should_stop`, `claude_should_stop`). Re-export `AGENT_TEAMS_ENV` from `spawn.rs` as
  `pub(super) use crate::claude::AGENT_TEAMS_ENV;` so `pressure/mod.rs:51,149,159` compiles
  unchanged, or fix those imports; pick one and keep it minimal.
- Remove every item from pressure that is now unused; `cargo clippy --all-targets -- -D warnings`
  must be clean with no `#[allow(dead_code)]` added.
- Move `test_ensure_marker_dir_creates_parent_and_is_idempotent` (pressure/tests.rs:136-152) and
  `test_classify_code_all_arms` (:274-282) into `session.rs`'s `#[cfg(test)] mod tests`
  unchanged except for the call path. They call `ensure_marker_dir` and `classify_code`, which
  become private to `session.rs`. Moving is not deleting; these are the only two permitted test
  moves. `pressure/tests.rs` otherwise takes import fixes only. Do not delete or weaken any
  other test.

Find every consumer before editing:

```bash
rg -n 'ExitAction|ClaudeOutcome|AGENT_TEAMS_ENV|classify_code|classify_exit|send_sigterm|ensure_marker_dir|run_claude_foreground|POLL_INTERVAL_MS|TERM_GRACE_MS' loom/src
```

## New tests (in `session.rs`, `#[cfg(test)] mod tests`)

The module path must be `claude::session::tests`, because acceptance runs
`cargo test --lib claude::session::`.

1. `run_foreground_completes_when_marker_appears`: pass `claude_path = /bin/sh`, with args
   `["-c", "touch \"$0\"; exec sleep 30", <marker path>]`. Expect `ClaudeOutcome::Completed`,
   the marker deleted afterwards, and the whole call well under 30 s. `exec sleep` makes SIGTERM
   kill the real process, so nothing outlives the test (see `mistakes/detached-spawn-in-tests.md`).
2. `run_foreground_reports_self_exit`: `/bin/sh -c "exit 3"` gives `Exited(status)` with
   `status.code() == Some(3)`.
3. `run_foreground_clears_stale_marker`: pre-create the marker, run `/bin/sh -c "exit 0"`, and
   expect `Exited` (not `Completed`), because the stale marker was cleared before the spawn.
4. The two moved pressure tests (see "Pressure after the move"):
   `test_ensure_marker_dir_creates_parent_and_is_idempotent` and `test_classify_code_all_arms`,
   unchanged except for the call path.
5. `run_foreground_sets_cwd_and_agent_teams_env`: `/bin/sh` with args
   `["-c", "pwd > \"$0.env\"; printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS >> \"$0.env\"; touch \"$0\"; exec sleep 30", <marker path>]`
   and a `TempDir` as `cwd`. Expect `Completed`, and assert `<marker>.env` holds the cwd on its
   first line and `1` on its second. Pressure's tests pin only `claude_args`
   (pressure/tests.rs:240-257); no test runs the spawn path, so these `session.rs` tests are the
   only coverage for spawn, cwd, env, marker and teardown.
6. `run_foreground_marker_then_exit_is_completed`: `/bin/sh` with args
   `["-c", "touch \"$0\"; exit 0", <marker path>]` gives `Completed`, and the marker is deleted.
7. `run_foreground_marker_then_nonzero_exit_is_completed`: the same with `exit 3` gives
   `Completed`.

Use `tempfile::TempDir` for the marker directory (it honours `TMPDIR`; never hard-code `/tmp`).
Running `/bin/sh` directly avoids writing an executable script, which avoids ETXTBSY races
(`mistakes/test-concurrency-and-fixtures.md`).

## Requirements the main agent verifies

`cargo build`, `cargo test --lib`, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` are clean; every file stays under 400 lines and every function under 50.

## Proof (run from `loom/`, ONE command)

```bash
cargo test --lib claude::session::
```

If a compile error is in a file you do not own, report it; do not edit it. The main agent runs
the full gate (build, clippy, fmt, tests) after each wave.

Report the exact public API you ended with, and any deviation from this brief with the reason.
Do not commit and do not run the full suite.
