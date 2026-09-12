#!/usr/bin/env bash
# A symlinked heartbeat file must still exit 0 and must never be written
# through - the guard at post-tool-use.sh's heartbeat block exists precisely
# to refuse that. But that refusal must skip ONLY the heartbeat write: it is
# not a whole-script `exit 0`, so a Write/Edit tool call under a symlinked
# heartbeat file must still get its path recorded via
# `loom context record-edit`. This is a regression test for the defect where
# the guard exited the entire script, silently disabling edit recording
# whenever the heartbeat happened to be a symlink.
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

if [[ ! -f "$MARKER" ]]; then
	echo "FAIL: a symlinked heartbeat file must not disable edit recording"
	exit 1
fi
if ! grep -qF -- "--path src/foo.rs" "$MARKER"; then
	echo "FAIL: record-edit was not called with the worktree-relative path. Got: $(cat "$MARKER")"
	exit 1
fi

echo "PASS"
