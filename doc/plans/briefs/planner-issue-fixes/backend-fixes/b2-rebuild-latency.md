# B2: a pressed rebuild must start at once (#16)

Tier: opus at effort xhigh (`loom-senior-software-engineer`, hard debugging). Read `../common.md` first. Knowledge: `doc/loom/knowledge/architecture/summary-store.md` (sections "Rebuild Scheduler", "Rebuilds and Cost") and `architecture/planning-session-bridge.md`.

## Symptom

The operator pressed Rebuild in the planner, and the rebuild took about two minutes to start.

## Evidence gathered at planning time

- The press path: `web/src/plan/summaries/state.ts:157` posts `/api/summaries/rebuild`, and the server answers through `summaries/routes.rs:149-160`, `rebuilds.enqueue(ids, Trigger::Button)`. That goes to `rebuilds/api.rs:92` `scheduler.press`, then `scheduler.rs:346-350` (`forget`, `promote(ids, Via::Button)`, `try_start`), then `queue.rs` `try_start`, then `RealSpawner::start_summary` (`spawner.rs`). Next come `bridge.start_session` and the bridge driver tick (`chat/mod.rs`, 200 ms), which calls `spawn_pending`, which calls `host.spawn` (an interactive `claude` in tmux). The opening is pasted after `SessionStart`, while the session is Idle.
- **Every row** in the operator's cost ledger (`~/.loom/plan/loom-f1ee67247a0f/summary-cost.jsonl`; 143 rows on 2026-11-03) has `"trigger":"background"`, and none has `"button"`. Presses happened. So either a press never starts its own session and a later background pass does the work, or attribution mislabels a pressed session (`attribution.rs` `trigger_of` / `pressed_or_background`, about lines 353-368; `Attribution::summary_started(session, triggers)`, about lines 105-111; `scheduler/session.rs:44`; the triggers captured as `running_triggers` in `rebuilds.rs`, about line 339). Both are defects. The first would explain a two-minute wait.
- `try_start` returns early, without starting and without saying why, while a question is open, a session is running (one at a time, stall guard `NO_SEAL_TIMEOUT` 600 s), the server is read-only, a suspension is held, or the bridge answers `Busy("recovery in progress")`.
- "Rebuild all" in the approval notice (`web/src/plan/summaries/approval-notice.tsx`) and in the activity popover (`rebuild-question.tsx`) answers a question through `/api/summaries/answer`, not through the press route. Trace that path too.
- The session is launched with `--strict-mcp-config` and `--setting-sources`, so no user MCP server or user hook slows the cold start.
- The latest real rebuild, 2026-11-03: the opening reached the session at 15:42:48.991Z (transcript `9a71f551-748c-4646-8ae6-87e50e38a188.jsonl`, created 15:42:49 local +02:00). The first seal followed at 15:43:36Z. So the session itself started within a second of its transcript. The delay lies before the spawn.

## Task

1. Reproduce before fixing. Use the fake host and fake spawner the tests already use (`plan_session/bridge/fake_host.rs`, `bridge/harness.rs`, `plan_server/summaries/scheduler/tests*.rs`, `rebuilds/tests*.rs`). Drive both the press route and the answer route through to the spawn. Record the timeline of each and every state that defers the start. Record it with `loom memory note`.
2. Fix the cause so these three things hold:
   - A press, or a "Rebuild all" answer, while nothing is running and no question blocks it, reaches the backend spawn within one bridge tick. Nothing waits for a background pass.
   - A press while a background session is running is not left waiting silently. Settle how it is handled: the pressed ids join the next start, which begins the moment the running session seals or ends. While it waits, the panel's existing fields say why (`running` names the session, `start_error` or the queue names the waiting ids). Use the existing `SchedulerView`/panel wire. Add no new wire field. If one seems unavoidable, stop and report it, because the web stages do not expect one.
   - A pressed session is attributed `button` in the cost ledger. **Settled rule:** a session is `button` when any of its ids was queued by a press (`Via::Button`) or by a "Rebuild all" answer (`Via::Approved`), else `background`. Two code paths contradict it today, and either alone yields the all-background ledger: `scheduler/session.rs::running_triggers` (~L38-46) maps only `Via::Button` to `Trigger::Button`, so an approved id (`scheduler.rs` ~L335, `promote(&open.ids, Via::Approved)`) logs `background`; and `attribution.rs::pressed_or_background` (~L361-367) needs every id of a line pressed, so a press that a background id joined logs `background`. Fix both; `Trigger` already has `Button`, so no wire field changes.
3. Write a regression test for each defect you fix. Each must fail on the code at the stage's base: name the wrong behaviour it fails on in your report. The three outcomes above carry these exact test function names, which the stage's `after_stage` checks search for under `loom/src`:
   - `a_pressed_rebuild_reaches_the_spawner_within_one_tick` (a press, and a "Rebuild all" answer, with nothing running and no question open). At least one case enters through the facade (`Rebuilds::enqueue(ids, Trigger::Button)` or the answer route) and observes the spawner's start, so a scheduler-only fix that the facade bypasses still fails it;
   - `a_pressed_session_is_costed_as_button`: three cases, a press, a "Rebuild all" answer, and a press whose batch a background id joined. For each, end the session through the attribution path that writes the cost line (as `attribution/tests/costing.rs` does), read the ledger back with `cost::read_log`, and assert the parsed line's trigger is `button`. A `running_triggers()` assertion alone does not prove the emitted ledger;
   - `a_press_during_a_running_session_joins_the_next_start` (no second start while one runs; the next start includes the pressed ids the moment the running session seals or ends).
   Seams the tests can drive: `Scheduler::press(ids, now, spawner)` (`scheduler.rs`), `running_triggers()` (`scheduler/session.rs`), `Rebuilds::new_for_test` (`rebuilds.rs`) with `enqueue(ids, Trigger::Button)` (`rebuilds/api.rs`), and the stub spawner and `setup()` harness in `loom/tests/plan_summary_rebuild_contracts.rs`. If the defect lies past the scheduler (the bridge's `Busy("recovery in progress")` path in `plan_session/bridge/recovery.rs`, or the 200 ms driver tick in `plan_server/chat/mod.rs`), the first test drives the bridge harness (`bridge/harness.rs`, `bridge/fake_host.rs`) through to `host.spawn` instead.
4. If you cannot reproduce the delay, say so first in your report with the timelines you recorded. Do not write tests that pass at base to fill the names: the main agent escalates an unreproduced cause.

## Files you own

`loom/src/plan_server/summaries/**`, `loom/src/plan_session/bridge/start.rs`, `loom/src/plan_session/bridge/opening.rs`, `loom/src/plan_session/bridge/rebuilding.rs`, `loom/src/plan_session/bridge/tests_opening.rs`, `loom/src/plan_session/bridge/tests_rebuilding.rs`.

If the cause lies in a file outside this list (for example `plan_server/chat/mod.rs`, `plan_session/bridge/recovery.rs`, `driver.rs` or `session.rs`), stop and report the file and the change instead of editing it. The stage's files include those paths so the main agent can send the change to a follow-up worker.

## Proof

`cargo test --lib plan_server::summaries::`, `cargo test --lib plan_session::bridge::` and `cargo test --test plan_summary_rebuild_contracts` once at the end. Report the result lines, the before/after timeline, and for each named test the wrong behaviour it fails on at base.
