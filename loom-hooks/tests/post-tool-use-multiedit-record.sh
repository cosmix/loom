#!/usr/bin/env bash
# The hook remains silent and must not invoke the obsolete edit recorder.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/work"
MARKER="$TMP/record-edit-called"

cat >"$TMP/bin/loom" <<SH
#!/usr/bin/env bash
if [[ "\$1" == "context" && "\$2" == "record-edit" ]]; then
	printf '%s\n' "\$*" >"$MARKER"
	exit 0
fi
exit 1
SH
chmod +x "$TMP/bin/loom"

INPUT='{"tool_name":"MultiEdit","tool_input":{"file_path":"src/bar.rs","edits":[{"old_string":"a","new_string":"b"}]}}'

set +e
OUTPUT=$(printf '%s' "$INPUT" |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
	PATH="$TMP/bin:/usr/bin:/bin" \
	LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$TMP/work" \
	bash "$HOOK")
CODE=$?
set -e

if [[ $CODE -ne 0 ]]; then
	echo "FAIL: expected exit 0, got $CODE"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected empty stdout, got: $OUTPUT"
	exit 1
fi

if [[ -e "$MARKER" ]]; then
	echo "FAIL: hook invoked the obsolete edit recorder"
	exit 1
fi

echo "PASS"
