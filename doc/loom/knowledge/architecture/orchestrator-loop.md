---
---
# Orchestrator Loop

> Tick order, Monitor, liveness

## Orchestrator Main-Loop Tick Sequence (Exact Call Order)

The runtime loop lives in `loom/src/orchestrator/core/run.rs`. `run_tick` (lines 77–109) executes the following sequence; `poll_run_events` (lines 111–124) then handles monitor events in automatic mode.

```text
1. check_target_guard()
2. reconcile_and_update_graph()
3. sync_graph_with_stage_files()
4. sync_queued_status_to_files()
5. check_pending_disputes()
6. apply_pending_verdicts()
7. spawn_merge_resolution_sessions()
8. drain_stage_spools()
9. drain_session_inboxes()
10. sweep_merged_leftovers()
11. hand_off_frozen_contract_phases()
12. start_ready_stages()
13. monitor.poll() → handle_events() [automatic mode]
```

The constructor and configuration live in `core/orchestrator.rs`; that file does not own the loop. Worker draining is not a separate call in this sequence. Verdicts are applied before scheduling, so a stage re-queued by a verdict can start in the same tick. Spools, inboxes, and frozen-contract handoff also run in manual mode.

`initialize_run` (lines 45–73) validates state, starts the target guard, reconciles and synchronizes stages, recovers orphaned sessions, refreshes readiness, synchronizes queued stages, checks disputes, applies verdicts, spawns merge resolvers, and sweeps leftovers. `wait_for_next_tick` uses the configured poll interval with 100 ms shutdown checks.

## Monitor Subsystem (orchestrator/monitor/)

Full file list:

- `core.rs` — `Monitor` struct, `poll()` API, stage/session loading
- `config.rs` — `MonitorConfig` (work_dir, hung_timeout, etc.)
- `detection.rs` — `Detection` struct: `detect_stage_changes()`, `detect_session_changes()`, `detect_heartbeat_events()`
- `events.rs` — `MonitorEvent` enum (stage/session/heartbeat event variants)
- `failure_tracking.rs` — Consecutive failure escalation logic
- `handlers.rs` — `Handlers` struct: handoff/crash-report generation; holds optional `LivenessService`
- `heartbeat.rs` — `HeartbeatWatcher` with 300s hung timeout
- `hung_latch.rs` — when a silent session's report becomes escalation evidence rather than a warning (see [Signal Generation Pipeline](signal-generation.md) § Soft Signals for the detection pipeline this drives)
- `context.rs` — `context_health(tokens, ceiling)` bands an absolute token count as a fraction of its resolved ceiling: Green `<60%`, Yellow `60-90%`, Red `>=90%` (see [Context Ceiling](context-ceiling.md))
- `tests.rs` — Unit tests

**`Monitor::poll()` flow:**

1. Load all stages from `.loom/work/stages/*.md`
2. Load all sessions from `.loom/work/sessions/*.md`
3. `detection.detect_stage_changes()` — file-level changes
4. `detection.detect_session_changes()` — PID liveness, status transitions
5. `detection.detect_heartbeat_events()` — hung detection via `HeartbeatWatcher`
6. Return `Vec<MonitorEvent>`

**LivenessService injection:** `Monitor::set_liveness(liveness: LivenessService)` is called by the
orchestrator after the shared `SessionBackend` is constructed. Backend liveness uses the lane recorded
on `Session.backend` and verified process identity; missing identity is unverifiable, not permission to
fall back to a raw PID signal.
