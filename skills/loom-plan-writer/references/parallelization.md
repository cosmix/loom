# Hierarchies, Teams, Ultracode

Read when: a stage needs more than about six workers, inter-agent messaging, or a Workflow fan-out.

- **2-level hierarchy** (main → coordinators → workers; workers NEVER spawn subagents) — for >~6 well-defined tasks in 2–4 DISJOINT file territories. Use an `EXECUTION PLAN - HIERARCHICAL` table (`SKILL.md` Section 5's `Worker | Role | Tier | Files owned | Shared context | Brief path` format, one row per coordinator and per nested worker), each worker's brief written to `doc/plans/briefs/<plan-slug>/<stage-id>/<worker>.md`, an OPTIONAL per-coordinator `Verify:` line — AT MOST ONE narrowly-scoped check over the files that coordinator's workers wrote, run ONCE, skipped if the coordinator is unsure; it is not a substitute for real verification, which stays the stage's main agent's job (full compile/test/lint) — plus the statements "Territories are DISJOINT" and "Workers NEVER spawn subagents." Coordinator and worker model follows BLOCK-B (haiku for mechanical edits such as a rename or a config value; codex luna for boilerplate, scaffolding, and simple unit tests; sonnet or codex terra for common implementation and integration tests; opus for mainstream architecture and algorithm implementation; fable only for visual/UI design, a bug that survived a delegated fix attempt, or extremely challenging algorithmic design) picked per task — not a blanket sonnet default that skips that judgment call. Spawn workers BY AGENT TYPE or an untyped worker inherits the stage's own main model. On a larger or harder territory, an opus coordinator orchestrating sonnet or codex workers is a common shape (judgment at the seam, cheap execution at the leaves), chosen per task rather than by rote. Mechanics/preambles: the `loom-orchestration` skill, `## Rule 6 — Subagents`.
- **Ultracode** (`ultracode: true`) — licenses the stage's session for Workflow orchestration: scripted fan-out/verify over tens of agents inside ONE session, zero cross-stage merges. Reach for it whenever a stage — or a would-be GROUP of sibling stages — matches any of: ≳10 homogeneous work units (files to migrate, modules to audit, endpoints to cover); breadth-first exploration or research whose total coverage exceeds one context window; a high-stakes verification gate wanting multi-perspective adversarial review (N independent skeptics / judge panels, not one reviewer); or generating competing implementations and selecting the best. Check every candidate group of parallel stages against this list before defaulting to more stages — don't wait for it to become obvious.
- **Stage-collapse rule.** Prefer ONE ultracode stage over 3+ parallel sibling stages that perform the SAME operation on different file sets — every extra stage costs a worktree, a session spin-up, a branch merge, and merge-conflict risk with its siblings, where a Workflow runs the identical fan-out inside one session with no cross-stage merge at all. Heuristic: siblings differing only in WHICH files they touch → collapse into one ultracode stage; siblings differing in WHAT they do → keep them as separate stages.
- **Cost/latency discipline — stay judicious.** Multi-agent orchestration runs roughly an order of magnitude more tokens than a single session (published measurements land around 15×), and wall-clock stretches once fan-out queues past the runtime's concurrency ceiling (~16 agents run concurrently; the rest wait). License it PER STAGE with the existing MANDATORY one-sentence justification in the description — never as a plan-wide default. Do NOT ultracode ordinary implementation, small scope (below ~10 units), or tightly coupled/sequential work — multi-agent measurably underperforms on tightly interdependent coding. Run the Workflow's worker agents at the cheapest adequate tier (sonnet) so the multiplier lands on the cheap rate, not the expensive one.
- **Claude-only.** Ultracode Workflow fan-out spawns CLAUDE subagents only — the codex lane (`gpt-5.6-terra` / `gpt-6-luna`) is not addressable from inside a Workflow script. A stage licensed for both lanes uses the Workflow for its Claude-side fan-out and reaches `loom-codex-forwarder` Agent spawns outside the Workflow for codex work. Ultracode is therefore never a reason to list codex in `implementers`, nor vice versa.
- **Agent teams** — wide, exploratory scope needing inter-agent comms or dynamic task discovery (~7× whole-job cost; the `loom-orchestration` skill, `## Rule 6 — Subagents`). Don't use for concrete file-partitioned work.

## Hierarchical worker table example

**Large fan-out (>~6 workers)** — use an `EXECUTION PLAN - HIERARCHICAL` block (coordinators × workers) instead of a flat wave, so the main agent absorbs a few compact summaries instead of a dozen raw results (the `loom-orchestration` skill, `## Rule 6 — Subagents`):

```yaml
description: |
  Implement 12 endpoint handlers plus tests.
  Use parallel subagents and skills to maximize performance.
  EXECUTION PLAN - HIERARCHICAL (2-LEVEL CAP). Each brief is written to
  doc/plans/briefs/endpoints-plan/add-endpoints/<worker>.md and committed
  alongside the plan. Spawn coordinators as general-purpose with a model
  override, ALL in ONE message; each coordinator spawns its own workers BY
  AGENT TYPE, each with the fixed prompt plus "Your brief: <path>. Read it in
  full before anything else."
  This stage licenses both lanes (implementers ["codex", "claude"]). Each
  handler is one file whose signature is pinned in its brief, so A1-A3 and
  B1-B3 are codex units: coordinators spawn them as loom-codex-forwarder in
  the FOREGROUND, six at once, within the fan-out cap of 6. The test rows own
  several files and stay on Claude.

  | Worker | Role | Tier | Files owned | Shared context | Brief path |
  | ------ | ---- | ---- | ------------ | --------------- | ---------- |
  | Coordinator A | REST | sonnet | src/api/rest/** | — | doc/plans/briefs/endpoints-plan/add-endpoints/coord-a.md |
  | A1 | users.rs | codex terra | src/api/rest/users.rs | src/api/rest/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/a1-users.md |
  | A2 | orders.rs | codex terra | src/api/rest/orders.rs | src/api/rest/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/a2-orders.md |
  | A3 | billing.rs | codex terra | src/api/rest/billing.rs | src/api/rest/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/a3-billing.md |
  | A4 | REST tests | sonnet | tests/api/rest/** | src/api/rest/** (read-only) | doc/plans/briefs/endpoints-plan/add-endpoints/a4-tests.md |
  | Coordinator B | GraphQL | sonnet | src/api/graphql/** | — | doc/plans/briefs/endpoints-plan/add-endpoints/coord-b.md |
  | B1 | queries.rs | codex terra | src/api/graphql/queries.rs | src/api/graphql/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/b1-queries.md |
  | B2 | mutations.rs | codex terra | src/api/graphql/mutations.rs | src/api/graphql/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/b2-mutations.md |
  | B3 | subscriptions.rs | codex terra | src/api/graphql/subscriptions.rs | src/api/graphql/mod.rs | doc/plans/briefs/endpoints-plan/add-endpoints/b3-subscriptions.md |
  | B4 | GraphQL tests | sonnet | tests/api/graphql/** | src/api/graphql/** (read-only) | doc/plans/briefs/endpoints-plan/add-endpoints/b4-tests.md |

  Coordinator A verify (optional, ONE scoped check, skip if unsure): cargo test --test rest_api
  Coordinator B verify (optional, ONE scoped check, skip if unsure): cargo test --test graphql
  Territories are DISJOINT. Workers NEVER spawn subagents.
  Coordinators return compact summaries only. The stage's main agent runs the
  full build/test/lint gate — a coordinator's scoped check never substitutes for it.
```
