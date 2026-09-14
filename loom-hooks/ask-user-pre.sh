#!/usr/bin/env bash
# loom pre-AskUserQuestion hook - runs before asking user a question
# Called by Claude Code's PreToolUse hook mechanism
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "AskUserQuestion", "tool_input": {...}, ...}
#
# Environment variables (set by loom worktree settings):
#   LOOM_SESSION_ID - The session identifier
#   LOOM_STAGE_ID   - The stage being worked on
#   LOOM_WORK_DIR   - Path to the state directory (.loom/work/, or the
#                     legacy .work/ for a workspace that already resolved
#                     to it)

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

# Capture stdin instead of blindly draining it: this hook is the only signal
# that decides whether a stage flips to waiting-for-input, so it needs to be
# able to tell a real AskUserQuestion call from Claude Code's other internal
# paths onto the same permission pipeline (a `confirmWithUser` helper, a
# plugin `ui.ask` bridge) that never log a tool_use at all.
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Only run if this is a loom-managed session
if [ -z "$LOOM_STAGE_ID" ] || [ -z "$LOOM_SESSION_ID" ]; then
	exit 0
fi

# Verify this really is a PreToolUse call for AskUserQuestion before acting on
# it. Missing jq, or absent fields, fails open - behave as before this check
# existed - rather than risk never marking a real question as waiting. The
# decision is captured in ACT rather than acted on immediately, so the trigger
# still gets recorded (see below) even when it turns out to be spurious.
HAVE_JQ=0
TOOL_NAME=""
HOOK_EVENT=""
AGENT_ID=""
CLAUDE_SESSION_ID=""
TOOL_USE_ID=""
SOURCE=""
ACT=1
if command -v jq &>/dev/null; then
	HAVE_JQ=1
	TOOL_NAME=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
	HOOK_EVENT=$(printf '%s' "$INPUT_JSON" | jq -r '.hook_event_name // empty' 2>/dev/null || true)
	if [ -n "$TOOL_NAME" ] && [ "$TOOL_NAME" != "AskUserQuestion" ]; then
		ACT=0
	fi
	if [ -n "$HOOK_EVENT" ] && [ "$HOOK_EVENT" != "PreToolUse" ]; then
		ACT=0
	fi
	AGENT_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
	CLAUDE_SESSION_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
	TOOL_USE_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_use_id // empty' 2>/dev/null || true)
	SOURCE=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.metadata.source // empty' 2>/dev/null || true)
fi

# Change to the project directory (parent of the state directory). The
# current layout nests the state dir two levels below the project root
# (<root>/.loom/work); a workspace that already resolved to the legacy
# layout nests it one level below (<root>/.work) - see doc/plans,
# "Back-compat".
if [ -n "$LOOM_WORK_DIR" ]; then
	case "$LOOM_WORK_DIR" in
	*/.loom/work) cd "$(dirname "$(dirname "$LOOM_WORK_DIR")")" 2>/dev/null || exit 0 ;;
	*) cd "$(dirname "$LOOM_WORK_DIR")" 2>/dev/null || exit 0 ;;
	esac
fi

# Record the trigger before checking ACT - this trail is what attributes the
# NEXT spurious trigger, so it has to exist even for one that turns out to be
# unwanted. The no-jq fallback always acts (fail-open), so it always logs
# acted:true.
if [ -n "$LOOM_WORK_DIR" ]; then
	HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
	if mkdir -p "$HOOKS_DIR" 2>/dev/null; then
		EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
		TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
		if [ "$HAVE_JQ" = "1" ]; then
			ACTED_JSON="true"
			[ "$ACT" = "0" ] && ACTED_JSON="false"
			jq -nc \
				--arg timestamp "$TIMESTAMP" \
				--arg stage_id "$LOOM_STAGE_ID" \
				--arg session_id "$LOOM_SESSION_ID" \
				--arg tool_name "$TOOL_NAME" \
				--arg hook_event_name "$HOOK_EVENT" \
				--arg agent_id "$AGENT_ID" \
				--arg claude_session_id "$CLAUDE_SESSION_ID" \
				--arg tool_use_id "$TOOL_USE_ID" \
				--arg source "$SOURCE" \
				--argjson acted "$ACTED_JSON" \
				'{timestamp:$timestamp,stage_id:$stage_id,session_id:$session_id,event:"AskUserQuestion",payload:{type:"AskUserQuestion",phase:"pre",tool_name:$tool_name,hook_event_name:$hook_event_name,agent_id:$agent_id,claude_session_id:$claude_session_id,tool_use_id:$tool_use_id,source:$source,acted:$acted}}' \
				>>"$EVENTS_FILE" 2>/dev/null || true
		else
			cat >>"$EVENTS_FILE" 2>/dev/null <<EOF || true
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"AskUserQuestion","payload":{"type":"AskUserQuestion","phase":"pre","acted":true}}
EOF
		fi
	fi
fi

if [ "$ACT" = "0" ]; then
	exit 0
fi

# Mark stage as waiting for user input
LOOM_HOOK_CONTEXT=1 "${LOOM_BIN:-loom}" stage waiting "$LOOM_STAGE_ID" 2>&1 || {
	echo "Note: Could not mark stage as waiting (loom not available)"
}

# Prepare notification message
MESSAGE="loom stage $LOOM_STAGE_ID needs your input"

# Send desktop notification based on platform
if [[ "$OSTYPE" == "darwin"* ]]; then
	# macOS notification
	osascript -e "display notification \"$MESSAGE\" with title \"loom\"" 2>/dev/null
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
	# Linux notification
	notify-send -u critical "loom" "$MESSAGE" 2>/dev/null
fi

# Ring terminal bell
printf '\a'

exit 0
