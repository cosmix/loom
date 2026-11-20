# E3: pane tail and stall-counter resets

Tier: sonnet. Read `doc/plans/briefs/open-issues-19-24/common.md` first; this brief adds only E3's part.
Plan Decision 12 (#20): the tail E2 attaches to a parked stage, `stall_recoveries` reset on approve, reset
and retry, and removal of PR #25's stall marker (task 4). Line numbers read at `ff3fe947` and re-checked at
`11859505` (PR #25, which this stage's worktree includes); anchor on symbols.

## Files owned (write only these; all under `loom/`)

`src/orchestrator/terminal/session_tail.rs` (new), `src/orchestrator/terminal/mod.rs`,
`src/orchestrator/terminal/tmux/capture.rs` (new), `src/orchestrator/terminal/tmux/mod.rs`,
`src/orchestrator/terminal/tmux/tests.rs`, `src/commands/stage/state.rs`, `src/commands/stage/state_tests.rs`,
`src/commands/stage/human_review.rs`, `src/commands/stage/human_review_tests.rs` (new),
`src/commands/stage/skip_retry.rs`, plus the PR #25 stall-marker files of task 4 (all under `loom/`):

- `src/models/stage/stall.rs` (delete), `src/models/stage/mod.rs`, `src/models/stage/types.rs`,
  `src/models/stage/defaults.rs`;
- `src/commands/status/data/mod.rs`, `.../data/heartbeat_facts.rs`, `.../data/heartbeat_facts_tests.rs`
  (delete), `.../data/collector.rs`, `.../data/sanitize.rs`;
- `src/commands/status/render/attention_model.rs`, `.../render/attention_model_tests.rs`,
  `.../render/attention_tests.rs`, `.../render/graph.rs`, `.../render/graph_tests.rs`;
- `src/commands/status/ui/tui/ledger/rows.rs`, `.../ui/tui/ledger/tests.rs`, `.../ui/tui/state_tests.rs`;
- `src/commands/status/web/model.rs`, `.../web/model_tests_stages.rs`, `src/daemon/wire_tests.rs`,
  `tests/stage_exits_contracts.rs`;
- `src/orchestrator/core/mod.rs`, `src/orchestrator/notify.rs`.

Read-only: `tmux/mod.rs:94` `TMUX_PROBE_TIMEOUT`, `:147` `run_tmux_control`, `:163` `socket_name`;
`tmux/viewer.rs:185` `tmux_session_name`, `:288` `endpoint_ready` (the precedent);
`orchestrator/spawner.rs:96` `read_log_tail`; `orchestrator/terminal/native/session_log.rs:23` `stderr_log_path`
(re-exported at `native/mod.rs:41`); `orchestrator/core/crash_classification.rs:189` (reads a stderr tail the same way).

## Pinned interfaces

You PROVIDE (quoted from common.md):

- "**Session tail (E3, `loom/src/orchestrator/terminal/session_tail.rs`, re-exported from
  `loom/src/orchestrator/terminal/mod.rs`).** `pub fn session_tail(session: &crate::models::session::Session,
  work_dir: &Path, lines: usize) -> Option<String>`: the last `lines` non-empty lines of the session's tmux pane
  (`capture-pane -p -J -S -<lines>` on the session's own `-L` socket, bounded by the tmux probe timeout), or for
  a native session the tail of its stderr log; `None` when nothing is readable. Output is control-character stripped."

E2 calls it as `crate::orchestrator::terminal::session_tail(&session, &work_dir, 40)`. You consume nothing from E1 or E2.
The frozen contract `tests/session_auth_and_stalls_contracts.rs` drives `loom stage human-review <id> --approve`
and expects `stall_recoveries` back at 0.

## Root cause (re-verified at 11859505, PR #25)

`Stage::stall_recoveries` (`models/stage/types.rs`) has one writer, `charge_stall_recovery`
(`recover_hung.rs:214`), and nothing ever resets it. So after two recoveries a stage a human has fixed and
re-queued exhausts its budget on the first stall of the fresh session. The resets that exist (PR #25 lists "`loom stage reset` does not reset `stall_recoveries`" as a follow-up; the
reset below is that fix):
`commands/stage/state.rs` `apply_reset` (`:335`, sets `status`, `retry_count`, `fix_attempts`, `session` and more,
not `stall_recoveries`); `human_review.rs` `handle_approve` (`:95`, inside the `update_stage` closure, sets
`fix_attempts = 0` only); `skip_retry.rs` `apply_retry_delta` (`:312`, reached from `retry` (`:43`) through
`persist_retry_delta`; verified that is where `loom stage retry` resets a stage).

## Tasks

### 1. `tmux/capture.rs` (new) and `tmux/mod.rs`

```rust
pub(super) fn capture_pane_argv(socket: &str, session_name: &str, lines: usize) -> Vec<String>;
pub(crate) fn capture_pane_tail(session: &Session, lines: usize) -> Option<String>;
```

- `capture_pane_argv` returns exactly `["-L", socket, "capture-pane", "-p", "-J", "-t", session_name, "-S", "-<lines>"]`
  (the last element is `format!("-{lines}")`).
- `capture_pane_tail`: `tmux_session_name(session)?` (from `super::viewer`), `socket_name(session)`; return `None`
  without spawning when `socket_path_for(&socket)` does not exist (the `endpoint_ready` stat precedent, so a
  dead or never-created server costs no subprocess); otherwise run
  `super::run_tmux_control(&args, super::TMUX_PROBE_TIMEOUT, format!("tmux capture-pane ({socket})"))` and return
  the lossy-UTF-8 stdout only when the status is success (`.ok()?`, `status.success()`). Raw text, no trimming:
  `session_tail` normalises. `run_tmux_control` pins `TERM=dumb` and the stage environment, which is what you want.
- `tmux/mod.rs` (336 lines): add `mod capture;` and `pub(crate) use capture::capture_pane_tail;` beside
  the existing `mod`/`pub use` lines (`:25-56`). Nothing else.

### 2. `session_tail.rs` (new) and `terminal/mod.rs`

- `pub fn session_tail(session, work_dir, lines) -> Option<String>`: `match session.backend` (`SessionBackendKind`
  `Tmux`/`Native`): tmux gives `tmux::capture_pane_tail(session, lines)`; native gives
  `crate::orchestrator::spawner::read_log_tail(&native::stderr_log_path(work_dir, &session.id),
  lines.saturating_mul(4).max(lines))`. `read_log_tail` cuts raw lines, blank ones included, so reading
  exactly `lines` of them could return fewer than `lines` non-empty lines when the log ends in blanks;
  the 4x read leaves room, and `normalise_tail` then keeps the last `lines` non-empty lines. Feed the raw
  text to `normalise_tail(&raw, lines)`.
- `fn normalise_tail(raw: &str, lines: usize) -> Option<String>` (private, tested inline): strip ANSI CSI
  sequences (`ESC [ ... final byte 0x40-0x7e`) and every other control character except `\n` (turn `\t` into one
  space; drop `\r`); `trim_end` each line; drop blank lines; keep the last `lines`; join with `\n`; `None` when
  `lines == 0` or nothing remains.
- `terminal/mod.rs` (20 lines): `mod session_tail;` and `pub use session_tail::session_tail;`.

### 3. Counter resets

- `state.rs` `apply_reset`: add `stage.stall_recoveries = 0;` with a one-line comment (a reset hands the stage to a fresh attempt).
- `human_review.rs` `handle_approve`: in the `update_stage` closure add `stage.stall_recoveries = 0;` after
  `stage.fix_attempts = 0;`. FIRST move the inline `#[cfg(test)] mod tests { ... }` (from `:183` to the end of
  the file) into the new `src/commands/stage/human_review_tests.rs`: the file is 399 lines and cannot grow.
  Dedent the module body one level, so `use super::*;` and the other `use` lines start the file; change nothing
  else: every assertion line stays verbatim, no test is renamed, dropped or edited. Replace the module with
  `#[cfg(test)] #[path = "human_review_tests.rs"] mod tests;`. The module path stays
  `commands::stage::human_review::tests`.
- `skip_retry.rs` `apply_retry_delta`: add `current.stall_recoveries = 0;` next to `current.last_failure_at = None;`
  and change its visibility from private to `pub(super)` (so `state_tests.rs` can call it; `reset_contract_budget`
  there is `pub(super)` the same way). The retry tests live in `skip_retry_tests.rs`, which is NOT yours: do not edit it.

### 4. Retire PR #25's stall marker

The operator parked #20 in needs-human-review (E2); the PR's STALLED marker on an `Executing` stage is
removed. E2 removes the handler side (`recover_hung.rs`, `recover_hung_tests.rs`, `event_handler.rs`). You
remove the rest; keep everything else in these files as the PR left it.

- `models/stage/stall.rs`: delete the file. `models/stage/mod.rs`: delete `mod stall;` and
  `pub use stall::StallExhaustion;`. `models/stage/types.rs`: delete the `stall_exhausted` field, its doc and
  the `use super::stall::StallExhaustion;` import. `models/stage/defaults.rs`: delete `stall_exhausted: None,`.
- `commands/status/data/mod.rs`: delete `StageSummary.stalled_after_recoveries` and its doc.
  `data/heartbeat_facts.rs`: delete the `stalled_after_recoveries` fn and field, the
  `#[cfg(test)] #[path = "heartbeat_facts_tests.rs"] mod tests;` declaration and the doc words about the
  stall; KEEP the PR's move of `judge_heartbeat_secs` into `HeartbeatFacts`. Delete
  `data/heartbeat_facts_tests.rs` (all four tests exercise the deleted fn). `data/collector.rs`: delete the
  `stalled_after_recoveries:` field init; keep `execution_models_for_stage` and
  `heartbeat.judge_heartbeat_secs`.
- `commands/status/render/attention_model.rs`: delete the `StageStatus::Executing => ("STALLED",
  stall_guidance(stage)?)` arm, `stall_takeover`, `stall_guidance`, and `stall_reason, stall_takeover_command`
  from the `crate::orchestrator::core` import. `render/attention_model_tests.rs`: delete test
  `an_executing_stage_left_stalled_asks_an_operator_to_take_it_over`. `render/graph.rs`: delete
  `write_stall_hint`, its call in `write_row_hints` and `stall_takeover` in the import; KEEP
  `write_row_hints` (it keeps `render_graph` at 64 lines). `render/graph_tests.rs`: delete tests
  `test_executing_stage_left_stalled_says_why_and_how_to_take_it_over` and
  `test_executing_stage_not_left_stalled_has_no_stall_hint`.
- Delete the `stalled_after_recoveries: None,` struct-literal lines in `data/sanitize.rs`,
  `render/attention_model_tests.rs`, `render/attention_tests.rs`, `render/graph_tests.rs`,
  `ui/tui/ledger/rows.rs`, `ui/tui/ledger/tests.rs`, `ui/tui/state_tests.rs`,
  `web/model_tests_stages.rs`, `src/daemon/wire_tests.rs`, `tests/stage_exits_contracts.rs` (leave the PR's
  parameter renames in that last file alone). Field lines are not assertions.
- `commands/status/web/model.rs`: delete `stage.stalled_after_recoveries = None;` in
  `without_merge_resolver_facts` and the "spent stall recoveries" words in its doc.
  `web/model_tests_stages.rs`: delete `stage.stalled_after_recoveries = Some(2);` in `stage_docs()` and put the
  comment back to its singular "this key ... drops it" form.
- `orchestrator/core/mod.rs`: delete `pub(crate) use event_handler::{stall_reason, stall_takeover_command};`.
  `orchestrator/notify.rs`: delete `notify_stall_recovery_exhausted`; KEEP `notifier_for`, the `loom-notify`
  thread, the `cfg!(test)` no-op and their tests.
- Confirm with `rg -n 'stall_exhausted|StallExhaustion|stalled_after_recoveries|stall_takeover|stall_reason|notify_stall_recovery_exhausted|write_stall_hint' loom/src loom/tests`
  that only E2's files (`recover_hung.rs`, `recover_hung_tests.rs`, `event_handler.rs`) still match when you
  finish; the crate compiles only after E2 finishes too, so the one check below stays as it is.
- Deleting the four tests above and `heartbeat_facts_tests.rs` removes assertion lines that exist at the
  stage's base. Expected test-integrity events, disputed by the orchestrator (never by you), on
  `loom/src/commands/status/render/attention_model_tests.rs`,
  `loom/src/commands/status/render/graph_tests.rs` and
  `loom/src/commands/status/data/heartbeat_facts_tests.rs`. List every deleted test in your report.

## Tests to write (exact paths)

- `orchestrator::terminal::tmux::tests` (append to `tmux/tests.rs`, 277 lines): `capture_pane_argv_is_exact`
  (call `capture::capture_pane_argv("loom-session-abc", "loom-stage", 40)` and assert the full vector, including
  `"-S"`, `"-40"` and that the socket and target are not shell-quoted).
- `orchestrator::terminal::session_tail::tests` (inline): `normalise_tail_strips_control_characters_and_blank_lines`,
  `normalise_tail_keeps_the_last_n_lines`, `normalise_tail_is_none_for_blank_input_and_zero_lines`, and
  `a_native_session_reads_its_stderr_log_tail` (a `Session::new()` with `backend = SessionBackendKind::Native`,
  a `TempDir` work dir, create the parent of `stderr_log_path(work, &session.id)` with `create_dir_all`, write
  five lines with ANSI colour and blank lines, assert the last three come back clean; a second log whose last
  raw lines are blank still yields `lines` non-empty lines, which the 4x read provides), and
  `a_tmux_session_without_a_server_has_no_tail` (a fresh `Session::new()` with `backend = Tmux`: its socket does
  not exist, so the result is `None` and no tmux process is spawned).
- `commands::stage::state::state_tests` (append to `state_tests.rs`, 227 lines):
  `reset_clears_the_stall_recovery_counter` (build the stage with the existing `executing_stage` helper, load it,
  set `stall_recoveries = 2`, `save_stage`, drive `loop_recovery::reset_with(temp.path(), "alpha", true, true,
  &runtime)` exactly as `reset_closes_open_disputes` does, assert 0) and
  `retry_delta_clears_the_stall_recovery_counter` (a `Stage` with `status` Blocked, `stall_recoveries: 2`; call
  `crate::commands::stage::skip_retry::apply_retry_delta(&mut stage, &planned, false)` and assert 0 and `Queued`).
- `commands::stage::human_review::tests` (append to `human_review_tests.rs`):
  `test_human_review_approve_resets_stall_recoveries` (`setup_stage(&temp, StageStatus::NeedsHumanReview,
  Some("stalled"))`, set `stall_recoveries = 2` and `save_stage`, `handle_approve("test-stage", temp.path())`, then
  assert status `Queued` and `stall_recoveries == 0`). Use the same helpers as `test_human_review_approve_closes_an_open_dispute`.

## Patterns to copy

- `tmux/viewer.rs:288-299` `endpoint_ready` for the stat-then-`run_tmux_control` shape and operation label.
- `orchestrator/core/crash_classification.rs:189-202` for tailing the stderr log.
- DO NOT copy `endpoint_ready`'s doc rule that only attach and the viewer reconciler may call it: that rule is
  about `has-session` as a liveness oracle. `capture_pane_tail` is a read of text for a report, never a liveness
  signal; its doc comment says so and says the monitor never derives life or death from it.

## Traps (knowledge, quoted)

- "every external command issued from the poll loop goes through `process::run_bounded`"
  (`mistakes/sessions-and-liveness.md`): E2 calls `session_tail` from the orchestrator's single poll thread, so
  the capture must stay under `TMUX_PROBE_TIMEOUT` (5 s) via `run_tmux_control`, never a raw `Command::output()`.
- "A control probe built on an unresolvable TERM exits non-zero, which reads identically to 'the server is not
  accepting clients'" (`tmux/mod.rs` `CONTROL_TERM_OVERRIDE`): reuse `run_tmux_control`, which pins `TERM=dumb`; do not build your own `Command`.
- Tests never start tmux or touch the real `HOME`/`.loom/work`; `a_tmux_session_without_a_server_has_no_tail` relies
  on the stat guard and a random session id. `socket_path_for` reads `$TMUX_TMPDIR`, which other tests mutate
  under `#[serial]`: your tests only read it, so no `#[serial]` is needed.
- A moved test file keeps its assertion lines verbatim (`common.md` worker rules); a moved-and-edited assertion is a test-integrity event.
- `human_review.rs` must end well under 400 lines after the move; `tmux/mod.rs` and `tmux/tests.rs` stay under 400.

## The one check you may run (once)

`cd loom && cargo test --lib orchestrator::terminal::session_tail:: 2>&1 | tail -30`. If the crate does not
compile because of another worker's symbol, say so and stop. No `cargo fmt`, no clippy.

## Report

Files changed; the one check and its result; confirmation that `human_review_tests.rs` holds the moved tests
unedited (count of `#[test]` functions before and after, plus your one new test); that `loom stage retry`
resets through `apply_retry_delta` in `skip_retry.rs` (or the contradiction if it does not); ledgered units shrunk (measure and report `file src/models/stage/types.rs`, expected about 696 lines, and
`function src/models/stage/defaults.rs default`, expected 76; the orchestrator updates the ledger); the
result of the task 4 `rg` confirmation; the deleted tests by name;
deviations from the pinned interface; surprises.
