#!/usr/bin/env bash
# Each tool family warns at most once per session (ledger kind "tools"): a
# second grep invocation in the SAME session is silent, but a different
# family (find) still gets its own first warning.
set -euo pipefail
# See the isolation comment in prefer-modern-tools-grep.sh: LOOM_WORK_DIR,
# LOOM_SESSION_ID, and LOOM_STAGE_ID together route the "tools" ledger to the
# live session directory when this runs inside a loom stage, which would
# make the SECOND assertion below pass for the wrong reason (an already-warm
# ledger from an earlier test) instead of exercising the once-per-session
# behavior against this file's own TMP.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/pmt-warnonce.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# run_hook <command> - reuses the SAME TMPDIR across every call, so the
# session-keyed "tools" ledger persists between them (unlike every other
# prefer-modern-tools-*.sh test file, which isolates per call on purpose).
run_hook() {
	local cmd="$1" input
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	printf '%s' "$input" | TMPDIR="$TMP" bash "$HOOK"
}

FIRST=$(run_hook 'grep -rn pattern src/')
if [[ "$FIRST" != *"LOOM_HOOK_WARN"* ]]; then
	echo "FAIL: first grep invocation should warn, got: $FIRST"
	exit 1
fi

SECOND=$(run_hook 'grep -rn other src/')
if [[ -n "$SECOND" ]]; then
	echo "FAIL: a second grep invocation in the same session should be silent, got: $SECOND"
	exit 1
fi

THIRD=$(run_hook 'find . -name "*.txt"')
if [[ "$THIRD" != *"LOOM_HOOK_WARN"* ]]; then
	echo "FAIL: a different family (find) should still get its own first warning, got: $THIRD"
	exit 1
fi

FOURTH=$(run_hook 'find . -name "*.rs"')
if [[ -n "$FOURTH" ]]; then
	echo "FAIL: a second find invocation in the same session should be silent, got: $FOURTH"
	exit 1
fi

echo "PASS"
