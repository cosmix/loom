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

## Container Topology: Host Repo Must Be Mounted Whole (not just worktree)

**Mistake:** Mounting only the stage worktree directory into the container (e.g., `-v .worktrees/<id>:/workspace`). Git worktrees store relative symlinks (`.work -> ../../.work`) that point into the parent repo tree. A partial mount breaks both the symlinks and git metadata.

**Why:** Git worktree metadata (`<worktree>/.git`) is a file pointing at `<repo>/.git/worktrees/<id>/`. Mounting only the worktree severs this link — git commands fail with "not a git repository".

**Prevention:** Always bind-mount the full host repo root at a fixed container path (`/repo`). Stage cwd = `/repo/.worktrees/<stage-id>`. Set `LOOM_WORK_DIR=/repo/.work` explicitly.

**Fix:** ContainerBackend uses `REPO_MOUNT = "/repo"` as the invariant; worktree path inside container is always derived from this constant.

## Config Must Be Persisted to .work/ Before loom run Reload

**Mistake:** Storing plan-level backend config only in memory (transient struct). On `loom run` restart (e.g., after daemon crash), the config is re-read from `.work/config.toml` — if it was never written there, the backend selection silently reverts to native.

**Why:** The orchestrator reconstructs its state entirely from disk on startup. Any config not written to `.work/config.toml` during `loom init` is gone on restart.

**Prevention:** `loom init --backend container` MUST write `[project_execution]` to `.work/config.toml`. `ContainerBackend::new()` reads this section and refuses with a clear error if it's absent.

**Fix:** `fs/work_dir::write_project_execution()` uses `toml_edit` for round-trip-safe writes. `ContainerBackend::new()` calls `work_dir_api::read_project_execution()` and bails if missing.

## Schema Root: LoomConfig vs Plan

**Mistake:** Passing the top-level YAML document (which wraps `loom:` key) where a `LoomConfig` (the inner object) is expected, or vice versa. This commonly manifests as "missing field" serde errors.

**Why:** Plan YAML has the structure `{ loom: LoomConfig }`. `parse_plan()` extracts the `loom:` block and deserializes that into `LoomMetadata` / `LoomConfig`, not the outer wrapper.

**Prevention:** The canonical deserialization root is `LoomConfig` (at `plan/schema/types.rs`), not the outer document. Nested fields (execution, stages, sandbox) live on `LoomConfig`.

## Session Identity: Backend Metadata Must Be Persisted

**Mistake:** Relying on transient session state to route kill/liveness calls after a daemon restart. If `session.backend` is not written to disk, the restarted daemon defaults to the wrong backend (e.g., attempts `kill -0` on a container PID).

**Why:** Sessions are reconstructed from `.work/sessions/<id>.md` on daemon restart. Any field not in the session file is lost.

**Prevention:** Add `#[serde(default)]` to backend-related session fields (backend, tracking_key, runtime, container_name) and ensure they are set before the session is written to disk. `Session::derive_tracking_key()` computes the container name from stage-id + session-id.

**Fix:** Session struct fields `backend`, `tracking_key`, `runtime`, `container_name` all use `#[serde(default)]` and are populated in `spawn_session` implementations before persistence.

## Liveness: Monitor Must Route Through TerminalBackend

**Mistake:** Monitoring thread reads the PID from the session file and calls `kill -0 <pid>` directly. This gives false "dead" signals for container sessions, which have a host-side wrapper PID that may be dead even though the container is healthy, or vice versa.

**Why:** Container session liveness is determined by `<runtime> inspect -f '{{.State.Running}}'`, not by host PID existence. The host wrapper PID can die after the container starts; container sessions look dead to `kill -0`.

**Prevention:** Always route session liveness through `LivenessService::is_alive(session)`. Never `kill -0` directly in the monitor.

**Fix:** `LivenessService` added in `orchestrator/liveness.rs`. Monitor thread holds `LivenessService`, not a raw `BackendDispatcher`.

## Run-Path Coverage: All Spawn Sites Must Use the Dispatcher

**Mistake:** Adding the BackendDispatcher for the main orchestrator loop but forgetting to update other spawn paths: foreground mode, daemon startup, merge resolver spawner, continuation (handoff) spawner, auto-merge spawner.

**Why:** Sessions are spawned from multiple entry points beyond the main orchestrator. Each missing path falls back to a hard-coded NativeBackend.

**Prevention:** When wiring a new backend dispatcher, `rg` for all `spawn_session\|spawn_merge_session\|spawn_knowledge_session` call sites before considering the work done. Typically 5+ sites: orchestrator main loop, foreground spawner, merge_handler, continuation, auto_merge.

## Init Re-run: WorkDir Initialization Must Support Reconfigure

**Mistake:** `WorkDir::initialize()` bails when `.work/` already exists (to prevent accidental re-init). Using it to reconfigure backend (e.g., `loom init --backend container` on an existing workspace) fails silently or corrupts state.

**Prevention:** Add `open_or_initialize()` for the reconfigure path. It reads existing config if `.work/` exists, applies only the backend-related fields, and writes them back. Do NOT replace `initialize()` — existing callers rely on its "bail if exists" guard.

**Fix:** `fs/work_dir.rs::open_or_initialize()` added; `loom init --backend container` uses this path; `initialize()` behavior unchanged.

## Firewall: Agent-Writable Allowlist Defeats the Firewall

**Mistake:** Mounting the allowlist file inside the agent-writable directory (e.g., inside the repo bind-mount) allows a compromised agent to append entries and bypass the egress filter.

**Why:** If the allowlist path is within `/repo` (which is mounted rw for the agent), the agent can overwrite it.

**Prevention:** Allowlist file must be mounted ro at a path outside the rw bind mount. In loom: `.work/network/allowed_domains.txt` on host, mounted ro at `/etc/loom/network/allowed_domains.txt` inside the container.

## Cache Key Completeness: Hash All Embedded Resources

**Mistake:** Fingerprinting only detected languages. If the Dockerfile template or firewall script changes, the cached image is stale but the fingerprint matches — agents get the old image with updated resources.

**Why:** `include_str!()` embeds resource content at compile time. A rebuilt loom binary has new content but the old fingerprint unless the hash includes the resource content.

**Prevention:** Include SHA-256 of ALL `include_str!()` resources in the fingerprint. In loom: fingerprint = `SHA-256(sorted_langs + DOCKERFILE_TMPL + FIREWALL_SH)`.

**Fix:** `compute_fingerprint_inner(langs, dockerfile_tmpl_content, firewall_sh_content)` — takes content as args for testability. `compute_fingerprint` calls it with the `include_str!` constants.

## include_str! Path Depth from Container Submodule

**Mistake:** When writing `include_str!()` inside `loom/src/orchestrator/terminal/container/fingerprint.rs`, using 5 `../` segments to reach `loom/resources/`. The correct count is 4.

**Why:** The file lives 4 levels below `loom/` (orchestrator → terminal → container → fingerprint.rs). Starting from the file's directory: `../../../../resources/Dockerfile.tmpl`.

**Prevention:** Count the directory components from the file to `loom/`, then use that many `../`. The compiler error message will suggest the correction if you get it wrong.

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

## Mount order inversion silently defeats the ro base

**What happened:** When hardening the container backend's `/repo` mount from rw to ro, a subagent could construct the mount list with the rw worktree overlay listed *before* the ro base mount (e.g., `--mount=type=bind,...,/repo/.worktrees/X` then `--mount=type=bind,...,/repo`). Docker/Podman apply mounts in argument order — later entries shadow earlier ones at overlapping paths. If the rw overlay on `.worktrees/X` is applied first and the ro base is applied second, the ro base silently overwrites (or shadows) the rw layer, making `/repo/.worktrees/X` also read-only. Worse, the reverse: if the ro base is applied first and then a rw overlay of `/repo` (for Merge stages) is applied second, the entire `/repo` becomes rw again.

**Why:** Container runtimes apply bind mounts in the order they appear in the run command. There is no error — the later mount simply takes precedence at overlapping prefixes.

**Prevention:** Always verify mount construction in `build_mounts` unit tests by asserting `mounts[0] == Mount::ro(_, "/repo")` (ro base is first), and that rw overlay paths are tighter subtrees that come after. For Merge/BaseConflict stages that need a full rw `/repo`, there should be no ro base at all — document this as the intentional exception.

**Fix:** The ro base mount must always be `args[0]` in the list, with all rw overlays appended after it. Add an assertion in `build_mounts_standard_stage_has_ro_repo_and_rw_worktree` that checks mount ordering by index, not just presence.

## Container backend: UID 1000 collision on Ubuntu 24.04 base (2026-05-11)

**What happened:** `loom init --backend container ...` failed at STEP 6/15 of the image build with `useradd: UID 1000 is not unique`. The Dockerfile (`loom/resources/Dockerfile.tmpl`) creates a `loom` user at UID 1000, but `mcr.microsoft.com/devcontainers/base:ubuntu-24.04` ships a pre-existing `ubuntu` user at UID 1000 / GID 1000 (new in 24.04 — 22.04 had no such pre-baked user).

**Misleading signal:** The Dockerfile guarded `useradd` with `if ! id -u ${USERNAME} >/dev/null 2>&1` — checking whether the *username* `loom` exists. It didn't, so `useradd` ran and collided with the existing user occupying that UID. The guard was correct in intent (skip creation if already present) but wrong in mechanism (check by name, not by UID).

**Why it broke:** `useradd --uid N` fails if UID `N` is taken, regardless of the username. The base image's `ubuntu` user holds UID 1000 from the moment the FROM line lands, so any `useradd --uid 1000 loom` is guaranteed to fail. Loom's image build also passes no `--build-arg USER_UID=...`, so the default is the only path exercised.

**Prevention:**

- When creating a fixed-UID user on a base image, always check by UID *and* by name. Evict whatever occupant currently holds the UID before calling `useradd --uid`.
- The canonical devcontainers pattern: `getent passwd ${USER_UID}` → if the matching name isn't ours, `userdel -r`, then create. Same for GID via `getent group`.
- When upgrading a base image's distro version, re-check whether common UIDs (1000, 1001) are now pre-occupied — Ubuntu 24.04 introduced this; future LTS releases may shift again.

**Fix:** `Dockerfile.tmpl` now runs an explicit eviction block before `useradd`: if `getent passwd ${USER_UID}` resolves to a non-`loom` user, `userdel -r` removes it (falling back to non-`-r` if the home dir is shared); same for GID. The fingerprint changes automatically because the template content is embedded in the image fingerprint (`fingerprint.rs:22`), so cached images rebuild without manual cache clearing.

## Session Files Outlive Their Containers

**What happened:** `loom container logs` and `loom container list` trust `container_name` in `.work/sessions/*.md` without checking if the container still exists. A stage that ran and completed leaves a session file with `container_name` set and `status: Completed`. The next call resolves the container name but fails at `<runtime> logs` because the container was already removed.

**Why:** Session files are NOT deleted when containers are removed. Deletion only happens on explicit `loom clean`. A session file with a populated `container_name` is not proof the container is live.

**Prevention:** Before executing any `<runtime> logs|exec|inspect` command against a resolved container name, call `<runtime> inspect -f '{{.State.Status}}' <name>` first. A non-zero exit or "No such container" means the container is gone — fall back to `.work/crashes/` log files. Detection: `rg 'container_name'` in session files does not imply container existence.

**Fix:** Filter by runtime `inspect` status before trusting the session-file `container_name`. `loom container logs` should use `query_container_status()` (already in list.rs) as a pre-flight before exec-ing into `<runtime> logs`.

---

## First-Match Session Iteration Is Unsafe for Retried Stages

**What happened:** `resolve_session_for_stage()` in `logs.rs` iterates `.work/sessions/*.md` and returns the first match for a given `stage_id`. A stage that was retried has multiple session files — an older file with status `Crashed` and a newer file with status `Running`. Iteration order in `fs::read_dir` is unspecified; the stale session may be picked first, pointing to the wrong (or removed) container.

**Why:** `fs::read_dir` returns entries in filesystem order, not creation order. Multiple session files can exist for one stage when retries produce new session IDs.

**Prevention:** When scanning for a session matching a `stage_id`, sort candidates by `last_active` descending (or by session ID timestamp suffix) before picking the first match. Prefer status `Running` over `Completed`/`Crashed` when multiple are present.

**Fix:** In `resolve_session_for_stage`, collect all matching sessions, sort by `last_active` DESC, and prefer `status == Running`. Fall back to the most-recently-active one.

---

## Missing Clearer Breaks Stale-Session Lookups

**What happened:** A `Session` method `set_container_identity` was added to write `runtime` and `container_name`. Without a matching `clear_container_identity` called after container removal, the session file permanently retains the container reference even after `rm -f`. Subsequent `loom container logs` or `loom container list` pick up the stale data and attempt to inspect a non-existent container.

**Why:** Every long-lived resource handle stored on a model struct requires symmetric setter AND clearer. Calling the setter on spawn but not calling the clearer on removal leaves the model in an inconsistent state.

**Prevention:** When adding any resource-identity field to `Session` (or similar models), always implement both setter and clearer in the same commit. Callers that remove the resource (e.g., `kill_session`, `spawn_common` cleanup) must call the clearer before persisting.

**Fix:** Call `session.clear_container_identity()` in `kill_session` and in the `spawn_common` error path, then persist the updated session file. `clear_container_identity` is already implemented in `models/session/methods.rs:135`.

## Container Retry Collisions: Preemptive Removal Pattern (2026-05-12)

**What happened:** When `spawn_session` failed and the stage was retried, `podman run` (or `docker run`) failed with "container name loom-<stage-id> is already in use". Similarly, worktrees and branches accumulated across retries.

**Why:** The failure path in `stage_executor.rs` only marked the stage `Blocked` — it did not clean up the half-spawned container, git worktree, or branch left behind.

**Prevention:**
1. `spawn_common` must call `preemptive_remove_existing(runtime, container_name)` — a best-effort `rm -f` — at the very top, before network/mount setup. This is cheap and idempotent (`rm -f` exits 0 for non-existent containers).
2. After the `spawn_session` call fails in `stage_executor.rs`, the rollback must clean: container (`preemptive_remove_existing`), worktree (`git::remove_worktree`), branch (`git::delete_branch`). Knowledge stages: container removal only (no worktree/branch to clean).
3. All cleanup calls must be wrapped in `let _ = ...` — cleanup failures must not hide the original error.

**Fix:** `preemptive_remove_existing` extracted as `pub(crate)` in `container/mod.rs`; failure rollback added to both the knowledge spawn path (~line 134) and standard-stage path (~line 364) in `stage_executor.rs`.

## Hook Installation Asymmetry: Native vs Container Hook Paths (2026-05-12)

**What happened:** Container-backend worktrees had `settings.local.json` with global hooks pointing to `~/.claude/hooks/loom/` (host paths). These paths don't exist inside the container — hooks dir is mounted at `/home/loom/.claude/hooks/loom`.

**Why:** `generate_hooks_settings` in `hooks/generator.rs` called `configure_loom_hooks(obj)` unconditionally. `loom_hooks_config()` ALWAYS returns host-side paths. Session-specific hooks via `HooksConfig::to_settings_hooks()` already used `script_path()` (correct); the global-hook emission path was the bug.

**Prevention:** `generate_hooks_settings` must branch on `config.backend`:
- Native: `configure_loom_hooks(obj)` (host paths)
- Container: `configure_loom_hooks_for_container(obj)` (uses `loom_hooks_config_for_dir("/home/loom/.claude/hooks/loom")`)

The private helper `configure_loom_hooks_with_dir(obj, hooks_dir)` parameterizes both native and container paths — reuse it for both branches.

**Fix:** `fs/permissions/hooks.rs` — extracted `loom_hooks_config_for_dir(dir)` + `configure_loom_hooks_with_dir(obj, dir)` helpers; added `configure_loom_hooks_for_container(obj)`. `hooks/generator.rs` now branches on backend type.

## Per-Worktree Gitignore: Container settings.local.json Must Be Excluded (2026-05-12)

**What happened:** `settings.local.json` inside a container-backed worktree contains `/home/loom/.claude/hooks/loom/` paths. An agent could `git add .claude/settings.local.json` and commit these container-specific paths to the repo, poisoning the hook config for native-backend users.

**Why:** Container worktrees need `/home/loom/` paths in `settings.local.json`; host users expect `~/.claude/hooks/loom/` paths. Both can't be right simultaneously; only per-worktree exclusion keeps them separate.

**Prevention:** After creating any container-backed worktree, append `.claude/settings.local.json` to `<worktree>/.git/info/exclude` (idempotently). Use per-worktree exclude — NOT `.gitignore` (which would pollute the user's repo). For knowledge stages (no worktree): append to main repo's `.git/info/exclude`.

**Gotcha — gitignore path:** Per-worktree exclude is at `<repo>/.git/worktrees/<stage-id>/info/exclude` (the real gitdir path), NOT at `<worktree-dir>/.git/info/exclude`. The latter is a plain FILE containing a `gitdir:` pointer, not a directory.

## Container Backend Git Identity: GIT_AUTHOR_* Is the Right Mechanism (2026-05-12)

**What happened:** Container sessions had no `.gitconfig`, so git commits either used a broken/empty identity or failed with "Please tell me who you are."

**Why:** Containers don't inherit the host `~/.gitconfig`. There's no automatic mechanism to pass user identity into a container environment.

**Prevention:** Add `git_user_name: Option<String>` and `git_user_email: Option<String>` to `ProjectContainerConfig` (`plan/schema/execution.rs`). Populate at `loom init --backend container` time by reading `git config --global user.name/email` on the host. Inject as env vars in `ContainerBackend::build_env_for_session`:

```
GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL, GIT_COMMITTER_NAME, GIT_COMMITTER_EMAIL
```

Key rules:
- Inject ALL FOUR or NONE. Partial identity (name only, no email) produces inconsistent commits — harder to debug than no identity at all.
- Validate both fields: reject empty, >256 bytes, or any `char.is_control()`. Control chars are valid in podman `-e` values but produce malformed git objects.
- Validation at two boundaries: `loom init` (warn + scrub) and `.work/config.toml` read time (silent scrub to `None`).

**Fix:** `validate_git_identity()` in `plan/schema/execution.rs`; wired into `commands/init/execute.rs::sanitize_git_identity` (warns) and `fs/work_dir.rs::read_project_execution` (silent scrub).

## Clippy --all-targets Required to Catch Test-Module Lints (2026-05-12)

**What happened:** `cargo clippy -- -D warnings` (without `--all-targets`) did not compile test modules, so a style lint in `src/hooks/generator.rs` (items after a test module) went undetected during per-stage acceptance and only surfaced at integration-verify.

**Why:** `cargo clippy` without `--all-targets` compiles only the default target (lib + bin). Test code (`#[cfg(test)] mod tests { ... }`) is in a different target and requires `--all-targets` to be included.

**Prevention:** Stage acceptance criteria that include a clippy check should always use:
```
cargo clippy --all-targets -- -D warnings
```
Not `cargo clippy -- -D warnings`. The `--workspace` flag is also useful in monorepos.

## Reviewer False Alarm: Verify Behavior Changes Against the Diff (2026-05-12)

**What happened:** An integration-verify reviewer flagged a "HIGH native regression" in `hooks/generator.rs`, claiming the new backend match arm introduced double-firing of global hooks on native worktrees. The claim was false — the native branch was already unconditionally calling `configure_loom_hooks(obj)` before the change; the new commit only added the container arm.

**Why:** The reviewer analyzed the stage description's framing rather than the actual diff. The description said "branching on config.backend" which sounds like it changes native behavior; the diff showed the native arm was structurally identical to the pre-existing unconditional call.

**Prevention:** When a reviewer asserts a behavior change, verify against the actual diff:
```
git show <commit>~1 -- <file>  # before
git show <commit> -- <file>    # after
```
Do not trust verbal descriptions of what a commit does — always compare before/after diffs directly.
