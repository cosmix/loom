# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
>
> **Format:** Describe what went wrong, why, and how to avoid it next time.
>
> **Related files:** [conventions.md](conventions.md) for correct patterns, [patterns.md](patterns.md) for design guidance.

## Paths: working_dir Mismatch (Recurring)

**Mistake:** Acceptance criteria, artifact paths, and file checks used absolute paths like `loom/src/...` when `working_dir` was already `loom`, producing double-paths like `loom/loom/src/...`. Occurred in 5+ separate plans.
**Fix:** ALL paths in acceptance/artifacts/truths/wiring are relative to `working_dir`. If `working_dir: "loom"`, use `src/file.rs` not `loom/src/file.rs`. Set `working_dir` to where `Cargo.toml`/`package.json` lives.

## Stages: Marked Complete Without Implementation (Recurring)

**Mistake:** Multiple stages were marked Completed with no code committed. `stage_type: knowledge` auto-sets `merged=true` which masked missing work.
**Fix:** Always run acceptance criteria BEFORE marking stages complete. Verify actual artifacts exist.

## Phantom Merges: merged=true Without Verification

**Mistake:** `try_auto_merge()` set `merged=true` without verifying the commit was in target branch history. Merge verification errors fell through to `merged=true` fallback. Agents also edited `.work/` files directly.
**Fix:** Use `is_ancestor_of()` to verify merge before setting `merged=true`. Treat verification errors as `MergeBlocked`. Never edit `.work/` files directly.

## Binary: PATH vs target/debug/loom

**Mistake:** Agents invoked stale `target/debug/loom` instead of the installed version from PATH.
**Fix:** Always use `loom` from PATH. Exception: integration-verify of unreleased features may use `./loom/target/debug/loom`.

## Security: Consolidated Findings

- **Socket permissions:** Created with default umask (world-accessible). Fix: `umask(0o077)` before bind.
- **PID handling:** `pid as i32` can overflow; raw `libc::kill` mishandles `EPERM`/`ESRCH`. Fix: use `nix::sys::signal::kill`.
- **Script injection:** AppleScript/XTerm strings not escaped. Fix: escape backslashes and quotes.
- **TOML injection:** `config.toml` via string formatting. Fix: use `toml::to_string_pretty`.
- **File locking TOCTOU:** `locked_write` truncated before lock. Fix: extracted `fs/locking.rs` with open-lock-truncate-write-flush.
- **State machine bypass:** `--force-unsafe` and recovery bypass skip validation. Fix: log all bypasses.

## File Locking: Writing to Locked Handles

**Mistake:** `fs::write()` opens a NEW handle that ignores locks held by other handles.
**Fix:** Write to the locked handle: `file.set_len(0)`, `file.seek(Start(0))`, `file.write_all()`.

## String Handling: UTF-8 Truncation Panic

**Mistake:** Byte-level slicing `&s[..n]` panics on multi-byte UTF-8 characters.
**Fix:** Use `chars().take(n).collect::<String>()` for safe truncation.

## Source vs Installed: Editing Wrong File

**Mistake:** Edited `~/.claude/hooks/loom/` (installed copy) instead of `hooks/` (source). Lost on reinstall.
**Fix:** Always edit in project's `hooks/` directory.

## Module Refactoring: Duplicate Files

**Mistake:** Splitting `tests.rs` into `tests/mod.rs` without deleting original caused E0761.
**Fix:** When refactoring `foo.rs` to `foo/mod.rs`, DELETE the original file.

## Goal-Backward Verification: False Negatives

**Mistake:** (1) `cargo test 2>&1 | tail -1` fails due to trailing newline. (2) `pub fn foo` pattern misses `pub(super) fn foo`.
**Fix:** Filter for target line first, then check. Use regex `pub.*fn foo` to match all visibility modifiers.

## Permission Sync: Three Related Bugs

**Mistake:** (1) `copy_file_with_shared_lock` overwrote worktree permissions instead of merging. (2) Permissions with parent-relative or worktree paths filtered out. (3) Sync skipped when acceptance failed.
**Fix:** (1) Merge both sets before writing. (2) Transform to portable relative paths. (3) Sync unconditionally before checking acceptance.

## Sandbox: Contradictory Path Rules

**Mistake:** `merge_config()` added `doc/loom/knowledge/**` to both `allow_write` and `deny_write`.
**Fix:** Removed auto-add. Knowledge writes go through `loom` CLI (outside sandbox). Same path must never appear in both.

## Merge Handler: Inline Branch Names

**Mistake:** 8 instances of `format!("loom/{}")` instead of `branch_name_for_stage()`.
**Fix:** Always use `branch_name_for_stage()` for branch name construction.

## Test Code: Struct Init Without Default

**Mistake:** Stage struct tests use explicit constructors without `..Default::default()`. Adding new fields breaks ~10 locations.
**Fix:** Use `..Stage::default()` pattern. Also check `tests/` directory (not just `src/`) when adding fields.

## Timing: Missing Accumulation on Exit Transitions

**Mistake:** `accumulate_attempt_time` not called on `NeedsHandoff`/`BudgetExceeded`, permanently losing execution time.
**Fix:** Call `accumulate_attempt_time` on ALL exit transitions, not just `Completed`.

## Debug Output in Production

**Mistake:** `eprintln!` with `Debug:` prefix left in production code.
**Fix:** Use `tracing` crate with proper log levels.

## Test Environment Race Condition

**Mistake:** `test_loom_terminal_env_var_takes_precedence` uses `std::env::set_var` without `serial_test`.
**Fix:** Use `#[serial]` attribute on tests that modify environment variables.

## Daemon Module Visibility

**Mistake:** Used `crate::daemon::server::DaemonServer` but `server` module is private.
**Fix:** Use re-export path: `crate::daemon::DaemonServer`.

## Acceptance: Case Sensitivity in Patterns

**Mistake:** Template had lowercase text but acceptance criteria grep pattern required uppercase.
**Fix:** Ensure template text matches the exact case of acceptance criteria patterns.

## detection.rs: Session Exit for Merge States

**Mistake:** `detection.rs` only recognized `Completed` as normal session exit. Merge conflict sessions treated as crashes.
**Fix:** Added `MergeConflict | MergeBlocked` to the matches! pattern. When adding new terminal stage statuses, always update detection.rs.

## loom check: Negation Patterns are Literal

**Mistake:** Wiring check for `!Merge` was a false positive -- `!` is literal, not negation.
**Fix:** Use positive patterns in wiring checks. Use `truths` with shell commands for absence checks.

## Subagent File Overlap Causes Lost Work

**Mistake:** Multiple subagents writing the same file leads to lost work (last writer wins).
**Fix:** Every subagent MUST have exclusive write access to its files. Use file ownership tables. If overlap is unavoidable, use one subagent or handle sequentially.

## loom knowledge update: Path Resolution

**Mistake:** Running `loom knowledge update` from a subdirectory creates files relative to cwd, not worktree root.
**Fix:** Always run knowledge commands from the worktree root.

## Skill Documentation Freshness

**Mistake:** Skill files referenced old schema state after fields were added/removed.
**Fix:** Update skill files and feature code together when changing schemas.

## loom merge Command Removal

**Lesson:** `loom merge` duplicated `loom stage complete` functionality with 5 bugs. Removed entirely rather than fixing. When a command duplicates existing functionality and has multiple bugs, removal is better than repair.

## Using npx Instead of bunx

**Mistake:** Used npx instead of bunx during implementation.
**Fix:** Always use `bun`/`bunx` per project conventions. Check CLAUDE.md tool preferences before running package managers.

## Truths → Acceptance Unification

**What happened:** truths and truth_checks were separate fields on StageDefinition/Stage that overlapped with acceptance criteria. Unified into AcceptanceCriterion enum (Simple|Extended).

**Gotcha:** Old plans with truths: field parse without error (serde ignores unknown fields) but the data is silently dropped. If old plan relied ONLY on truths for goal-backward verification (no artifacts/wiring), validation now fails.

**How to avoid:** When removing fields from serde structs, consider adding deprecation warnings via custom deserializer for at least one release cycle. Not done here because project CLAUDE.md says no backwards compatibility needed.

## Stale References After Field Removal

**What happened:** After removing truths/truth_checks fields, stale references remained in comments (complete.rs:393), e2e test fixtures (plans.rs), README.md, skill files, and knowledge files.

**How to avoid:** When removing a struct field, grep the ENTIRE project (not just src/) for references. Include: tests/, doc/, skills/, README, knowledge files, comments, YAML fixtures.
