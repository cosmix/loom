# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
> This file is append-only - agents add discoveries, never delete.
>
> Format: Describe what went wrong, why, and how to avoid it next time.

(Add mistakes and lessons as you encounter them)

## Edited installed hook instead of source

**What:** Edited `~/.claude/hooks/loom/skill-trigger.sh` instead of `hooks/skill-trigger.sh` in the project.

**Why:** Followed settings.json path directly to installed file without considering source/install separation.

**Avoid:** Always edit hooks in project's `hooks/` directory. Installed copies (`~/.claude/hooks/loom/`) get overwritten on reinstall.

## Duplicate test files after refactoring

**What:** Splitting tests.rs into tests/mod.rs but not deleting original tests.rs caused E0761 (ambiguous module).

**Affected:** src/fs/permissions/ and src/verify/criteria/ had both tests.rs AND tests/mod.rs.

**Fix:** When refactoring tests.rs to tests/ directory, DELETE the original tests.rs file. Rust finds both patterns and fails.

## Acceptance criteria path issue

Used loom/src/... when working_dir=loom. Should use src/... (relative to working_dir).

## code-architecture-support Stage Marked Complete Without Changes

**What happened:** Stage marked completed but no code changes committed. Three subagent tasks were defined but none executed.

**Evidence:** Architecture variant missing from KnowledgeFile enum. No architecture refs in skill file. No branch/commits exist.

**Root cause:** stage_type: knowledge auto-sets merged=true before acceptance verification.

**Fix:** Run acceptance criteria BEFORE marking knowledge stages complete.
