# Code Review: Make Knowledge Commands Write to CWD Instead of Main Repo

**Plan:** PLAN-cwd-knowledge-resolution | **Generated:** 2026-04-16 06:19 UTC

## Summary

The plan enforces proper worktree isolation for knowledge commands by replacing path resolution from `main_project_root()` to `project_root()`, so knowledge files write to the current working directory instead of reaching back to the main repository. This requires knowledge-bootstrap to commit changes and makes integration-verify stages write knowledge to their worktree where it merges back with code. Changes span 6 command files, the knowledge stage signal, and the commit-guard hook, followed by comprehensive integration verification.

## Changes by Stage

### Integration Verification (integration-verify)

**Status:** executing  
**Purpose:** Final integration verification after all implementation stages complete.

Use parallel subagents and skills to maximize performance.

CRITICAL: This stage must verify FUNCTIONAL INTEGRATION, not just tests passing.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST — before any verification):
0a. Read the plan file from doc/plans/ (check .work/config.toml for source_path)
0b. Read ALL stage memories: loom memory show --all
0c. Read doc/loom/knowledge/*.md for architecture context and known mistakes

⛔ ZERO TOLERANCE: ALL compiler warnings, linter errors, test failures must be
FIXED (not suppressed). Nothing is "pre-existing" or "too trivial."

BUILD & TEST:

1. cargo test -p loom (full test suite)
2. cargo clippy -p loom -- -D warnings
3. cargo fmt -p loom --check
4. cargo build -p loom

FUNCTIONAL VERIFICATION (CRITICAL):
5. Verify NO remaining main_project_root() calls in knowledge command files:
   rg "main_project_root" loom/src/commands/knowledge/
   (should return 0 matches, or only in comments)
6. Verify the map command also uses project_root:
   rg "main_project_root" loom/src/commands/map.rs
   (should return 0 matches)
7. Verify knowledge stable prefix contains commit instructions:
   cargo test -p loom -- test_knowledge_stable_prefix_contains_required_sections --nocapture
8. Verify commit-guard.sh no longer early-exits for knowledge stages:
   rg "bypass commit requirement" hooks/commit-guard.sh
   (should return 0 matches)
9. Verify the renamed test exists and passes:
   cargo test -p loom -- test_knowledge_update_in_worktree_writes_to_worktree --nocapture

CODE REVIEW (MANDATORY):
10. Spawn loom-code-reviewer to verify:
    - No path traversal vulnerabilities introduced
    - project_root() return type handled correctly at each call site
    - No remaining semantic references to "write to main repo" in knowledge code
    - Signal content is consistent (no contradictions between sections)

KNOWLEDGE DISTILLATION (MANDATORY):
11. Record own discoveries to loom memory FIRST
12. Read all stage memories: loom memory show --all
13. Update doc/loom/knowledge/architecture.md:
    - Update the "StageType Enum" section (line 72) to note that Knowledge stages
      now require commits (remove "no commits" language)
    - Update "KnowledgeDir API" section if relevant
14. Update doc/loom/knowledge/patterns.md:
    - Update "Knowledge Systems Pattern" (line 80) to reflect that knowledge writes
      go to cwd, not main repo
    - Update "Stage Completion Pattern" (line 82-84) knowledge stage description
15. Remove any stale knowledge entries about "NO COMMITS" for knowledge stages

#### Files Changed

No changes recorded.

#### Key Decisions

- review/generate.rs main_project_root() usage is intentional *(loom review needs main repo .work/ state for cross-stage review compilation — not a knowledge command, does not write to knowledge dir)*

#### Notes

- found: patterns.md stage completion pattern (line 82-84) says 'No worktree, auto merged=true, skips merge' without mentioning commits are now required
- found: architecture.md line 72 says 'No worktree, no commits' for Knowledge stages — stale after CWD knowledge resolution plan made commits required
- found: stale doc comments in signals/knowledge.rs (lines 4, 22) said 'don't require commits' but cache.rs signal says COMMITS REQUIRED. Fixed both comments to match new behavior.

## Open Questions

No open questions.
