#!/usr/bin/env bash
# Malformed JSON on stdin must fail open: exit 0, no output. The fake `loom`
# binary fails (non-zero exit, no stdout) on invalid input, mirroring the
# real delegate's contract - this proves the wrapper's `|| true` swallows a
# failing delegate rather than propagating it.
set -euo pipefail
HOOK="$(dirname "$0")/../user-prompt-context.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/work"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
[[ "$1" == "hook" && "$2" == "user-prompt" ]] || exit 1
INPUT=$(cat)
jq -e . >/dev/null 2>&1 <<<"$INPUT" || exit 1
echo "SHOULD_NOT_PRINT"
SH
chmod +x "$TMP/bin/loom"

INPUT='{not valid json'

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
	echo "FAIL: expected no output for malformed JSON, got: $OUTPUT"
	exit 1
fi

echo "PASS"
