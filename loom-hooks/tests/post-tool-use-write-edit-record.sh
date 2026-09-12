#!/usr/bin/env bash
# A synthetic PostToolUse Write payload with a worktree-relative file_path
# must exit 0 with EMPTY stdout, and the shell hook must never write shared
# state itself - only the heartbeat file may exist under LOOM_WORK_DIR
# afterward. Edit recording is delegated to `loom context record-edit`
# (stubbed here); the stub writes a marker OUTSIDE LOOM_WORK_DIR purely to
# prove it was invoked with the right arguments, since the whole point of
# this design is that the Rust binary owns the shared write, not this script.
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

INPUT='{"tool_name":"Write","tool_input":{"file_path":"src/foo.rs"}}'

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

if [[ ! -f "$MARKER" ]]; then
	echo "FAIL: loom context record-edit was never invoked for a Write tool call"
	exit 1
fi
if ! grep -qF -- "--path src/foo.rs" "$MARKER"; then
	echo "FAIL: record-edit was not called with the worktree-relative path. Got: $(cat "$MARKER")"
	exit 1
fi

FILES=$(find "$TMP/work" -type f)
EXPECTED="$TMP/work/heartbeat/test-stage.json"
if [[ "$FILES" != "$EXPECTED" ]]; then
	echo "FAIL: expected only the heartbeat file under LOOM_WORK_DIR, got: $FILES"
	exit 1
fi

echo "PASS"
