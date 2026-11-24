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

Upward edges in production code (test modules excluded), each checked against the tree. The HMAC helpers live in `utils/hmac.rs`, so `handoff` and `commands` share them without an edge between the two; no `git` module imports `orchestrator`, and `subagent_lifecycle` imports no `commands`:

- **daemon -> commands:** `daemon/server/client.rs:89` (`commands::stage::admin_proof::verify_and_consume_admin_proof`).
- **orchestrator -> commands:** `orchestrator/core/coherence.rs:41` (`commands::stage::complete::strip_privileged_env_for_runtime`) and `orchestrator/skip.rs:45` (`commands::stage::native_gate::ensure_no_native_workers`).
- **process -> daemon:** `process/sandbox_probe/needs.rs:8` (`daemon::{socket_path_fits, SUN_PATH_MAX}`, defined in `daemon/socket.rs`). `daemon` imports `process`, so `process` is the lower layer.
- **models -> plan, verify, git, assets, fs:** `plan` at `models/dispute_criterion.rs:7` and `models/stage/{checks,types,methods}.rs`, `verify` at `models/dispute.rs:15-18`, `git` at `models/stage/merge_block.rs:8`, `assets` at `models/forward_receipt/transcript/argv.rs:24`, `fs::permissions` at `models/stage/sandbox_config.rs:161`.
- **verify, host_verify -> daemon:** `ContractRunReport` is defined in `stage_service/mod.rs:43` and re-exported by `daemon/protocol.rs`; `verify/contracts/{store,freeze,freeze/prepared}.rs` and `host_verify/{freeze,freeze_format,freeze_memo}.rs` import it through `crate::daemon`, and `stage_service` itself imports it from its own module. The edge `daemon -> stage_service` (`daemon/server/contracts.rs:18`) is a downward call.
- **handoff <-> orchestrator (cycle):** `handoff/mod.rs:28` re-exports `orchestrator::continuation` (the continuation code lives in `orchestrator` because of its spawner and signal dependencies) and `handoff/session_content.rs:21` imports `orchestrator::monitor::stage_context_tokens`; `orchestrator/continuation/context.rs:9-10`, `orchestrator/core/stage_handoff.rs:3`, `orchestrator/core/inbox_drain/complete.rs:13` and `orchestrator/monitor/completion_blockers.rs:6` import `crate::handoff`.

### Edges of `context`, `telemetry` and `process`

Checked with `rg -o 'crate::[a-z_]+' loom/src/<module>`, test files excluded:

- **`context`** imports `crate::context`, `fs`, `git`, `models`, `utils`, `yaml`, `testrun` and one `verify` edge (`context/worktree_graph.rs:27`, `verify::review::fingerprint::checkout_git`). It has no edge to `orchestrator`, `commands` or `daemon`. The `git` edges (`context/refresh/source_graph/generation.rs::working_tree`, `layer.rs`, `worktree_graph.rs`, `census/mod.rs`) list tracked files and judge tree cleanliness, and they point downward.
- **`telemetry`** imports `crate::fs`, `crate::git` and `crate::relay`, never a layer above them. The orchestrator calls into it (`orchestrator/core/stage_telemetry.rs`), never the reverse.
- **`process`** holds the env allowlist and is consumed by both `verify` and the terminal spawner without importing either. Its one upward edge is the `daemon` import listed above.

The rule to preserve: the orchestrator calls into `context`, never the other way around. A `use crate::orchestrator` appearing under `loom/src/context/` outside test files is a regression; `rg 'crate::orchestrator' loom/src/context` finds it.

(A match for `crate::external` under `context/extract/rust/tests.rs::FIXTURE` is inside a golden test fixture string, not a real import.)

## The Seam

Before this, `UserConfig::value_of` (`loom/src/user_config/render.rs::value_of`) returned `(String,
Origin)` for every key regardless of kind, and each downstream surface re-derived the type it was
never sent (regex/`==` string comparisons in the TUI and the TS client). `ConfigValue`
(`loom/src/user_config/value.rs::ConfigValue`) closes that gap:

```rust
#[serde(untagged)]
pub enum ConfigValue { Bool(bool), Number(u32), Text(String) }
```

`#[serde(untagged)]` makes the JSON wire native — `Bool(true)` serializes as `true`, `Number(24)` as
`24`, `Text("opus")` as `"opus"` — and the declaration order matters: untagged deserialization tries
variants in order, so `Bool` must be tried before `Number` or a JSON `true` risks matching the wrong
arm first.

`ValueKind` (`loom/src/user_config/keys.rs::ValueKind`) is the TOML _shape_ a `KeySpec` accepts —
`Bool`, `Number`, `Enum(&'static [&'static str])`, `String` (free text) — one level more specific
than `ConfigValue`'s three variants: an `Enum` and a free `String` both carry as `ConfigValue::Text`,
distinguished only by which `ValueKind` they're checked against.

`value_of` now returns `(ConfigValue, Origin)`, built from `KeySpec::parse` (`user_config/keys.rs::parse`) on
the write path and from `ConfigValue::checked` (see below) on the read path, so both paths share one
typed representation.

## `ConfigValue::checked` — the Mismatch Contract

`checked(self, kind: &ValueKind, name: &str) -> Result<Self>` (`user_config/value.rs::checked`) validates a value
against the kind a caller expects (`Bool`↔`Bool`, `Number`↔`Number`, `Text`↔`Enum`/`String`, `Enum`
additionally requiring the text be a listed variant). **On a mismatch it always errors — never
coerces**, even when the value's `Display` rendering would happen to reparse as `kind` (e.g.
`Text("false")` checked against `Bool` is rejected, not silently accepted as `Bool(false)`). For
`Bool`/`Number`/`Enum` the error reproduces the exact wording `ConfigValue::parse` would have
produced for that `kind` given the value's `Display` text — formatted via `{:?}` (debug-quoted) so a
dashboard client's `"abc"` and a CLI operator's typed `abc` report identically. `String` has no
`parse` failure to match, so `string_error` is its own message. A test asserting reject-on-mismatch
needs a value whose TEXT would still parse for the target kind (e.g. `Text("false")` vs `Bool`) —
one that never reparses proves nothing about coercion.

## Threading Through the Four Downstream Surfaces

- **CLI** (`loom/src/commands/config/mod.rs::print_key`): calls `config.value_of(spec)`, `ConfigValue`
  inferred from the tuple return — a wiring grep for the literal `ConfigValue` in this file will
  never match (see [pinned-literals-ledgers-and-wiring](../mistakes/pinned-literals-ledgers-and-wiring.md)).
- **TUI** (`loom/src/commands/config/tui/`): edits per-kind — arrows/space cycle an `Enum`/`Bool`,
  `Enter` opens a text editor only for `Number`/`String`. `commands/config/tui/render/table.rs::value_cell` renders
  `ConfigValue::Text` straight into a ratatui `Cell` with **no control-character stripping** — inert
  today because no `KEYS` entry uses `ValueKind::String` and the TUI only ever edits the operator's
  own file; strip control characters before the first `String`-kind key ships, or accept the risk
  explicitly (see [concerns.md](../concerns.md)).
- **Web API** (`config_api/`): `POST /api/config` carries native JSON — `true`, `800000`, `"opus"` —
  not a wrapped string. `configValueSchema` on the TS side (`web/src/api/config.ts`) stays
  kind-agnostic (`z.union` of boolean/number/string for every entry) rather than refining per
  `kind.type`; the server is the sole validator, and `formatValue` deliberately tolerates a
  mismatch. This means an out-of-range or malformed JSON body (a `Number` field as `-1` or `1.5`)
  fails ALL three untagged variants before any registry validator runs, producing serde's generic
  "did not match any variant" instead of the registry's friendly range message — accepted as a wire
  limitation of `#[serde(untagged)]`, not fixed by this plan.
- **React** (`web/src/components/settings-*.tsx`): `EnumSelect` keeps a code path that synthesizes a
  display option for a current value that is off the registry's variant list, even though the
  `ConfigValue::Enum` invariant makes that path unreachable today — kept deliberately as a guard
  against a future registry dropping a variant (`web/src/components/settings-control.tsx::EnumSelect`).

## Test Coverage Gap

`ConfigState`'s fields are private to `tui::state`, unreachable from `tui::tests::cycling` as a
sibling module, and no `KEYS` entry has kind `String` — so nothing in the TUI test suite can build a
`ValueKind::String` row and drive `ConfigState::cycle` on it. `String`'s refuse-to-cycle behavior is
covered only indirectly: sharing the `Number` arm in `cycle`, plus a `ConfigTui::opens_editor(&ValueKind::String)`
assertion. Closing this gap needs either a registered `String` key or a `pub(super)` test
constructor for `ConfigState`.

## TS Fixture Coupling

`web/src/components/settings-model.test.ts` is fixture-driven, not inline: it imports
`@/api/fixtures/config.json` and calls `configResponseSchema.parse(fixtureJson)` at module scope, so
every `sectionRows`/`laneState`/`filterSections` test runs off that one real fixture. Any change to
`configKindSchema` in `web/src/api/config.ts` throws at import time unless the fixture is
regenerated in the same change.
