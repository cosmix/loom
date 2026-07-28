# Subagent Hierarchy

> Flat fan-out vs 2-level coordinator hierarchy vs agent teams: when to use each, model mix, file exclusivity.

## Subagent Hierarchy + Ultracode Guidance (2026-06-12)

Loom teaches a 2-level subagent hierarchy (main agent → coordinator subagents → worker subagents) for large well-defined fan-out, and a per-stage `ultracode: bool` flag that licenses Workflow orchestration. The guidance is mirrored across multiple surfaces that MUST stay consistent.

**Capability facts:**

- Nested subagents are supported since Claude Code 2.1.172 (changelog: "Sub-agents can now spawn their own sub-agents (up to 5 levels deep)"); empirically confirmed 2026-06-12 on 2.1.175 (a subagent spawned a nested Explore agent, no flags needed). On older versions the Task tool is simply absent one level down; the coordinator does the work itself.
- The platform allows 5 levels; **loom caps trees at 2 by policy** (auditable file-exclusivity, bounded cost/failure blast radius), enforced via the WORKER PREAMBLE prose, not code.
- The `tools:` frontmatter in `agents/*.md` governs capability: `loom-software-engineer` and `loom-senior-software-engineer` list `Task` (can coordinate); `loom-code-reviewer` does not (always a leaf).
- **Model-inheritance trap:** a nested worker spawned WITHOUT an agent type defaults to the MAIN session model — on an `opus` stage that silently makes every worker opus. Hence "spawn workers BY AGENT TYPE" on every surface.
- **Ultracode licensing:** Claude Code's Workflow tool requires explicit opt-in; the documented trigger is the literal keyword `ultracode` in the session's prompt. Loom controls every spawned session's initial prompt, so `stage.ultracode` injects the keyword at spawn (`orchestrator/terminal/native/mod.rs`, `SessionType::Stage` prompt arm).

**Canonical keyword table (wording changes must update ALL listed surfaces):**

| Exact phrase | Must appear in |
| --- | --- |
| `2-LEVEL CAP` | CLAUDE.md.template (Rule 6c), SKILL.md (§4 + hierarchical-blocks subsection), cache.rs, sections.rs, agents/loom-software-engineer.md, agents/loom-senior-software-engineer.md |
| `Workers NEVER spawn subagents` | CLAUDE.md.template (Rule 6c + Critical Reminders), SKILL.md, cache.rs, sections.rs, agents/loom-software-engineer.md |
| `DISJOINT` (phrasing may vary: territories / file territory / worker file sets) | all hierarchy surfaces |
| `compact summary` (any case) | coordinator preamble, SKILL.md, cache.rs, sections.rs |
| `BY AGENT TYPE` | CLAUDE.md.template, signals (cache.rs); "BY TYPE" in sections.rs |
| `ultracode` | CLAUDE.md.template (Parallelization Strategy), SKILL.md (ULTRACODE STAGES), plan/schema/types.rs, sections.rs, native/mod.rs |
| Threshold | `more than ~6 independent worker tasks` → hierarchy; `~6 or fewer` → flat (all decision surfaces) |

**Surface inventory:**

1. `CLAUDE.md.template` — Rule 6c (decision table, criteria, COORDINATOR/WORKER PREAMBLEs), Rule 6/6b/7 amendments, Parallelization Strategy rows, Ultracode Stages subsection, Critical Reminders item 5
2. `skills/loom-plan-writer/SKILL.md` — §4 criteria-keyed strategies block (1/2/2a/2b/3 — deliberately NOT a ranking), decision table `>~6 worker tasks?` column, HIERARCHICAL EXECUTION PLAN BLOCKS subsection, ULTRACODE STAGES subsection, §5 description requirements, Example 5 (flat-12 vs 3×4 honest cost comparison)
3. `orchestrator/signals/cache.rs` — standard stable prefix only ("Subagent Hierarchies (2-LEVEL CAP)" block); knowledge/IV/knowledge-distill prefixes untouched (3 of 4 prefix hashes unchanged)
4. `orchestrator/signals/format/sections.rs` — semi-stable "## Delegation Choices" (three-way: flat / hierarchy / teams; replaced the old "## Agent Teams" header) + gated "## Ultracode Mode" section
5. `orchestrator/terminal/native/mod.rs` — spawn-prompt ultracode keyword (Stage sessions only; merge/knowledge spawns don't need it)
6. `agents/loom-software-engineer.md` (## Delegation: worker/coordinator roles), `agents/loom-senior-software-engineer.md` (hierarchy line in "What you define")

**Pinning tests:** `cache.rs::test_generate_stable_prefix_contains_required_sections` (hierarchy asserts), `tests_cache.rs::test_signal_contains_delegation_choices_three_way` (incl. negative assert on the old `## Agent Teams` header), `tests_cache.rs::test_signal_ultracode_section_gated`, `plan/schema/tests/ultracode_tests.rs` (parse/default/advisory), `commands/init/tests.rs` (definition→stage propagation).

**Consistency greps (run after any wording change):**

```bash
rg -n "2-LEVEL CAP" CLAUDE.md.template skills/loom-plan-writer/SKILL.md loom/src/orchestrator/signals/ agents/
rg -n "Workers NEVER spawn subagents" CLAUDE.md.template skills/loom-plan-writer/SKILL.md loom/src/orchestrator/signals/ agents/loom-software-engineer.md
rg -n "BY AGENT TYPE" CLAUDE.md.template loom/src/orchestrator/signals/
rg -n "ultracode" CLAUDE.md.template skills/loom-plan-writer/SKILL.md loom/src/plan/schema/types.rs loom/src/orchestrator/signals/
rg -n "HIERARCHY SECOND" skills/   # must be ZERO hits (criteria-keyed, not ranked)
```

**Watch item:** whether Claude Code hooks (PreToolUse etc.) fire identically for depth-2 subagents is undocumented upstream. Loom's `commit-filter.sh` detection walks the process tree (nearest claude ancestor vs `LOOM_MAIN_AGENT_PID`) and is depth-agnostic by construction, but re-verify on major Claude Code upgrades.
