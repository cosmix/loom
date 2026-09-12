#!/usr/bin/env bash
# Heartbeat writers must respect the current stage owner, serialize
# SessionStart too, recover a genuinely abandoned mkdir lock, and never expose
# a partially-written JSON document to concurrent readers.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
POST_HOOK="$SCRIPT_DIR/../post-tool-use.sh"
STOP_HOOK="$SCRIPT_DIR/../subagent-stop.sh"
START_HOOK="$SCRIPT_DIR/../session-start.sh"
COMMON="$SCRIPT_DIR/../_common.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

heartbeat() {
	local workdir="$1" session="$2" tokens="$3"
	jq -n --arg stage_id test-stage --arg session_id "$session" --argjson tokens "$tokens" \
		'{stage_id:$stage_id,session_id:$session_id,timestamp:"2026-08-30T00:00:00.000Z",context_tokens:$tokens,transcript_path:null,last_tool:null,activity:"owner"}' \
		>"$workdir/heartbeat/test-stage.json"
}

stage_owner() {
	local workdir="$1" session="$2"
	mkdir -p "$workdir/stages" "$workdir/heartbeat"
	printf '%s\n' '---' "session: $session" 'description: test' '---' \
		'# Stage' 'session: wrong-body-value' \
		>"$workdir/stages/01-test-stage.md"
}

# A PostToolUse from an old session must leave the successor heartbeat intact.
POST_WORK="$TMP/post"
stage_owner "$POST_WORK" successor
heartbeat "$POST_WORK" successor 777
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"echo old"}}' |
	env LOOM_WORK_DIR="$POST_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=wrong-body-value \
	bash "$POST_HOOK"
if [[ "$(jq -r '.session_id' "$POST_WORK/heartbeat/test-stage.json")" != successor ]] ||
	[[ "$(jq -r '.context_tokens' "$POST_WORK/heartbeat/test-stage.json")" != 777 ]]; then
	echo "FAIL: stale PostToolUse overwrote successor heartbeat"
	exit 1
fi

# A late SubagentStop is also a heartbeat writer and must make the same check.
STOP_WORK="$TMP/stop"
stage_owner "$STOP_WORK" successor
heartbeat "$STOP_WORK" successor 888
STOP_INPUT=$(jq -nc --arg transcript_path "$TMP/subagents/agent-agent-1.jsonl" \
	'{agent_type:"worker",transcript_path:$transcript_path}')
printf '%s' "$STOP_INPUT" |
	env LOOM_WORK_DIR="$STOP_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=old-session \
	bash "$STOP_HOOK"
if [[ "$(jq -r '.session_id' "$STOP_WORK/heartbeat/test-stage.json")" != successor ]] ||
	[[ "$(jq -r '.context_tokens' "$STOP_WORK/heartbeat/test-stage.json")" != 888 ]]; then
	echo "FAIL: stale SubagentStop overwrote successor heartbeat"
	exit 1
fi

# SessionStart must wait on the same lock rather than racing its initial write.
START_WORK="$TMP/start"
stage_owner "$START_WORK" new-session
heartbeat "$START_WORK" predecessor 333
START_LOCK="$START_WORK/heartbeat/test-stage.json.lock"
mkdir -m 700 "$START_LOCK"
printf '%s' '{"source":"startup"}' |
	env LOOM_HEARTBEAT_LOCK_STALE_SECONDS=30 LOOM_WORK_DIR="$START_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=new-session \
	bash "$START_HOOK" >"$TMP/start.stdout" 2>"$TMP/start.stderr" &
START_PID=$!
sleep 0.1
if ! kill -0 "$START_PID" 2>/dev/null ||
	[[ "$(jq -r '.session_id' "$START_WORK/heartbeat/test-stage.json")" != predecessor ]] ||
	[[ "$(jq -r '.context_tokens' "$START_WORK/heartbeat/test-stage.json")" != 333 ]]; then
	echo "FAIL: SessionStart did not wait for the shared heartbeat lock"
	exit 1
fi
rmdir "$START_LOCK"
wait "$START_PID"
if [[ "$(jq -r '.session_id' "$START_WORK/heartbeat/test-stage.json")" != new-session ]]; then
	echo "FAIL: successor SessionStart did not replace the predecessor heartbeat"
	exit 1
fi

# Duplicate depth-prefixed stage records make ownership ambiguous. A hook must
# leave the existing owner untouched instead of selecting whichever glob entry
# happens to appear first.
AMBIG_WORK="$TMP/ambiguous"
stage_owner "$AMBIG_WORK" ambiguous-owner
cp "$AMBIG_WORK/stages/01-test-stage.md" "$AMBIG_WORK/stages/02-test-stage.md"
heartbeat "$AMBIG_WORK" predecessor 222
printf '%s' '{"source":"startup"}' |
	env LOOM_WORK_DIR="$AMBIG_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=ambiguous-owner \
	bash "$START_HOOK"
if [[ "$(jq -r '.session_id' "$AMBIG_WORK/heartbeat/test-stage.json")" != predecessor ]]; then
	echo "FAIL: ambiguous stage files authorized a heartbeat replacement"
	exit 1
fi

# Once assignment moves forward, a delayed old SessionStart must not reclaim
# the heartbeat it would have established earlier.
stage_owner "$START_WORK" successor-start
heartbeat "$START_WORK" successor-start 444
printf '%s' '{"source":"startup"}' |
	env LOOM_WORK_DIR="$START_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=old-start \
	bash "$START_HOOK"
if [[ "$(jq -r '.session_id' "$START_WORK/heartbeat/test-stage.json")" != successor-start ]] ||
	[[ "$(jq -r '.context_tokens' "$START_WORK/heartbeat/test-stage.json")" != 444 ]]; then
	echo "FAIL: stale SessionStart overwrote newer stage assignment"
	exit 1
fi

# A lock left behind by a killed process has a dead PID and old timestamp.
# Recovery is intentionally opt-in here through a short test grace period;
# production waits 30 seconds before considering a lock abandoned.
RECOVER_WORK="$TMP/recover"
stage_owner "$RECOVER_WORK" recovered
RECOVER_LOCK="$RECOVER_WORK/heartbeat/test-stage.json.lock"
mkdir -m 700 "$RECOVER_LOCK"
printf 'pid=999999\ncreated=1\n' >"$RECOVER_LOCK/owner"
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"echo recovered"}}' |
	env LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 LOOM_WORK_DIR="$RECOVER_WORK" \
	LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=recovered bash "$POST_HOOK"
if [[ ! -f "$RECOVER_WORK/heartbeat/test-stage.json" ]] || [[ -d "$RECOVER_LOCK" ]]; then
	echo "FAIL: abandoned heartbeat lock was not recovered"
	exit 1
fi

# A delayed former owner must never remove a successor's lock. Release checks
# the published PID instead of deleting whichever owner happens to be there.
FOREIGN_LOCK="$RECOVER_WORK/heartbeat/foreign.json.lock"
mkdir -m 700 "$FOREIGN_LOCK"
printf 'pid=999999\ncreated=1\n' >"$FOREIGN_LOCK/owner"
bash -c 'source "$1"; loom_heartbeat_lock_release "$2"' _ "$COMMON" "$FOREIGN_LOCK"
if [[ ! -d "$FOREIGN_LOCK" ]] || [[ ! -f "$FOREIGN_LOCK/owner" ]]; then
	echo "FAIL: stale releaser removed another writer's heartbeat lock"
	exit 1
fi

# Multiple concurrent refreshes may replace the file, but a reader must see
# only complete JSON because every replacement is a same-directory rename.
ATOMIC_WORK="$TMP/atomic"
stage_owner "$ATOMIC_WORK" writer
heartbeat "$ATOMIC_WORK" writer 1
for n in 1 2 3 4 5 6 7 8; do
	printf '%s' "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"echo $n\"}}" |
		env LOOM_WORK_DIR="$ATOMIC_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=writer \
		bash "$POST_HOOK" >"$TMP/atomic-$n.out" 2>"$TMP/atomic-$n.err" &
done
for ((n = 1; n <= 200; n++)); do
	if ! jq -e . "$ATOMIC_WORK/heartbeat/test-stage.json" >/dev/null 2>&1; then
		echo "FAIL: concurrent heartbeat reader observed invalid JSON"
		exit 1
	fi
done
wait

echo "PASS"
