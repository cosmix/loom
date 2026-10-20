# Code Review: Plan: Unify truths/truth_checks into acceptance

**Plan:** PLAN-unify-acceptance | **Generated:** 2026-03-26 21:49 UTC

## Summary

This plan unifies three overlapping verification systems (acceptance, truths, truth_checks) into a single AcceptanceCriterion enum supporting both simple shell commands and enhanced stdout/stderr checks. The refactoring eliminates semantic confusion for plan authors, consolidates verification logic into one runner, and removes the redundant fields from 16 files across the codebase via three parallel subagents.

## Changes by Stage

### Bootstrap Knowledge Base (knowledge-bootstrap)

**Status:** completed  
**Purpose:** Verify existing knowledge coverage for loom codebase.
Knowledge files already exist and are populated.

Use parallel subagents and skills to maximize performance.

Tasks:

1. Run loom knowledge check to verify coverage >= 50%
2. Review doc/loom/knowledge/mistakes.md for relevant past issues
3. Briefly scan architecture.md and patterns.md for verification system context

MEMORY RECORDING:

- Record any gaps found: loom memory note "gap: description"
- Record relevant mistakes: loom memory note "relevant mistake: description"

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- context: Goal-backward verification currently has 3 layers (truths, artifacts, wiring). After unification, truths/truth_checks merge into acceptance field, goal-backward keeps only artifacts/wiring/wiring_tests/dead_code_check. before_stage/after_stage kept as-is (different lifecycle).
- relevant mistake: 'Goal-Backward Verification: False Negatives' — grep patterns must match actual visibility modifiers. Relevant for wiring checks in the unified acceptance plan.
- relevant mistake: 'Test Code: Struct Init Without Default' — Stage struct tests use explicit constructors. Adding/removing fields (truths, truth_checks) breaks ~10 locations. Subagent 3 handles this.

### Unify truths/truth_checks into acceptance (unify-acceptance)

**Status:** completed  
**Purpose:** Replace truths and truth_checks fields with a unified acceptance field that
supports both simple string commands and enhanced TruthCheck-style entries.

Use parallel subagents and skills to maximize performance.

DESIGN SUMMARY:

- New AcceptanceCriterion enum (untagged serde): Simple(String) | Extended(TruthCheck)
- Remove truths and truth_checks fields from StageDefinition and Stage
- Keep before_stage, after_stage (different lifecycle purpose)
- Keep artifacts, wiring, wiring_tests, dead_code_check (non-shell-command)
- Acceptance runner handles both variants (simple=5min, extended=30s timeout)
- Goal-backward verification no longer handles shell commands
- has_any_goal_checks() checks only artifacts/wiring/wiring_tests/dead_code_check
- Validation: standard stages need non-empty acceptance OR goal-backward checks

SUBAGENT FILE ASSIGNMENTS:

Subagent 1 - Core Types & Plumbing:
  Files Owned: src/plan/schema/types.rs, src/models/stage/types.rs,
               src/models/stage/methods.rs, src/commands/init/plan_setup.rs,
               src/plan/schema/validation.rs
  Files Read-Only: src/verify/ (for understanding current usage)
  Task: Define AcceptanceCriterion enum, remove truths/truth_checks fields,
        add PartialEq to TruthCheck, update has_any_goal_checks(),
        update validation rules. Add helper methods command() and is_extended()
        on AcceptanceCriterion.

Subagent 2 - Verification Logic & Commands:
  Files Owned: src/verify/criteria/runner.rs, src/verify/goal_backward/mod.rs,
               src/verify/goal_backward/truths.rs, src/verify/mod.rs,
               src/commands/verify.rs, src/commands/stage/verify.rs,
               src/commands/stage/criteria_runner.rs,
               src/commands/stage/acceptance_runner.rs,
               src/orchestrator/signals/generate.rs
  Files Read-Only: src/plan/schema/types.rs (for new types),
                   src/verify/before_after.rs (keep as-is, uses TruthCheck)
  Task: Update acceptance runner to handle AcceptanceCriterion enum.
        For Simple: existing logic (5min timeout, exit 0).
        For Extended: 30s timeout, check exit_code, stdout_contains,
        stdout_not_contains, stderr_empty (reuse verify_truth_checks logic).
        Remove truths/truth_checks from goal-backward mod.rs.
        Remove verify_truths() from truths.rs (keep verify_truth_checks for before_after).
        Update display messages in commands/verify.rs.
        Update skill matching in signals/generate.rs.

Subagent 3 - Mechanical Updates:
  Files Owned: src/commands/stage/recover.rs, src/orchestrator/core/mod.rs,
               src/orchestrator/core/event_handler.rs, src/fs/stage_loading.rs,
               src/git/merge/status.rs, src/git/worktree/base.rs,
               src/commands/status/data/collector.rs,
               src/commands/status/ui/tui/renderer.rs,
               src/commands/status/ui/tree_widget_tests.rs,
               src/plan/schema/tests/mod.rs, src/plan/graph/tests.rs,
               src/commands/init/tests.rs, src/commands/run/tests.rs
  Files Read-Only: src/models/stage/types.rs (for new field shape)
  Task: Remove truths: Vec::new() and truth_checks: Vec::new() lines from
        all Stage and StageDefinition constructors. These fields no longer exist.
        Also update any test assertions that reference these fields.

NO FILE OVERLAP between subagents confirmed.

MEMORY RECORDING (use memory ONLY — never knowledge):
Record IMMEDIATELY when these happen — not at stage end:

- MISTAKE: tried X, failed -> loom memory note "mistake: ..."
- DECISION: chose X over Y -> loom memory decision "chose X" --context "why"
- SURPRISE: unexpected behavior -> loom memory note "found: ..."

#### Files Changed

- src/plan/schema/types.rs - added AcceptanceCriterion enum (Simple|Extended), Display impl, removed truths/truth_checks from StageDefinition, updated has_any_goal_checks
- src/models/stage/types.rs - changed acceptance to Vec<AcceptanceCriterion>, removed truths/truth_checks fields from Stage
- src/verify/goal_backward/ - removed truths/truth_checks from verification, kept verify_truth_checks for before_after.rs
- src/verify/criteria/runner.rs - acceptance runner now handles both Simple (5min) and Extended (30s + output checks) criteria
- src/plan/schema/validation.rs - updated validation: standard stages need acceptance OR goal-backward checks (not truths)

#### Key Decisions

- unified acceptance design: AcceptanceCriterion enum with Simple(String)|Extended(TruthCheck), untagged serde for YAML compat. Removed truths/truth_checks from StageDefinition and Stage. has_any_goal_checks() only checks artifacts/wiring/wiring_tests/dead_code_check. Validation requires non-empty acceptance OR goal-backward checks for standard stages. *(this is the cleanest approach: truths become acceptance criteria with 5min timeout, truth_checks become Extended acceptance criteria with 30s timeout + output checks. Goal-backward verification focuses on non-shell checks only.)*
- kept verify_truth_checks in truths.rs but removed verify_truths *(before_after.rs depends on verify_truth_checks for pre/post-condition verification. verify_truths only handled the old Vec<String> truths field which is being removed.)*
- replaced stage.truths with stage.artifacts in test files to satisfy goal-backward validation, since truths field was removed *(truths were used to satisfy has_any_goal_checks() in tests; artifacts serves same purpose)*

#### Notes

- found: validation_tests.rs test_integration_verify_with_truths_passes was renamed to test_integration_verify_with_artifacts_passes since truths no longer exist
- Core types changes complete: AcceptanceCriterion enum added, truths/truth_checks removed from StageDefinition and Stage, validation updated. Remaining 19 errors all in files owned by other subagents: signals/format, signals/generate, signals/knowledge, signals/recovery_format, verify/transitions/serialization, plan/graph/mod, fs/stage_loading, commands/diagnose, orchestrator/core/persistence

### Integration Verification (integration-verify)

**Status:** executing  
**Purpose:** Final integration verification after unification merge.

Use parallel subagents and skills to maximize performance.

CRITICAL: Verify FUNCTIONAL INTEGRATION, not just tests passing.

Build/Test Tasks:

- Full test suite (cargo test)
- Linting (cargo clippy -- -D warnings)
- Build verification (cargo build)

CODE REVIEW (MANDATORY):

- Spawn parallel review subagents:
  - security-engineer: check for injection in shell command handling
  - senior-software-engineer: verify clean enum design, no dead code from removed fields
  - /loom-testing skill: coverage of AcceptanceCriterion variants

FUNCTIONAL VERIFICATION (CRITICAL):

- Create a test plan YAML with both simple and extended acceptance criteria
- Run loom init on it to verify parsing works
- Verify old plans with truths: field don't crash (serde ignores unknown fields)
- Verify goal-backward still runs artifacts/wiring checks correctly

KNOWLEDGE CURATION (MANDATORY):

- Read all stage memory: loom memory show --all
- Curate mistakes to: loom knowledge update mistakes "..."
- Update patterns.md: loom knowledge update patterns "..." (Goal-Backward section)

DOCUMENTATION UPDATE (MANDATORY):

- Update doc/loom/knowledge/architecture.md (Goal-Backward section)
- Update doc/loom/knowledge/patterns.md (Goal-Backward section)
- Update doc/loom/knowledge/conventions.md (Plan YAML Schema section)

#### Files Changed

No changes recorded.

#### Key Decisions

- GapType::TruthFailed retained in result.rs because it is still used by verify_truth_checks() which powers before_after.rs checks - not dead code *(truths.rs module is intentionally kept as it provides the TruthCheck execution logic used by before_stage/after_stage verification)*
- accepted AcceptanceCriterion #[serde(untagged)] enum design *(Simple(String)|Extended(TruthCheck) with untagged serde is correct — strings try Simple first (succeed), objects try Simple (fail) then Extended (succeed). Error messages for malformed objects are poor but acceptable.)*

#### Notes

- found: README.md and 5 skill files still reference truths as a valid plan field — needs updating in separate pass since skills/ is outside this plan's file scope
- found: security review identified inconsistent validation — setup, wiring_tests.command, dead_code_check.command lack same validation as acceptance criteria (pre-existing, not from this change)
- found: StageDefinition has no deny_unknown_fields — old plans with truths:/truth_checks: parse silently (data dropped). Acceptable per CLAUDE.md no-backwards-compat policy.
- found: old plans with truths: field parse fine (serde ignores unknown fields) but the truths data is silently LOST. If an old plan relied ONLY on truths for goal-backward verification with no artifacts/wiring, it would now fail validation since has_any_goal_checks() no longer counts truths. This is a semantic breaking change for old plans.
- found: README.md lines 190-209, 226, 242 still reference truths/truth_checks as valid plan fields. Multiple skill files (loom-plan-writer, loom-before-after, loom-wiring-test, loom-dead-code-check) also reference truths. Knowledge files (architecture.md:147, patterns.md:88, mistakes.md:12,116, entry-points.md:116) also reference truths. All need updating.
- found: installed loom binary (/home/dkaponis/.local/bin/loom) is from 15:39:05, BEFORE the unify-acceptance changes. Unit tests pass but loom init fails because it runs the old binary that has acceptance: Vec<String> instead of Vec<AcceptanceCriterion>. The debug binary at loom/target/debug/loom has the changes (23:30:43).
- Fixed 2 code issues: (1) stale comment in complete.rs:393 updated from 'truths, artifacts, wiring' to 'artifacts, wiring, wiring_tests, dead_code', (2) removed truths: fields from e2e fixture plans.rs - these were silently ignored by serde but misleading in test fixtures
- Architecture review of unify-acceptance: Found 5 categories of issues - (1) stale comment in complete.rs:393 referencing truths, (2) e2e fixture plans.rs still has truths: fields in YAML which are silently ignored by serde, (3) README.md extensively references truths field as if it still exists, (4) skill files reference truths everywhere, (5) knowledge files reference truths as current. Code changes themselves are clean and well-structured.
- Security review of acceptance criteria execution system completed. Key findings: (1) command injection is by-design (trusted model), (2) setup/wiring_tests/dead_code_check/change_impact commands lack validation, (3) Extended criteria TruthCheck fields not validated beyond command, (4) output checking uses substring matching which is correct for the use case, (5) ReDoS potential in wiring regex patterns

## Open Questions

No open questions.
