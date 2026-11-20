# W4: boot ID consumers, review-gate hint, checker regex, web 408 flake (issue #24, Rust side)

Read `doc/plans/briefs/open-issues-19-24/common.md` first. You are a sonnet worker of stage
`platform-portability`. Plan Decisions 6 and 7 are binding.

## Role and issue

Inside the macOS sandbox the `sysctl-read` allowlist lacks `kern.bootsessionuuid`, so
`SystemBootClock::boot_id` fails with EPERM and every `loom subagents watch` exits 1 (and with no
review round recorded, nothing tells the main agent why). You wire the boot-ID source W5 writes
into the wait lease and the session wrapper, make the review gate and `loom stage review status`
say how many reviewer spawns were never harvested, fix the knowledge checker's reading of
`AGENTS.md.template`, and diagnose the web 408 flake.

## Files owned (the plan's W4 row, package-relative to `loom/`)

`src/commands/subagents/wait/lease.rs`, `src/commands/subagents/wait/lease_boot_tests.rs` (new),
`src/commands/subagents/wait/mod.rs`, `src/orchestrator/terminal/native/wrapper/host_env.rs`,
`src/orchestrator/terminal/native/launch/host.rs`, `src/commands/stage/review_status.rs`,
`src/verify/review/gate.rs`, `src/verify/review/gate_tests.rs`,
`src/fs/knowledge/chunker/references.rs`, `src/commands/status/web/head.rs`,
`src/commands/status/web/tests/errors.rs`, `src/commands/status/web/mod.rs`,
`src/commands/status/web/connection.rs`, `src/commands/status/web/unserved.rs` (new),
`src/orchestrator/terminal/native/tests_wrapper_env.rs`, `src/completions/dynamic/commands.rs`.
Write nothing else.

## Pinned interfaces (common.md, quoted)

- "Boot ID (W5 writes `loom/src/process/boot_id.rs`; the orchestrator adds `pub mod boot_id;` to
  `loom/src/process/mod.rs` before the wave). `pub const BOOT_ID_ENV: &str = "LOOM_BOOT_ID";`
  `pub fn resolve_boot_id(env_value: Option<&str>, os_boot_id: impl FnOnce() ->
  anyhow::Result<String>) -> anyhow::Result<String>`; `pub fn os_boot_id() ->
  anyhow::Result<String>` (the bodies move here from `SystemBootClock::boot_id` in
  `loom/src/commands/subagents/wait/lease.rs`, which W4 then deletes); `pub fn current_boot_id() ->
  anyhow::Result<String>` = `resolve_boot_id(std::env::var(BOOT_ID_ENV).ok().as_deref(),
  os_boot_id)`. W4 renders `LOOM_BOOT_ID` into the session wrapper from
  `WrapperHostEnv::boot_id: Option<String>`, filled in `launch/host.rs` with
  `crate::process::boot_id::os_boot_id().ok()`."
- Skip ledger row written by W3, read by you: one JSON line per skipped `loom-code-reviewer` stop in
  `<work>/subagents/<stage>/stop-skips.jsonl`:
  `{"ts":"...","agent_id":"<id>","agent_type":"loom-code-reviewer","reason":"<code>"}`.
- Start rows (`starts.jsonl`, written by `subagent-start.sh`) carry `agent_id` and `agent_type`.

## Tasks

### A. Boot ID in the wait lease (`lease.rs`, `wait/mod.rs`, `lease_boot_tests.rs`)

1. `impl BootClock for SystemBootClock`: one `fn boot_id(&self) -> Result<String> {
   crate::process::boot_id::current_boot_id() }` for every OS. Delete the three cfg variants
   (`:39-58`) and, at your base (PR #25), every macOS boot function: `macos_boot_id` (now
   `macos_boot_session_uuid().or_else(|_| macos_boot_time())`), `macos_boot_time` (sysctl
   `kern.boottime`, formatted `boottime:<sec>.<usec>`) and `macos_boot_session_uuid` (the old body,
   `:101-170` with its doc comment), plus the whole `#[cfg(all(test, target_os = "macos"))] mod
   macos_boot_tests` at the end of the file (`:380-395`). Anchor by symbol. `rg -F
   'kern.bootsessionuuid' src/commands/subagents` and `rg -F 'kern.boottime' src` must find nothing
   afterwards, comments included. The `kern.boottime` fallback is not carried over: macOS recomputes
   it when the wall clock is stepped, so its value can change within one boot, and a lease written
   under one source and read under the other would look like a different boot. `LOOM_BOOT_ID` from
   the daemon removes the need for any fallback (W5's `os_boot_id` reads `kern.bootsessionuuid`
   only).
2. Imports: after the deletion `bail!` is used only by the `not(any(linux, macos))` functions, so on
   Linux the `bail` import warns and `-D warnings` fails. Drop `bail` from the `use anyhow::{...}`
   line and write `anyhow::bail!(...)` in those two functions. `Context` stays used
   (`clock_nanoseconds`, `deadline_after`). `lease_tests.rs` (not yours) was edited by PR #25
   (`assert_private_lease_directories` canonicalizes the temp dir for macOS `/var`); leave it.
3. `wait/mod.rs`: add `#[cfg(test)] mod lease_boot_tests;` beside `lease_tests`.
4. `lease_boot_tests.rs` (`use super::lease::{BootClock, SystemBootClock};` plus
   `serial_test::serial`; a local `EnvVarGuard` with `set` and `unset`, a copy of
   `src/completions/install/tests.rs:21-48`, the repo's convention of one private guard per test
   module; do not add a shared module):
   - `a_valid_session_boot_id_wins_over_the_os_source` (`#[serial]`): set `LOOM_BOOT_ID` to
     `0f8fad5b-d9cb-469f-a165-70867728950e`; `SystemBootClock.boot_id()` returns exactly that.
   - `a_malformed_session_boot_id_is_ignored` (`#[serial]`): set it to `not-a-uuid`; the result is
     either `Ok(v)` with `v != "not-a-uuid"` or an `Err` from the OS source, never the env value.
   - `an_unset_session_boot_id_uses_the_os_source` (`#[serial]`): unset; on Linux the result equals
     the trimmed content of `/proc/sys/kernel/random/boot_id`, elsewhere just call it.

### B. Wrapper export (`host_env.rs`, `launch/host.rs`)

1. `WrapperHostEnv` gains `pub boot_id: Option<String>` with a doc comment ("exported as
   `LOOM_BOOT_ID`"); `render` adds `("LOOM_BOOT_ID", self.boot_id.clone())` after `LOOM_HOOK_PATH`,
   skipping `None` and empty strings through the existing `filter_map` (add `.filter(|v| !v.is_empty())`
   only if needed). `launch/host.rs` `wrapper_env` sets
   `boot_id: crate::process::boot_id::os_boot_id().ok()`; the daemon reads it unsandboxed.
2. Tests in `host_env.rs` `mod tests` (new fns): `boot_id_renders_as_loom_boot_id` (a
   `WrapperHostEnv { boot_id: Some(..), ..Default::default() }` renders `LOOM_BOOT_ID=<value>` once)
   and `a_default_host_env_renders_no_boot_id`.
3. The test helper `full_host_env()` in `src/orchestrator/terminal/native/tests_wrapper_env.rs:53-59`
   builds `WrapperHostEnv` with all three fields and no `..Default::default()`, so adding the field
   breaks that file's compile. Add the one line `boot_id: None,` to that literal (it is in your
   row; no assertion line changes).

### C. Reviewer-spawn hint (`gate.rs`, `review_status.rs`, `gate_tests.rs`)

1. `gate.rs`: `pub fn harvest_hint(work_dir: &Path, stage_id: &str, rounds: usize) -> Option<String>`.
   Read `<work_dir>/subagents/<stage_id>/starts.jsonl`; spawns = distinct `agent_id` values of rows
   whose `agent_type` is exactly `loom-code-reviewer`. Return `None` unless spawns > rounds.
   `k = spawns - rounds`. Text, exactly:

   ```text
   Reviewer stop events: 3 reviewer spawns, 1 rounds, 2 stop events not harvested.
     skipped stop: agent <agent_id>: <reason>
   Run the review again with LOOM_HOOK_DEBUG=1 to see why the SubagentStop hook skipped a stop.
   ```

   The `skipped stop` lines come from `stop-skips.jsonl` (latest five, `agent_type` exactly
   `loom-code-reviewer`). Reading is bounded and defensive because a session can write these files:
   refuse a symlink or non-file (`symlink_metadata`), cap at 1 MiB, ignore unparsable lines, and
   render `reason` and `agent_id` keeping only `[A-Za-z0-9_.-]` characters, at most 64 each.
   Never fail: every IO problem yields no hint. Do not reuse `commands/subagents/ledger.rs`
   `json_lines` (private, and its module is not yours).
2. `gate::check`: after building the failure message with the unchanged `failure_message(...)`,
   append `"\n" + hint` when `harvest_hint(work_dir, &stage.id, rounds.len())` is `Some`. Do not
   change `failure_message`'s signature (`gate_tests.rs` calls the gate through `check`).
3. `review_status.rs`: after `print_rounds(&stage_id, &rounds);` print the hint (`println!`) when
    present, before the blank line. One call, no new function.
4. Tests (new fns at the end of `gate_tests.rs`, reuse its `fixture()`; write `starts.jsonl` and
    `stop-skips.jsonl` rows with `std::fs::write` under `fx.work_dir.join("subagents").join(STAGE)`):
    `harvest_hint_counts_spawns_without_rounds`, `harvest_hint_is_none_when_rounds_cover_spawns`,
    `harvest_hint_ignores_non_reviewer_spawns`, `harvest_hint_sanitizes_ledger_text` (a reason with
    spaces, newline and escape characters renders without them),
    `gate_failure_message_carries_the_harvest_hint` (no round recorded, two reviewer start rows, one
    skip row: the `check` error contains `2 reviewer spawns, 0 rounds, 2 stop events not harvested`
    and the skip reason).

### D. Knowledge checker (`references.rs`)

 1. In `references_in_line`, a `SOURCE_PATH_REGEX` match is skipped when the next character is an
    identifier character (existing rule) or is `.` followed by an identifier character. That makes
    `` `AGENTS.md.template` `` a non-reference to `AGENTS.md`, while a sentence-final `` `a.rs.` ``
    still counts. Keep the rule in one small helper `fn continues_name(rest: &str) -> bool`.
 2. Append an inline `#[cfg(test)] mod tests` (the file has none) with
    `a_backticked_template_filename_is_not_a_reference_to_its_prefix` (a body whose backtick span
    holds `AGENTS.md.template` yields no reference), `a_plain_source_path_is_still_a_reference`, and
    `a_path_followed_by_sentence_punctuation_is_still_a_reference` (a span holding `src/a.rs.`).

### E. Web 408 flake (`head.rs`, `mod.rs`, `connection.rs`; static, no run)

`commands::status::web::tests::errors::a_silent_client_is_answered_with_a_408` failed once at
`ff3fe947` in a loaded full-suite run with an EMPTY response, and passed 10 of 10 under
`scripts/flake-check.sh` load 8. The crate does not compile mid-wave, so you cannot reproduce it:
the orchestrator runs `scripts/flake-check.sh --runs 50 --load 16
commands::status::web::tests::errors::` after the wave. Your job is static.
14. Read `head.rs` (`complete`, `peek_head`), `connection.rs` (`gate`, `drain_pending`, `fail`),
    `limits.rs` and `mod.rs:339-383` (`spawn_connection`). The client in
    `tests.rs:109-118` writes nothing and never half-closes, so an empty `read_to_string` means the
    server closed with FIN and wrote no response. The paths that do that:
    `peek_head` `Ok(0)` and its catch-all `Err(error)` arm (this includes `ErrorKind::Interrupted`:
    a recv under `SO_RCVTIMEO` returns EINTR on any delivered signal regardless of `SA_RESTART`);
    `complete` returning `None` when `running` clears; `gate` returning `None` when
    `set_read_timeout` or `set_write_timeout` fails; and, in `mod.rs`,
    `spawn_connection` dropping the stream when `set_nonblocking(false)` or `local_addr()` fails or
    `thread::Builder::spawn` fails under thread or pid pressure (`mod.rs:380-382` only logs). The
    connection cap answers 503, never an empty body.
15. Fix inside `head.rs`: in `peek_head`, treat `ErrorKind::Interrupted` like `WouldBlock` and
    `TimedOut` (retry until the budget ends, then 408). Put the kind test in
    `fn is_transient_peek_error(kind: ErrorKind) -> bool` with an inline `mod tests` in `head.rs`:
    `would_block_timed_out_and_interrupted_are_transient` and
    `connection_reset_and_broken_pipe_are_terminal`.
16. Fix the spawn-failure drop in `spawn_connection`. `mod.rs` is 398 lines and cannot grow, so
    the function moves: create `src/commands/status/web/unserved.rs` holding `spawn_connection`
    (moved verbatim from `mod.rs:338-383`, with its `#[allow(clippy::too_many_arguments)]` and
    the imports it needs), a `pub(super) fn answer_unserved(stream: &mut TcpStream)` and an
    inline `#[cfg(test)] mod tests`. `mod.rs` keeps exactly `mod unserved;` (beside its sibling
    module declarations) plus the one call, now `unserved::spawn_connection(`; delete the old
    function and any import it alone used. In `unserved.rs`, `try_clone` the stream before
    `thread::Builder::spawn`; when the spawn fails, call `answer_unserved` on the clone
    instead of only logging, so a loaded server never closes a connection silently.
    `answer_unserved` answers `503 Service Unavailable` through `connection::fail` (which
    already drains the unread request bytes first, as every other error response does) after
    setting the write timeout as `connection::reject_overloaded` does. Test it over a loopback
    pair guarded by `crate::commands::status::web::tests::skip_without_loopback` (`pub(super)` in
    `web/tests.rs`, so visible here; pattern: `tests/errors.rs`) in `unserved.rs`'s own test
    module, never in `tests/errors.rs`: the client writes nothing, reads to EOF, and sees a
    `503` status line. In `gate` (`connection.rs`), a `set_*_timeout` failure keeps returning
    `None` (a socket that refuses a timeout cannot be written safely); say so in a comment.
17. Record `loom memory note "found: web 408 empty-response candidates: <the list in step 14>;
    fixed the EINTR retry in head.rs and the spawn-failure 503 in unserved.rs"` with `--evidence`
    pointing at `mod.rs:380` (where the drop sits at `ff3fe947`). Leave `tests/errors.rs` unchanged (its assertion lines cannot be
    edited, and an EINTR cannot be injected there). The orchestrator runs the flake check after
    the wave.

### F. Hidden flags in completions (`src/completions/dynamic/commands.rs`)

W1 adds the hidden `--daemon-child` flag to `loom run`. `complete_flags` pushes every argument's
long and short name, so completions would offer it; the subcommand and value completers in the
same file already skip hidden items (`visible_subcommand_names`, `complete_flag_choices`).

1. In `complete_flags`, iterate `command.get_arguments().filter(|arg| !arg.is_hide_set())`.
2. Append an inline `#[cfg(test)] mod tests` (the file has none; keep its assertions out of
   `src/completions/dynamic/tests/`, which C2 owns) with
   `run_completions_never_offer_the_hidden_daemon_child_flag`: `complete_flags(&["run"],
   "--daemon")` does not contain `--daemon-child`, and `complete_flags(&["run"], "--")` still
   contains `--manual`.

## Patterns to copy

`launch/host.rs` `wrapper_env` and `host_env.rs` `render` (one export per `filter_map` entry);
`completions/install/tests.rs:21-48` for the env guard; `review_status.rs` `print_rounds` for output
style. Do not copy `commands/subagents/ledger.rs` `json_lines` (private).

## Traps

- Acceptance greps: no `kern.bootsessionuuid` anywhere under `src/commands/subagents`, and no
  `kern.boottime` anywhere under `src`.
- Knowledge (`mistakes/test-concurrency-and-fixtures.md`): env-mutating tests are `#[serial]` and
  restore the previous value on drop; a stray `LOOM_BOOT_ID` leaks into every later test.
- `lease.rs` is 395 lines at your base and drops about 100 (the macOS functions and
  `macos_boot_tests`); `gate.rs` is 133 and grows by about 90; every file
  stays at or under 400 and every function at or under 50 lines. `web/mod.rs` is 398 lines: step
  16 must leave it shorter, never longer.
- A hint is advisory text on an error path; it must never turn a passing gate into a failing one or
  panic on a malformed ledger.

## The one check

None: the crate does not compile until W1, W2 and W5 return, and the orchestrator runs
`cargo test --lib` filters afterwards. Do not run cargo or the flake script.

## Report

Files changed; the exact candidate list from step 14; the `tests_wrapper_env.rs` note; the final
line counts of `web/mod.rs` and `web/unserved.rs`; deviations from the pins.
