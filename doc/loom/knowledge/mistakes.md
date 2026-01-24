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

## Dependency stage marked complete without implementation

**What happened:** code-architecture-support stage was marked Completed without adding the Architecture enum to knowledge.rs. Integration-verify had to fix it.

**Why:** Agent didn't verify acceptance criteria passed before completing.

**How to avoid:** Always run acceptance criteria before marking stages complete.

## Acceptance Criteria Path Mismatch

**Issue:** Stage had working_dir: loom but acceptance paths assumed worktree root.

**Root cause:** Paths like loom/src/... failed when running from within loom/.

**Fix:** Use paths relative to working_dir: src/file.rs (not loom/src/file.rs), ../TEMPLATE (not TEMPLATE).

## PID Overflow Risk

**Where:** daemon/server/core.rs:71,82 casts `pid as i32`

**Issue:** PIDs on modern systems can exceed i32::MAX, causing wrap-around.

**Fix:** Use i32::try_from(pid)? to validate range before libc::kill().

**Lesson:** Always validate numeric conversions for system call parameters.

## Socket Permission Oversight

**Where:** daemon/server/lifecycle.rs binds Unix socket without setting permissions.

**Issue:** Socket created with default umask - may be readable/writable by other users.

**Risk:** Other users could connect to daemon, send commands, or cause DoS.

**Fix:** Set socket permissions explicitly after bind with chmod 0600 or fchmod.

## State Machine Bypass Patterns

**Where:** commands/stage/complete.rs:43-78 --force-unsafe flag; recovery.rs:278 recovery bypass.

**Issue:** Multiple paths allow bypassing state machine validation.

**Risk:** Can corrupt dependency tracking, allow invalid transitions, break invariants.

**Mitigation:** Log all bypasses, require explicit --assume-merged, warn users clearly.

## Debug Output in Production

**Where:** eprintln! statements with 'Debug:' prefix in complete.rs:127,130, orchestrator.rs:75,77.

**Issue:** Debug output mixed with user-facing messages in production code.

**Impact:** Confuses users, clutters output, inconsistent UX.

**Fix:** Use tracing crate with log levels, or remove debug output before release.

## Acceptance Path Mismatch (2026-01-24)

**What:** Criteria had 'test -f loom/src/...' but working_dir was 'loom', looking for 'loom/loom/src/...'

**Why:** File paths not adjusted for working_dir setting.

**Fix:** When working_dir is a subdir, paths must be relative to it. Use 'test -f src/foo.rs' not 'test -f loom/src/foo.rs'.

## Promoted from Memory [2026-01-24 15:04]

### Notes

- Added DIRECTORY HIERARCHY section with ASCII diagram, path resolution formula, and debugging checklist
- Updated format.rs with working_dir and Execution Path display in Target section plus WHERE COMMANDS EXECUTE reminder box

### Decisions

- **Implementing working_dir clarifications in sequential order: 1) CLAUDE.md.template with DIRECTORY HIERARCHY section, 2) format.rs with working_dir and execution_path in Target/Acceptance sections, 3) cache.rs Path Boundaries update, 4) tests**
  - *Rationale:* Tasks overlap files so sequential implementation is required per assignment


