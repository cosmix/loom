# W1: the agent ends a finished chat (codex astra, xhigh)

Read `common.md` beside this file first. It pins the fence and the end path.

## Files you own

- `loom/src/plan_session/bridge/**` (except the frozen `agent_end_contract_tests.rs`)
- `loom/src/plan_session/transcript/**`
- `loom/src/plan_session/model/**`
- `loom/src/planning/guidance/text/common_brief.md`
- `loom/src/planning/guidance/mod.rs` (only if the brief text is assembled there; `CLOSING_MESSAGE`
  is unchanged)

## Entry points (the tree includes the merged background-activity plan; read it, do not assume)

- `bridge/mod.rs`: `EndCause` (about line 197). Add `Agent`. Find every exhaustive match with
  `loom map --impact EndCause` and handle the new variant. If a match in `loom/src/plan_server/**`
  needs an arm, report it; that directory is not yours.
- `bridge/ending.rs` `plan_end` (about line 329): the first end request sets the plan. An agent end
  needs `Closing::Skip`, but must not take the `EndMode::Teardown` branch that denies pending items.
  Read `request_end` (`bridge/commands.rs` about line 334) and `begin_teardown`. Choose the mode
  whose close happens at the close gate, after the background table drains. Explain the choice in
  a rustdoc line.
- `bridge/driver.rs` `on_idle` (the main Stop handler, and the close gate the background plan
  rewrote), `closes_at_stop`, and the tick (`tick_session`).
  - Make the close a state check: when an end is planned, the closing write is confirmed or
    skipped, the activity is idle, no delivery is pending and the background table holds no running
    agent, set the session's close.
  - Run the check on the Stop and on every tick, and on the background table's change if that is
    where it is observed. The edge-only check at the Stop is the defect that leaves a chat stuck in
    ending when its agents finish after the Stop. This covers user ends too.
- Detecting the fence. Find where a turn's assistant text items live: `transcript/records.rs`,
  `model/mod.rs` `Item`, and how the driver sees the current turn. At the Stop, look at the turn's
  last assistant text item and apply the pinned rule: the final block, the exact info string, and a
  JSON body with `decisions` and `questions` arrays of strings. Put the parser in its own small
  module (`bridge/done_fence.rs` or `transcript/done_fence.rs`) with unit tests:
  - a valid fence;
  - text after the fence;
  - the info string `loom-done-later`;
  - an unparsable body;
  - a fence inside an earlier item followed by a later plain item;
  - CRLF line ends;
  - a body over 64 KiB, which is refused without parsing.
- Queue and delivery. Read `bridge/flow.rs` for the pending-delivery and queue state (`take_paste`,
  `delivered`). An agent end is not planned while either is non-empty.
- `common_brief.md`: add `## Ending the chat` with the text pinned in `common.md`. Keep the existing
  sections. If a test pins the brief's headings or length, update that test only if it is in your
  files; otherwise report it.

## Regression tests (beyond the frozen contracts)

Use the fake-host harness, as `bridge/closing_contract_tests.rs` and `bridge/tests_end.rs` do. Add:

- A user End whose closing Stop arrives while agent a1 still runs. Then a1's hand-back and its
  SubagentStop arrive with no further main Stop. The session ends within a few seconds, and the
  closing message was pasted once.
- An agent-ended chat that the user then sends a message to before teardown is refused the way any
  message to an ending chat is.
- A Summary session's turn with a `loom-done` fence ends the way a Summary session always ends: the
  agent end applies only to Authoring.

## Constraint

You cannot run these tests: the harness needs TempDir writes that the codex sandbox refuses. Run
`cargo check --manifest-path loom/Cargo.toml --all-targets` and `cargo test
--manifest-path loom/Cargo.toml --lib plan_session::bridge::done_fence --no-run`. Write the tests
carefully against the harness's real API (`super::fake_host`). The orchestrator runs them.

## Done

`cargo check --manifest-path loom/Cargo.toml --all-targets` is clean. Report the files changed, the
end mode you chose and why, every match arm outside your files that needs `EndCause::Agent`, and
any assumption about record shapes you could not confirm from the fixtures under
`loom/tests/fixtures/plan_session/`.
