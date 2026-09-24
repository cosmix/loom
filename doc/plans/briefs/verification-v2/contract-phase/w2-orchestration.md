# contract-phase / W2 — contract spawn, exit handling, contract signal, v2 signal section

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D8, D16 (standard frozen-contracts
bullet only). Knowledge: `mistakes/adjudication-autonomy-deadlock.md` (all of it: every trap
there applies to a second session kind); `patterns/stage-lifecycle-and-verification.md`
"Locked Stage Read-Modify-Write Pattern (A-5)"; `architecture/signal-generation.md`.

Pinned names from other workers: `SessionType::Contract`, `Session::new_contract(stage_id)`,
`backend.spawn_contract_session(stage, worktree, session, signal_path)` (W1);
`crate::verify::contracts::store::{load_freeze(work_dir, stage_id) -> Result<Option<FreezeRecord>>, attempts_spent(work_dir, stage_id) -> Result<u32>, spend_attempt(work_dir, stage_id) -> Result<u32>}`
(W4).

## Files you own

`loom/src/orchestrator/core/stage_executor.rs` (780 lines, ledgered; `start_stage` 357 ledgered),
`loom/src/orchestrator/core/stage_spawn.rs` (new), `loom/src/orchestrator/core/mod.rs`,
`loom/src/orchestrator/core/event_handler.rs` (`handle_one_event` 116 ledgered),
`loom/src/orchestrator/core/event_handler/contract_phase.rs` (new),
`loom/src/orchestrator/monitor/events.rs`, `loom/src/orchestrator/monitor/session_events.rs`,
`loom/src/orchestrator/monitor/detection.rs` (`detect_stage_changes` ledgered at 56),
`loom/src/orchestrator/signals/contract.rs` (new), `loom/src/orchestrator/signals/v2_section.rs`
(new), `loom/src/orchestrator/signals/mod.rs`, `loom/src/orchestrator/signals/generate.rs`.

## Tasks

1. Extract the spawn tail of `start_stage`: write-ahead session (L320-322), the locked
   `update_stage` (L328-344), `graph.mark_executing`, sandbox validation, `reconcile_overlay`,
   `require_stage_hooks`, signal generation (L393-414) and the backend spawn (L429-455). Move it
   into `stage_spawn.rs` as `spawn_stage_agent(&mut self, stage, worktree, kind: AgentKind)`,
   where `AgentKind` is `Implementation` or `Contract`. For `Contract` it writes ahead
   `Session::new_contract`, generates the contract signal, and calls `spawn_contract_session`.
   `start_stage` chooses `Contract` when `stage.plan_version == 2`, `stage_type == Standard`,
   the stage has contracts and `load_freeze` returns `None`. Otherwise it chooses
   `Implementation`. Lower the two ledger entries to their new exact counts.
2. Detection. A Claude session idles at its prompt when its work is done, so the end of the
   contract phase is detected from the freeze record, not from a process exit (DESIGN D8).
   - Each tick, a stage that is `Executing`, whose session is `SessionType::Contract`, and whose
     `load_freeze` is `Some`, raises the new `MonitorEvent::ContractPhaseFinished { stage_id }`
     (`monitor/events.rs`). Put the check in `monitor/detection.rs` beside the other per-stage
     detections, as a helper called from `detect_stage_changes`, and pay for the call line.
   - In `session_events.rs`'s vanished-process chain (L202-217, beside
     `finished_adjudication_session` L248-267), a Contract session whose process vanished
     without a freeze raises `MonitorEvent::ContractSessionEnded { stage_id }`, never a crash
     record.
3. `event_handler/contract_phase.rs` handles both. The dispatch arm in `handle_one_event` must
   not grow it; extract to pay for the lines.
   - finished: re-read the stage under the lock; require `Executing` and a Contract session.
     Take the contract agent down with the take-down path in
     `event_handler/stage_takedown.rs` (`take_down_agents`, which kills and confirms death). If
     any survivor remains, do nothing this tick. Otherwise clear the session and call
     `spawn_stage_agent(.., AgentKind::Implementation)`.
   - ended without a freeze: `spend_attempt`; attempts ≤ 3 ⇒
     `spawn_stage_agent(.., AgentKind::Contract)`; otherwise move the stage to
     `NeedsHumanReview` with the reason
     `contract session ended 3 times without freezing contracts`.
   - The stage never leaves `Executing` between the two sessions. No new `StageStatus`, no new
     transition edge.
4. `signals/contract.rs`: `generate_contract_signal(stage, work_dir, ...)`, same file layout and
   write path as the stage signal, with the content DESIGN D8 lists. Reuse the knowledge-brief
   and skills helpers `generate.rs` already calls. For each contract, the adapter comes from
   `contract.runner` or, when absent, `crate::skills::project` detection for the package that owns
   the file, and the single-test command from `testrun::registry::by_name(..)`. An unsupported
   runner is stated in the signal.
5. `signals/v2_section.rs`: `append_v2_section(content: &mut String, stage: &Stage, work_dir: &Path)`,
   a no-op unless `stage.plan_version == 2`. In this stage it renders the standard-stage
   "## Frozen Contracts" block (DESIGN D16 first bullet): contract ids, files, "never edit these
   files", `loom stage contracts show` and `restore`. Structure it as one function per block;
   review-harvest-gate adds its blocks in a sibling file. Call it from `generate.rs` next to
   `append_stage_feedback` (L311-335).

## Named tests (binding)

In `event_handler/contract_phase.rs`'s test module, or a sibling `*_tests.rs` via `#[path]`:

- `contract_session_spawns_before_stage_session`: `start_stage` on a v2 standard stage with one
  contract and no freeze writes ahead and records a `SessionType::Contract` session, and leaves
  the stage `Executing`. Use the orchestrator test harness in
  `orchestrator/core/event_handler/tests.rs` and a fake backend; find the existing fake with
  `loom map --find-all FakeBackend` or its equivalent.
- `contract_phase_finished_spawns_stage_session`: with a freeze present and the contract agent
  taken down, `ContractPhaseFinished` replaces the session with a `SessionType::Stage` session and
  the stage stays `Executing`; with a surviving contract agent, nothing changes that tick.
- `contract_session_exit_without_freeze_respawns`: `ContractSessionEnded` respawns a contract
  session and spends one attempt. The fourth end moves the stage to `NeedsHumanReview`.

## Proof (one command, once, after W1 and W4 report)

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::core::`

## Report

Files changed; exact new counts of `stage_executor.rs`, `start_stage` and `handle_one_event`;
the proof result.
