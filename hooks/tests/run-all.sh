#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
ERRORS=()

run_test() {
    local name="$1"
    local script="$2"
    if output=$(bash "$script" 2>&1); then
        echo "  PASS: $name"
        ((PASS++)) || true
    else
        echo "  FAIL: $name"
        echo "    Output: $output"
        ((FAIL++)) || true
        ERRORS+=("$name")
    fi
}

echo "Running hook tests..."
run_test "prefer-modern-tools: grep triggers warning" "$SCRIPT_DIR/prefer-modern-tools-grep.sh"
run_test "prefer-modern-tools: find triggers warning" "$SCRIPT_DIR/prefer-modern-tools-find.sh"
run_test "prefer-modern-tools: rg has no warning" "$SCRIPT_DIR/prefer-modern-tools-rg.sh"
run_test "prefer-modern-tools: quoted grep no warning" "$SCRIPT_DIR/prefer-modern-tools-quoted.sh"
run_test "post-tool-use: heartbeat only, no tool event" "$SCRIPT_DIR/post-tool-use-tool-event.sh"
run_test "post-tool-use does not persist tool output" "$SCRIPT_DIR/post-tool-use-empty-output.sh"
run_test "loom-control-complete: exact verified route only" "$SCRIPT_DIR/loom-control-complete.sh"
run_test "loom-control-complete: tokenized pre-filter catches forged quoting, ignores prose" "$SCRIPT_DIR/loom-control-tokenized-prefilter.sh"
run_test "session-start: compact source emits re-anchor" "$SCRIPT_DIR/session-start-compact.sh"
run_test "worktree-file-guard: malformed non-empty input is blocked" "$SCRIPT_DIR/worktree-file-guard-truncated-input.sh"
run_test "worktree-file-guard: MultiEdit honours the worktree boundary" "$SCRIPT_DIR/worktree-file-guard-multiedit.sh"
run_test "plans-path-guard: blocks ~/.claude/plans write" "$SCRIPT_DIR/plans-path-guard-blocks-claude-plans.sh"
run_test "plans-path-guard: blocks ~/.claude/projects/*/plans edit" "$SCRIPT_DIR/plans-path-guard-blocks-project-plans.sh"
run_test "plans-path-guard: allows doc/plans and other .claude paths" "$SCRIPT_DIR/plans-path-guard-allows-doc-plans.sh"
run_test "plans-path-guard: blocks MultiEdit to .claude plans, allows doc/plans" "$SCRIPT_DIR/plans-path-guard-multiedit.sh"
run_test "codex-forward-guard: agent_type gate pins both shim types" "$SCRIPT_DIR/codex-forward-guard-agent-type.sh"
run_test "codex-forward-guard: blocks Edit by a forwarder" "$SCRIPT_DIR/codex-forward-guard-blocks-edit.sh"
run_test "codex-forward-guard: companion Bash allowed, other Bash blocked" "$SCRIPT_DIR/codex-forward-guard-bash-companion-only.sh"
run_test "codex-forward-guard: plain subagents, main sessions, no-path all untouched" "$SCRIPT_DIR/codex-forward-guard-ignores-others.sh"
run_test "codex-forward: wrapper preserves task argv" "$SCRIPT_DIR/codex-forward-wrapper.sh"
run_test "codex-forward-guard: quoted and escaped prompts round-trip" "$SCRIPT_DIR/codex-forward-guard-quoting.sh"
run_test "git-add-guard: quoted prose allowed, real args blocked" "$SCRIPT_DIR/git-add-guard-quoting.sh"
run_test "stage-terminal-guard: blocks completed/verified, allows others" "$SCRIPT_DIR/stage-terminal-guard.sh"
run_test "stage-terminal-guard: blocks MultiEdit once the stage is terminal" "$SCRIPT_DIR/stage-terminal-guard-multiedit.sh"
run_test "user-prompt-context: short prompt produces no output" "$SCRIPT_DIR/user-prompt-context-short-prompt.sh"
run_test "user-prompt-context: no LOOM_WORK_DIR exits silently" "$SCRIPT_DIR/user-prompt-context-no-workdir.sh"
run_test "user-prompt-context: malformed JSON fails open" "$SCRIPT_DIR/user-prompt-context-malformed-json.sh"
run_test "user-prompt-context: output is a single JSON object" "$SCRIPT_DIR/user-prompt-context-single-json-line.sh"
run_test "user-prompt-context: oversized delegate output is suppressed" "$SCRIPT_DIR/user-prompt-context-oversized-output.sh"
run_test "post-tool-use: records Write/Edit path via loom context record-edit" "$SCRIPT_DIR/post-tool-use-write-edit-record.sh"
run_test "post-tool-use: exits 0 when loom is absent from PATH" "$SCRIPT_DIR/post-tool-use-no-loom-binary.sh"
run_test "post-tool-use: symlinked heartbeat skips only the heartbeat write" "$SCRIPT_DIR/post-tool-use-symlinked-heartbeat.sh"
run_test "post-tool-use: records MultiEdit file_path via loom context record-edit" "$SCRIPT_DIR/post-tool-use-multiedit-record.sh"
run_test "post-tool-use: records NotebookEdit notebook_path via loom context record-edit" "$SCRIPT_DIR/post-tool-use-notebookedit-record.sh"

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    echo "Failed tests: ${ERRORS[*]}"
    exit 1
fi
