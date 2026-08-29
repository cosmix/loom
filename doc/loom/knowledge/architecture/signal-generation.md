# Signal Generation

> How a stage signal is assembled: stable-prefix cache, shared append_* helpers, per-stage-type prefixes, soft signals.

## Signal Generation Pipeline (orchestrator/signals/) [DETAILED]

The signal system assembles agent prompt files in a **4-section Manus KV-cache pattern** for token efficiency.

### Call Hierarchy

```text
generate_signal_with_skills() [generate.rs]
  └─ build_signal_context()           # assembles EmbeddedContext
       └─ build_embedded_context_with_stage_and_session()
            ├─ reads handoff (V1 prose / V2 structured)
            ├─ read_plan_overview()
            ├─ KnowledgeDir::has_content()
            └─ format_memory_for_signal(last 10 entries only)
  └─ format_signal_content() [format/mod.rs]
       └─ format_signal_with_metrics()
            ├─ select stable prefix from cache.rs (by stage type)
            ├─ format_semi_stable_section() [sections.rs:15]
            ├─ format_dynamic_section() [sections.rs:382]
            ├─ format_recitation_section() [sections.rs:665]
            └─ SignalMetrics::from_sections() → SHA-256 hash first 16 hex chars
```

Knowledge stages use a SEPARATE path: `generate_knowledge_signal()` [knowledge.rs:23].

### Four Stable-Prefix Generators (cache.rs)

All generators are composed from shared `append_*` helpers and produce immutable KV-cached text.

| Generator          | Function                                      | Line | Stage Type                     |
| ------------------ | --------------------------------------------- | ---- | ------------------------------ |
| Standard           | `generate_stable_prefix()`                    | 225  | `StageType::Standard`          |
| Integration-Verify | `generate_integration_verify_stable_prefix()` | 360  | `StageType::IntegrationVerify` |
| Knowledge-Distill  | `generate_knowledge_distill_stable_prefix()`  | 510  | `StageType::KnowledgeDistill`  |
| Knowledge          | `generate_knowledge_stable_prefix()`          | 658  | `StageType::Knowledge`         |

**Standard prefix section order (approx lines 225-355):**

1. Worktree Context header
2. Isolation Boundaries (3 bullets)
3. `append_path_boundaries()` — ALLOWED/FORBIDDEN paths table
4. working_dir reminder
5. Execution Rules header
6. Worktree Isolation detail
7. Delegation & Efficiency (subagents + hierarchies + agent teams)
8. `append_subagent_restrictions()` — NO commit/complete/add-A rules
9. When to Commit block (`append_commit_timing_rules()`, `signals/helpers.rs`) — commits are the ORCHESTRATOR's, ONLY at the end, after every subagent has returned and the gate is green
10. `append_completion_rules()`
11. `append_adversarial_review()` — Mini Adversarial Code Review (6 dimensions)
12. Dedicated Silent Failure Check block (Standard only; IV has its own section)
13. Stage Memory guidance
14. `append_git_staging_full()` (Standard ONLY; IV/KnowledgeDistill use `append_git_staging_rules()`; both now live in `signals/helpers.rs`)
15. `append_common_footer()`

**Integration-Verify key differences:** ZERO TOLERANCE box at top; no full git-staging box; now requires agent teams (MUST).

**Knowledge-Distill:** Mission = curate memories → knowledge; includes documentation update reminder.

**Knowledge prefix key differences:** No worktree; COMMITS REQUIRED; "Your Mission = build briefing document"; 6-step workflow; agent teams for bootstrap.

### Shared append_* Helpers

The full table lives in the `## Shared append_* Helpers (cache.rs:51-~180)` section below — an abridged copy that predated `append_anti_slop_guidance()` and `append_adversarial_review()` was deleted from this spot on 2026-07-30. Consult the one table; do not re-inline a second.

### Semi-Stable Section (format/sections.rs:15-378)

Changes per **stage type**, not per session. Key sub-sections:

- **Knowledge reference box** (lines 22-32): the Knowledge Brief footer emits `loom knowledge context --stage <id> --query "<question>" --budget-tokens <n>` (`format/brief.rs::format_knowledge_brief`) when knowledge exists
- **Stage-type-aware reminder box** (lines 35-140): Knowledge/IV/KnowledgeDistill → "KNOWLEDGE UPDATES REQUIRED"; Standard → "SESSION MEMORY REQUIRED"
- **Knowledge management section** (lines 142-290): If knowledge empty → 4-step exploration order; if present → "Extend as you work"
- **Delegation Choices** (lines 339-386): Subagents vs. Hierarchy vs. Agent Teams decision
- **Ultracode License** (lines 388-413): Gated on `embedded_context.ultracode`; now also states the Claude-only Workflow-fan-out rule — the codex lane (`gpt-5.6-terra`/`gpt-5.6-luna`) is not addressable from a Workflow script, so on a stage licensed for both, codex-tier work goes through normal `loom-codex-forwarder` Agent spawns outside the Workflow
- **Sandbox Restrictions** (lines 417-419): Sandbox summary if present, rendered by `format_sandbox_section()` in `format/sandbox_section.rs` (moved out of `sections.rs` 2026-08-26 — `sections.rs` carries a maintainability-ledger FILE entry). When the sandbox is enabled, it always carries `append_package_cache_note()` — the package-manager-cache carve-out note — after the filesystem deny/allow-write block and before the network section
- **Skill Recommendations** (lines 421-426): Skill index matches

### Dynamic Section (format/sections.rs:382-661)

Per-session content. Includes Target (session/stage/plan IDs, working_dir, execution path), Plan Overview, Assignment, Dependency Status + Outputs, Handoff Content, Acceptance Criteria, Goal-Backward Verification (artifacts, wiring, wiring_tests, dead_code).

### Recitation Section (format/sections.rs:665-765)

End of signal for maximum attention. Includes: Compaction Imminent warning (≥75% usage), Context Budget Warning, Immediate Tasks, Stage end sequence line (`append_stage_end_sequence()`, `format/helpers.rs` — recited right after the task list, before the trailing blank line), Stage Memory (with PROMINENT WARNING if empty).

### EmbeddedContext Struct (types.rs:25-61)

Single container flowing through all 4 sections. The struct is currently `types.rs:24-73` — this
heading's `25-61` range is stale, and the field list below has changed: `knowledge_has_content: bool`
was replaced by `context_pack` and `knowledge_tree_empty`.

```rust
pub struct EmbeddedContext {
    pub handoff_content: Option<String>,      // V1 prose handoff
    pub parsed_handoff: Option<HandoffV2>,    // V2 structured handoff
    pub plan_overview: Option<String>,
    pub context_pack: Option<ContextPack>,    // types.rs:35 - this stage's retrieved brief, when retrieval selected anything
    pub knowledge_tree_empty: bool,           // types.rs:45 - doc/loom/knowledge/ has no real content; distinct from context_pack (see doc comment at the field)
    pub memory_content: Option<String>,       // Last 10 entries
    pub skill_recommendations: Vec<SkillMatch>,
    pub context_budget: Option<f32>,
    pub context_usage: Option<f32>,
    pub sandbox_summary: Option<SandboxSummary>,
    pub cross_stage_summary: Option<String>,  // IV/KnowledgeDistill only
    pub wiring_checklist: Option<String>,     // IV/KnowledgeDistill only
    pub ultracode: bool,
    pub implementers: Implementers,           // Licensed lanes, in preference order
    pub codex_available: bool,                // codex CLI + plugin installed; resolved once at build time
    pub subagent_timeout_secs: Option<u64>,   // Per-stage override; None emits nothing
}
```

### Caching

SHA-256 of stable prefix text → first 16 hex chars → `SignalMetrics::stable_prefix_hash`. Cache invalidated whenever the stable prefix Rust code changes. Semi-stable, dynamic, recitation sections are always regenerated.

## Shared append_* Helpers (cache.rs:51-~180)

| Helper                                 | Lines    | Content                                                                                                                                                                                            | Used By                                                                                                                                                                                                                                                                                                                               |
| -------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `append_path_boundaries()`             | 54-63    | ALLOWED/FORBIDDEN paths table                                                                                                                                                                      | Standard, IV, KnowledgeDistill                                                                                                                                                                                                                                                                                                        |
| `append_subagent_restrictions()`       | 66-93    | NO git/loom/add-A rules; memory recording guide                                                                                                                                                    | Standard (233), IV (424)                                                                                                                                                                                                                                                                                                              |
| `append_completion_rules()`            | signals/helpers.rs | Settled-stage doctrine (via `append_settled_completion_rules`) + acceptance, handoff, no retry rules                                                                                     | Standard, IV, KnowledgeDistill                                                                                                                                                                                                                                                                                                        |
| `append_settled_completion_rules()`    | signals/helpers.rs | "`loom stage complete` is the LAST act" / settled-stage checklist / "post-completion work is LOST WORK" + verify-acceptance line                                                          | `append_completion_rules()`, Knowledge prefix (directly)                                                                                                                                                                                                                                                                              |
| `append_isolation_boundaries_simple()` | 108-113  | 2-bullet version                                                                                                                                                                                   | IV (408), KnowledgeDistill (508)                                                                                                                                                                                                                                                                                                      |
| `append_execution_rules_intro()`       | 119-124  | "Follow CLAUDE.md" short header                                                                                                                                                                    | IV (412), KnowledgeDistill (512), Knowledge (594)                                                                                                                                                                                                                                                                                     |
| `append_common_footer()`               | 127-142  | Binary usage, state files, context recovery                                                                                                                                                        | ALL 4 prefixes                                                                                                                                                                                                                                                                                                                        |
| `append_git_staging_full()`            | signals/helpers.rs (moved 2026-08-26, same reason as `append_completion_rules`) | Full staging rules + danger box                                                                                                                                          | Standard only                                                                                                                                                                                                                                                                                                                         |
| `append_git_staging_rules()`           | signals/helpers.rs (moved 2026-08-26, same reason as `append_completion_rules`) | Shorter version                                                                                                                                                          | IV, KnowledgeDistill                                                                                                                                                                                                                                                                                                                  |
| `append_commit_timing_rules()`         | signals/helpers.rs | "When to Commit" doctrine — commits are the ORCHESTRATOR's, made ONLY as the final step of the stage, after every subagent has returned and the verification gate is green; `gate`/`review` params interpolate the per-stage-family wording                                             | ALL 4 prefixes, called right after the `**Completion:**` header line                                                                                                                                                                                                                                                                  |
| `append_anti_slop_guidance()`          | ~171+    | ZERO TOLERANCE anti-slop rules box                                                                                                                                                                 | ALL 4 prefixes (after exec-rules intro, before Delegation)                                                                                                                                                                                                                                                                            |
| `append_adversarial_review()`          | ~104-122 | Mini adversarial code review — 6 dimensions (quality/architecture·SOLID, idiomatic, security, wiring, dead code, DRY across whole codebase) + a closing "tests actually exercise the change" check | Standard (replaces old "Self-Review" block), IV (after Mission). **Code-producing prefixes ONLY** — NOT knowledge or knowledge-distill (both emit only markdown). NOTE: silent-failure detection is NOT in this helper — Standard has its own dedicated block right after the call; IV has its own `SILENT FAILURE DETECTION` section |
| `append_stage_end_sequence()`          | format/helpers.rs | Recites the commit order (subagents returned → gate green → review returned/fixed → gate green again → commit → `loom stage complete`) at the end of "## Immediate Tasks" for maximum attention                                                                                     | `format_recitation_section()` (all stage types)                                                                                                                                                                                                                                                                                       |
| `append_package_cache_note()`          | format/helpers.rs | Names the package-manager caches (bun/npm/pnpm/yarn/deno/cargo/rustup/uv/pip/go) writable inside the sandbox, and the two carve-out limits (not-yet-existing cache dirs, relocated cache env vars)                                                                                    | `format_sandbox_section()` (`format/sandbox_section.rs`), when the sandbox is enabled                                                                                                                                                                                                                                                |

**Adding a new helper:** Follow same `fn append_xxx(content: &mut String)` pattern. Call it explicitly from each generator where wanted — it's NOT auto-injected. Placement: cache.rs's "Shared content blocks" cluster is the historical home, but cache.rs carries a maintainability-ledger FILE entry, so new helpers (and any that need to grow) go to the unledgered `signals/helpers.rs` instead — `append_completion_rules`/`append_settled_completion_rules` moved there 2026-08-14. Same rule in format/: the recitation section's budget-exceeded box lives in `format/helpers.rs::append_budget_exceeded_box` because `sections.rs` is ledgered.

**Per-stage code review:** The mandatory mini adversarial code review lives in `append_adversarial_review()` (`pub(crate)`) and is injected into the two code-producing stable prefixes (Standard, IntegrationVerify). It supersedes the older standard-prefix "Self-Review Before Completion" block. Documentation stages (Knowledge, KnowledgeDistill) deliberately omit it — they produce only markdown, so there is no code to review; the cache tests negative-assert its absence there.

**Stable prefix selection — single source of truth:** `cache::stable_prefix_for(stage_type)` is the ONE place that maps stage type → prefix generator (explicit 4-arm match). Both the regular path (`format/mod.rs::format_signal_with_metrics`) and the recovery path (`recovery_format.rs`) call it, so they can never drift.

**Resume-path coverage (important):** The review (and all execution guidance) must reach a stage no matter which signal spawns it. Three paths: (1) regular spawn + automatic crash retry → `format_signal_with_metrics()` → `stable_prefix_for()`; (2) continuation/handoff → `generate_signal()` → same path; (3) **manual recovery** (`loom stage recover`, `loom stage retry`) → `recovery_format.rs::format_recovery_signal()`. The recovery signal is built outside the KV-cache path; it now embeds the FULL stable prefix via `stable_prefix_for(stage.stage_type)` (replacing its old hand-rolled "## Worktree Context" stub), so a resumed stage gets the same rules — review, subagent restrictions, git-staging, anti-slop, completion — as a fresh spawn, correctly gated by stage type (Knowledge/KnowledgeDistill prefixes carry no review). Tests: `recovery.rs::test_generate_recovery_signal` (Standard → review + subagent restrictions + execution rules present) and `test_recovery_signal_omits_review_for_documentation_stage` (KnowledgeDistill → no review). `tests_commit_timing.rs` covers the commit-timing doctrine itself: presence/wording across all 4 stable prefixes, the code-vs-documentation gate condition, `CLAUDE.md.template` agreement, the retired header-bullet wording being gone, the recitation's stage-end-sequence ordering, and the sandbox section's package-cache note.

## Soft Signals

Soft signals are advisory per-session notices persisted to disk so that dedup survives daemon restarts. File: `.work/monitor/soft-signals.jsonl` (JSONL, append-only, no compaction).

**Schema (single variant today):**

```json
{
  "kind": "possibly_stuck",
  "session_id": "s1",
  "stage_id": "my-stage",
  "recent_events": 10,
  "failure_count": 9,
  "failure_ratio": 0.9,
  "emitted_at": "<RFC3339>",
  "expires_at": "<RFC3339>"
}
```

**Decay window:** `DECAY_WINDOW_SECS = 120` — signals expire 120 seconds after they are written. `read_active(work_dir, now)` filters out expired signals. `read_active_for_session(work_dir, now, session_id)` further filters by session.

**Detection pipeline:**

1. `post-tool-use.sh` updates a private per-session heartbeat and persists no tool output.
2. Heartbeat and session liveness drive hung-session reporting. The legacy `tool_analysis` reader can consume older `.work/tool-events.jsonl` data, but no production hook writes new records.
3. Soft signals remain bounded, advisory state. `daemon/server/status.rs::collect_status()` calls `soft_signals::read_active_for_session()` to derive `Stage.is_possibly_stuck` at read time; the field is never persisted to stage files (`#[serde(skip)]`).
4. Static `loom status` reads through the same helper.

**Key files:** `orchestrator/monitor/soft_signals.rs` (schema + I/O), `orchestrator/monitor/tool_analysis.rs` (analysis), `orchestrator/monitor/detection.rs` (event emission), `daemon/server/status.rs` (status derivation).
