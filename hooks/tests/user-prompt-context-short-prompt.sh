#!/usr/bin/env bash
# A short acknowledgement prompt earns no retrieval - the hook must print
# nothing and exit 0. The fake `loom` binary below stands in for the Rust
# delegate and mirrors its documented fail-open contract (prompt shorter
# than 24 characters after trim -> no output) so this test exercises the
# real wrapper end to end without requiring a built loom binary.
set -euo pipefail
HOOK="$(dirname "$0")/../user-prompt-context.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/work"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
[[ "$1" == "hook" && "$2" == "user-prompt" ]] || exit 1
PROMPT=$(cat | jq -r '.prompt // empty')
[[ ${#PROMPT} -ge 24 ]] || exit 0
jq -nc '{hookSpecificOutput:{hookEventName:"UserPromptSubmit",additionalContext:"brief"}}'
SH
chmod +x "$TMP/bin/loom"

INPUT='{"session_id":"s1","prompt":"ok thanks"}'

set +e
OUTPUT=$(printf '%s' "$INPUT" |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH -u LOOM_SESSION_ID \
	PATH="$TMP/bin:/usr/bin:/bin" LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="test-stage" \
	bash "$HOOK")
CODE=$?
set -e

if [[ $CODE -ne 0 ]]; then
	echo "FAIL: expected exit 0, got $CODE"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected no output for a short acknowledgement prompt, got: $OUTPUT"
	exit 1
fi

echo "PASS"
