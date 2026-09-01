#!/usr/bin/env bash
# On success the hook must print exactly one JSON object on one line - never
# multiple lines, never non-JSON. The fake `loom` binary emits the exact
# shape the real delegate contract specifies.
set -euo pipefail
HOOK="$(dirname "$0")/../user-prompt-context.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/work"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
[[ "$1" == "hook" && "$2" == "user-prompt" ]] || exit 1
cat >/dev/null
jq -nc '{hookSpecificOutput:{hookEventName:"UserPromptSubmit",additionalContext:"the retrieved brief"}}'
SH
chmod +x "$TMP/bin/loom"

INPUT='{"session_id":"s1","prompt":"explain how the retrieval pipeline picks which knowledge sections to quote"}'

OUTPUT=$(printf '%s' "$INPUT" |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH -u LOOM_SESSION_ID \
	PATH="$TMP/bin:/usr/bin:/bin" LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="test-stage" \
	bash "$HOOK")

LINE_COUNT=$(printf '%s\n' "$OUTPUT" | wc -l)
if [[ "$LINE_COUNT" -ne 1 ]]; then
	echo "FAIL: expected exactly 1 line of output, got $LINE_COUNT. Output: $OUTPUT"
	exit 1
fi

set +e
TYPE=$(printf '%s' "$OUTPUT" | jq -e type 2>&1)
JQ_CODE=$?
set -e
if [[ $JQ_CODE -ne 0 || "$TYPE" != '"object"' ]]; then
	echo "FAIL: expected a single JSON object, got type $TYPE (jq exit $JQ_CODE)"
	exit 1
fi

echo "PASS"
