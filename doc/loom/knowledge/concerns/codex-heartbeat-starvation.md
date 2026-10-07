# Codex Heartbeat Starvation

> Heartbeat starvation, stall limits

## Long Codex Runs Starve the Loom Heartbeat

A foreground codex-lane run (`loom-codex-forwarder`) is ONE Bash tool call that blocks until codex
returns, with no subagent underneath it. The session heartbeat
(`.loom/work/heartbeat/<stage-id>.json`) is refreshed only by shell hooks:
`loom-hooks/session-start.sh` (initial), `loom-hooks/post-tool-use.sh` (after every tool use) and
`loom-hooks/subagent-stop.sh` (after every `SubagentStop`, with `activity: "subagent <agentId>
finished"`, which covers a parent blocked on Task-tool subagents). Neither PostToolUse nor
SubagentStop can fire until the Bash call returns, so a pure-codex run longer than the stage's
budget leaves the heartbeat stale while the stage is healthy.

Budget: `DEFAULT_HUNG_TIMEOUT_SECS = 300` (`monitor/heartbeat.rs`), overridable per stage with
`subagent_timeout_secs` through `Stage::effective_subagent_timeout_secs()`.
`MonitorConfig::hung_timeout` is only the fallback for a session whose stage cannot be resolved by
id.

**A stale heartbeat is acted on, not only printed.** `MonitorEvent::SessionHung` reaches
`on_session_hung` (`orchestrator/core/event_handler/recover_hung.rs`); the rules are in
[Soft Signals](../architecture/signal-generation.md#soft-signals). For this lane they mean:

- a stage that had worked and then blocks on one codex call for 3x its budget is handed off and
  re-queued, up to `MAX_STALL_RECOVERIES` (2) times, then parked in `NeedsHumanReview`;
- a stage whose FIRST tool call is a long foreground subagent or codex run has no post-tool
  heartbeat yet and `context_tokens` is still 0, so it counts as never worked and is parked and
  killed at ONE budget (300 s by default), with no recovery. A reviewer flagged it
  (`orchestrator/monitor/never_worked.rs`); stages that open with a long call need a raised
  `subagent_timeout_secs`.

**Mitigation is doctrine, not a monitor change.** Keep each codex task bounded, and set
`subagent_timeout_secs` on stages that legitimately block for longer. The loom-orchestration
skill's Rule 6 ("Checking on subagents") routes the check through the one-background-watch
pattern: it blocks until every subagent settles or the timeout fires, exits 0 vs. 2 and states
which branch fired. The orchestrator keeps waiting while a subagent reports `tool-wait` or
`generating`: takeover or re-assignment needs positive evidence of death (idle past the budget with
NO transcript growth), never elapsed time alone. `loom subagents list` and `harvest` report
per-subagent state (`done`, `tool-wait`, `generating`, `unknown`) read from each subagent's own
transcript, so that evidence is not a judgment call made from silence.

**Deliberately OUT OF SCOPE: raising `MonitorConfig::hung_timeout`.** A global raise would blind the
monitor to genuinely dead sessions on every other stage in order to quiet one lane, and the
per-stage override already covers the real case.

## `loom status` "Stale" Badge Is Not Stage-Aware

Two independent 300s constants with different consumers:

- **detection** — `orchestrator::monitor::heartbeat::DEFAULT_HUNG_TIMEOUT_SECS`
  (`monitor/heartbeat.rs`), per-stage overridable via `subagent_timeout_secs`.
- **display** — `models::constants::STALENESS_THRESHOLD_SECS` (`models/constants.rs`), used by
  `commands/status/data/heartbeat_facts.rs` and `commands/status/render/activity.rs`. The display
  threshold stays a fixed 300 s (`(stale)`).

`subagent_timeout_secs` reroutes only the detection one. A stage with `subagent_timeout_secs: 900`
stays healthy to the orchestrator until 900s but renders `Stale` / "session may be hung" in
`loom status` from 301s — which can push an operator into intervening on a healthy stage.

Flagged twice (implementation, then confirmed by integration-verify) and NOT fixed on purpose:
`determine_activity_status(session, staleness_secs)` takes no `Stage`, so making it stage-aware
means threading the effective timeout through that call site AND the render path — a status
subsystem change with its own test surface, unrelated to the codex lane. Fix if picked up later:
pass `Stage::effective_subagent_timeout_secs()` into `determine_activity_status` and the activity
renderer instead of the constant.
