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
  - _Rationale:_ Tasks overlap files so sequential implementation is required per assignment

## Acceptance Criteria working_dir Mismatch

**What happened:** cargo test/clippy failed because working_dir was '.' but Cargo.toml is in 'loom/'.

**Why:** Plan didn't verify working_dir matches where build tools run.

**Fix:** Set working_dir to directory containing Cargo.toml/package.json, or use 'cd loom &&' prefix.

## Promoted from Memory [2026-01-24 15:18]

### Notes

- Integration verification passed: all tests pass (16+28+3+5), clippy clean, build succeeds. DIRECTORY HIERARCHY section properly added to CLAUDE.md.template with three-level model diagram and path resolution formula. Signal format.rs properly includes working_dir display with execution path computation.

## Promoted from Memory [2026-01-24 15:22]

### Notes

- Fixed acceptance criteria path mismatch in plan: with working_dir='loom', paths must be relative to loom/ directory. Changed CLAUDE.md.template to ../CLAUDE.md.template and loom/src/... to src/... This demonstrates the exact issue the DIRECTORY HIERARCHY documentation was created to prevent.

## Promoted from Memory [2026-01-24 15:58]

### Decisions

- **Fixed case sensitivity in Stage Completion Checklist - changed 'before commit' to 'BEFORE COMMIT'**
  - _Rationale:_ The acceptance criteria pattern requires uppercase. Original template had lowercase which failed the pattern match.

## Promoted from Memory [2026-01-24 17:32]

### Notes

- Explored orchestrator core: 8 handler modules with clear separation of concerns. active_sessions accessed from 10+ locations - potential for refactoring to encapsulate access patterns.
- Error handling is excellent: zero unwrap() in main code, systematic use of anyhow::Result with context. No consolidation needed.
- Security patterns are solid: minisign verification for updates, 0o600 socket permissions, whitelist input validation. Worktree isolation is git-level only (not OS-level).

### Decisions

- **Knowledge update has 500 char limit - break content into multiple smaller updates (~20-30 lines each)**
  - _Rationale:_ Discovered during knowledge bootstrap when longer updates failed

## Promoted from Memory [2026-01-24 17:56]

### Decisions

- **Session state machine: Added Spawning -> Crashed transition**
  - _Rationale:_ H6: handle spawn failures gracefully
- **Reset command: Clear all timing and retry fields**
  - _Rationale:_ H10: leaving stale timing data
- **Atomic update: graph first, file second, rollback on failure**
  - _Rationale:_ H5: prevent inconsistent state

## Promoted from Memory [2026-01-24 19:22]

### Decisions

- **Used chars().take().collect::<String>() pattern for UTF-8 safe string truncation**
  - _Rationale:_ Byte-level slicing like &s[..n] can panic on multi-byte UTF-8 characters (emoji are 4 bytes, CJK are 3 bytes). Using chars().count() and chars().take(n) ensures we truncate at character boundaries, not byte boundaries.
- **Fixed file lock issue by writing directly to locked file handle**
  - _Rationale:_ fs::write() opens a NEW file handle which doesn't respect locks held by other handles. Instead, use file.set_len(0), file.seek(Start(0)), file.write_all() to write to the same locked handle.
- **Used bytes().take_while() for counting ASCII backticks in YAML parser**
  - _Rationale:_ When mixing string operations, stay in byte land consistently. find() returns byte positions, so use bytes().count() instead of chars().count() for ASCII characters like backticks to keep all positions in bytes.

## Promoted from Memory [2026-01-24 19:24]

### Notes

- Fixed recovery signal parsing (H11): Now properly parses crash_report_path, last_heartbeat, and recovery_actions from recovery signal files. Added helper functions parse_timestamp, parse_last_heartbeat, and parse_recovery_actions.
- Fixed signal format validation (H12): Added validation for expected section headers, logs warnings for missing required sections and unexpected sections. Clear error messages for missing stage_id.
- Fixed AppleScript injection (H33): Added escape_applescript_string function that escapes backslashes and quotes. Applied to both Terminal.app and iTerm2 script generation.
- Bug encountered: extract_field initially searched for 'Field:' but markdown format uses '**Field**:', causing parse failures. Fixed by updating function to try bold pattern first, then plain pattern.

### Decisions

- **Used toml crate serialization for config.toml generation instead of string formatting**
  - _Rationale:_ Prevents TOML injection attacks via malicious plan names/paths. toml::to_string_pretty properly escapes all string values.

## Promoted from Memory [2026-01-24 19:38]

### Notes

- Integration verification: All 185 tests pass (133 unit + 16 failure_resume + 28 integration + 3 stage_transitions + 5 doc-tests)
- Verified: Self-update uses minisign signature verification (src/commands/self_update/signature.rs) - downloads signature file, verifies BEFORE writing binary
- Applied cargo fmt to fix minor formatting differences in completion.rs, crash_handler.rs, recovery.rs

### Decisions

- **Acceptance criterion for banned git commands passes**
  - _Rationale:_ Matches in code are documentation warnings against the practice, not actual usage

## Promoted from Memory [2026-01-25 13:03]

### Notes

- extract_stage_from_worktree_path has no external callers - only used in its own tests. Simple deletion without needing to update imports elsewhere.

## Promoted from Memory [2026-01-25 13:05]

### Notes

- Consolidated get_merge_point and get_source_path into fs/mod.rs, updated progressive_merge to re-export from crate::fs, kept config_ops wrapper for WorkDir interface

### Decisions

- **Keep config_ops::get_plan_source_path for WorkDir interface, implement by calling new fs::get_source_path internally**
  - _Rationale:_ plan_lifecycle.rs uses WorkDir interface extensively, changing all callers would be more disruptive

## Promoted from Memory [2026-01-25 17:12]

### Notes

- Budget warning section placed in format_recitation_section for maximum attention (Manus pattern)
- Budget populated from stage.context_budget (u32) with CONTEXT_CRITICAL_THRESHOLD *100.0 as default, usage calculated as (tokens/limit)*100
- Signal enhancement complete: added context_budget and context_usage fields to EmbeddedContext, implemented budget warning display in format_recitation_section, populated fields in both generate functions from stage and session data
- BLOCKER: Branch has context_budget field in Stage/StageDefinition types but 50+ test files not updated to include this field in struct initialization. Need broader refactoring to fix all test code.

### Decisions

- **Added context budget fields to EmbeddedContext struct**
  - _Rationale:_ Enables signal files to display budget warnings when agents approach or exceed their context limits

## status-enhance acceptance criteria path issue

Criteria used 'loom/src/...' but working_dir='loom'. Fix paths to use 'src/...' not 'loom/src/...'

## codebase-mapping marked complete without code (2026-01-25)

Stage marked Completed but code never written:

- No loom map command
- No Stack/Concerns knowledge types
- No map module in src/

Stage marked complete without verifying acceptance criteria passed.

## Promoted from Memory [2026-01-25 23:08]

### Notes

- implement-fix stage was marked complete without actual implementation - knowledge.rs still requires .work directory

### Decisions

- **Removed work_dir.load() calls from knowledge commands**
