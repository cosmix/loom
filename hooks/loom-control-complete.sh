#!/usr/bin/env bash
# Trusted PostToolUse bridge for one exact sandboxed completion command.

set -euo pipefail

read_input() {
	if command -v gtimeout &>/dev/null; then
		gtimeout 1 cat 2>/dev/null || true
	elif command -v timeout &>/dev/null; then
		timeout 1 cat 2>/dev/null || true
	else
		cat 2>/dev/null || true
	fi
}

fail_closed() {
	printf 'LOOM_CONTROL_ERROR: %s\n' "$1" >&2
	exit 2
}

resolve_trusted_loom() {
	local candidate resolved
	if [[ ${LOOM_CONTROL_TESTING:-} == 1 && -d "$(dirname "$0")/tests" ]]; then
		candidate=${LOOM_CONTROL_TEST_BIN:-}
		[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || return 1
		printf '%s\n' "$candidate"
		return 0
	fi
	for candidate in \
		"${HOME:-}/.local/bin/loom" \
		"${HOME:-}/.cargo/bin/loom" \
		/usr/local/bin/loom \
		/opt/homebrew/bin/loom; do
		[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || continue
		resolved=$(cd "$(dirname "$candidate")" 2>/dev/null && pwd -P)/$(basename "$candidate")
		case "$resolved" in /tmp/* | /private/tmp/* | /var/tmp/* | "$WORKTREE_PATH"/*) continue ;; esac
		printf '%s\n' "$resolved"
		return 0
	done
	return 1
}

INPUT_JSON=$(read_input)
[[ -n "$INPUT_JSON" ]] || exit 0
[[ "$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty')" == Bash ]] || exit 0

STAGE_ID=${LOOM_STAGE_ID:-}
SESSION_ID=${LOOM_SESSION_ID:-}
WORKTREE_PATH=${LOOM_WORKTREE_PATH:-}
[[ -n "$STAGE_ID" && -n "$SESSION_ID" && -n "$WORKTREE_PATH" ]] || exit 0
# Membership, not presence (same rule as loom_current_worktree in _common.sh).
# A loom worktree is `<repo>/.worktrees/<stage-id>`; main-repo sessions
# (knowledge, merge, base-conflict) own no worktree and complete in-process, so
# this bridge must stay out of their way rather than pin their command.
[[ "$WORKTREE_PATH" =~ /\.worktrees/[^/]+ ]] || exit 0
case "$STAGE_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper stage identity" ;; esac
case "$SESSION_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper session identity" ;; esac

COMMAND=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty')
LOWER_COMMAND=$(printf '%s' "$COMMAND" | tr '[:upper:]' '[:lower:]')
case "$LOWER_COMMAND" in *loom*complete*) ;; *) exit 0 ;; esac

LOOM_BIN=$(resolve_trusted_loom) || fail_closed "no fixed trusted loom installation was found"
PINNED_COMMAND="$LOOM_BIN stage complete $STAGE_ID"
HAS_RESULT=$(printf '%s' "$INPUT_JSON" | jq -r 'has("tool_result") or has("tool_response")')
if [[ "$HAS_RESULT" != true ]]; then
	if [[ "$COMMAND" == "loom stage complete $STAGE_ID" ]]; then
		fail_closed "retry with the pinned command: $PINNED_COMMAND"
	fi
	[[ "$COMMAND" == "$PINNED_COMMAND" ]] || fail_closed "completion must be one exact pinned command: $PINNED_COMMAND"
	exit 0
fi

[[ "$COMMAND" == "$PINNED_COMMAND" ]] || fail_closed "completion result was not produced by the exact pinned command"

IS_ERROR=$(printf '%s' "$INPUT_JSON" | jq -r '(.tool_result.is_error // .tool_response.is_error // false)')
[[ "$IS_ERROR" == false ]] || exit 0
MARKER="LOOM_CONTROL_VERIFICATION_PASSED stage=$STAGE_ID session=$SESSION_ID"
HAS_MARKER=$(printf '%s' "$INPUT_JSON" | jq -r --arg marker "$MARKER" '
  [.tool_result.stdout, .tool_result.output, .tool_response.stdout, .tool_response.output]
  | map(select(type == "string")) | join("\n") | split("\n") | index($marker) != null')
[[ "$HAS_MARKER" == true ]] || exit 0

if ! BROKER_OUTPUT=$(LOOM_CONTROL_BROKER=1 "$LOOM_BIN" stage complete "$STAGE_ID" --session "$SESSION_ID" 2>&1); then
	fail_closed "daemon completion broker failed: $BROKER_OUTPUT"
fi

jq -n --arg message "Stage '$STAGE_ID' completion was accepted by the daemon." \
	'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
