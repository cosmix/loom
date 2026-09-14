#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SESSION_START="$SCRIPT_DIR/../session-start.sh"
POST_TOOL_USE="$SCRIPT_DIR/../post-tool-use.sh"
STAGE_ID="progress-stage"
SESSION_ID="progress-session"
SEEDED_AT="2000-01-01T00:00:00.000Z"
LEGACY_AT="1999-01-01T00:00:00.000Z"

TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/progress-heartbeat.XXXXXX")
if [[ -z "$TEST_ROOT" || ! -d "$TEST_ROOT" ]]; then
	echo "FAIL: mktemp did not create a unique fixture directory"
	exit 1
fi
trap 'rm -rf "$TEST_ROOT"' EXIT

WORK_DIR="$TEST_ROOT/work"
TRANSCRIPT="$TEST_ROOT/transcripts/$SESSION_ID.jsonl"
HEARTBEAT="$WORK_DIR/heartbeat/$STAGE_ID.json"
mkdir -p "$WORK_DIR/stages" "$WORK_DIR/sessions" "${TRANSCRIPT%/*}"
cat >"$WORK_DIR/stages/$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
session: $SESSION_ID
---
EOF
jq -nc --arg stage "$STAGE_ID" --arg session "$SESSION_ID" \
	'{stage_id:$stage,session_id:$session}' >"$WORK_DIR/sessions/$SESSION_ID.json"

append_usage() {
	local tokens="$1"
	jq -nc --argjson tokens "$tokens" \
		'{type:"assistant",message:{usage:{input_tokens:$tokens,cache_creation_input_tokens:0,cache_read_input_tokens:0}}}' \
		>>"$TRANSCRIPT"
}

fail_heartbeat() {
	local message="$1"
	echo "FAIL: $message"
	jq . "$HEARTBEAT" 2>/dev/null || true
	exit 1
}

reset_heartbeat_times() {
	local timestamp="$1" temp="$HEARTBEAT.reset"
	jq --arg timestamp "$timestamp" \
		'.timestamp = $timestamp | .progress_at = $timestamp' "$HEARTBEAT" >"$temp"
	mv "$temp" "$HEARTBEAT"
}

run_bash_tool() {
	local command="$1" input
	input=$(jq -nc --arg command "$command" --arg transcript "$TRANSCRIPT" \
		'{tool_name:"Bash",tool_input:{command:$command},transcript_path:$transcript}')
	printf '%s' "$input" | bash "$POST_TOOL_USE" >/dev/null
}

assert_observation() {
	local label="$1" tokens="$2" progress_at="$3"
	if ! jq -e --arg progress_at "$progress_at" --arg transcript "$TRANSCRIPT" \
		--argjson tokens "$tokens" '
			.timestamp > $progress_at and
			.progress_at == $progress_at and
			.activity_kind == "observation" and
			.context_tokens == $tokens and
			.transcript_path == $transcript
		' "$HEARTBEAT" >/dev/null; then
		fail_heartbeat "$label did not advance observation state exactly"
	fi
}

assert_progress() {
	local label="$1" tool="$2"
	if [[ "$tool" == "<null>" ]]; then
		if jq -e '
			.progress_at == .timestamp and
			.activity_kind == "progress" and
			.last_tool == null
		' "$HEARTBEAT" >/dev/null; then
			return 0
		fi
	elif jq -e --arg tool "$tool" '
			.progress_at == .timestamp and
			.activity_kind == "progress" and
			.last_tool == $tool
		' "$HEARTBEAT" >/dev/null; then
		return 0
	fi
	fail_heartbeat "$label was not recorded as useful progress"
}

export LOOM_WORK_DIR="$WORK_DIR"
export LOOM_STAGE_ID="$STAGE_ID"
export LOOM_SESSION_ID="$SESSION_ID"
export LOOM_BIN="$TEST_ROOT/missing-loom"
unset LOOM_HOOK_PATH LOOM_SESSION_TYPE

append_usage 1
if [[ ! -s "$TRANSCRIPT" ]]; then
	echo "FAIL: transcript fixture is empty"
	exit 1
fi
START_INPUT=$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{source:"startup",transcript_path:$transcript}')
printf '%s' "$START_INPUT" | bash "$SESSION_START" >/dev/null
if [[ ! -s "$HEARTBEAT" ]]; then
	echo "FAIL: SessionStart did not seed a heartbeat"
	exit 1
fi
if ! jq -e --arg transcript "$TRANSCRIPT" '
	.progress_at == .timestamp and
	.activity_kind == "progress" and
	.context_tokens == null and
	.transcript_path == $transcript
' "$HEARTBEAT" >/dev/null; then
	fail_heartbeat "SessionStart seed fields differ"
fi

commands=(
	"loom subagents list"
	"/usr/local/bin/loom subagents harvest"
	"env FOO=1 loom subagents list"
	"git status"
)
tokens=(10 20 30 40)
for index in "${!commands[@]}"; do
	reset_heartbeat_times "$SEEDED_AT"
	append_usage "${tokens[$index]}"
	run_bash_tool "${commands[$index]}"
	assert_observation "${commands[$index]}" "${tokens[$index]}" "$SEEDED_AT"
done

reset_heartbeat_times "$SEEDED_AT"
EDIT_INPUT=$(jq -nc --arg transcript "$TRANSCRIPT" --arg path "$TEST_ROOT/edited.rs" \
	'{tool_name:"Edit",tool_input:{file_path:$path,old_string:"old",new_string:"new"},transcript_path:$transcript}')
printf '%s' "$EDIT_INPUT" | bash "$POST_TOOL_USE" >/dev/null
assert_progress "Edit" "Edit"

reset_heartbeat_times "$SEEDED_AT"
run_bash_tool "cargo build"
assert_progress "cargo build" "Bash"

reset_heartbeat_times "$SEEDED_AT"
run_bash_tool "loom subagents list && cargo build"
assert_progress "mixed observation and build" "Bash"

reset_heartbeat_times "$SEEDED_AT"
run_bash_tool "&&"
assert_progress "malformed Bash command" "Bash"

LEGACY_TEMP="$HEARTBEAT.legacy"
jq --arg timestamp "$LEGACY_AT" \
	'del(.progress_at) | .timestamp = $timestamp' "$HEARTBEAT" >"$LEGACY_TEMP"
mv "$LEGACY_TEMP" "$HEARTBEAT"
append_usage 50
run_bash_tool "loom subagents list"
assert_observation "legacy heartbeat observation" 50 "$LEGACY_AT"

reset_heartbeat_times "$SEEDED_AT"
bash -c '
	source "$1/_common.sh"
	source "$1/_lifecycle.sh"
	loom_lifecycle_refresh_heartbeat "$2" "$3" "$4" \
		progress "subagent agent-1 finished" subagent-stop
' _ "$SCRIPT_DIR/.." "$WORK_DIR" "$STAGE_ID" "$SESSION_ID"
assert_progress "subagent-stop terminal evidence" "<null>"

reset_heartbeat_times "$SEEDED_AT"
bash -c '
	source "$1/_common.sh"
	source "$1/_lifecycle.sh"
	loom_lifecycle_refresh_heartbeat "$2" "$3" "$4" \
		observation "teammate worker-1 idle" teammate-idle
' _ "$SCRIPT_DIR/.." "$WORK_DIR" "$STAGE_ID" "$SESSION_ID"
if ! jq -e --arg progress_at "$SEEDED_AT" '
	.timestamp > $progress_at and
	.progress_at == $progress_at and
	.activity_kind == "observation" and
	.last_tool == null and
	.activity == "teammate worker-1 idle"
' "$HEARTBEAT" >/dev/null; then
	fail_heartbeat "teammate-idle did not preserve useful progress time"
fi

echo "PASS"
