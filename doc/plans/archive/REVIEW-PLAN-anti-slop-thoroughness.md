# Code Review: Anti-Slop Thoroughness — Forcing-Function + Repair Existing Gates

**Plan:** PLAN-anti-slop-thoroughness | **Generated:** 2026-06-15 14:27 UTC

## Summary

## Overview

## Changes by Stage

### Forcing-function + revive compaction reminder (forcing-function-and-hooks)

**Status:** completed  
**Purpose:** Add the anti-slop forcing-function to the static template and the generated
signal prefixes, strengthen bootstrap mapping, and revive the dead
post-compaction reminder. All four subagents own DISJOINT files.

Use parallel subagents and skills to maximize performance.

SUBAGENT FILE ASSIGNMENTS:
  Subagent 1 - Template (loom-software-engineer):
    Files Owned: CLAUDE.md.template
    Task: In Engineering Discipline A "Think Before Coding" (~lines 37-41),
      append, as plain prose (NO persona line):
      (a) an "UNDERSTAND-FIRST LADDER (before writing code)" numbered list:
          1. Read doc/loom/knowledge/ fully; if absent/sparse for your area,
             build it (loom knowledge bootstrap) BEFORE implementing.
          2. Map the area: call paths, data flow, every caller/consumer; know
             the blast radius.
          3. Search for existing functions/utilities/patterns to REUSE first.
          4. Only then implement — minimal and surgical.
          5. Cannot verify a fact you rely on? Do NOT guess. Check it. If you
             genuinely cannot (autonomous stage), record via loom memory
             decision/note and proceed against the most defensible reading of
             acceptance — never silently.
      (b) a "BANNED — self-reject and redo before reporting done:" list with the
          literal phrases: "areas left untouched", "I guessed / should have
          checked", "that is on me, I should have checked", and "reporting
          unverified work as done".
      The block MUST contain the exact string: Understand before acting; do not guess.
      Add a one-line cross-reference from Critical Reminders (~line 557, reminder 7).
  Subagent 2 - Generated prefixes (loom-software-engineer):
    Files Owned: loom/src/orchestrator/signals/cache.rs,
      loom/src/orchestrator/signals/format/sections.rs,
      loom/src/orchestrator/signals/tests_cache.rs
    Files Read-Only: loom/src/orchestrator/signals/generate.rs
    Task:
      1. Add a shared helper fn append_anti_slop_guidance(content: &mut String)
         to the "Shared content blocks" cluster (cache.rs:51-169), same builder
         pattern as append_common_footer (cache.rs:127). Its text is the ladder +
         banned-list and MUST contain the exact string:
         Understand before acting; do not guess.
      2. Call it from ALL FOUR generators — generate_stable_prefix:174,
         generate_integration_verify_stable_prefix:313,
         generate_knowledge_distill_stable_prefix:447,
         generate_knowledge_stable_prefix:527 — placed AFTER the execution-rules
         intro and BEFORE the Delegation/Completion block. Do NOT place it before
         a prefix's identity block (e.g. integration-verify "ZERO TOLERANCE").
      3. In generate_knowledge_stable_prefix "Your Mission" (~cache.rs:545-560),
         change step 1 to demand EXHAUSTIVE mapping: "Exhaustively map the
         codebase (hierarchically) — entry points, every module, data flow,
         patterns, conventions; leave no major area unmapped."
      4. In sections.rs "Exploration Order" (~209-220) add a line: cover ALL
         areas you touch — no sampling.
      5. Update tests: add asserts for the new substring to the four
         *_contains_required_sections tests in cache.rs and the content test in
         tests_cache.rs; keep existing asserts green.
      Do NOT edit generate.rs (owned by the parallel wire-dormant-gates stage).
  Subagent 3 - Bootstrap mapping prompts (loom-software-engineer):
    Files Owned: loom/src/commands/knowledge/bootstrap.rs
    Task: In build_system_prompt() (~103-128) and build_initial_prompt()
      (~131-146), require an EXHAUSTIVE map (every major component/module, full
      data flow, all entry points, blast radius of areas the plan will change) —
      "go beyond the auto loom map". Strengthen Agent 1 (Architecture) to
      "exhaustively map ALL components and their relationships" and the synthesis
      pass to confirm no major area was left unmapped.
  Subagent 4 - Compaction reminder (loom-software-engineer):
    Files Owned: hooks/session-start.sh, hooks/post-tool-use.sh,
      hooks/tests/session-start-compact.sh, hooks/tests/run-all.sh
    Task:
      1. In hooks/session-start.sh: keep the heartbeat write for all sources;
         ADDITIONALLY parse the SessionStart stdin JSON for .source (use jq —
         already used by other hooks; keep the existing stdin-drain/timeout
         guard) and, when .source is "compact" or "resume", emit a
         hookSpecificOutput.additionalContext JSON on stdout (precedent:
         hooks/prefer-modern-tools.sh) containing a short re-anchor pointer:
         re-read ${LOOM_WORK_DIR}/signals/${LOOM_SESSION_ID}.md, run
         "loom memory list", read the latest .work/handoffs/ file, and the line
         "Understand before acting; do not guess." For startup/clear, emit nothing extra.
      2. In hooks/post-tool-use.sh: remove the now-dead compaction-recovery
         stderr reminder branch (~lines 128-144) it replaces.
      3. Add hooks/tests/session-start-compact.sh: pipe {"source":"compact"} with
         LOOM_WORK_DIR and LOOM_SESSION_ID set, assert the additionalContext +
         the resolved signal path appear on stdout; register it in
         hooks/tests/run-all.sh.

NO FILE OVERLAP between subagents confirmed. NO overlap with wire-dormant-gates
(which owns orchestrator/core/ and signals/generate.rs only).

MEMORY RECORDING (use loom memory ONLY — never loom knowledge, never auto-memory):

- MISTAKE: loom memory note "mistake: ... because ... fixed by ..."
- DECISION: loom memory decision "..." --context "..."
SUBAGENT MEMORY — include in every subagent prompt: record mistakes/decisions/
surprises via loom memory; do NOT record procedural actions.

#### Files Changed

- cache.rs - append_anti_slop_guidance helper added in shared-content-blocks cluster; called from all 4 prefix generators
- session-start.sh - stdin now captured (not drained), compact/resume source emits hookSpecificOutput with signal re-anchor
- post-tool-use.sh - removed compaction-recovery marker branch (~17 lines) now superseded by session-start hook

#### Key Decisions

- placed append_anti_slop_guidance call after exec-rules intro, before Delegation block in each generator *(keeps identity blocks (ZERO TOLERANCE etc.) before the ladder, matches the signal spec's ordering requirement)*

#### Notes

- gotcha: 'TODO' inside a Rust string literal (e.g., in a prompt string) triggers the ArtifactStub checker in loom stage complete. Prevention: scan your own format!/push_str strings for the word TODO before completing — rephrase to 'fixme markers' or similar if it's just a reference, not a stub.
- found: session-start.sh discarded stdin entirely (>/dev/null); had to change to capture-into-variable pattern (same as post-tool-use.sh) before parsing .source

### Repair before_stage + code_review (dead schema) (wire-dormant-gates)

**Status:** completed  
**Purpose:** Two of loom's enforcement surfaces are parsed but never run. Wire both so the
forcing-function can be enforced. This is architectural — it threads dormant
config through orchestrator internals; make correct integration decisions and
record them.

Use parallel subagents and skills to maximize performance.

IMPORTANT BOUNDARY: own ONLY loom/src/orchestrator/core/** (before_stage) and
loom/src/orchestrator/signals/generate.rs (code_review). Do NOT edit cache.rs or
format/sections.rs — they belong to the parallel forcing-function-and-hooks
stage. If code_review rendering seems to belong in the stable prefix, instead
render it in generate.rs (it is stage-specific/dynamic, not stable-cacheable).

SUBAGENT FILE ASSIGNMENTS:
  Subagent 1 - Execute before_stage checks (loom-senior-software-engineer):
    Files Owned: loom/src/orchestrator/core/stage_executor.rs (and, only if
      strictly necessary, sibling files in loom/src/orchestrator/core/)
    Files Read-Only: loom/src/verify/goal_backward/truths.rs,
      loom/src/commands/stage/complete.rs (see how after_stage runs at 866-887),
      loom/src/plan/schema/types.rs, loom/src/models/stage/types.rs
    Task: before_stage checks (TruthCheck list) are defined but executed nowhere
      (only after_stage runs, at complete.rs:866-887). Run them at stage spawn:
      reuse verify::goal_backward::truths::verify_truth_checks (truths.rs:16-134),
      mirroring the after_stage call site, but BEFORE the session is spawned in
      the start/spawn path of stage_executor.rs. On any gap, do NOT spawn — hold
      the stage (transition to Blocked) with the gap message. Add unit tests
      proving a failing before_stage check blocks spawn and a passing one allows
      it (mirror the after_stage tests in loom/src/verify/before_after.rs).
  Subagent 2 - Consume code_review dimensions (loom-senior-software-engineer):
    Files Owned: loom/src/orchestrator/signals/generate.rs
    Files Read-Only: loom/src/plan/schema/types.rs (CodeReviewConfig ~100-115),
      loom/src/orchestrator/signals/cache.rs
    Task: StageDefinition.code_review (dimensions: Vec<String>, require_all) is
      parsed but never surfaced to the integration-verify agent. Thread it into
      signal generation and render a "## Review Dimensions" section into the
      integration-verify signal in generate.rs, honoring require_all (list each
      dimension as a checkbox the agent must address). Add a unit test asserting
      the rendered signal contains the configured dimensions and reflects
      require_all. If code_review must be threaded through an embedded-context
      struct, make that change in generate.rs's own types or signals/types.rs —
      NOT in cache.rs/sections.rs.

NO FILE OVERLAP between subagents, and NONE with forcing-function-and-hooks.

NOTE: these changes affect the loom binary, not the running daemon. Verify via
cargo tests here; live effect requires a reinstall (out of scope).

MEMORY RECORDING (loom memory ONLY — never loom knowledge, never auto-memory):

- DECISION: loom memory decision "where before_stage runs / how code_review is
  threaded" --context "why"
- GOTCHA: loom memory note "gotcha: ..."
SUBAGENT MEMORY — include in both subagent prompts.

#### Files Changed

- loom/src/orchestrator/signals/generate.rs - wired dormant code_review: load_code_review_for_stage() reads it from the plan (resolve_source_path+parse_plan), render_review_dimensions() emits a '## Review Dimensions' checkbox section honoring require_all, appended to the integration-verify signal (Adjudicator-Feedback append pattern). plan/schema/mod.rs re-exports CodeReviewConfig. before_stage left untouched (already wired).

#### Key Decisions

- Surface code_review by reading it from the plan inside generate.rs (resolve_source_path + parse_plan + find by id), NOT by adding a code_review field to the runtime Stage model *(generate.rs only receives &Stage; the runtime Stage has no code_review field (only StageDefinition does). Adding it to Stage would touch models/types.rs + plan_setup.rs + ~15 full Stage{} literals and risk merge conflicts with the parallel forcing-function-and-hooks stage. Reading from the plan mirrors how after_stage is consumed at completion (commands/stage/complete.rs:834 load_stage_definition_from_plan) and keeps changes inside the assigned boundary (generate.rs + one re-export line in plan/schema/mod.rs). Cost bounded by gating the lookup to IntegrationVerify spawns.)*

#### Notes

- gotcha: plan claimed before_stage is dead schema ('executed nowhere, only after_stage runs'), but before_stage is ALREADY fully wired: copied StageDefinition->Stage at plan_setup.rs:280, executed at spawn in stage_executor.rs:219-256 (run_before_stage_checks -> Blocked on gaps BEFORE session spawn), and unit-tested in verify/before_after.rs. The wiring acceptance check (grep before_stage in stage_executor.rs) already passes. Only code_review was truly dead. Rule: verify a 'dead schema' claim against the actual spawn path before writing code; knowledge/before_after-execution-status entry already documented this.

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification after both implementation stages merge.

Use parallel subagents and skills to maximize performance.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory goes through loom memory.

CONTEXT GATHERING (FIRST):

- Read the plan file from doc/plans/ (check .work/config.toml for source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for architecture context and known mistakes

⛔ ZERO TOLERANCE: fix ALL compiler warnings, clippy errors, test failures.
Nothing is "pre-existing".

BUILD & TEST:

1. cargo test --manifest-path loom/Cargo.toml (full suite)
2. cargo clippy --manifest-path loom/Cargo.toml -- -D warnings
3. cargo build --manifest-path loom/Cargo.toml
4. bash hooks/tests/run-all.sh

CODE REVIEW (spawn parallel loom-code-reviewer subagents; fix all findings with
loom-senior-software-engineer):
5. duplication/reuse — did any change reinvent an existing utility? (dogfoods
   the new code_review dimension on this very stage)
6. architecture/quality and security review of the hook + orchestrator changes

FUNCTIONAL VERIFICATION (in-tree — live behavior needs reinstall, do NOT attempt
a live compaction here):
7. Confirm the anti-slop text reaches all four generated prefixes (the
   cache.rs *_contains_required_sections tests) and the template
   (rg "Understand before acting" CLAUDE.md.template).
8. Confirm before_stage execution is covered by a test that blocks spawn on a
   failing check, and code_review rendering by a test asserting dimensions appear.
9. Confirm hooks/tests/session-start-compact.sh passes.

Record discoveries to loom memory for knowledge-distill. Do NOT use loom knowledge here.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- knowledge-distill TODO (could not fix here — doc/loom/knowledge is sandbox deny_write + signal says no loom knowledge in integration-verify): two knowledge files describe the now-removed compaction-recovery marker flow and are STALE. (1) entry-points.md:~556 still says post-tool-use.sh 'checks .work/compaction-recovery/<session> marker' — false; re-anchor now lives in session-start.sh on source=compact/resume. (2) architecture.md:~626 references compaction-recovery. Update both to describe the session-start.sh re-anchor mechanism.
- orphan-after-merge: forcing-function plan removed the compaction-recovery READER (~18 lines) from post-tool-use.sh but left the WRITER in pre-compact.sh (RECOVERY_DIR + mkdir/touch at lines 57,71-73) — dead writes nothing consumed. Fix: removed the orphaned marker writes from pre-compact.sh; re-anchoring now flows ONLY through session-start.sh on SessionStart(source=compact/resume). Detection rule: when deleting a hook that READS a marker, grep all hooks for the WRITER too (rg compaction-recovery) and remove both ends.
- dogfood-find (duplication dimension worked): code_review review of this very stage flagged that generate.rs load_code_review_for_stage re-inlined the resolve_source_path+parse_plan+find(stage) sequence that commands/verify.rs::load_stage_definition_from_plan already encapsulated. Fix: RELOCATED load_stage_definition_from_plan from commands/verify.rs to crate::plan::parser (its natural layer, next to parse_plan); re-exported via plan/mod.rs; all 3 callers (commands/stage/complete.rs, commands/stage/verify.rs, orchestrator/signals/generate.rs) now delegate. Why not the reviewer's 'just call commands::verify' suggestion: orchestrator had ZERO existing crate::commands deps, so that would have added the first orchestrator->commands layering edge. Relocating to plan/ removes the dup with no new edge (both commands and orchestrator already depend on plan).

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Curate stage memories into permanent knowledge and update user-facing docs.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).

CONTEXT GATHERING (FIRST):

1. Read the plan file from doc/plans/ (check .work/config.toml for source_path)
2. Read ALL stage memories: loom memory show --all
3. Read doc/loom/knowledge/*.md

MEMORY CURATION:
4. Curate mistakes as actionable prevention rules (loom knowledge update mistakes).
5. Record the two dead-schema findings in concerns.md: before_stage and
   code_review were parsed-but-unrun before this plan; note to flag any other
   parsed-but-unused schema. Also note the still-dead post-commit stderr
   reminder in post-tool-use.sh (separate follow-up).
6. Curate the signal/hook architecture changes into architecture.md.
7. Run: loom review; remove stale entries.

DOCUMENTATION UPDATE (MANDATORY):
8. Update README.md / CONTRIBUTING.md to reflect: the new anti-slop guidance,
   the revived before_stage gate (plans can now use before_stage checks), and
   code_review dimensions now being surfaced to integration-verify. Only update
   relevant sections — do NOT rewrite whole files. If no user-facing behavior
   changed for a doc, skip and record WHY in memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- updated README.md Stage Fields table to add before_stage, after_stage, code_review rows; updated integration-verify Stage Type Behavior to mention code_review dimensions *(These are user-facing schema fields that authors can now rely on. CONTRIBUTING.md needed no changes — it only covers dev setup, quality gates, and release signing, none of which changed.)*

#### Notes

- stale knowledge locations identified: (1) architecture.md Hook System section says session-start.sh 'does NOT parse stdin source field' - STALE, it now does. (2) architecture.md post-tool-use.sh Compaction Recovery section describes removed mechanism. (3) architecture.md Handoff System item 5 says post-tool-use.sh detects recovery marker - STALE removed. (4) architecture.md before_stage/code_review table says code_review 'NOT surfaced to signal generation' - STALE, now wired for IV signals. (5) entry-points.md Hook Scripts table lists stale behavior for session-start.sh, post-tool-use.sh, pre-compact.sh.
- PLAN-anti-slop-thoroughness summary: (1) forcing-function-and-hooks: added append_anti_slop_guidance to cache.rs (called from all 4 prefix generators), changed session-start.sh to capture stdin and emit hookSpecificOutput re-anchor on compact/resume source, removed compaction-recovery reader (~17 lines) from post-tool-use.sh. (2) wire-dormant-gates: before_stage was already wired (plan was wrong), code_review wired via load_code_review_for_stage() reading from plan file in generate.rs (NOT added to Stage struct). (3) integration-verify fixed: moved load_stage_definition_from_plan from commands/verify.rs to plan/parser (natural layer), removed orphaned compaction-recovery WRITER from pre-compact.sh.

## Open Questions

No open questions.
