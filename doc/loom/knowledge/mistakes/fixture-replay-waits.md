# Fixture Replay Waits

> Waiting on a state a replayed session already holds races the bridge

## A Fixture-Replay Test Waited on the State It Already Held

**What happened:** `plan_session::bridge::tests_slash::p2_usage_context` failed once in a full suite (15066 passed, 1 failed) and reproduced 2 of 12 runs with CPU burners at `--test-threads=64`: the paste queued (`Queued q-2`) instead of being delivered.

**Why:** `fixture_replay.rs::Replay::until` only appends fixture lines to the spool, and `session(&h)` starts `Idle`, so `wait_state(Idle)` passed on the pre-replay state before the bridge read the replayed `UserPromptSubmit`. A wait on a state the session already holds proves nothing about a write that has not been consumed yet.

**Prevention:** after `until()`, call `Replay::consumed(h, s)` before any state wait the session may already satisfy; it waits until `session.flow.applied` (`driver.rs`, never pruned) holds a key for each written spool line's `received_at`. It covers spool lines only: transcripts have no mark, so a test asserting transcript-derived state right after `consumed()` can still race. The match is by count per `received_at`, sound because the bridge's key hashes the payload, which holds `session_id`; two fixture lines in one millisecond that differ only outside the payload over-count and the wait times out after 5 s, loudly. Prefer `wait_until` on the effect itself when the effect is observable. Open helper items: [fixture-replay leftovers](../concerns/code-quality-and-hook-debt.md#fixture-replay-test-helper-leftovers).
