---
sources:
- loom/src/commands/subagents/wait/mod.rs
- loom/src/commands/subagents/wait/lease.rs
- loom/src/commands/subagents/wait/stall.rs
- loom/src/commands/subagents/wait/model.rs
- loom/src/commands/subagents/wait/codex_binding.rs
- loom-hooks/codex-forward-guard.sh
- loom-hooks/codex-forward.sh
- loom-hooks/subagent-start.sh
- loom-hooks/subagent-stop.sh
verified: ace1efbd8fd861fc0efcb26e8d49c2ef29876c67
---

# Orchestrator Loop

> Ticks

## Orchestrator Main-Loop Tick Sequence (Exact Call Order)

The runtime loop lives in `loom/src/orchestrator/core/run.rs`. `run_tick` executes the following sequence; `poll_run_events` then handles monitor events in automatic mode.

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
10. tick_host_jobs()
11. sweep_merged_leftovers()
12. dispute_refused_freezes()
13. hand_off_frozen_contract_phases()
14. start_ready_stages()
15. monitor.poll() → handle_events() [automatic mode]
```

The constructor and configuration live in `core/orchestrator.rs`; that file does not own the loop. Worker draining is not a separate call in this sequence. Verdicts are applied before scheduling, so a stage re-queued by a verdict can start in the same tick. Spools, inboxes, and frozen-contract handoff also run in manual mode.

`initialize_run` validates state, starts the target guard, reconciles and synchronizes stages, recovers orphaned sessions, refreshes readiness, synchronizes queued stages, checks disputes, applies verdicts, spawns merge resolvers, and sweeps leftovers. `wait_for_next_tick` uses the configured poll interval with 100 ms shutdown checks.

## Monitor Subsystem (orchestrator/monitor/)

Main files:

- `core.rs` — `Monitor` struct, `poll()` API, stage/session loading
- `config.rs` — `MonitorConfig` (work_dir, hung_timeout, etc.)
- `detection.rs` — `Detection` struct: `detect_stage_changes()`, `detect_session_changes()`, `detect_heartbeat_events()`
- `events.rs` — `MonitorEvent` enum (stage/session/heartbeat event variants)
- `crash_redelivery.rs` — delivers a reported crash again until its stage has moved on; the handler's `reported_crashes` latch makes a repeat harmless, and the waits between failed deliveries double up to a limit
- `foreground.rs`, `foreground_progress.rs`, `direct_worker.rs` — bounded foreground work evidence for stall decisions ([Heartbeat and Activity](status-data-model.md#heartbeat-and-activity))
- `handlers.rs` — `Handlers` struct: handoff/crash-report generation; holds optional `LivenessService`
- `heartbeat.rs` — `HeartbeatWatcher` with 300s hung timeout
- `hung_latch.rs` — when a silent session's report becomes escalation evidence rather than a warning (see [Signal Generation Pipeline](signal-generation.md) § Soft Signals for the detection pipeline this drives)
- `judge_watchdog.rs`, `running_command.rs` — when a silent judge is closed, and the process probe that holds a judge while it waits on a command of its own
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

## Stall Escalation During a Running Completion

A hung report escalates to recovery (re-queue, or park after `MAX_STALL_RECOVERIES`) once a session's silence reaches three times its response budget (`orchestrator/monitor/stall_line.rs::is_stall_escalation`). A zero budget only ever warns.

The pinned `loom stage complete` is one foreground Bash call, so no heartbeat fires until its last check. While the session's latest heartbeat carries `COMPLETION_ACTIVITY` ("Stage completion running", written by the PreToolUse branch of `loom-control-complete.sh`), the line becomes `completion_line_secs`:

- (acceptance + wiring_tests + dead-code check + regression test) × `DEFAULT_COMMAND_TIMEOUT` (300 s) + 1800 s, for goal-backward/contract checks, impact tests and the review gate;
- never below the normal line;
- finite, so a hung completion is still recovered.

The monitor's hung latch (`hung_latch.rs`) and the handler (`core/event_handler/recover_hung.rs`) use the same function, so the two cannot disagree. The warning says the session "is running its stage completion".

Residuals:

- A completion whose PostToolUse never fires keeps the mark until the next main-agent progress write, bounded by the line.
- `loom status` still badges such a session "Stale".

## Launching the Daemon: Re-Exec and Readiness Pipe

`loom run` never forks the daemon. `DaemonServer::start` calls `launch::spawn_daemon`
(`daemon/server/launch.rs`), which re-executes `current_exe()` as
`loom run --daemon-child <absolute work root>` plus the run's flags (`--manual`, `-p N`,
`--no-merge`) and waits for the child to report ready. The child process:

- starts from `env_clear` plus the daemon allowlist (`DaemonEnvironment`, see
  [The Host Environment Allowlist](execution-containment.md#the-host-environment-allowlist)), with
  `LOOM_TERMINAL` set from the terminal `loom run` detected, stdin on `/dev/null`, and stdout and
  stderr on one pipe; the working directory is inherited because `loom repair` matches daemons by cwd;
- is the hidden `--daemon-child` flag (refused with `--foreground`, absent from `--help`);
  `commands/run/daemon_child.rs::execute` requires an absolute root, calls
  `signing::take_from_process()`, then `DaemonServer::serve`; `main.rs` skips terminal recovery for it
  so no escape sequences reach the pipe;
- `serve` (`daemon/server/lifecycle.rs`) calls `setsid`, takes the singleton lock before any
  destructive step, removes the stale socket, publishes the pid file and tokens, keeps the pipe,
  redirects stdout and stderr to `orchestrator.log` (rotating the previous one), writes `0x02`, then
  `run_server` binds the socket and writes `0x01` only after a successful bind (restoring the umask
  first on a failed bind).

The argv `loom run --daemon-child <root>` keeps the `pgrep -af 'loom run'` singleton check valid.

## Readiness Handshake and Failure Reports

`await_ready` reads the pipe in 50 ms slices. `0x02` means output now goes to the log, `0x01` means
ready, and any other bytes are diagnostic text (a startup error before the log redirect), of which it
keeps the last 64 KiB. Success needs `0x01` and the child still alive after a 1 s grace, and not
dumping core: when the grace ends, `ready_unless_dumping` reads `CoreDumping:` from
`/proc/<pid>/status` (`launch/child.rs::core_dumping`, always false without `/proc`). A piped core
handler (apport, systemd-coredump) holds an aborted child unreapable for about a second, past the
grace, so a dumping child is waited for up to the 10 s deadline and reported as an exit. Failures
exit 1 and name what happened plus the captured text and, once `0x02` was seen, the last 20 lines of
`orchestrator.log` (passed through `terminal_safe`):

| Outcome                                       | Report                                                                                                                                            |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| child exits before `0x01` or within the grace | its exit status or signal; the pipe is drained up to EOF or 1 s (`DRAIN_LIMIT`), because a process the child started could still hold a write end |
| child still dumping core when the grace ends  | its signal once the core handler releases it, or "still dumping core" after the deadline                                                          |
| pipe closes with the child alive              | the child is SIGTERMed (then SIGKILL after 2 s) and reaped                                                                                        |
| no `0x01` by the 10 s deadline                | SIGTERM, reap, then the text and log tail                                                                                                         |
| read or `try_wait` error                      | `await_ready` terminates a still-running child on every `Err`                                                                                     |

`terminate` signals only a child `try_wait` still reports running, so a reaped child's reusable pid
is never signalled. Unit tests never start a real daemon: `spawn_daemon` is disabled under
`cfg(test)`, and an integration target that reaches it calls `disable_spawn_for_tests` first. No
automated test runs the production re-exec; see
[No Automated Test For the Daemon Re-Exec](../concerns/platform-and-commit-gaps.md#the-production-daemon-re-exec-has-no-automated-test).

## The Socket Path Rule

`daemon/socket.rs` is the one place the socket is resolved. `socket_path(work_dir)` canonicalizes
the state root (a stage worktree's `.loom/work` is a symlink to it) and falls back to the given
spelling; the daemon's own bind keeps `work_dir.join(SOCKET_FILE)` because its root is already real.
`socket_path_fits` budgets `SUN_PATH_MAX = 104` bytes with a strict `<` (the NUL needs a byte; 104 is the
macOS/BSD bound, applied on Linux too) and `socket_path_problem` words the refusal with the longest
repository path that fits. `loom run` refuses a background run whose resolved path does not fit in
`prepare_background_run`, right after `work_dir.load()` and before the plan is marked in progress, so a
refusal never leaves the plan in progress; `--foreground` binds no socket and is not refused;
`loom init` warns from `create_or_adopt_work_dir`. Clients treat a path past the limit as
`DaemonReach::Unreachable`: see [Stage-to-Daemon Channels](../patterns/stage-daemon-channels.md).

## Daemon socket exchange and subscriber framing

One-shot RPC and stop requests use `daemon/deadline_transport.rs::DeadlineStream`. It starts one monotonic deadline before nonblocking Unix connect and carries it through poll-based writes and framed reads. A timeout after connection is an uncertain exchange and is never a reason to retry a mutating request or route around a live daemon.

Live status and web subscribers use `daemon/wire.rs::ResponseFrameReader`, which retains partial prefix and body bytes across idle socket timeouts and limits both frame size and incomplete-frame lifetime. The daemon keeps one shared writer lock per connection across request replies, status and log broadcasts. Subscription acknowledgement is written before adding the connection to that subscriber list; broadcasts encode once, snapshot subscribers, write outside the list lock, and evict only failed peers.

## Never Pipe a Background `loom subagents watch`/`wait` Through `tail`

A background `loom subagents watch --worker ... | tail -20` "completes" immediately with exit 0 even while workers are still running, because the pipe's exit status is `tail`'s, not `watch`'s — a stale, unversioned PATH `loom` binary lacking a new flag (e.g. `--worker`) also fails with "unexpected argument" that a `tail`-piped background call then silently swallows. **Prevention:** never pipe a background wait through `tail` or any other command; run it bare through `run_in_background` so its own exit code reaches the harness, and check `loom subagents watch --help` against the actual PATH binary (not `loom/target/debug/loom`) before relying on a new flag — an installed binary can lag the worktree's own tree.

## Where the Boot ID Comes From

A wait lease persists its deadline under the boot it was written in (`Deadline.boot_id` in `commands/subagents/wait/model.rs`), and the engine ends a wait with "system boot changed while waiting" when the identity differs. `SystemBootClock::boot_id` calls `process::boot_id::current_boot_id`, which resolves in this order:

1. `LOOM_BOOT_ID` from the environment, when it is UUID-shaped (trimmed; anything else is ignored).
2. The OS source: `/proc/sys/kernel/random/boot_id` on Linux, `sysctl kern.bootsessionuuid` on macOS. Other platforms bail.

The daemon computes the OS value when it writes a session wrapper (`launch/host.rs` `wrapper_env`, `WrapperHostEnv.boot_id`) and the wrapper exports it as `LOOM_BOOT_ID`, because the daemon reads the source unsandboxed and a session's sandbox can deny it (the macOS sandbox's `sysctl-read` allowlist has no `kern.bootsessionuuid`). `kern.boottime` is not an acceptable identity: macOS recomputes it when the wall clock is stepped, and a value written under one source and read under the other looks like another boot. See [Hook Tests Passed Without Exercising the BSD Forms](../mistakes/hooks-shell-portability.md) for the platform-portability work that introduced this.

## Owned Subagent Waits (`loom subagents wait`/`watch`)

> One bound wait per parent session, keyed to an explicit worker set, replacing a bare-timeout background poll.

**Module:** `loom/src/commands/subagents/wait/` — `lease.rs` (one lease per parent session, dirfd-anchored `fs::safe_fs` primitives, reuses `safe_fs::flock_exclusive` (widened to `pub(crate)`) rather than a second lock implementation), `lease_fs.rs` (owner/mode checks on pre-existing scratch dirs), `engine.rs` (the poll/settle loop), `stall.rs` (hung-worker rules, 2026-09-14), `identity.rs` (`--worker claude:<agent-id>` / `--worker codex:<unit-id>` parsing), `codex_binding.rs` (the wait-only Codex binding, reusing `codex_lifecycle::ledger` readers widened to `pub(crate)` instead of a second JSONL parser), `model.rs`, `output.rs`, `mod.rs` (CLI wiring). Companion job liveness and log freshness live in `loom/src/codex_lifecycle/progress.rs`.

**Contract:** `loom subagents watch --worker claude:<agent-id> --worker codex:<unit-id> [--session <parent-uuid>] [--stall-secs <n>] --timeout <secs>` binds that worker set once, holds one lease for the calling parent session, prints one initial and one terminal record, then exits; in human mode it also writes one line per active native Codex worker saying how long ago the provider last made progress (`wait/mod.rs::emit_active_native_progress`, first poll and then at most once per `native_worker::ids::PROGRESS_EVENT_INTERVAL_SECS`, 30 s; Claude workers and legacy Codex jobs get no line; the JSON stream is unchanged, and the throttle shares the supervisor's interval constant, so changing that constant changes the cadence). It requires `LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR` (`wait/identity.rs::scope_from_environment`), so it cannot be used from an interactive session outside a stage: exit 5, and the error names the Agent tool's completion notification as the wait to use there. `loom subagents wait --receipt` has no such gate: without `LOOM_STAGE_ID` it finds the stage by scanning `subagents/*/forward-receipts.jsonl` (`forward_jobs_wait.rs::wait_stage`). The bare `--timeout <secs>`-only form with no `--worker` is REJECTED. Exit codes: `0` every bound worker has fresh correlated success evidence; `2` the wait deadline passed (NOT proof any worker died); `3` a bound worker failed or was cancelled; `4` a wait for this parent session already exists (`AlreadyWaiting` same set, `Busy` different set — only one lease per parent session); `5` worker identity or terminal evidence is unknown (never success); `6` a bound worker is hung (`WorkerOutcome::Stalled`, `EXIT_STALLED`). `list`/`harvest` stay one-shot diagnostics, never a wait substitute.

**Hung detection (`stall.rs`, `progress.rs`):** a worker whose process died or whose evidence stopped growing past the budget is `Stalled`, not `Active` until the deadline ([mistake](../mistakes/subagent-orchestration.md)). The budget is `--stall-secs`, else the stage's `subagent_timeout_secs`, else 600 s (`DEFAULT_STALL_SECS`; the session heartbeat's 300 s is too short for a thinking turn). Claude worker rules, from the transcript's last-entry timestamp via `classify::analyze_with_evidence_at_ceiling`: `Done` idle > budget (turn ended, no SubagentStop record); `Generating` idle > budget; `ToolWait` on `Agent`/`Task` never (nested spawns have no cap); `ToolWait` on `Bash` only past `max(budget, 1800 s)` — 19,007 measured Bash tool-waits included 71 past 600 s and one of 1,298 s, since a call held at a permission prompt has not started and the tool's 600 s cap does not bind it; `ToolWait` on any other tool idle > budget (the longest measured was a 121 s `WebFetch`). Companion codex rules, only while the record says `running`: `pid` gone (`kill(pid, 0)` ESRCH, or zombie) is `Failed` — definitive, the job cannot complete; newest of log mtime / `updatedAt` / `startedAt` / `createdAt` older than the budget is `Stalled`; `queued` is running. Only signal 0 is ever sent from Rust; the wrapper's cancel owns killing. Failed/Cancelled outrank Stalled; Stalled outranks all-succeeded. On 6 the orchestrator `TaskStop`s the Claude worker (a cancelled legacy codex job needs nothing; a native worker is cancelled with `loom subagents cancel`) and re-delegates the remainder in a smaller brief. Per-poll cost: `engine::codex_outcome` calls `companion_outcome_with_progress`, so a companion job is located once per poll (`loom/src/commands/subagents/wait/engine.rs::codex_outcome`, `commands/subagents/wait/stall.rs::detect`); there is no second `locate_job` call for separate stall classification.

**Heartbeat progress/observation split:** the same stage introduced `progress_at`/`activity_kind` on the heartbeat JSON so that repeated `subagents list`/`watch`/`harvest` polling does not reset the hung-detection clock — see [Monitoring Patterns](../patterns/orchestrator-daemon-loop.md#monitoring-patterns).

**Codex identity contract (worker-evidence W2):** the `PreToolUse` `codex-forward-guard.sh` mints `invocation_id` (`inv-<32 hex>`) and derives `unit_id` (`--unit-id` or `fwd-<forwarder agent id>`), appends a v2 authorization row to `subagents/<stage>/codex.jsonl`, and injects `--unit-id U --invocation-id I` through PreToolUse `updatedInput`; the wrapper exports `CODEX_COMPANION_SESSION_ID=loom.v1:<stage>:<loom session>:<unit>:<invocation>` (`:` is outside the safe-id charset, so the encoding is unambiguous) and `CLAUDE_PLUGIN_DATA=$HOME/.codex/plugin-data`; companion v1.0.6 copies that session id verbatim into `job.sessionId`. The daemon joins a companion job to its authorization row by exact decoded `sessionId` — never by time or "newest job" (never-mtime-as-identity) — and appends `CodexCompanion` Authorization+Observation records via `subagent_lifecycle::store::append_locked`. Legacy unit ids must match `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$` (native unit ids forbid dots) (leading alphanumeric — a leading `-` reads as a flag in the wrapper argv, a leading `.` carries path meaning); the guard also blocks a forwarder call outside an active loom stage (`LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR` all set and safe), since outside a stage the wrapper cannot encode `CODEX_COMPANION_SESSION_ID` and no daemon reconciles.

**Native Codex workers (`wait/native.rs`):** a `codex:<unit>` selector binds to the stage's native record (`native_worker::index::unit_state`) when one exists; otherwise the authorization-row binding above applies unchanged. Every poll then reads the record directly and never the lifecycle journal, so a corrupt journal cannot turn a native result into `Unknown`. `outcome_for` maps, first match wins: ownership `Unverified` is `Unknown`; a non-terminal phase quiet past the stall budget is `Stalled` (exit 6); other non-terminal phases and `Terminal` with ownership `Held` are `Active`; `Succeeded` needs ownership `Released`; `Cancelled`, `Failed` and `TimedOut` map to their outcomes. Design: [Native Codex Workers](codex-concurrency.md#consumer-gates).

**Codex forwarder job deadline (Legacy lane):** inside the Linux stage sandbox a companion job does not outlive its forwarder: a job dies silently when the wrapper's status wait returns, so the wrapper does not hand a job off. The deadline is 1500000 ms, with the forwarder's Bash timeout at 1560000 ms; stage sessions allow that through `BASH_MAX_TIMEOUT_MS=3600000` (`orchestrator/terminal/native/wrapper.rs`), and a live 650 s subagent Bash call under that setting ran to completion. `loom-hooks/codex-forward.sh` cancels a job still running at the deadline (`codex-companion.mjs cancel <job> --json`, which interrupts the turn and kills the process tree), prints `LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":..,"outcome":"timed_out","exit_code":124}` and `state: timed_out`, and exits 124; `ForwardState::TimedOut` (serialized `timed_out`) is terminal. The direct lane's supervisor (`_codex-direct.py`) kills at the same deadline and returns `cancelled`/124. Sizing: [Codex Concurrency](codex-concurrency.md#job-sizing). A timed-out job is never re-forwarded as is: re-split the remainder against the partial tree; a single retry keeps the same unit id, a split gives each part its own id.

**Codex unit verification limit:** `/tmp/loom-loop-checks` (and the stage TMPDIR generally) is mounted READ-ONLY inside the codex companion sandbox, so any test a codex unit runs against stage fixtures fails at setup before exercising code. Never ask a codex unit to run fixture-backed tests for its own acceptance; at most a compile-only check (`cargo check`/`cargo test --no-run`) — the orchestrator runs the unit's real module tests immediately afterward. A compile-only check cannot catch a behavioral regression (e.g. a swapped-out test fixture helper that silently changes what state a test observes) — the orchestrator's post-unit test run is what catches those, so run it right after every codex unit, not batched at the end.

## Known limitation: the deadline race window

`codex-forward.sh` reports `timed_out` whenever the 1500000 ms `status --wait` returns with the job still `queued`/`running`; it does not re-check whether the job completed in the instant between that return and its bounded `cancel` call. A job finishing inside that window is reported timed out and cancelled, though its edits stay in the tree. Deliberate: closing it needs a second non-wait `status` call, which every test stub (`loom-hooks/tests/codex-forward-wrapper.sh`, `loom/tests/codex_evidence/fake_companion.rs`) pins against. The re-split guidance remains correct at that boundary.

`loom knowledge update`/`replace-section` resolve `doc/loom/knowledge/` from the shell's working directory: run from `loom/` they scaffold a second tree under `loom/doc/` and write there. Run them from the repository root.

**Named spawns are refused.** An `Agent` spawn that passes `name` runs as a teammate whose id is
`name@session-<8hex>`. It idles instead of stopping, and `subagent-start.sh`/`subagent-stop.sh`
drop ids containing `@`, so no starts row or terminal record exists for it. `WorkerSpec::from_str`
(`commands/subagents/wait/model.rs`) rejects any `claude:` id containing `@` with "named spawns
cannot be watched: spawn the subagent without `name`, or wait for its Agent-tool completion
notice". Rule 6 of the skill and of `CLAUDE.md.template` forbids `name` on subagent spawns (agent
teams excepted).

**A companion liveness check can false-positive across the forwarder's sandbox PID namespace:** `loom subagents watch` can exit 3 claiming a codex companion job's process was "gone
while its record says running" as little as 118 s after the forward started. The low PID reported points at
the forwarder's own sandbox PID namespace being invisible to the watcher process — `kill(pid, 0)`
resolves against the wrong namespace. Treat that exit as unverified and wait for the forwarder's own
`LOOM-FORWARD-END` report before concluding the job failed. Separately, a watch naming only `codex:`
workers exits 5 ("worker set does not resolve to one Claude parent UUID") — always include the
forwarder's own `claude:<agent-id>` in the `--worker` set alongside the codex unit id.
