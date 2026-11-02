#!/usr/bin/env bash
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-start.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/work" "$TMP/bin" "$TMP/project/claude-parent-uuid/subagents"
LOOM_CALLS="$TMP/loom-calls"
cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$LOOM_CALLS"
if [[ "$*" == "hook review-start" ]]; then
	cat >"$LOOM_REVIEW_INPUT"
fi
exit 0
SH
chmod +x "$TMP/bin/loom"

# The Claude Code hooks reference's SubagentStart payload: transcript_path names
# the parent session's transcript, and the worker's own file does not exist yet.
PARENT_TRANSCRIPT="$TMP/project/claude-parent-uuid.jsonl"
INPUT=$(jq -nc --arg transcript "$PARENT_TRANSCRIPT" \
	'{agent_id:"agent-1",agent_type:"loom-software-engineer",session_id:"claude-parent-uuid",
	  transcript_path:$transcript,hook_event_name:"SubagentStart"}')
printf '%s' "$INPUT" |
	env PATH="$TMP/bin:/usr/bin:/bin" LOOM_CALLS="$LOOM_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-a" LOOM_SESSION_ID="parent-session" \
		bash "$HOOK"

ROW="$TMP/work/subagents/stage-a/starts.jsonl"
if ! jq -e '
    .agent_id == "agent-1" and
    .agent_type == "loom-software-engineer" and
    .stage_id == "stage-a" and
    .parent_session_id == "claude-parent-uuid" and
    .loom_session_id == "parent-session" and
    (keys_unsorted == ["agent_id", "agent_type", "stage_id", "parent_session_id", "loom_session_id", "ts"])
' "$ROW" >/dev/null; then
	echo "FAIL: SubagentStart row confused the Claude transcript UUID with Loom ownership"
	cat "$ROW" 2>/dev/null || true
	exit 1
fi

if [[ -f "$LOOM_CALLS" ]] && grep -q 'worker-brief' "$LOOM_CALLS"; then
	echo "FAIL: SubagentStart called loom hook worker-brief"
	cat "$LOOM_CALLS"
	exit 1
fi

REVIEW_INPUT_FILE="$TMP/review-input"
REVIEW_EVENT=$(jq -nc --arg transcript "$PARENT_TRANSCRIPT" \
	'{agent_id:"agent-1",agent_type:"loom-code-reviewer",
	  session_id:"claude-parent-uuid",transcript_path:$transcript,
	  hook_event_name:"SubagentStart"}')
printf '%s' "$REVIEW_EVENT" |
	env PATH="$TMP/bin:/usr/bin:/bin" LOOM_CALLS="$LOOM_CALLS" \
		LOOM_REVIEW_INPUT="$REVIEW_INPUT_FILE" LOOM_WORK_DIR="$TMP/work" \
		LOOM_STAGE_ID="stage-a" LOOM_SESSION_ID="parent-session" bash "$HOOK"
# Start evidence is keyed on the exact invocation: the transcript in this
# payload is the parent's, so it must not reach the delegate.
if ! jq -e '. == {stage_id:"stage-a",session_id:"claude-parent-uuid",agent_id:"agent-1"}' \
	"$REVIEW_INPUT_FILE" >/dev/null; then
	echo "FAIL: reviewer start did not capture the exact invocation"
	cat "$REVIEW_INPUT_FILE" 2>/dev/null || true
	exit 1
fi
if [[ $(grep -c '^hook review-start$' "$LOOM_CALLS") != 1 ]]; then
	echo "FAIL: reviewer start was not captured exactly once"
	exit 1
fi

# A payload with no transcript_path at all still captures a reviewer's start.
rm -f "$REVIEW_INPUT_FILE"
printf '%s' '{"agent_id":"agent-3","agent_type":"loom-code-reviewer","session_id":"claude-parent-uuid"}' |
	env PATH="$TMP/bin:/usr/bin:/bin" LOOM_CALLS="$LOOM_CALLS" \
		LOOM_REVIEW_INPUT="$REVIEW_INPUT_FILE" LOOM_WORK_DIR="$TMP/work" \
		LOOM_STAGE_ID="stage-a" LOOM_SESSION_ID="parent-session" bash "$HOOK"
if [[ ! -s "$REVIEW_INPUT_FILE" ]]; then
	echo "FAIL: reviewer start capture must not depend on transcript_path"
	exit 1
fi

# The Claude parent UUID is the usage join key. A Loom session id from the
# wrapper is independent and must not substitute for missing payload identity.
INPUT_NO_PARENT='{"agent_id":"agent-2","agent_type":"loom-software-engineer"}'
printf '%s' "$INPUT_NO_PARENT" |
	env PATH="$TMP/bin:/usr/bin:/bin" LOOM_CALLS="$LOOM_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-c" LOOM_SESSION_ID="loom-session" \
		bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-c/starts.jsonl" ]]; then
	echo "FAIL: SubagentStart row was written without the Claude parent UUID"
	exit 1
fi

# Missing parent identity must fail open without creating an unscoped row.
printf '%s' "$INPUT" |
	env -u LOOM_SESSION_ID PATH="$TMP/bin:/usr/bin:/bin" LOOM_CALLS="$LOOM_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-b" bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-b/starts.jsonl" ]]; then
	echo "FAIL: unscoped SubagentStart row was written without LOOM_SESSION_ID"
	exit 1
fi

echo "PASS"
