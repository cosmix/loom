#!/usr/bin/env bash
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-start.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/work"
INPUT='{"agent_id":"agent-1","agent_type":"loom-software-engineer","session_id":"claude-parent-uuid"}'
printf '%s' "$INPUT" |
	env LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-a" LOOM_SESSION_ID="parent-session" \
		bash "$HOOK"

ROW="$TMP/work/subagents/stage-a/starts.jsonl"
if ! jq -e '
    .agent_id == "agent-1" and
    .agent_type == "loom-software-engineer" and
    .stage_id == "stage-a" and
    .parent_session_id == "claude-parent-uuid" and
    .loom_session_id == "parent-session"
' "$ROW" >/dev/null; then
	echo "FAIL: SubagentStart row confused the Claude transcript UUID with Loom ownership"
	cat "$ROW" 2>/dev/null || true
	exit 1
fi

# The Claude parent UUID is the usage join key. A Loom session id from the
# wrapper is independent and must not substitute for missing payload identity.
INPUT_NO_PARENT='{"agent_id":"agent-2","agent_type":"loom-software-engineer"}'
printf '%s' "$INPUT_NO_PARENT" |
	env LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-c" LOOM_SESSION_ID="loom-session" \
		bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-c/starts.jsonl" ]]; then
	echo "FAIL: SubagentStart row was written without the Claude parent UUID"
	exit 1
fi

# Missing parent identity must fail open without creating an unscoped row.
printf '%s' "$INPUT" |
	env -u LOOM_SESSION_ID LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-b" bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-b/starts.jsonl" ]]; then
	echo "FAIL: unscoped SubagentStart row was written without LOOM_SESSION_ID"
	exit 1
fi

echo "PASS"
