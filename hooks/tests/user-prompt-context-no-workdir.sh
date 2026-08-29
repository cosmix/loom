#!/usr/bin/env bash
# LOOM_WORK_DIR unset means there is no loom session to retrieve against -
# the hook must exit 0 with no output, without ever invoking `loom`.
#
# has_retrievable_context() also falls open when LOOM_WORK_DIR is unset by
# walking upward from $PWD for `.work/`, `doc/loom/knowledge/`, or
# `.loom/cache/context-v1/` - so this test must run from a cwd genuinely
# outside any loom checkout, or it would find this repo's own trees and
# invoke the stub after all. Resolve HOOK to an absolute path FIRST, then cd
# into a fresh subdirectory of the mktemp dir before invoking it - do not
# "helpfully" drop this cd.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/user-prompt-context.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/outside"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
echo "SHOULD_NOT_RUN"
SH
chmod +x "$TMP/bin/loom"

INPUT='{"session_id":"s1","prompt":"a perfectly ordinary implementation question about the codebase"}'

set +e
OUTPUT=$(cd "$TMP/outside" && printf '%s' "$INPUT" |
	env -u LOOM_WORK_DIR -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH -u LOOM_SESSION_ID \
	PATH="$TMP/bin:/usr/bin:/bin" LOOM_STAGE_ID="test-stage" \
	bash "$HOOK")
CODE=$?
set -e

if [[ $CODE -ne 0 ]]; then
	echo "FAIL: expected exit 0, got $CODE"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected no output when LOOM_WORK_DIR is unset, got: $OUTPUT"
	exit 1
fi

echo "PASS"
