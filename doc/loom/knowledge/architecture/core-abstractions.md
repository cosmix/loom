# Core Abstractions

> ExecutionGraph, Stage, Session, Orchestrator, TerminalBackend — plus data flow and .work/ file ownership.

## Core Abstractions

### ExecutionGraph (plan/graph/builder.rs)

DAG of stages with dependency tracking. `get_ready()` returns stages with all deps satisfied (status == Completed AND merged == true). Cycle detection via DFS at build time.

### Stage State Machine (models/stage/)

```text
WaitingForDeps --> Queued --> Executing --> Completed
                     |            |
                     v            +--> Blocked, NeedsHandoff, WaitingForInput,
                  Skipped              MergeConflict, CompletedWithFailures, MergeBlocked,
                                       NeedsHumanReview, NeedsAdjudication
```

13 variants total, including `NeedsAdjudication`. `verified` is only a deserialization alias for
`Completed`, not a separate runtime state. Terminal states: Completed, Skipped. Transitions are
validated in transitions.rs. See [patterns.md -- State Machine Pattern](../patterns.md#state-machine-pattern).

**Documented state-machine bypasses:** Two paths intentionally bypass `try_transition`:

1. **`--force-unsafe`** (`handle_force_unsafe_completion`) — sets `Status::Completed` from any state. Manual recovery only.
2. **Phantom-merge revert** (`reconcile_main_repo_active_merge` and `complete()`'s `RevertAndSpawnResolver` arm) — flips a `Completed + merged=true` stage back to `MergeConflict + merged=false + merge_conflict=true` when an active main-repo merge is attributed to that stage. The bypass is necessary because `Completed` is terminal; `try_transition` would refuse, but this is exactly the case the bypass is designed for. All such mutations are logged at `error` level.

### StageType Enum (plan/schema/types.rs)

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, commits required (directly to main), auto merged=true, exploration focus
- **IntegrationVerify** -- Second-to-last quality gate combining code review AND functional verification
- **KnowledgeDistill** -- Final stage, runs after integration-verify, curates session memories into permanent knowledge (worktree stage; **opus default, `xhigh` reasoning effort** — every `StageType` arm returns opus, see `models/stage/types.rs::default_model`)

Signal generation has 4 stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill).

### Session Lifecycle (models/session/)

States: Spawning -> Running -> Completed | Crashed | ContextExhausted | Paused. Tracks PID, terminal window ID, absolute `context_tokens` plus `transcript_path` (not a usage percentage — the `context_limit` field was deleted, so there is no denominator stored per-session; the ceiling is resolved per stage, see architecture.md "Context Budget Enforcement"), timestamps.

### SessionBackend (orchestrator/terminal/)

`SessionBackend` is the shared dispatcher for every spawn, kill, and liveness operation. It selects
the native host-terminal lane or the opt-in tmux lane per spawn and persists the lane on
`Session.backend`. Later kill and liveness calls dispatch by that recorded value, so configuration
changes and daemon restarts cannot route an existing session through the wrong backend.

`LivenessService` wraps the same shared `Arc<SessionBackend>`. Process checks use verified PID plus
start-time identity; a missing or mismatched identity fails closed instead of falling back to raw
`kill -0` signaling.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md
   --> Parse plan, create .work/, write stage files

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Create worktree + signal --> Spawn session --> Monitor via LivenessService

4. Agent reads signal, executes, runs: loom stage complete <id>

5. Progressive merge into main branch (dependency order)
```

### IPC Protocol (`daemon/protocol.rs`, `daemon/wire.rs`)

Unix socket at `.work/orchestrator.sock`. A fixed capability-and-credential preface is authenticated
before the length-prefixed JSON body is allocated. Requests are capped at 64 KiB, responses at
2 MiB, and absolute read deadlines plus bounded workers, queue slots, subscriber counts, and
in-flight bytes prevent slow or oversized clients from exhausting the daemon. User requests cover
status/log subscriptions, Ping, Unsubscribe, and DisputeCriteria; Stop requires a one-time operator
proof.

## File Ownership

| Directory             | Owner Module                     | Purpose              |
| --------------------- | -------------------------------- | -------------------- |
| `.work/stages/`       | orchestrator/core/persistence.rs | Stage state          |
| `.work/sessions/`     | orchestrator/core/persistence.rs | Session state        |
| `.work/signals/`      | orchestrator/signals/            | Agent assignments    |
| `.work/handoffs/`     | orchestrator/continuation/       | Context dumps        |
| `.work/config.toml`   | commands/init/, commands/run/    | Plan reference       |
| `.worktrees/`         | git/worktree/                    | Isolated workspaces  |
| `doc/loom/knowledge/` | fs/knowledge.rs                  | Persistent learnings |
