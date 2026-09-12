#!/usr/bin/env bash
# NotebookEdit does NOT carry its target at `.tool_input.file_path` - its
# field is `.notebook_path`, confirmed against `worktree-file-guard.sh`'s
# `extract_path()`, which already special-cases the same tool for the same
# reason. A payload with `notebook_path` but no `file_path` must still be
# recorded, and recorded at the notebook path, not silently dropped or
# recorded as an empty/wrong path.
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

INPUT='{"tool_name":"NotebookEdit","tool_input":{"notebook_path":"notebooks/analysis.ipynb","cell_id":"1","new_source":"print(1)"}}'

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
	echo "FAIL: loom context record-edit was never invoked for a NotebookEdit tool call"
	exit 1
fi
if ! grep -qF -- "--path notebooks/analysis.ipynb" "$MARKER"; then
	echo "FAIL: record-edit was not called with the notebook_path from a NotebookEdit payload. Got: $(cat "$MARKER")"
	exit 1
fi

echo "PASS"
