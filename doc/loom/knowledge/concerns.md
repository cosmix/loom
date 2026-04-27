# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations (2026-01-29)

> **Full details:** See [architecture.md § Review Findings - Layering Violations](architecture.md#review-findings---layering-violations-2026-01-29)

Critical violations where lower layers import from higher layers:

- daemon imports commands (mark_plan_done_if_all_merged)
- orchestrator imports commands (check_merge_state)
- git/worktree imports orchestrator (hook config)
- models imports plan/schema (type definitions)

## Security Concerns

### Release Asset Verification Gap

Only binary files are signature-verified via minisign. Non-binary release assets lack verification:

- CLAUDE.md.template
- agents.zip
- skills.zip

**Recommended:** Add SHA256 checksum verification for all release assets.

## Code Quality Concerns

### Code Consolidation Needed

> **Full details:** See [conventions.md § Code Consolidation Opportunities](conventions.md#code-consolidation-opportunities-2026-01-29)

Key duplications needing consolidation:

- parse_stage_from_markdown: 4 copies
- branch_name_for_stage: 22+ inline format!() calls
- extract_yaml_frontmatter: 2 copies
- compute_level: 4 copies in status modules

### Debug Output in Production

`eprintln!` statements with 'Debug:' prefix in production code (complete.rs, orchestrator.rs). Should use tracing crate with log levels.

## ReDoS Potential in Plan Pattern Regex

User-provided regex patterns in plan files (failure_patterns, wiring patterns) are compiled and executed without complexity checks. While mitigated by trust model (plan authors = trusted), consider adding regex timeout or complexity limits for defense in depth.

Files: src/verify/baseline/capture.rs:76-79, src/verify/baseline/compare.rs:155-158

## Bootstrap Settings Backup Risk

`bootstrap.rs:write_bootstrap_sandbox()` keeps the settings.local.json backup in memory only (`Option<String>`). If the process is killed between writing sandbox settings and restoring the original, user settings are permanently lost. Low probability since bootstrap is interactive, but a disk-based temp backup would be more robust.

## Bootstrap Tool Restriction Scope

`bootstrap.rs:57` uses `Bash(loom knowledge*)` which allows all knowledge subcommands (init, check, gc, show) not just `update`. Harmless since other subcommands are read-only, but could be tightened to `Bash(loom knowledge update*)` for principle of least privilege.

## Hook Pattern Matching: False Positives on Embedded Content (2026-03-31)

All PreToolUse hooks (worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
prefer-modern-tools.sh) and Rust validators (bash.rs) matched patterns against
full bash command strings including heredoc bodies and -m/--message content.
Keywords in commit messages or string literals triggered false blocks.

Issue #13: git commit -m "Add .worktrees/ to .gitignore" blocked by
worktree-isolation.sh because .worktrees/ appeared in message text.

Fix: Introduced _common.sh with strip_embedded_content() that removes heredoc
bodies and message content before pattern matching. Rust parallel implementation
in validators/bash.rs. Also tightened commit-filter.sh attribution pattern to
require Co-Authored-By: header prefix instead of substring matching.

Hooks affected: worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
prefer-modern-tools.sh, validators/bash.rs.

## Hook Debug Logging to /tmp/ (2026-03-31)

Several hooks (worktree-isolation.sh, commit-filter.sh, prefer-modern-tools.sh) hardcode debug log paths to `/tmp/<name>-debug.log`. Under `set -euo pipefail`, if `/tmp/` is not writable (e.g., sandboxed environments), the hook script exits immediately with error. `git-add-guard.sh` already uses a gated `debug()` pattern that only writes when `GIT_ADD_GUARD_DEBUG=1` is set. Other hooks should adopt the same pattern.

## ~~Sandbox Test Failures in fs::permissions~~ (RESOLVED 2026-04-16)

Fixed by `install_loom_hooks_to(path)` in commit 8d2bf2e. Tests now use temp directories via `ensure_loom_permissions_to(repo_root, Some(&hooks_dir))` instead of writing to the real `~/.claude/` directory.

## Rust/Shell Heredoc Terminator Divergence

The Rust `strip_embedded_content` in `bash.rs:79` uses `line.trim() == marker` (tolerates indented terminators), while the shell version in `_common.sh:44` uses `$0 == marker` (exact line match). Both fail-safe but should be aligned for consistency.

## Codex Findings Fixed (2026-04-16)

The following Codex review findings from PLAN-fix-codex-findings are now resolved:

- **H-01**: worktree-file-guard.sh registered for Read, Glob, Grep (hooks.rs:87-112)
- **H-02**: Plan sandbox config threaded to OrchestratorConfig in both foreground and daemon paths
- **H-03**: Fail-closed error handling in load_stage — only reconstructs on file-not-found, not parse errors
- **H-04**: finalize_merge_resolution handles both MergeConflict and MergeBlocked
- **M-03**: Budget check decoupled from health bucket guard — runs every poll tick
- **M-04**: merge_resolved() and merge_retry() use resolve_target_branch() instead of default_branch()
- **M-07**: Daemon status categorizes NeedsHandoff/WaitingForInput as "executing" matching CLI

Additionally fixed during integration-verify:

- **is_manually_merged**: Updated to use resolve_target_branch() instead of default_branch(), added work_dir parameter to detect_worktree_status() and is_manually_merged() for config access

## BranchMissing Phantom-Merge Risk in merge_handler.rs (2026-04-16)

`handle_merge_session_completed` at line 97-103 treats `MergeState::BranchMissing` as a successful merge by calling `finalize_merge_resolution` which unconditionally sets `merged=true`. This violates the project invariant that daemon-side paths must never write `merged=true` without git ancestry verification.

Scenario: merge session dies, `check_merge_state` returns Conflict/Unknown, branch was deleted without being merged (e.g., manual `git branch -D`), code assumes "branch missing = cleaned up after merge."

Pre-existing issue, not introduced by the merge conflict session lifecycle fix. The `ProgressiveMergeResult::is_success()` method also still classifies `NoBranch` as success, inconsistent with `progressive_complete.rs` treating it as `Blocked`.

## Dead Code: is_knowledge_stage()

models/stage/methods.rs:443 defines is_knowledge_stage() but it is never called. All call sites use direct stage_type comparison. Contains fragile heuristic name matching that duplicates detect_stage_type() logic. Consider removing or consolidating with detect_stage_type and check_knowledge_recommendations.

## BaseConflict Carve-out is Heuristic (2026-04-27)

`attribute_main_repo_merge` carves out `loom/_base/*` merges with a heuristic on the current branch name and on `SessionType::BaseConflict` session metadata. If a base-merge ever runs from a non-`loom/_base/*` branch (manual flow, future refactor) and no `BaseConflict` session is alive, attribution would tie the active merge to the stage whose branch HEAD shows up in `MERGE_HEAD` — leading to a spurious revert.

**Hardening path:** Tag base merges explicitly via session metadata (e.g., a marker file or distinct `SessionType::BaseConflict` always present during the base-merge window) and key the carve-out off that signal alone, not the current branch name. Until then, the heuristic is documented here so future work knows where to look.
