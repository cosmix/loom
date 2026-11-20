# Brief: startup-deadline (native Codex worker startup watchdog)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/native_worker/machine/mod.rs`, `loom/src/native_worker/machine/turns.rs`, `loom/src/native_worker/machine/tests_cancel.rs`, `loom/src/native_worker/machine/tests.rs`, `loom/src/native_worker/supervisor/mod.rs`, `loom/src/native_worker/supervisor/tests.rs`.

Frozen, read-only: `loom/src/native_worker/machine/startup_contract_tests.rs`.

## Facts (verified)

- The reducer arms the unit deadline only at thread start (`machine/mod.rs:230`); `machine/turns.rs:159` ignores a deadline before the first turn; `on_deadline` (164-178) sets TimedOut and interrupts with `INTERRUPT_GRACE_SECS` (5 s). Pinned today by `tests_cancel.rs:213` `deadline_before_first_turn_is_ignored`.
- The supervisor (`supervisor/mod.rs:63-64, 92-93, 119-121, 197`) delivers the deadline input from `deadline_at`; it has no startup timer, so a hung `initialize` or `thread/start` emits no progress and is never killed (`loom subagents watch` only marks it stalled after 600 s).
- Unit deadline: `native_worker/ids.rs:10` `NATIVE_DEADLINE_SECS = 1500`; unchanged.

## Change

1. `machine/mod.rs`: `pub const STARTUP_DEADLINE_SECS: u64 = 120;`. On worker start, emit an effect that arms a startup deadline (`ArmDeadline`-style, distinct kind) at now + 120 s.
2. A startup-deadline input before any thread has started settles the worker `TimedOut` with reason `startup deadline (120 s) passed before the Codex thread started` and stops the app-server (the same teardown effects a cancel uses before a turn). After the thread starts, the startup deadline input is ignored; the unit deadline arms at thread start as today.
3. `supervisor/mod.rs`: keep a separate `startup_deadline_at`, deliver its input when it passes, clear it when the thread starts.
4. `tests_cancel.rs`: replace `deadline_before_first_turn_is_ignored` with a test that the unit deadline is still ignored before the first turn while the startup deadline is not (the stage INTEGRITY line names this edit). Add a supervisor test with a scripted peer that never answers `initialize`.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib native_worker::machine::` and `--lib native_worker::supervisor::` pass with the frozen contract.

## Traps

- `native-macos-boundary` later edits `supervisor/mod.rs` (teardown gating near :341); keep your change away from that region.
- Bwrap-backed supervisor tests skip on macOS; your new test must use the scripted peer, not a real boundary.
