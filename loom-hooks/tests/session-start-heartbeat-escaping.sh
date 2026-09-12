#!/usr/bin/env bash
# session-start-heartbeat-escaping.sh - regression test for session-start.sh's
# heartbeat write: it used to build the JSON record with a raw heredoc, so a
# transcript_path containing a `"` produced a heartbeat file that was not
# valid JSON (breaking the Rust HeartbeatWatcher and the carry-forward `jq`
# in post-tool-use.sh/subagent-stop.sh). It now builds the record with
# `jq -n --arg`, matching the other two heartbeat writers.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../session-start.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

LOOM_WORK_DIR="$TMP/work"
LOOM_STAGE_ID="test-stage"
LOOM_SESSION_ID="test-session"
mkdir -p "$LOOM_WORK_DIR"

TRANSCRIPT_PATH='/home/user/some"quote/path.jsonl'
INPUT=$(jq -nc --arg tp "$TRANSCRIPT_PATH" '{transcript_path:$tp,source:"startup"}')

OUTPUT=$(printf '%s' "$INPUT" |
	env -u LOOM_HOOK_DEBUG -u COMMIT_FILTER_DEBUG \
		LOOM_WORK_DIR="$LOOM_WORK_DIR" LOOM_STAGE_ID="$LOOM_STAGE_ID" LOOM_SESSION_ID="$LOOM_SESSION_ID" \
		bash "$HOOK")

if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected no stdout for a 'startup' source, got: $OUTPUT"
	exit 1
fi

HEARTBEAT_FILE="$LOOM_WORK_DIR/heartbeat/${LOOM_STAGE_ID}.json"
if [[ ! -f "$HEARTBEAT_FILE" ]]; then
	echo "FAIL: heartbeat file was not created at $HEARTBEAT_FILE"
	exit 1
fi

# The heartbeat directory must be created 700, matching post-tool-use.sh -
# SessionStart runs first, so without this the directory would sit at the
# mkdir default (0755) until the first PostToolUse tightened it.
HEARTBEAT_DIR="$LOOM_WORK_DIR/heartbeat"
DIR_MODE=$(stat -c '%a' "$HEARTBEAT_DIR" 2>/dev/null || stat -f '%Lp' "$HEARTBEAT_DIR" 2>/dev/null)
if [[ "$DIR_MODE" != "700" ]]; then
	echo "FAIL: heartbeat directory mode expected 700, got $DIR_MODE"
	exit 1
fi

if ! jq -e . "$HEARTBEAT_FILE" >/dev/null 2>&1; then
	echo "FAIL: heartbeat file is not valid JSON"
	cat "$HEARTBEAT_FILE"
	exit 1
fi

# All seven keys the Rust Heartbeat struct deserializes must be present.
for key in stage_id session_id timestamp context_tokens transcript_path last_tool activity; do
	if ! jq -e "has(\"$key\")" "$HEARTBEAT_FILE" >/dev/null 2>&1; then
		echo "FAIL: heartbeat file is missing key '$key'"
		cat "$HEARTBEAT_FILE"
		exit 1
	fi
done

GOT_STAGE=$(jq -r '.stage_id' "$HEARTBEAT_FILE")
if [[ "$GOT_STAGE" != "$LOOM_STAGE_ID" ]]; then
	echo "FAIL: stage_id mismatch - expected $LOOM_STAGE_ID, got $GOT_STAGE"
	exit 1
fi

GOT_TRANSCRIPT=$(jq -r '.transcript_path' "$HEARTBEAT_FILE")
if [[ "$GOT_TRANSCRIPT" != "$TRANSCRIPT_PATH" ]]; then
	echo "FAIL: transcript_path did not round-trip through the quote"
	echo "  expected: $TRANSCRIPT_PATH"
	echo "  got:      $GOT_TRANSCRIPT"
	exit 1
fi

echo "PASS"
