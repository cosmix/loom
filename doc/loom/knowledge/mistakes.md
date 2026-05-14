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

## Phantom Merges from Defensive "Assume Merged" Branches (2026-04-15)

**What happened:** Seven daemon-side code paths wrote `merged: true` to escape an earlier respawn-loop bug without verifying git ancestry. A user lost real work: stage `oauth-hardening` was marked merged but its commits stayed stranded on `loom/oauth-hardening`; a downstream stage then worktreed off main and produced overlapping, incomplete code. Smoking gun log: `Completed stage has no completed_commit, assuming merged stage_id=integration-verify`.

**Misleading signal:** The original respawn-loop bug (commit `1af9827`, see `doc/merge-resolve-bug-notes.md`) was patched by force-writing `merged=true` when stage was already Completed — the rationale "stage's work is done, don't revert to MergeBlocked" looked defensible. Similarly, seven separate sites used "assume already merged" / "legacy stage" / "avoid stuck-in-MergeBlocked loops" as justification for lying about merge state.

**Why it broke:** `merged=true` is a contract with the dependency scheduler. Dependents satisfy their deps by reading `dep.merged`. Lying about it silently propagates broken state across the DAG: dependents spawn with a wrong base branch, their commits overlap partially with the unmerged dep, progressive merge fails downstream.

**Prevention — INVARIANT:** **Daemon-side automated paths MUST NEVER write `merged: true` without git ancestry verification (`is_ancestor_of` returning `Ok(true)`).** The only exemptions are explicit user intent: `loom stage complete --force-unsafe --assume-merged`, `loom stage merge --resolved`, knowledge stages (no branch by design), and `loom worktree remove` cleanup.

**Detection rules for future work:**

- Any `stage.merged = true` write outside the exemption list is a phantom-merge candidate. Must be preceded by a git-verified `is_ancestor_of(completed_commit, target_branch)` returning `Ok(true)`.
- "Stage is Completed (terminal), can't go back" is NOT a license to write merged=true. `Completed + !merged` is a valid resting place — `spawn_merge_resolution_sessions` only acts on `MergeConflict`/`MergeBlocked`, so no respawn loop results.
- Dependency scheduling must cross-check ancestry (`are_all_dependencies_satisfied` in `verify/transitions/state.rs`), not trust the `merged` flag alone. Knowledge stages are the only exemption.
- `loom repair` catches stages with `merged: true` whose commit is not in the target branch — run on suspected phantom merges.

**Fix (implemented in this change):** Seven writer sites (recovery.rs, merge_handler.rs × 5, progressive_complete.rs) now leave `Completed + !merged` as the resting state instead of lying. `check_merge_state` returns `Unknown` for non-knowledge stages whose merged flag can't be ancestry-verified. `are_all_dependencies_satisfied` cross-checks ancestry per dep. `start_stage` adds a spawn-time defense-in-depth check. A one-shot retry on daemon start handles the `--no-verify`-then-restart case. `loom repair` detects and reverts phantom merges. Status UI renders `Completed + !merged` as yellow "unmerged" with a hint to run `loom stage merge <id>`.

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

## gawk vs POSIX awk (2026-03-31)

**What happened:** Initial `_common.sh` used gawk-specific `match()` with array capture (3rd argument), which failed with syntax errors on standard awk and macOS default awk.
**Why:** gawk extensions are not available on all platforms. macOS ships with BSD awk.
**How to avoid:** Always use POSIX awk features only. For complex string extraction, use `substr()`+`sub()` approach instead of `match($0, pattern, arr)`.

## Hook Integration Tests Need _common.sh (2026-03-31)

**What happened:** After adding `_common.sh` as a dependency sourced by hooks, 12 integration tests in `hooks_commit_filter.rs` failed because the test setup didn't install `_common.sh` alongside the hook script.
**Why:** Hooks source `_common.sh` via `source "$(dirname "$0")/_common.sh"` — tests must install all dependencies in the temp directory.
**How to avoid:** When adding shared utilities sourced by hooks, update ALL integration test `setup_hook()` functions to also install the shared utility.

## Cross-Platform Timeout in Hooks (2026-03-31)

**What happened:** `git-add-guard.sh` used bare `timeout` command without `gtimeout` fallback, which fails silently on macOS without GNU coreutils.
**Why:** macOS doesn't have `timeout` by default; GNU coreutils provides it as `gtimeout`.
**How to avoid:** All hooks reading stdin MUST use the three-way cascade: `gtimeout` → `timeout` → `cat`.

## Knowledge Commands: CWD Resolution (2026-04-16)

**What happened:** Knowledge commands used `main_project_root()` which followed `.work` symlinks to resolve to the main repo root. In worktree contexts (e.g., integration-verify stages), `loom knowledge update` wrote to the main repo instead of the worktree, causing cross-worktree state pollution.
**Why:** `main_project_root()` was designed to always find the true main repo root, which was correct for `.work/` state but wrong for knowledge files that should be worktree-local.
**Prevention:** Use `project_root()` (cwd-relative) for file writes that should respect worktree isolation. Use `main_project_root()` only for accessing shared state (`.work/`). Always run `loom knowledge update` from the worktree root, not a subdirectory.
**Fix:** Replaced all `main_project_root()` calls in knowledge commands and map.rs with `project_root()`. Updated signal content to require commits for knowledge stages. Removed commit-guard.sh bypass for knowledge stages.

## Merge Conflict Session Lifecycle: Original Session Continued Running (2026-04-16)

**What happened:** When `loom stage complete` detected a merge conflict during progressive merge, the original execution session continued running instead of exiting. Three coordinated issues prevented clean handoff to the resolution session:

1. `complete_with_merge()` returned `Ok(false)` on merge conflict, which propagated back to `complete.rs:623` without error — the session stayed alive
2. `commit-guard.sh` (Stop hook) set `stage_incomplete=1` for `MergeConflict` status, blocking the session from exiting even if it tried
3. `spawn_merge_resolution_sessions()` didn't kill the stale original session, leaving a zombie process that blocked merge resolver spawning

**Why:** The `Ok(false)` return was designed for "merge didn't succeed but keep running" — wrong mental model. Merge conflict means "your work is done, hand off to resolver." The commit-guard didn't distinguish between "stage still executing" and "stage waiting for merge resolution." And session cleanup assumed sessions would exit on their own.

**Prevention:**

- When adding new terminal/handoff stage statuses, always update: (1) `complete_with_merge` return behavior, (2) `commit-guard.sh` case statement, (3) `detection.rs` normal-exit matches, (4) `spawn_merge_resolution_sessions` cleanup logic
- Use `bail\!()` not `Ok(false)` when the session MUST exit — `Ok(false)` leaves the caller alive
- Test the full lifecycle: stage completes → merge conflicts → original session exits → resolver spawns → resolver resolves

**Fix:** Four-part coordinated change:

- `progressive_complete.rs`: Changed `Ok(false)` to `bail\!()` for Conflict and Blocked arms, forcing session exit with clear message
- `commit-guard.sh`: Changed MergeConflict case to allow session exit (no longer sets stage_incomplete)
- `merge_handler.rs`: Added `kill_session()` call for stale Stage sessions before spawning merge resolver
- `merge.rs`: Added "Inherited Responsibilities" section to merge signal explaining resolver owns the stage

## Stale Documentation After Adding Enum Variants (2026-04-16)

**What happened:** After adding KnowledgeDistill as the 4th StageType variant, three stale references remained: entry-points.md said 3 variants (should be 4), SKILL.md said Integration Verify Stage (Last) (now second-to-last), and sections.rs comment said integration-verify only (code had moved to KnowledgeDistill block).

**Why:** The implementation stage focused on Rust code changes and missed docs/comments that reference counts or ordering.

**Prevention:** When adding a new enum variant that changes ordering or counts, search all knowledge files for old counts, search skills for ordering claims, and search source comments for stale stage-type references.

## Phantom Merges from `--force-unsafe` Shortcuts (2026-04-27)

**What happened:** `loom stage complete --no-verify --force-unsafe --assume-merged` (and a related `--force-unsafe` alone path) wrote `merged: true` without ever verifying git ancestry. Three concrete failure modes:

1. **Phantom merge via `--assume-merged`.** `complete.rs::handle_force_unsafe_completion` set `merged = true` regardless of git reality, re-introducing the phantom-merge class via a user shortcut.
2. **Stuck `Completed + !merged` with active merge.** With `--force-unsafe` alone after a previous resolver session died mid-merge (`.git/MERGE_HEAD` set), the daemon retry called `merge_stage`, which failed; the next resolver ran `get_conflicting_files_from_status`, which destructively `git merge --abort`ed the existing active merge.
3. **`loom stage complete` on a `MergeConflict` stage.** Ran the full acceptance + goal-backward + progressive-merge pipeline, none of which is the resolver's job.

**Misleading signal:** Both `--force-unsafe` shortcuts looked defensible because they were "explicit user intent". But `--force-unsafe --assume-merged` made `merged: true` a contract violation: the dependency scheduler reads `dep.merged` and queues dependents as if the work landed. Cross-references the existing 2026-04-15 `Phantom Merges from Defensive "Assume Merged" Branches` entry — this is the user-shortcut variant of the same class.

**Why it broke:** Three preconditions all had to be wrong simultaneously: (a) no attribution check tied `MERGE_HEAD` to a specific stage, (b) `--assume-merged` skipped ancestry verification, (c) helpers that mutate git state (`merge_stage`, `get_conflicting_files_from_status`) had no guard against running over an in-progress merge. Together they made the active merge invisible to recovery.

**Prevention — Routing-and-Attribution INVARIANT:** *An active merge on disk may block or guide recovery, but it must not mutate a stage unless loom can attribute that merge to that stage.*

- `MERGE_HEAD` in the main repo is global. Every state-machine mutation triggered by detection must come with proof of attribution: orphaned `SessionType::Merge` metadata, `MERGE_HEAD` commit matching `loom/<stage-id>` HEAD, or `completed_commit` match. Without attribution, refuse — never mutate.
- `--force-unsafe --assume-merged` must verify ancestry via `verify_merge_succeeded` before writing `merged=true`.
- `--force-unsafe` alone must refuse if an attributed active merge exists for THIS stage (would orphan MERGE_HEAD).
- Routing must be a pure read-only function (`route_complete_for_conflicts`) — persistence happens only on the success path so refusal preserves stage state.

**Fix (this change):**

- New module `git/merge/in_progress.rs` is the single source of truth for `MERGE_HEAD` detection.
- New module `orchestrator/merge_attribution.rs` ties active merges to specific stages via session metadata, branch HEAD, or `completed_commit`.
- `route_complete_for_conflicts` (in `commands/stage/complete.rs`) is the new pure routing seam — read-only, never mutates.
- `merge_verify::verify_or_derive_completed_commit` shared helper enforces ancestry for `--assume-merged` and `loom stage merge --resolved`.
- Daemon recovery runs `reconcile_main_repo_active_merge` BEFORE `sync_graph_with_stage_files` and BEFORE `recover_orphaned_sessions` so attribution sees session metadata before recovery deletes it.
- `sync_graph_with_stage_files` re-verifies `Completed + merged=true` non-knowledge stages, deriving from branch HEAD when missing and reverting `merged=false` when unverifiable.

## Helpers That Abort Active Merges (2026-04-27)

**What happened:** `merge_stage` and `get_conflicting_files_from_status` both ran `git merge --abort` on the repo as part of their normal flow (cleanup after success, abort the test merge). When invoked while a real merge was already in progress, they destroyed the user's resolution work.

**Misleading signal:** Both helpers acquire `MergeLock` at entry, so concurrent loom-driven merges are serialized. The bug is not concurrency — it's that the helpers don't distinguish "no merge in progress" from "a merge IS in progress that I didn't start".

**Prevention:** Helpers that mutate git merge state MUST refuse with `require_no_active_merge` when `MERGE_HEAD` is set on the repo path they're running in. Never silently `git merge --abort`. Defense in depth: even if attribution misses an active merge upstream, the guard surfaces an error instead of corrupting state.

**Fix:** Added `require_no_active_merge(repo_root)` helper in `git/merge/mod.rs`; called from `merge_stage` and `get_conflicting_files_from_status` after acquiring the merge lock. Both bail with a distinct error pointing at the path where the merge is in progress.

## Stale Acceptance Criteria Referencing External Plan Files

**What happened:** An `integration-verify` stage had an acceptance criterion `cargo run -- plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md`. That plan file was deleted during housekeeping (`doc: remove completed plans`) AFTER the stage was authored but BEFORE it ran. The criterion failed at execution time with a file-not-found error, requiring `--no-verify` to complete.

**Why:** Plan files in `doc/plans/` are subject to archiving/deletion as a normal maintenance operation. A file that exists when you write a criterion may not exist when the stage executes, especially for long-running plans.

**Prevention:** When generating acceptance criteria for `integration-verify` stages, never reference plan files from `doc/plans/` directly. Instead, use self-contained fixtures: create a temp file via `TempDir` + `write_plan` in Rust tests (see `tests/integration/plan_verify.rs` for the pattern). If a live-CLI smoke test is needed, write a minimal inline plan to a temp path rather than relying on a file that may be archived.

**Fix:** Use test fixtures that are fully controlled by the test suite. Reference `tests/integration/plan_verify.rs` as the canonical example of building plan fixtures without touching `doc/plans/`.

## Schema Root: LoomConfig vs Plan

**Mistake:** Passing the top-level YAML document (which wraps `loom:` key) where a `LoomConfig` (the inner object) is expected, or vice versa. This commonly manifests as "missing field" serde errors.

**Why:** Plan YAML has the structure `{ loom: LoomConfig }`. `parse_plan()` extracts the `loom:` block and deserializes that into `LoomMetadata` / `LoomConfig`, not the outer wrapper.

**Prevention:** The canonical deserialization root is `LoomConfig` (at `plan/schema/types.rs`), not the outer document. Nested fields (execution, stages, sandbox) live on `LoomConfig`.

## Session Identity: Backend Metadata Must Be Persisted

**Mistake:** Relying on transient session state to route kill/liveness calls after a daemon restart.

**Why:** Sessions are reconstructed from `.work/sessions/<id>.md` on daemon restart. Any field not in the session file is lost.

**Prevention:** Add `#[serde(default)]` to backend-related session fields and ensure they are set before the session is written to disk.

## Liveness: Monitor Must Route Through LivenessService

**Mistake:** Monitoring thread reads the PID from the session file and calls `kill -0 <pid>` directly.

**Prevention:** Always route session liveness through `LivenessService::is_alive(session)`. Never `kill -0` directly in the monitor.

**Fix:** `LivenessService` added in `orchestrator/liveness.rs`, wrapping `Arc<NativeBackend>`. The monitor thread holds the `LivenessService`, not a raw backend handle.

## Run-Path Coverage: All Spawn Sites Must Use the Shared Backend

**Mistake:** Wiring a session-spawning change into the main orchestrator loop but forgetting the other spawn paths: foreground mode, daemon startup, merge resolver spawner, continuation (handoff) spawner, auto-merge spawner.

**Why:** Sessions are spawned from multiple entry points beyond the main orchestrator. Each missed path drifts from the shared `Arc<NativeBackend>` the orchestrator holds.

**Prevention:** When changing session spawning, `rg` for all `spawn_session\|spawn_merge_session\|spawn_knowledge_session` call sites before considering the work done. Typically 5+ sites: orchestrator main loop, foreground spawner, merge_handler, continuation, auto_merge.

## toml_edit vs toml: Different Use Cases

**Mistake:** Using `toml_edit Item -> serde` for reading nested config sections. `toml_edit` is designed for round-trip writes; its typed access silently drops nested sub-tables.

**Why:** `toml_edit::Item` doesn't implement full `serde::Deserialize` for complex nested structures the same way `toml::Value` does.

**Prevention:** Use `toml_edit` for writes (round-trip safe). Use `toml` (re-parse the full file with `toml::Value`, then `try_into::<T>()` on the section) for typed reads of nested structures.

## Adding Session Fields: ~15-20 Struct Literal Breakages

**Mistake:** Adding a field to `Session` struct and expecting `cargo build` to guide you to all the breakages. Test files in `tests/` are not compiled by default and may not show breakages until `cargo test`.

**Why:** Rust requires all struct fields to be initialized in struct literals (unless `..Default::default()` spread is used). `Session` is constructed explicitly in ~15-20 locations across `src/` and `tests/`.

**Prevention:** Use `..Session::default()` spread in all struct literals. When adding fields to Session/Stage/LoomConfig, run `cargo test --all` (not just `cargo build`) to catch `tests/` breakages. Alternatively, write a context-aware patch script.

## macOS GUI App CLI Not on PATH — Detection-Spawn Mismatch (2026-04-27)

**What happened:** `TerminalEmulator::Ghostty` detection succeeded on macOS via a `/Applications/Ghostty.app` path-existence fallback (detection.rs:190-191), but spawn called `Command::new("ghostty")` and failed with "Failed to spawn terminal 'ghostty'. Is it installed?" The Ghostty CLI binary lives inside the bundle at `/Applications/Ghostty.app/Contents/MacOS/ghostty` and is not added to PATH (ghostty-org/ghostty#2483). Detection picked the terminal; spawn couldn't launch it.

**Misleading signal:** `which::which("ghostty")` failing was *handled* by an explicit `.app` existence check that succeeded. The fallback proved the GUI app was installed, not that its CLI was reachable from a child `Command`. Two-binary detection (`which` OR `.app exists`) silently expanded the set of "detected" terminals beyond the set of "spawnable via PATH" terminals.

**Why it broke:** Detection logic and spawn logic relied on different existence proofs. Detection accepted "the .app exists" as sufficient; spawn assumed the binary was on PATH. The asymmetry produced a guaranteed runtime failure for any macOS user without a manual PATH shim.

**Prevention:**

- For any `TerminalEmulator` variant whose detection has a path-based fallback (anything beyond `which::which(binary())` succeeding), the corresponding `build_command()` arm MUST use a launch path that does not depend on PATH — typically `open -na <AppName> --args ...` (see patterns.md "macOS GUI App Launch Pattern") or AppleScript via `osascript`. Treat any macOS `.app`-bundled tool as PATH-unreachable by default.
- When adding a new terminal emulator: check that detection and spawn agree about *how* the binary is reachable. If detection falls back to `.app` existence, spawn must NOT call `Command::new(binary())` directly on macOS.

**Fix:** `Self::Ghostty` arm in `emulator.rs:build_command()` is now cfg-gated; macOS reassigns `command = Command::new("open")` and uses `open -na Ghostty --args --working-directory=... --title=... -e bash -c CMD`. Linux behavior unchanged. `binary()` still returns `"ghostty"` (correct for Linux PATH lookup and for any macOS user with a manual shim). Tests `test_ghostty_build_command_macos` and `test_ghostty_build_command_linux` are cfg-gated so each runs on its target platform.

## Clippy --all-targets Required to Catch Test-Module Lints (2026-05-12)

**What happened:** `cargo clippy -- -D warnings` (without `--all-targets`) did not compile test modules, so a style lint in `src/hooks/generator.rs` (items after a test module) went undetected during per-stage acceptance and only surfaced at integration-verify.

**Why:** `cargo clippy` without `--all-targets` compiles only the default target (lib + bin). Test code (`#[cfg(test)] mod tests { ... }`) is in a different target and requires `--all-targets` to be included.

**Prevention:** Stage acceptance criteria that include a clippy check should always use:

```bash
cargo clippy --all-targets -- -D warnings
```

Not `cargo clippy -- -D warnings`. The `--workspace` flag is also useful in monorepos.

## Reviewer False Alarm: Verify Behavior Changes Against the Diff (2026-05-12)

**What happened:** An integration-verify reviewer flagged a "HIGH native regression" in `hooks/generator.rs`, claiming the new backend match arm introduced double-firing of global hooks on native worktrees. The claim was false — the native branch was already unconditionally calling `configure_loom_hooks(obj)` before the change; the new commit only added the container arm.

**Why:** The reviewer analyzed the stage description's framing rather than the actual diff. The description said "branching on config.backend" which sounds like it changes native behavior; the diff showed the native arm was structurally identical to the pre-existing unconditional call.

**Prevention:** When a reviewer asserts a behavior change, verify against the actual diff:

```bash
git show <commit>~1 -- <file>  # before
git show <commit> -- <file>    # after
```

Do not trust verbal descriptions of what a commit does — always compare before/after diffs directly.

## Session Liveness: Use tracking_key, Not stage_id

**What happened:** `kill_session` and `is_session_alive` in `orchestrator/terminal/native/mod.rs` used `format!("loom-{stage_id}")` for window titles and bare `stage_id` for PID key lookups. This worked for standard stages but silently missed merge sessions, knowledge sessions, and base-conflict sessions whose spawns use prefixed tracking keys.

**Why:** Standard stages dominate the mental model; their PID key and stage_id happen to align. But `Session.tracking_key` is the canonical OS-level resource identifier — it encodes the prefix/suffix needed for non-standard session types.

**Prevention:** Any OS-resource lookup keyed on a session (window title, PID file, process name) MUST use `session.tracking_key`, not `stage_id` or `format!("loom-{stage_id}")`. Verify by running a merge-resolver or knowledge session and checking that kill/liveness correctly targets it.

**Fix:** `native/mod.rs` updated to use `session.tracking_key` in all OS lookups.

## Parallel Deletion Stages: Straggler Files Outside Assignment Tables

**What happened:** After a parallel subagent deletion stage (`remove-container-keep-scaffolding`), 7 files remained with stale container references because they were not assigned to any subagent: `commands/mod.rs`, `completions/dynamic/tests.rs`, `plan/schema/mod.rs`, `commands/handoff/create.rs`, `commands/stage/tests/session.rs`, `orchestrator/preflight.rs`. These caused compile failures discovered only at integration-verify.

**Why:** Parallel subagent deletion scopes by files owned — files that re-export, import, or reference the deleted code but weren't explicitly in the ownership table are silently missed. Test files (`#[cfg(test)]`) are especially prone since `cargo build` doesn't compile them.

**Prevention:**

- After any parallel deletion stage, the MAIN AGENT must run `cargo build && cargo test --no-run` (not just `cargo build`) — test-only files don't appear in a lib build.
- Before assigning subagents, run `rg` for the target symbols across the ENTIRE tree including `tests/`, `mod.rs` re-exports, and completions.

## Struct Field Removal: Straggler Initializers Across Workspace

**What happened:** Removing a struct field (e.g., removing the `execution` field from `LoomConfig`) left ~25 straggler struct literal initializers across test fixtures, core modules, and examples. Each was an explicit `execution: None` / `execution_backend: None` / `backend: Default::default()` that subagents missed because they only searched within their assigned file set.

**Why:** Rust requires all struct fields in literals unless `..Default::default()` spread is used. In a workspace with many test fixtures, explicit literals far outnumber `Default` spreads.

**Prevention:** After removing a struct field, the main agent MUST `rg` the WHOLE tree (including `tests/`) for `<field_name>:` before considering the work done. Do not rely on per-subagent grep scoped to owned files.

**Fix:** Used `..LoomConfig::default()` spread in all new struct literals going forward.

## Stale Code Comments After Large Structural Removals

**What happened:** The container backend removal (`remove-container-keep-scaffolding` + `collapse-backend-scaffolding`) correctly deleted code but left stale references in comments across 7+ files: `monitor/{core,handlers,detection}.rs` referenced `dispatcher`, `daemon/server/client.rs` had admin-token rationale citing containers, `commands/stage/{complete,knowledge_complete}.rs` had isolated-git/container comments. These were caught only at `integration-verify`.

**Why:** The stage that owned doc cleanup (`strip-container-docs`) ran `rg` for identifiers and string literals but did not search comments or table cells. Comments describing removed concepts stay syntactically valid and compile fine.

**Prevention:** A stage that owns cleanup of a removed concept must `rg` the whole tree for:

1. Identifier names (already done)
2. Human-readable name/framing in comments and docstrings (often missed)
3. Table cells in markdown files, knowledge docs, and SKILL.md files

Use `rg -i "container\|docker\|dispatcher" loom/src/ --include="*.rs"` to catch all forms.

## Aggregated Wiring Re-Verification: Double-Applied working_dir

**What happened:** `run_aggregated_wiring_reverification` in `commands/stage/complete.rs` was called with `acceptance_dir` (already resolved to `worktree_root + integration-verify.working_dir`) and then joined each prior stage's `working_dir` on top, producing paths like `loom/loom/src/...`. The wiring check reported "Wiring source file missing" for every prior stage.

**Why:** `acceptance_dir` is computed as `worktree_root + working_dir`, so it is already a fully resolved path. Joining another `working_dir` on top re-applies it.

**Prevention — Detection rule:** Any code path that loops over prior stages and builds a source-file path MUST start from `worktree_root`, then join the per-stage `working_dir`. Never start from an already-resolved `acceptance_dir`.

**Fix:** Changed call site to pass `worktree_root` (from `StageExecutionPaths`) through `run_verification_phase` into the aggregated re-verifier; each stage's `working_dir` is joined against the worktree root.

## Knowledge Prose Staleness After Sandbox/Permission-Mode Changes (2026-05-14)

**What happened:** After changing `default_mode_for()` in `sandbox/config.rs` to return `AcceptEdits` for Standard and IntegrationVerify stages (previously `Auto`), three knowledge file locations still referenced the old `auto` default:

1. `architecture.md` — Security Model section said `Standard/IntegrationVerify → auto`
2. `entry-points.md` — Remote Control §1 table said `Standard / IntegrationVerify → Auto`
3. `patterns.md` — Sandbox permission_mode Resolution table showed `auto` for both types

These stale entries would have misled future agents into using `permission_mode: auto` when the actual default is already `accept-edits`.

**Why:** The implementation stage correctly updated Rust source + tests, but did not search knowledge files for old values. Knowledge files are not compiled, so no tool catches the mismatch.

**Prevention:** After changing any `default_mode_for()`-style constant or sandbox default:

1. `rg -l "auto\|Auto" doc/loom/knowledge/` — find knowledge files with the old value
2. Update each stale entry with `loom knowledge replace-section` or direct Edit
3. Verify with `rg "permission.mode" doc/loom/knowledge/` that all entries agree

**Generalization:** Any plan that changes an enumerated default (permission modes, stage-type behavior, config field defaults) MUST include a step that searches `doc/loom/knowledge/` for old values and corrects them. This applies even when the code change is a single-line constant update.
