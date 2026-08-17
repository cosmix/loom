#!/usr/bin/env bash
# MultiEdit modifies files exactly like Edit/Write and carries its target at
# the same `.tool_input.file_path` position (confirmed against Claude Code's
# published PostToolUse examples, which match "Write|Edit|MultiEdit" and read
# `.tool_input.file_path` for all three). It must be recorded the same way.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d)
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

if [[ ! -f "$MARKER" ]]; then
	echo "FAIL: loom context record-edit was never invoked for a MultiEdit tool call"
	exit 1
fi
if ! grep -qF -- "--path src/bar.rs" "$MARKER"; then
	echo "FAIL: record-edit was not called with the file_path from a MultiEdit payload. Got: $(cat "$MARKER")"
	exit 1
fi

echo "PASS"
