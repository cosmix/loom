# Spurious waiting-for-input stages

> Stages flipped to waiting-for-input with no AskUserQuestion

## What happened

Two stages of PLAN-loop-recovery (`worker-evidence`, 2026-09-13T22:10:52Z; `completion-recovery`, 2026-09-14T11:23:23Z) went `Executing -> WaitingForInput` while their sessions kept running tools for hours. `loom status` showed `?` with the resume hint; the completion broker refused `loom stage complete` with `stage is not executing` (`daemon/server/control_complete.rs`, `validate_active_identity`). Both flips landed a few seconds after the main agent's turn ended with a Stop hook, while a freshly spawned `loom-codex-forwarder` subagent was in its first model turn.

## Why

- The only writer of that status is `loom stage waiting <id>` (`commands/stage/state.rs`, `waiting`), and its only caller is `loom-hooks/ask-user-pre.sh`, registered as `PreToolUse` with matcher `AskUserQuestion`.
- Neither hook read its stdin, so any invocation of the AskUserQuestion permission pipeline flipped the stage, even one no tool_use ever recorded. Claude Code 2.1.270 drives that pipeline from internal paths without a tool call (a `confirmWithUser` helper used for its model-switch dialog, and a plugin `ui.ask` bridge); the concrete trigger in these two runs left no trace in any transcript, hook event log, or daemon log.
- Nothing reconciled the state afterwards. The daemon only mirrors disk to graph for `WaitingForInput` (`orchestrator/core/recovery.rs`, `sync_graph_with_stage_files`); the PostToolUse hook that would have resumed the stage never fires when there was no tool execution.

## Prevention

- A status that claims the session is idle must be checked against the session's own progress. A stage in `WaitingForInput` whose heartbeat shows the main agent executed a tool after `updated_at` (same session id, `subagent: false`, last tool not `AskUserQuestion`) is not waiting; the monitor moves it back to `Executing`. Subagent heartbeats never count: a genuine question blocks the main agent while its background subagents keep writing heartbeats for the same stage, so the heartbeat now records `subagent` (`orchestrator/monitor/input_wait.rs`, called from `Monitor::poll`).
- Hooks that mutate stage state must parse their stdin and act only on the event and tool they were registered for; `ask-user-pre.sh`/`ask-user-post.sh` now exit early on any other `tool_name`/`hook_event_name` and append an `AskUserQuestion` line to `.loom/work/hooks/events.jsonl` (phase, agent_id, Claude session id, tool_use_id, `tool_input.metadata.source`) so the next spurious trigger is attributable.
- Diagnosing this class: `rg -n 'is waiting for user input' .loom/work/orchestrator.log*` gives the tick; the stage file's `updated_at` gives the write; `rg -l 'PreToolUse:AskUserQuestion' ~/.claude/projects/<project>/` says whether a real question fired. Absence of that hook event with the status present is this bug.

## Fix

- `orchestrator/monitor/input_wait.rs` + wiring in `orchestrator/monitor/core.rs` (`Monitor::poll`), tests in `orchestrator/monitor/tests/input_wait.rs`.
- `loom-hooks/ask-user-pre.sh`, `loom-hooks/ask-user-post.sh`, regression test `loom-hooks/tests/ask-user-hooks.sh`.
- Operator workaround for a stuck stage: `loom stage resume <stage-id>`; the running session then finishes normally. This CANNOT be run from inside the stuck stage's own session — `.loom/work/stages` is read-only there (EROFS on the `.tmp` write) and the daemon control socket is unreachable — it must run from outside the sandbox.

## A subagent's stop heartbeat resumed a real wait (2026-09-16)

**What happened:** In an integration-verify stage, the main agent spawned a background subagent at 13:23:24, asked the user a question at 13:23:47 (`loom stage waiting` fired correctly), and the subagent's `SubagentStop` heartbeat at 13:25:19 made the reconciler resume the stage to `Executing`; the dashboard read "working" for 45 minutes while the agent was actually blocked on the question.

**Why:** `loom-hooks/_lifecycle.sh` `loom_lifecycle_refresh_heartbeat` writes heartbeats for `SubagentStop` (`loom-hooks/subagent-stop.sh`) and `TeammateIdle` events with no `subagent` field and `last_tool:null`; `Heartbeat`'s `#[serde(default)] pub subagent: bool` deserializes the missing field as `false`, so `orchestrator/monitor/input_wait.rs`'s `stale_wait_progress` read it as main-agent progress newer than the wait's `updated_at`.

**Prevention:** every writer of a heartbeat file must set `subagent` explicitly, and the reconciler treats only a main-agent heartbeat that names its tool as proof of progress; `rg -n 'heartbeat' loom-hooks/*.sh -l` enumerates the writers.

**Fix:** `loom_lifecycle_refresh_heartbeat` now writes `subagent:true`, and `stale_wait_progress` returns `None` for any heartbeat with `last_tool == None` before its `AskUserQuestion` check; regression coverage in `orchestrator/monitor/tests/input_wait.rs` and `loom-hooks/tests/subagent-stop-heartbeat-subagent-flag.sh`.
