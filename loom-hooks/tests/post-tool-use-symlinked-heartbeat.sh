#!/usr/bin/env bash
# A symlinked heartbeat must not be followed, and no edit recording occurs.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/work/heartbeat"
MARKER="$TMP/record-edit-called"
EVIL_TARGET="$TMP/evil-target"

printf 'do not touch me\n' >"$EVIL_TARGET"
ORIGINAL_CONTENT=$(cat "$EVIL_TARGET")
ln -s "$EVIL_TARGET" "$TMP/work/heartbeat/test-stage.json"

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
	echo "FAIL: expected exit 0 with a symlinked heartbeat file, got $CODE"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected empty stdout, got: $OUTPUT"
	exit 1
fi

if [[ ! -L "$TMP/work/heartbeat/test-stage.json" ]]; then
	echo "FAIL: the heartbeat path must remain a symlink, never be replaced"
	exit 1
fi

NEW_CONTENT=$(cat "$EVIL_TARGET")
if [[ "$NEW_CONTENT" != "$ORIGINAL_CONTENT" ]]; then
	echo "FAIL: the symlink target was written through. expected: $ORIGINAL_CONTENT, got: $NEW_CONTENT"
	exit 1
fi

if [[ -e "$MARKER" ]]; then
	echo "FAIL: hook invoked the obsolete edit recorder"
	exit 1
fi

echo "PASS"
