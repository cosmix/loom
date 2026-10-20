# Code Review: Split Knowledge Distillation from Integration-Verify

**Plan:** PLAN-split-knowledge-distill | **Generated:** 2026-04-16 07:57 UTC

## Summary

This plan introduces a new `KnowledgeDistill` stage type to split knowledge curation (mechanical work) from `integration-verify` (code review and functional verification). By isolating knowledge distillation into its own worktree stage running on sonnet with high reasoning effort, integration-verify becomes faster while knowledge curation becomes cheaper. The implementation updates the StageType enum, signal generation system, plan-writer skill documentation, and knowledge files to establish the new four-stage bookend pattern (knowledge-bootstrap → implementation → integration-verify → knowledge-distill).

## Changes by Stage

### Integration Verification (integration-verify)

**Status:** executing  
**Purpose:** Final integration verification after all stages complete.

Use parallel subagents and skills to maximize performance.

CRITICAL: Verify FUNCTIONAL INTEGRATION, not just tests passing.

CONTEXT GATHERING (FIRST):

- Read the plan file from doc/plans/ (check .work/config.toml for source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for architecture context and known mistakes

BUILD & TEST (zero tolerance):

1. cargo build
2. cargo test (all tests)
3. cargo clippy -- -D warnings
4. cargo fmt --check

CODE REVIEW (MANDATORY):
5. Spawn PARALLEL loom-code-reviewer subagents:

- security review: OWASP Top 10, input validation, secrets
- architecture review: code organization, patterns, coupling
1. Fix ALL issues found

FUNCTIONAL VERIFICATION (MANDATORY):
7. Verify StageType::KnowledgeDistill is properly wired:

- Enum variant exists and serializes as "knowledge-distill"
- detect_stage_type() recognizes it
- is_knowledge_stage() returns FALSE for knowledge-distill
- check_knowledge_recommendations() doesn't count knowledge-distill as a knowledge stage
- Signal prefix selection dispatches correctly
- Semi-stable section shows knowledge management content
- Cross-stage summary and wiring checklist populated for KD
1. Verify integration-verify prefix NO LONGER contains knowledge distillation
2. Verify knowledge-distill prefix DOES contain knowledge distillation
3. Validate the new type end-to-end by creating a minimal test plan
    in a temp dir with a knowledge-distill stage and running loom init
    against it to verify parsing and validation succeed
4. Run: cargo test -- --test-threads=1 (verify no parallel test issues)

REPO-WIDE STALE REFERENCE CHECK:
12. Run: rg -i "integration.verify.*last\|last.*integration.verify" .
    to find any remaining docs/comments that claim IV is the last stage
13. Run: rg "curate.*knowledge\|knowledge.*distill\|memory.*curate" loom/src/orchestrator/signals/cache.rs
    to verify no knowledge distillation content remains in IV prefix
14. Fix any stale references found

KNOWLEDGE DISTILLATION (MANDATORY):
15. Record own discoveries to loom memory FIRST
16. Read all stage memories: loom memory show --all
17. Curate valuable insights into permanent knowledge
18. Update architecture.md if structure changed
19. Generate review document: loom review

#### Files Changed

No changes recorded.

#### Key Decisions

- did not remove is_knowledge_stage() dead code *(It is pre-existing, not introduced by KnowledgeDistill changes. Removing it would change the scope of this integration-verify.)*

#### Notes

- gotcha: Bash tool escapes HTML comment char in all contexts. Writing plan files with <\!-- markers requires Python with chr() or base64 workaround. Heredocs, printf, and even Python string literals all get the char escaped by the shell layer.
- found: is_knowledge_stage() in methods.rs:443 is dead code — defined but never called. All call sites use direct stage_type comparison. Not a new issue (pre-existing), but the KD exclusion logic was added to a dead method.
- found: sections.rs:247 comment said 'integration-verify only' but the code block was inside a KnowledgeDistill check. Fixed. Prevention: update comments when moving code between stage-type branches.
- found: SKILL.md line 979 still said 'Integration Verify Stage (Last)' but it is now second-to-last with knowledge-distill as last. Fixed. Prevention: when adding bookend stages, search all docs for ordering claims.
- found: entry-points.md had stale '3 stage-type variants' after KnowledgeDistill was added as 4th variant. Fixed to '4'. Prevention: when adding new enum variants, grep all knowledge files for old counts.

## Open Questions

No open questions.
