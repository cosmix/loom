# Hook Content Stripping

> Stripping heredoc bodies and -m text before matching, the full hook inventory, and the limits of that stripping.

## Hook Content-Stripping Pattern

Hooks that validate bash commands must strip embedded text content before
pattern matching. The strip_embedded_content() function (in hooks/\_common.sh
for shell, validators/bash.rs for Rust) removes:

1. Heredoc bodies (awk state machine tracking <<MARKER to MARKER)
2. -m / --message quoted content (sed replacements)

Each hook sources \_common.sh via: source "$(dirname "$0")/\_common.sh"

Full hook inventory (18 top-level scripts in `hooks/`; 29 including `hooks/tests/`):

- PreToolUse: worktree-isolation.sh, commit-filter.sh, subagent-verify-guard.sh,
  git-add-guard.sh, prefer-modern-tools.sh, worktree-file-guard.sh,
  plans-path-guard.sh, ask-user-pre.sh
- PostToolUse: post-tool-use.sh, ask-user-post.sh
- Stop: commit-guard.sh, learning-validator.sh
- SessionStart: session-start.sh
- SessionEnd: session-end.sh
- PreCompact: pre-compact.sh
- UserPromptSubmit: skill-trigger.sh
- Library: \_common.sh (sourced, not registered)
- Git-side: git-pre-commit-hook.sh (appended to `.git/hooks/pre-commit` by `loom init`;
  the only top-level script not in `LOOM_HOOKS`)

The `PreToolUse` array in `fs/permissions/hooks.rs` has **13 entries** — several hooks are
registered against more than one matcher (worktree-isolation on Bash/Edit/Write,
worktree-file-guard on Read/Glob/Grep, plans-path-guard on Edit/Write). Its exact length and
per-index order are asserted by `fs/permissions/tests/hooks_tests.rs::test_hooks_config_structure`,
so adding a hook means updating that test too.

## Hook Content-Stripping Pattern (Updated 2026-03-31)

All PreToolUse hooks that match command patterns MUST use `strip_embedded_content()` before pattern matching to prevent false positives from keywords appearing inside commit messages or heredoc bodies.

**Architecture:**

- `_common.sh` provides `strip_embedded_content()` (shared across all shell hooks)
- `loom/src/hooks/validators/bash.rs` provides Rust equivalent `strip_embedded_content()`
- Phase 1: awk state machine strips heredoc bodies (`<<MARKER` to `^MARKER$`)
- Phase 2: sed strips `-m`/`--message` quoted content

**Usage pattern:**

1. Source `_common.sh` at top of hook
2. Call `stripped=$(strip_embedded_content "$cmd")`
3. Use `$stripped` for pattern detection (git -C, .worktrees/, ../../, grep, find)
4. Use original `$cmd` for patterns that MUST match message body (e.g., Co-Authored-By)

**Commit-filter dual-check:**

- STRIPPED_COMMAND for detecting `git commit` (prevents "commit" in messages from triggering)
- ORIGINAL COMMAND for Co-Authored-By check (anchor `^` prevents mid-line false positives)

**Security posture:** All stripping failures result in false positives (overly strict), never bypasses (permissive). This is the correct safety direction for development hooks.

**Hooks using this pattern:** worktree-isolation.sh, commit-filter.sh, git-add-guard.sh, prefer-modern-tools.sh
