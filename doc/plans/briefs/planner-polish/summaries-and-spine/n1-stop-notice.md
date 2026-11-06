# N1: one notice after Stop (issue 3, D1)

Tier: haiku (`loom-software-developer`). Read `common.md` D1.

## Change

In `loom/src/plan_session/transcript/content.rs`, `interrupt()` (lines 121-143): after `let Some(turn) = self.current_or_last()`, skip building and pushing the `Item::Notice { kind: NoticeKind::Interrupted, … }` when the turn's last item is an `Item::Notice` whose `kind` is `NoticeKind::Stopped`. Everything else in the function runs unchanged:
- the `RecordMark::Interrupted` push above it;
- marking pending tools `NotFinished`;
- closing the open turn.

The comment above the function says why: the Stop button already announced the interrupt.

## Tests

In `loom/src/plan_session/transcript/tests.rs`, next to the test around line 261 that expects `notice("n-u3", NoticeKind::Interrupted, "Interrupted")` (keep it unchanged: an interrupt with no Stopped notice before it keeps its line), add `an_interrupt_after_a_stopped_notice_adds_no_second_notice`:
- add a Stopped notice through `add_server_item`, as `Session::interrupt` does (`bridge/commands.rs:316-338`);
- ingest the interrupt record;
- assert the turn holds exactly one notice, the Stopped one;
- assert a pending tool in that turn became `NotFinished`.

## Contract (written before you start; make it pass, do not edit it)

`loom/src/plan_session/transcript/stop_contract_tests.rs`, `stop_then_interrupt_shows_one_notice`.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib plan_session::transcript::`
