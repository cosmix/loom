# Core Abstractions

> Graph

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

1. **`--force-unsafe`** (`commands/stage/complete_force.rs::handle_force_unsafe_completion`) — sets `Status::Completed` from any state. Manual recovery only. Without `--assume-merged` it records the committed branch tip as `completed_commit` (`progressive_complete/operator_commit.rs::branch_work`, validated under the stage lock) and leaves the stage unmerged for the daemon's auto-merge, which re-checks zero commits under `MergeGate::Enforce`; a missing or zero-commit branch records nothing. With `--assume-merged` the router first proves the commit is in the target.
2. **Attributed active merge** (`reconcile_main_repo_active_merge` and `complete()`'s `RevertAndSpawnResolver` arm): moves a Completed, unmerged stage to `MergeConflict + merge_conflict=true` when an active main-repo merge is attributed to it. The bypass is needed because `Completed` is terminal and `try_transition` would refuse. A final stage (Completed with `merged` or `merge_assumed`) is never moved. The move is logged at `error` level.

**Transitions FROM `NeedsAdjudication`** (`transitions.rs`) — note it can loop to itself:

- `Queued` — verdict applied, stage re-queued
- `NeedsAdjudication` — evidence loop (another round on the same dispute)
- `NeedsHumanReview` — by design, a **`Reject` verdict**: the adjudicator upheld the criterion and ruled the implementation wrong, while the agent disputed it as impossible. Neither side can move, so this is the one outcome a human is needed for. Also reached when a bound is exhausted: the evidence loop (`MAX_EVIDENCE_ROUNDS`, 5), the per-stage amendment cap (`max_amendments_per_stage`, default 10), or the adjudication respawn budget (`MAX_ADJUDICATION_ATTEMPTS`, 3). See conventions.md § Adjudicator Transport Convention.

`CompletedWithFailures` also transitions into `NeedsAdjudication` (dispute filed after a failed completion) and into `NeedsHumanReview` (budget escalation).

### StageType Enum (plan/schema/types.rs)

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, commits required (directly to main), auto merged=true, exploration focus
- **IntegrationVerify** -- Second-to-last quality gate combining code review AND functional verification
- **KnowledgeDistill** -- Final stage, runs after integration-verify, curates session memories into permanent knowledge (worktree stage; **sonnet default, `high` reasoning effort** — the one `StageType` arm that does not return opus, see `models/stage/defaults.rs::default_model`; configurable per stage type via `[models]`, see conventions/model-and-effort-config.md)

Signal generation has 4 stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill).

### Session Lifecycle (models/session/)

States: Spawning -> Running -> Completed | Crashed | ContextExhausted | Paused. Tracks PID, terminal window ID, absolute `context_tokens` plus `transcript_path` (not a usage percentage — there is no `context_limit` field, so there is no denominator stored per-session; the ceiling is resolved per stage, see architecture.md "Context Budget Enforcement"), timestamps.

### SessionBackend (orchestrator/terminal/)

`SessionBackend` is the shared dispatcher for every spawn, kill, and liveness operation. It selects
the default tmux lane or the opt-in native host-terminal lane per spawn and persists the lane on
`Session.backend`. Later kill and liveness calls dispatch by that recorded value, so configuration
changes and daemon restarts cannot route an existing session through the wrong backend.

`LivenessService` wraps the same shared `Arc<SessionBackend>`. Process checks use verified PID plus
start-time identity; a missing or mismatched identity fails closed instead of falling back to raw
`kill -0` signaling.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md
   --> Parse plan, create .loom/work/, write stage files

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Create worktree + signal --> Spawn session --> Monitor via LivenessService

4. Agent reads signal, executes, runs: loom stage complete <id>

5. Progressive merge into main branch (dependency order)
```

### IPC Protocol (`daemon/protocol.rs`, `daemon/wire.rs`)

Unix socket at `.loom/work/orchestrator.sock`, dialed by every client through `daemon::socket_path` (the resolved state root; a path past `SUN_PATH_MAX`, 104 bytes, is `DaemonReach::Unreachable`). A fixed capability-and-credential preface is authenticated
before the length-prefixed JSON body is allocated. Requests are capped at 64 KiB, responses at
2 MiB, and absolute read deadlines plus bounded workers, queue slots, subscriber counts, and
in-flight bytes prevent slow or oversized clients from exhausting the daemon. User requests cover
status/log subscriptions, Ping, Unsubscribe, and DisputeCriteria; Stop requires a one-time operator
proof.

## File Ownership

| Directory                | Owner Module                     | Purpose              |
| ------------------------ | -------------------------------- | -------------------- |
| `.loom/work/stages/`     | orchestrator/core/persistence.rs | Stage state          |
| `.loom/work/sessions/`   | orchestrator/core/persistence.rs | Session state        |
| `.loom/work/signals/`    | orchestrator/signals/            | Agent assignments    |
| `.loom/work/handoffs/`   | orchestrator/continuation/       | Context dumps        |
| `.loom/work/config.toml` | commands/init/, commands/run/    | Plan reference       |
| `.worktrees/`            | git/worktree/                    | Isolated workspaces  |
| `doc/loom/knowledge/`    | fs/knowledge.rs                  | Persistent learnings |

## Layering Violations (Known Issues)

Correct dependency direction: commands/ -> orchestrator/ -> models/ (top), daemon/ / git/ / plan/ (middle), fs/ (bottom).

Upward edges in production code (test modules excluded), each checked against the tree:

- **daemon -> commands:** `daemon/server/client.rs:89` (`commands::stage::admin_proof::verify_and_consume_admin_proof`).
- **orchestrator -> commands:** `orchestrator/core/coherence.rs:41` (`commands::stage::complete::strip_privileged_env_for_runtime`) and `orchestrator/skip.rs:45` (`commands::stage::native_gate::ensure_no_native_workers`).
- **handoff -> commands:** `handoff/completion/attest.rs:14` (`commands::stage::admin_hmac::{constant_time_eq, hmac_sha256}`; the other user is `commands/stage/admin_proof.rs:6`).
- **subagent_lifecycle -> commands:** `subagent_lifecycle/claude.rs:8` imports `StartedAgentTypeIndex` through `commands::subagents::ledger`, which only re-exports `provider_data::ledger`.
- **git -> orchestrator:** `git/cleanup/orphans.rs:10` (`orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`).
- **process -> daemon:** `process/sandbox_probe/needs.rs:8` (`daemon::{socket_path_fits, SUN_PATH_MAX}`, defined in `daemon/socket.rs`). `daemon` imports `process`, so `process` is the lower layer.
- **models -> plan, verify, git, assets, fs:** `plan` at `models/dispute_criterion.rs:7` and `models/stage/{checks,types,methods}.rs`, `verify` at `models/dispute.rs:15-18`, `git` at `models/stage/merge_block.rs:8`, `assets` at `models/forward_receipt/transcript/argv.rs:24`, `fs::permissions` at `models/stage/sandbox_config.rs:161`.
- **stage_service -> daemon:** `ContractRunReport` is defined in `stage_service/mod.rs:43` and re-exported by `daemon/protocol.rs:93`; `stage_service/contracts_choices.rs:15`, `verify/contracts/*` and `host_verify/*` import it through `crate::daemon`. The edge `daemon -> stage_service` (`daemon/server/contracts.rs:18`) is a downward call.
- **handoff <-> orchestrator (cycle):** `handoff/mod.rs:28` re-exports `orchestrator::continuation` (the continuation code lives in `orchestrator` because of its spawner and signal dependencies) and `handoff/session_content.rs:21` imports `orchestrator::monitor::stage_context_tokens`; `orchestrator/continuation/context.rs:9-10`, `orchestrator/core/stage_handoff.rs:3`, `orchestrator/core/inbox_drain/complete.rs:13` and `orchestrator/monitor/completion_blockers.rs:6` import `crate::handoff`.

### Edges of `context`, `telemetry` and `process`

Checked with `rg -o 'crate::[a-z_]+' loom/src/<module>`, test files excluded:

- **`context`** imports `crate::context`, `fs`, `git`, `models`, `utils`, `yaml`, `testrun` and one `verify` edge (`context/worktree_graph.rs:27`, `verify::review::fingerprint::checkout_git`). It has no edge to `orchestrator`, `commands` or `daemon`. The `git` edges (`context/refresh/source_graph/generation.rs::working_tree`, `layer.rs`, `worktree_graph.rs`, `census/mod.rs`) list tracked files and judge tree cleanliness, and they point downward.
- **`telemetry`** imports `crate::fs`, `crate::git` and `crate::relay`, never a layer above them. The orchestrator calls into it (`orchestrator/core/stage_telemetry.rs`), never the reverse.
- **`process`** holds the env allowlist and is consumed by both `verify` and the terminal spawner without importing either. Its one upward edge is the `daemon` import listed above.

The rule to preserve: the orchestrator calls into `context`, never the other way around. A `use crate::orchestrator` appearing under `loom/src/context/` outside test files is a regression; `rg 'crate::orchestrator' loom/src/context` finds it.

(A match for `crate::external` under `context/extract/rust/tests.rs::FIXTURE` is inside a golden test fixture string, not a real import.)
