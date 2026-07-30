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

| Generator | Function | Line | Stage Type |
|-----------|----------|------|------------|
| Standard | `generate_stable_prefix()` | 174 | `StageType::Standard` |
| Integration-Verify | `generate_integration_verify_stable_prefix()` | 313 | `StageType::IntegrationVerify` |
| Knowledge-Distill | `generate_knowledge_distill_stable_prefix()` | 447 | `StageType::KnowledgeDistill` |
| Knowledge | `generate_knowledge_stable_prefix()` | 527 | `StageType::Knowledge` |

**Standard prefix section order (approx lines 174-310):**

1. Worktree Context header
2. Isolation Boundaries (3 bullets)
3. `append_path_boundaries()` — ALLOWED/FORBIDDEN paths table
4. working_dir reminder
5. Execution Rules header
6. Worktree Isolation detail
7. Delegation & Efficiency (subagents + hierarchies + agent teams)
8. `append_subagent_restrictions()` — NO commit/complete/add-A rules
9. `append_completion_rules()`
10. `append_adversarial_review()` — Mini Adversarial Code Review (6 dimensions)
11. Dedicated Silent Failure Check block (Standard only; IV has its own section)
12. Stage Memory guidance
13. `append_git_staging_full()` (Standard ONLY; IV/KnowledgeDistill use `append_git_staging_rules()`)
14. `append_common_footer()`

**Integration-Verify key differences:** ZERO TOLERANCE box at top; no full git-staging box; now requires agent teams (MUST).

**Knowledge-Distill:** Mission = curate memories → knowledge; includes documentation update reminder.

**Knowledge prefix key differences:** No worktree; COMMITS REQUIRED; "Your Mission = build briefing document"; 6-step workflow; agent teams for bootstrap.

### Shared append_* Helpers

The full table lives in the `## Shared append_* Helpers (cache.rs:51-~180)` section below — an abridged copy that predated `append_anti_slop_guidance()` and `append_adversarial_review()` was deleted from this spot on 2026-07-30. Consult the one table; do not re-inline a second.

### Semi-Stable Section (format/sections.rs:15-378)

Changes per **stage type**, not per session. Key sub-sections:

- **Knowledge reference box** (lines 22-32): `loom knowledge show` commands if knowledge exists
- **Stage-type-aware reminder box** (lines 35-140): Knowledge/IV/KnowledgeDistill → "KNOWLEDGE UPDATES REQUIRED"; Standard → "SESSION MEMORY REQUIRED"
- **Knowledge management section** (lines 142-290): If knowledge empty → 4-step exploration order; if present → "Extend as you work"
- **Delegation Choices** (lines 319-345): Subagents vs. Hierarchy vs. Agent Teams decision
- **Ultracode License** (lines 347-362): Gated on `embedded_context.ultracode`
- **Sandbox Restrictions** (lines 365-368): Sandbox summary if present
- **Skill Recommendations** (lines 370-374): Skill index matches

### Dynamic Section (format/sections.rs:382-661)

Per-session content. Includes Target (session/stage/plan IDs, working_dir, execution path), Plan Overview, Assignment, Dependency Status + Outputs, Handoff Content, Acceptance Criteria, Goal-Backward Verification (artifacts, wiring, wiring_tests, dead_code).

### Recitation Section (format/sections.rs:665-765)

End of signal for maximum attention. Includes: Compaction Imminent warning (≥75% usage), Context Budget Warning, Immediate Tasks, Stage Memory (with PROMINENT WARNING if empty).

### EmbeddedContext Struct (types.rs:24-50)

Single container flowing through all 4 sections:

```rust
pub struct EmbeddedContext {
    pub handoff_content: Option<String>,      // V1 prose handoff
    pub parsed_handoff: Option<HandoffV2>,    // V2 structured handoff
    pub plan_overview: Option<String>,
    pub knowledge_has_content: bool,
    pub memory_content: Option<String>,       // Last 10 entries
    pub skill_recommendations: Vec<SkillMatch>,
    pub context_budget: Option<f32>,
    pub context_usage: Option<f32>,
    pub sandbox_summary: Option<SandboxSummary>,
    pub cross_stage_summary: Option<String>,  // IV/KnowledgeDistill only
    pub wiring_checklist: Option<String>,     // IV/KnowledgeDistill only
    pub ultracode: bool,
}
```

### Caching

SHA-256 of stable prefix text → first 16 hex chars → `SignalMetrics::stable_prefix_hash`. Cache invalidated whenever the stable prefix Rust code changes. Semi-stable, dynamic, recitation sections are always regenerated.

## Shared append_* Helpers (cache.rs:51-~180)

| Helper | Lines | Content | Used By |
|--------|-------|---------|---------|
| `append_path_boundaries()` | 54-63 | ALLOWED/FORBIDDEN paths table | Standard, IV, KnowledgeDistill |
| `append_subagent_restrictions()` | 66-93 | NO git/loom/add-A rules; memory recording guide | Standard (233), IV (424) |
| `append_completion_rules()` | 96-102 | Acceptance, handoff, no retry rules | Standard (254), IV (433), KnowledgeDistill (515) |
| `append_isolation_boundaries_simple()` | 108-113 | 2-bullet version | IV (408), KnowledgeDistill (508) |
| `append_execution_rules_intro()` | 119-124 | "Follow CLAUDE.md" short header | IV (412), KnowledgeDistill (512), Knowledge (594) |
| `append_common_footer()` | 127-142 | Binary usage, state files, context recovery | ALL 4 prefixes |
| `append_git_staging_full()` | 145-160 | Full staging rules + danger box | Standard only |
| `append_git_staging_rules()` | 162-169 | Shorter version | IV, KnowledgeDistill |
| `append_anti_slop_guidance()` | ~171+ | ZERO TOLERANCE anti-slop rules box | ALL 4 prefixes (after exec-rules intro, before Delegation) |
| `append_adversarial_review()` | ~104-122 | Mini adversarial code review — 6 dimensions (quality/architecture·SOLID, idiomatic, security, wiring, dead code, DRY across whole codebase) + a closing "tests actually exercise the change" check | Standard (replaces old "Self-Review" block), IV (after Mission). **Code-producing prefixes ONLY** — NOT knowledge or knowledge-distill (both emit only markdown). NOTE: silent-failure detection is NOT in this helper — Standard has its own dedicated block right after the call; IV has its own `SILENT FAILURE DETECTION` section |

**Adding a new helper:** Follow same `fn append_xxx(content: &mut String)` pattern. Place in the "Shared content blocks" cluster (lines 51-~180). Call it explicitly from each generator where wanted — it's NOT auto-injected.

**Per-stage code review:** The mandatory mini adversarial code review lives in `append_adversarial_review()` (`pub(crate)`) and is injected into the two code-producing stable prefixes (Standard, IntegrationVerify). It supersedes the older standard-prefix "Self-Review Before Completion" block. Documentation stages (Knowledge, KnowledgeDistill) deliberately omit it — they produce only markdown, so there is no code to review; the cache tests negative-assert its absence there.

**Stable prefix selection — single source of truth:** `cache::stable_prefix_for(stage_type)` is the ONE place that maps stage type → prefix generator (explicit 4-arm match). Both the regular path (`format/mod.rs::format_signal_with_metrics`) and the recovery path (`recovery_format.rs`) call it, so they can never drift.

**Resume-path coverage (important):** The review (and all execution guidance) must reach a stage no matter which signal spawns it. Three paths: (1) regular spawn + automatic crash retry → `format_signal_with_metrics()` → `stable_prefix_for()`; (2) continuation/handoff → `generate_signal()` → same path; (3) **manual recovery** (`loom stage recover`, `loom stage retry`) → `recovery_format.rs::format_recovery_signal()`. The recovery signal is built outside the KV-cache path; it now embeds the FULL stable prefix via `stable_prefix_for(stage.stage_type)` (replacing its old hand-rolled "## Worktree Context" stub), so a resumed stage gets the same rules — review, subagent restrictions, git-staging, anti-slop, completion — as a fresh spawn, correctly gated by stage type (Knowledge/KnowledgeDistill prefixes carry no review). Tests: `recovery.rs::test_generate_recovery_signal` (Standard → review + subagent restrictions + execution rules present) and `test_recovery_signal_omits_review_for_documentation_stage` (KnowledgeDistill → no review).

## Soft Signals

Soft signals are advisory per-session notices persisted to disk so that dedup survives daemon restarts. File: `.work/monitor/soft-signals.jsonl` (JSONL, append-only, no compaction).

**Schema (single variant today):**

```json
{"kind":"possibly_stuck","session_id":"s1","stage_id":"my-stage","recent_events":10,"failure_count":9,"failure_ratio":0.9,"emitted_at":"<RFC3339>","expires_at":"<RFC3339>"}
```

**Decay window:** `DECAY_WINDOW_SECS = 120` — signals expire 120 seconds after they are written. `read_active(work_dir, now)` filters out expired signals. `read_active_for_session(work_dir, now, session_id)` further filters by session.

**Detection pipeline:**

1. `post-tool-use.sh` appends rows to `.work/tool-events.jsonl` on every tool call.
2. `orchestrator/monitor/tool_analysis::analyze_session()` reads the last 50 events for a session and computes `ToolAnalysis`.
3. Stuck criteria: `recent_failure_count >= 5 (STUCK_MIN_EVENTS)` AND `failure_ratio >= 0.80 (STUCK_FAILURE_RATIO)` within a 60-second rolling window (`STUCK_WINDOW_SECS`). Failure-shaped events: `is_error == true` OR `output_bytes == Some(0)`.
4. On detection, monitor emits `MonitorEvent::PossiblyStuck`; the event handler calls `soft_signals::append(work_dir, &signal)`.
5. `daemon/server/status.rs::collect_status()` calls `soft_signals::read_active_for_session()` to derive `Stage.is_possibly_stuck` at read time (never persisted to stage files — `#[serde(skip)]`).
6. Static `loom status` reads via `commands/status/data.rs::collect_status_data()` using the same helper.

**Key files:** `orchestrator/monitor/soft_signals.rs` (schema + I/O), `orchestrator/monitor/tool_analysis.rs` (analysis), `orchestrator/monitor/detection.rs` (event emission), `daemon/server/status.rs` (status derivation).
