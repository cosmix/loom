# Authoring Detail — full text behind the short forms

Read when: a short form in `SKILL.md` Sections 2, 4, 5, 6, 7, 9 or 10 leaves a question open. Each heading names the `SKILL.md` section it expands.

## Sections 2 and 4 — Exploration and model selection

Follow `../SKILL.md` Sections 2 and 4. The allocation policy lives in
`../../loom-orchestration/SKILL.md#rule-7--model-allocation`; quote applicable
rules in each worker brief.

## Section 5 — Parallelization

> ⚠️ **STAGES ARE EXPENSIVE** — each creates a worktree, spawns a session, costs real time and tokens. STRONGLY prefer subagents within ONE stage over additional stages.
>
> ⚠️ **BIAS TOWARD AS FEW SUBAGENTS AS POSSIBLE.** Fewer, larger-context subagents with well-scoped disjoint file territories beat many tiny ones — every subagent spin-up costs coordination overhead and a slice of context, and a well-specified subagent can absorb more work than a narrowly-scoped one. Before fanning out, ask whether ONE subagent (or a small number, each owning a whole disjoint territory) can do it; split further only when a territory is a whole separate job or a single subagent's assignment would blow its own context budget. Group small tasks into ONE subagent, never one subagent per task or per file — four files with a one-line edit each is ONE subagent. The `>~6 worker tasks?` column below counts subagents after grouping, never raw tasks.

Classic mistakes:

- 4 stages each editing an independent config file → 1 stage with as few subagents as the file territories require.
- A cohesive feature split BY LAYER (schema / runtime / doctrine, or model / service / controller) because each layer imports the one before it. Every one of those is a compile-order dependency, so they all answer NO to Q1: one stage, a foundation step for the shared contract, then parallel subagents over disjoint files. This is the most common fragmentation there is, because "B imports A" feels like a stage boundary when it is only a compile ordering.

### Subagent file exclusivity (CRITICAL)

- Each subagent MUST have EXCLUSIVE write access to its files — **two subagents writing one file = LOST WORK.** Include a file-ownership table in the stage description.
- **File-exclusivity is necessary but NOT sufficient — check TYPE/import dependencies too.** If subagent A's file DEFINES a type/signature/API that subagent B's file imports, running them in parallel is a race even with disjoint WRITE sets (B compiles against a contract A hasn't written). Assign the shared type/signature/API to a FOUNDATION worker that finishes BEFORE the consumer subagents fan out; the main agent writes it only when it passes orchestration Rule 7's small-change test.

### Briefs as files

Each worker's brief is written to `doc/plans/briefs/<plan-slug>/<stage-id>/<worker>.md` and committed
alongside the plan — not pasted inline into the stage description. The stage description then carries
a TABLE, not prose, naming every worker:

`Worker | Role | Tier | Files owned | Shared context | Brief path`

The rules the old prose `EXECUTION PLAN` block carried still apply and belong in the prose around the
table: territories are DISJOINT — no two rows share a write path; workers NEVER spawn subagents; the
orchestrator spawns every worker BY AGENT TYPE, ALL in ONE message. Each spawn gets a short fixed
prompt plus the line `Your brief: <path>. Read it in full before anything else.` — the full task detail
(exact paths, signatures, patterns to match, explicit steps, acceptance) lives in the brief file, not
in the prompt or the table. Pick each row's tier and write its brief by `SKILL.md` Section 4's lowest-tier,
fullest-brief rule.

```yaml
description: |
  Implement auth, logging, and metrics modules.
  Use parallel subagents and skills to maximize performance.
  Territories below are DISJOINT. Workers NEVER spawn subagents. Spawn every
  worker BY AGENT TYPE, ALL in ONE message, each with the fixed prompt plus
  "Your brief: <path>. Read it in full before anything else."

  | Worker | Role     | Tier   | Files owned      | Shared context            | Brief path |
  | ------ | -------- | ------ | ---------------- | -------------------------- | ---------- |
  | W1     | Auth     | sonnet | src/auth/*.rs     | src/config.rs (read-only)  | doc/plans/briefs/add-modules/add-auth-logging-metrics/w1-auth.md |
  | W2     | Logging  | sonnet | src/logging/*.rs  | src/config.rs (read-only)  | doc/plans/briefs/add-modules/add-auth-logging-metrics/w2-logging.md |
  | W3     | Metrics  | sonnet | src/metrics/*.rs  | src/config.rs (read-only)  | doc/plans/briefs/add-modules/add-auth-logging-metrics/w3-metrics.md |
```

For each Codex job, forward ONE line naming its absolute brief path; the forwarder never
retypes the brief. Without a committed brief, write one under the worktree scratch directory.
The brief names owned files, entry symbols, done condition and traps, and quotes applicable
declared-skill rules: Codex opens no skill files. Keep a concern's code, consumers and tests
together in dependency order, spanning several files where cohesive. Waves follow dependencies
on another job's result on disk; never slice jobs into waves by the cap of 6.

Match agent type to work: execution → `loom-software-engineer` (pins sonnet), `loom-mechanical-engineer` for mechanical pieces, or `loom-codex-forwarder` for a codex unit on a stage licensed for codex (`codex-implementers.md`); judgment → `loom-senior-software-engineer`.

## Section 6 — Prose promises

**⛔ Prose promises MUST land in the YAML — a deliverable named only in prose is built by NOBODY.** Loom's gates see only `acceptance`/`artifacts`/`wiring`/`wiring_tests`; a capability that lives in the overview alone lets every stage complete green while the promise is never written. (Logged: a plan called an uploader "load-bearing" in prose, assigned it to no stage, closed green — and stalled its consumer plan at zero code.) Mechanics:

- **Write the overview LAST, derived from the stage graph** — never the reverse. Prose describing work no stage owns is the single highest-value thing to lint for.
- For every capability the prose names ("ships X", "exposes Y", a public-contract section), grep your OWN plan: the symbol must appear in exactly ONE stage's `artifacts:` AND be proven by a `wiring:` pattern or behavioral `acceptance` entry. Zero YAML hits outside the prose = plan defect.
- **Stage completion is not interface completion.** "All stages Completed" implies "all promised symbols exist" ONLY if each promised export is encoded as an artifact plus a consumer-side proof — encode it.
- If a stage's acceptance can only be met by editing file X, X belongs in that stage's `files:` — a read-only list that excludes the seam converts a 3-line edit into a blocker.

## Section 7 — working_dir and memory routing

`EXECUTION_PATH = WORKTREE_ROOT / working_dir`. ALL paths — `acceptance`, `artifacts`, `wiring.source` — resolve relative to it. Imagine you `cd`-ed into `EXECUTION_PATH` first.

**Pre-flight (answer before writing any acceptance criterion):** (Q1) what is `working_dir`? (Q2) do the build files exist at that path — if `working_dir: "loom"`, `Cargo.toml` must be at `loom/`? (Q3) are all my paths relative to `working_dir`, not repo root?

```yaml
- id: build-check
  working_dir: "loom"          # Cargo.toml lives in loom/
  acceptance:
    - "cargo test --lib feature::"     # scoped to this stage's own module
    - "./target/debug/myapp --help"    # ✅  (or bare "myapp --help" if on PATH)
  artifacts: ["src/feature.rs"]        # ✅ resolves to loom/src/feature.rs
  # ❌ "loom/src/feature.rs" would become loom/loom/src/feature.rs
```

Common symptoms: `could not find Cargo.toml` → `working_dir` wrong; double-path `loom/loom/...` → drop the redundant prefix; `rg` finds nothing → searching from the wrong dir. **Mixed directories? Separate stages — one working_dir each.**

For memory routing, use `../SKILL.md` Section 7 and quote applicable instructions
in worker briefs.

## Section 9 — Silent-failure awareness

Follow the failure, dispute and block rules in `../SKILL.md` Section 9.

## Section 10 — Merge vs. separate, sequential stages

**Merge vs. separate stages** — independent file changes belong in ONE stage with parallel subagents (worktree + session + merge ×1), NOT one stage each (×N cost, N merges, conflict risk). Separate stages only when the Stage Necessity Test (`SKILL.md` Section 5) forces it: a merge-order dependency, file overlap, a named verification checkpoint, or a context-budget overflow. A compile-order dependency is a foundation step, not a stage boundary.

**Sequential stages when files overlap** — two edits to the SAME file can't run in parallel; chain them with `dependencies` so loom serializes the worktrees (no merge conflict):

```yaml
- id: add-auth-to-handler
  dependencies: ["knowledge-bootstrap"]
  files: ["src/api/handler.rs"]
  wiring:
    - source: "src/api/handler.rs"
      pattern: "auth_middleware"
      description: "Auth middleware applied to handler"
- id: add-logging-to-handler
  dependencies: ["add-auth-to-handler"]   # sequential — same file
  files: ["src/api/handler.rs"]
  wiring:
    - source: "src/api/handler.rs"
      pattern: "log_request"
      description: "Request logging added to handler"
```
