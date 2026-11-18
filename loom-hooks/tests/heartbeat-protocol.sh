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
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/heartbeat-protocol.XXXXXX") && [[ -n "$TMP" ]] || {
	echo "FAIL: could not create scratch directory"
	exit 1
}
trap '[[ -n "${TMP:-}" ]] && rm -rf -- "$TMP"' EXIT

heartbeat() {
	local workdir="$1" session="$2" tokens="$3"
	jq -n --arg stage_id test-stage --arg session_id "$session" --argjson tokens "$tokens" \
		'{stage_id:$stage_id,session_id:$session_id,timestamp:"2026-08-30T00:00:00.000Z",context_tokens:$tokens,transcript_path:null,last_tool:null,activity:"owner"}' \
		>"$workdir/heartbeat/test-stage.json"
}

stage_owner() {
	local workdir="$1" session="$2"
	mkdir -p "$workdir/stages" "$workdir/heartbeat"
	printf '%s\n' '---' 'id: test-stage' "session: $session" 'description: test' '---' \
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
STOP_PARENT_ID="stop-parent"
STOP_PARENT="$TMP/claude-project/$STOP_PARENT_ID.jsonl"
STOP_WORKER="$TMP/claude-project/$STOP_PARENT_ID/subagents/agent-agent-1.jsonl"
mkdir -p "${STOP_WORKER%/*}" "$STOP_WORK/subagents/test-stage"
printf '%s\n' '{"type":"parent","message":"waiting"}' >"$STOP_PARENT"
printf '%s\n' '{"type":"assistant","message":"done"}' >"$STOP_WORKER"
jq -nc --arg parent "$STOP_PARENT_ID" \
	'{agent_id:"agent-1",agent_type:"worker",stage_id:"test-stage",
	  parent_session_id:$parent,loom_session_id:"old-session",
	  ts:"2000-01-01T00:00:00.000Z"}' >"$STOP_WORK/subagents/test-stage/starts.jsonl"
STOP_INPUT=$(jq -nc --arg session_id "$STOP_PARENT_ID" --arg transcript_path "$STOP_PARENT" \
	--arg agent_transcript_path "$STOP_WORKER" \
	'{session_id:$session_id,hook_event_name:"SubagentStop",agent_id:"agent-1",
	  agent_type:"worker",transcript_path:$transcript_path,
	  agent_transcript_path:$agent_transcript_path}')
printf '%s' "$STOP_INPUT" |
	env LOOM_WORK_DIR="$STOP_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=old-session \
	bash "$STOP_HOOK"
if [[ "$(jq -r '.session_id' "$STOP_WORK/heartbeat/test-stage.json")" != successor ]] ||
	[[ "$(jq -r '.context_tokens' "$STOP_WORK/heartbeat/test-stage.json")" != 888 ]]; then
	echo "FAIL: stale SubagentStop overwrote successor heartbeat"
	exit 1
fi
if [[ -e "$STOP_WORK/subagents/test-stage/lifecycle.jsonl" ]]; then
	echo "FAIL: stale SubagentStop wrote lifecycle evidence for its successor"
	exit 1
fi

# SessionStart must wait on the same lock rather than racing its initial write.
START_WORK="$TMP/start"
stage_owner "$START_WORK" new-session
heartbeat "$START_WORK" predecessor 333
START_LOCK="$START_WORK/heartbeat/test-stage.json.lock"
mkdir -m 700 "$START_LOCK"
# A mkdir shim marks the hook's first attempt on this lock. Without it, a hook
# merely slower than the pause below would pass as waiting; a hook that never
# takes the lock now fails on the marker.
START_SHIMS="$TMP/start-shims"
mkdir "$START_SHIMS"
REAL_MKDIR=$(command -v mkdir)
cat >"$START_SHIMS/mkdir" <<EOF
#!/bin/sh
for arg in "\$@"; do
	[ "\$arg" != "$START_LOCK" ] || : >"$TMP/start-reached-lock"
done
exec "$REAL_MKDIR" "\$@"
EOF
chmod +x "$START_SHIMS/mkdir"
printf '%s' '{"source":"startup"}' |
	env PATH="$START_SHIMS:$PATH" LOOM_HEARTBEAT_LOCK_STALE_SECONDS=30 LOOM_WORK_DIR="$START_WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=new-session \
	bash "$START_HOOK" >"$TMP/start.stdout" 2>"$TMP/start.stderr" &
START_PID=$!
for ((n = 0; n < 400; n++)); do
	[[ ! -e "$TMP/start-reached-lock" ]] || break
	kill -0 "$START_PID" 2>/dev/null || break
	sleep 0.01
done
if [[ ! -e "$TMP/start-reached-lock" ]]; then
	echo "FAIL: SessionStart never tried the shared heartbeat lock"
	exit 1
fi
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
mkdir -p -m 700 "$RECOVER_LOCK/owner.999999.1.0"
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"echo recovered"}}' |
	env LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 LOOM_WORK_DIR="$RECOVER_WORK" \
	LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=recovered bash "$POST_HOOK"
if [[ ! -f "$RECOVER_WORK/heartbeat/test-stage.json" ]] || [[ -d "$RECOVER_LOCK" ]]; then
	echo "FAIL: abandoned heartbeat lock was not recovered"
	exit 1
fi

# A creator can die between mkdir and publishing owner metadata.
EMPTY_LOCK="$RECOVER_WORK/heartbeat/empty.json.lock"
mkdir -m 700 "$EMPTY_LOCK"
touch -t 200001010000 "$EMPTY_LOCK"
LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$EMPTY_LOCK"
[[ ! -d "$EMPTY_LOCK" ]] || { echo "FAIL: abandoned empty lock was not recovered"; exit 1; }

# Pause two reclaimers after each has read the same dead owner. The first
# retires that generation and a live writer enters before the second resumes.
# The stale decision must leave the new owner in place and a third writer out.
RACE_LOCK="$RECOVER_WORK/heartbeat/race.json.lock"
mkdir -p -m 700 "$RACE_LOCK/owner.999999.1.0"
start_paused_reclaimer() {
	bash -c '
		source "$1"
		marker="$3"
		kill() {
			if [[ "$1" == -0 && "$2" == 999999 ]]; then
				: >"$marker-paused"
				while [[ ! -e "$marker-resume" ]]; do sleep 0.01; done
			fi
			builtin kill "$1" "$2"
		}
		loom_heartbeat_lock_recover_if_abandoned "$2" || true
	' _ "$COMMON" "$RACE_LOCK" "$TMP/$1" &
}
await_path() {
	local n
	for ((n = 0; n < 400; n++)); do
		[[ -e "$1" ]] && return 0
		sleep 0.01
	done
	echo "FAIL: $2"
	exit 1
}
start_paused_reclaimer reclaimer-a
RECLAIMER_A=$!
start_paused_reclaimer reclaimer-b
RECLAIMER_B=$!
await_path "$TMP/reclaimer-a-paused" "first reclaimer did not pause"
await_path "$TMP/reclaimer-b-paused" "second reclaimer did not pause"
: >"$TMP/reclaimer-a-resume"
wait "$RECLAIMER_A"
[[ ! -e "$RACE_LOCK" ]] || { echo "FAIL: first reclaimer did not retire the dead owner"; exit 1; }
bash -c '
	source "$1"
	loom_heartbeat_lock_acquire "$2" || exit 1
	: >"$3/successor-entered"
	while [[ ! -e "$3/release-successor" ]]; do sleep 0.01; done
	loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$RACE_LOCK" "$TMP" &
SUCCESSOR_PID=$!
await_path "$TMP/successor-entered" "successor did not acquire"
SUCCESSOR_OWNER=$(compgen -G "$RACE_LOCK/owner.*" || true)
[[ -n "$SUCCESSOR_OWNER" ]] || { echo "FAIL: successor owner was not published"; exit 1; }
: >"$TMP/reclaimer-b-resume"
wait "$RECLAIMER_B"
bash -c '
	source "$1"
	loom_heartbeat_lock_acquire "$2" || exit 1
	: >"$3/third-entered"
	loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$RACE_LOCK" "$TMP" &
THIRD_PID=$!
sleep 0.2
if [[ -e "$TMP/third-entered" ]] || [[ "$(compgen -G "$RACE_LOCK/owner.*" || true)" != "$SUCCESSOR_OWNER" ]]; then
	echo "FAIL: delayed reclaimer removed a live successor's lock"
	exit 1
fi
: >"$TMP/release-successor"
wait "$SUCCESSOR_PID"
wait "$THIRD_PID"
[[ -e "$TMP/third-entered" ]] || { echo "FAIL: third writer never acquired after release"; exit 1; }

# A delayed former owner must never remove a successor's lock. Release touches
# only owner entries carrying this process's PID.
FOREIGN_LOCK="$RECOVER_WORK/heartbeat/foreign.json.lock"
mkdir -p -m 700 "$FOREIGN_LOCK/owner.999999.1.0"
bash -c 'source "$1"; loom_heartbeat_lock_release "$2"' _ "$COMMON" "$FOREIGN_LOCK"
if [[ ! -d "$FOREIGN_LOCK/owner.999999.1.0" ]]; then
	echo "FAIL: stale releaser removed another writer's heartbeat lock"
	exit 1
fi

# Unknown or malformed lock state is never stolen, however old it looks.
for MALFORMED in owner.garbage owner.999999.1.x; do
	MALFORMED_LOCK="$RECOVER_WORK/heartbeat/malformed-$MALFORMED.lock"
	mkdir -p -m 700 "$MALFORMED_LOCK/$MALFORMED"
	touch -t 200001010000 "$MALFORMED_LOCK"
	if LOOM_HEARTBEAT_LOCK_STALE_SECONDS=0 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$MALFORMED_LOCK" ||
		[[ ! -d "$MALFORMED_LOCK/$MALFORMED" ]]; then
		echo "FAIL: malformed owner entry $MALFORMED was stolen"
		exit 1
	fi
done
for SHAPE in file symlink; do
	SHAPE_LOCK="$RECOVER_WORK/heartbeat/shape-$SHAPE.lock"
	mkdir -m 700 "$SHAPE_LOCK"
	if [[ "$SHAPE" == file ]]; then
		: >"$SHAPE_LOCK/owner.999999.1.0"
	else
		ln -s "$TMP" "$SHAPE_LOCK/owner.999999.1.0"
	fi
	if LOOM_HEARTBEAT_LOCK_STALE_SECONDS=0 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$SHAPE_LOCK" ||
		[[ ! -e "$SHAPE_LOCK/owner.999999.1.0" ]]; then
		echo "FAIL: non-directory owner entry ($SHAPE) was stolen"
		exit 1
	fi
done

# A live owner that started before it took the lock is safe at any age.
LIVE_LOCK="$RECOVER_WORK/heartbeat/live.json.lock"
sleep 30 &
LIVE_PID=$!
LIVE_OWNER="$LIVE_LOCK/owner.$LIVE_PID.$(date +%s).0"
mkdir -p -m 700 "$LIVE_OWNER"
touch -t 200001010000 "$LIVE_LOCK"
if LOOM_HEARTBEAT_LOCK_STALE_SECONDS=0 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$LIVE_LOCK" ||
	[[ ! -d "$LIVE_OWNER" ]]; then
	kill "$LIVE_PID" 2>/dev/null || true
	echo "FAIL: a live owner was stolen"
	exit 1
fi

# The same PID under an owner stamped long before that process existed is PID
# reuse: the writer that took the lock is gone, and nothing else will free it.
REUSED_LOCK="$RECOVER_WORK/heartbeat/reused.json.lock"
mkdir -p -m 700 "$REUSED_LOCK/owner.$LIVE_PID.1.0"
if ! LOOM_HEARTBEAT_LOCK_STALE_SECONDS=30 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$REUSED_LOCK" ||
	[[ -e "$REUSED_LOCK" ]]; then
	kill "$LIVE_PID" 2>/dev/null || true
	echo "FAIL: a lock held only by a reused PID was never recovered"
	exit 1
fi
kill "$LIVE_PID" 2>/dev/null || true
wait "$LIVE_PID" 2>/dev/null || true

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
