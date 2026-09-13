#!/usr/bin/env bash
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-start.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/work" "$TMP/bin" "$TMP/parent/subagents"
BIND_CALLS="$TMP/bind-calls"
cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$BIND_CALLS"
exit 0
SH
chmod +x "$TMP/bin/loom"

TRANSCRIPT="$TMP/parent/subagents/agent-agent-1.jsonl"
printf '%s\n' '{"message":{"content":"<!-- loom-worker-brief nonce=0123456789abcdef0123456789abcdef -->"}}' >"$TRANSCRIPT"
INPUT=$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{agent_id:"agent-1",agent_type:"loom-software-engineer",session_id:"claude-parent-uuid",transcript_path:$transcript}')
printf '%s' "$INPUT" |
	env PATH="$TMP/bin:/usr/bin:/bin" BIND_CALLS="$BIND_CALLS" \
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

EXPECTED_BIND="hook worker-brief --bind-agent agent-1 --agent-type loom-software-engineer --transcript $TRANSCRIPT"
if [[ ! -f "$BIND_CALLS" ]] || [[ $(wc -l <"$BIND_CALLS") -ne 1 ]] || [[ "$(<"$BIND_CALLS")" != "$EXPECTED_BIND" ]]; then
	echo "FAIL: SubagentStart did not pass the exact worker-brief bind argv"
	cat "$BIND_CALLS" 2>/dev/null || true
	exit 1
fi

INPUT_NO_TRANSCRIPT='{"agent_id":"agent-3","agent_type":"loom-software-engineer","session_id":"claude-parent-uuid"}'
printf '%s' "$INPUT_NO_TRANSCRIPT" |
	env PATH="$TMP/bin:/usr/bin:/bin" BIND_CALLS="$BIND_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-d" LOOM_SESSION_ID="parent-session" \
		bash "$HOOK"
if [[ $(wc -l <"$BIND_CALLS") -ne 1 ]]; then
	echo "FAIL: SubagentStart invoked worker-brief without a transcript path"
	exit 1
fi

# The Claude parent UUID is the usage join key. A Loom session id from the
# wrapper is independent and must not substitute for missing payload identity.
INPUT_NO_PARENT='{"agent_id":"agent-2","agent_type":"loom-software-engineer"}'
printf '%s' "$INPUT_NO_PARENT" |
	env PATH="$TMP/bin:/usr/bin:/bin" BIND_CALLS="$BIND_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-c" LOOM_SESSION_ID="loom-session" \
		bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-c/starts.jsonl" ]]; then
	echo "FAIL: SubagentStart row was written without the Claude parent UUID"
	exit 1
fi

# Missing parent identity must fail open without creating an unscoped row.
printf '%s' "$INPUT" |
	env -u LOOM_SESSION_ID PATH="$TMP/bin:/usr/bin:/bin" BIND_CALLS="$BIND_CALLS" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="stage-b" bash "$HOOK"
if [[ -e "$TMP/work/subagents/stage-b/starts.jsonl" ]]; then
	echo "FAIL: unscoped SubagentStart row was written without LOOM_SESSION_ID"
	exit 1
fi

echo "PASS"
