#!/usr/bin/env bash
# Test: session-end.sh records the SessionEnd hook's `reason` field (and
# handles an empty/no-JSON stdin) in the logged events.jsonl entry.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../session-end.sh"
source "$SCRIPT_DIR/_path_without.sh"

TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
NOLOOM_PATH=$(path_without loom)
trap 'rm -rf "$TMPDIR_TEST" "$NOLOOM_PATH"' EXIT

LOOM_WORK_DIR="$TMPDIR_TEST"
LOOM_STAGE_ID="test-stage"
LOOM_SESSION_ID="session-test-abc"

mkdir -p "${LOOM_WORK_DIR}/hooks" "${LOOM_WORK_DIR}/stages"
# A depth-prefixed, non-completed stage file, matching what `loom run` writes
# before any session spawns (session-end.sh globs "*-${LOOM_STAGE_ID}.md").
printf '%s\n' '---' 'status: Executing' '---' 'status: Completed' 'status: Verified' >"${LOOM_WORK_DIR}/stages/01-${LOOM_STAGE_ID}.md"

EVENTS_FILE="${LOOM_WORK_DIR}/hooks/events.jsonl"

# Case 1: stdin carries a "reason":"other" payload (an instant-exit startup
# refusal arrives this way) - stage is not completed, so the hook also tries
# `loom handoff`; PATH is arranged so `loom` is not found to keep this test
# hermetic.
echo '{"hook_event_name":"SessionEnd","reason":"other"}' | \
	PATH="$NOLOOM_PATH" \
	LOOM_WORK_DIR="$LOOM_WORK_DIR" \
	LOOM_STAGE_ID="$LOOM_STAGE_ID" \
	LOOM_SESSION_ID="$LOOM_SESSION_ID" \
	bash "$HOOK"

if [[ ! -f "$EVENTS_FILE" ]]; then
	echo "FAIL: events.jsonl was not created"
	exit 1
fi

LAST_LINE=$(tail -n 1 "$EVENTS_FILE")
if ! echo "$LAST_LINE" | jq -e . >/dev/null 2>&1; then
	echo "FAIL: last line of events.jsonl is not valid JSON: $LAST_LINE"
	exit 1
fi

REASON=$(echo "$LAST_LINE" | jq -r '.payload.reason')
if [[ "$REASON" != "other" ]]; then
	echo "FAIL: expected .payload.reason == 'other', got '$REASON'"
	exit 1
fi

COMPLETED=$(echo "$LAST_LINE" | jq -r '.payload.completed')
if [[ "$COMPLETED" != "false" ]]; then
	echo "FAIL: expected .payload.completed == false, got '$COMPLETED'"
	exit 1
fi

# Case 2: empty stdin - the event must still be appended, with an empty
# reason rather than the hook failing.
: | \
	PATH="$NOLOOM_PATH" \
	LOOM_WORK_DIR="$LOOM_WORK_DIR" \
	LOOM_STAGE_ID="$LOOM_STAGE_ID" \
	LOOM_SESSION_ID="$LOOM_SESSION_ID" \
	bash "$HOOK"

LAST_LINE2=$(tail -n 1 "$EVENTS_FILE")
if ! echo "$LAST_LINE2" | jq -e . >/dev/null 2>&1; then
	echo "FAIL: last line of events.jsonl (case 2) is not valid JSON: $LAST_LINE2"
	exit 1
fi

REASON2=$(echo "$LAST_LINE2" | jq -r '.payload.reason')
if [[ "$REASON2" != "" ]]; then
	echo "FAIL: expected .payload.reason == '' for empty stdin, got '$REASON2'"
	exit 1
fi


# Exercise the real handoff path for body prose, terminal status and ambiguity.
STUB="$TMPDIR_TEST/loom-stub"
printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\n" handoff >>"$HANDOFF_LOG"' >"$STUB"
chmod +x "$STUB"
export HANDOFF_LOG="$TMPDIR_TEST/handoffs"
run_handoff_case() {
    printf '{}' | LOOM_BIN="$STUB" LOOM_WORK_DIR="$LOOM_WORK_DIR" \
        LOOM_STAGE_ID="$LOOM_STAGE_ID" LOOM_SESSION_ID="$LOOM_SESSION_ID" bash "$HOOK"
}
run_handoff_case
[[ $(jq -r '.payload.completed' "$EVENTS_FILE" | tail -n 1) == false ]]
[[ $(wc -l <"$HANDOFF_LOG" | tr -d ' ') == 1 ]]
for terminal in Completed Verified; do
    printf '%s\n' '---' "status: $terminal" '---' >"$LOOM_WORK_DIR/stages/01-$LOOM_STAGE_ID.md"
    run_handoff_case
    [[ $(jq -r '.payload.completed' "$EVENTS_FILE" | tail -n 1) == true ]]
    [[ $(wc -l <"$HANDOFF_LOG" | tr -d ' ') == 1 ]]
done
printf '%s\n' '---' 'status: Completed' '---' >"$LOOM_WORK_DIR/stages/$LOOM_STAGE_ID.md"
run_handoff_case
[[ $(jq -r '.payload.completed' "$EVENTS_FILE" | tail -n 1) == false ]]
[[ $(wc -l <"$HANDOFF_LOG" | tr -d ' ') == 2 ]]

echo "PASS: session-end records the SessionEnd reason from stdin"
