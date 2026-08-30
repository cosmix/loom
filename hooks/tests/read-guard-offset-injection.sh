#!/usr/bin/env bash
# read-guard-offset-injection.sh - regression test for the read-guard.sh
# arithmetic-injection defect: `tool_input.offset`/`.limit` used to be
# interpolated straight into a `$(( ))` arithmetic context with no
# validation, so a crafted offset such as "KIND[$(touch ...)]" ran arbitrary
# shell commands on every Read tool call - and a merely malformed offset
# (e.g. "1 2") printed a bash "syntax error in expression" to stderr. Both
# are now guarded: offset/limit must match ^[0-9]+$ before use, and a value
# that fails validation is treated as absent rather than interpolated.
#
# A well-formed numeric offset/limit must still resolve to the exact same
# LINES value as before, so this also pins that behavior down.
#
# Also covers `pages`: it is not an arithmetic vector, but it flows straight
# into LINES and from there into the TSV read ledger, so a value carrying a
# tab or newline would write a malformed ledger row. A malformed pages value
# must be treated as absent, and a well-formed one ("N" or "N-M") unchanged.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../read-guard.sh"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

TARGET_FILE="$TMP/target.txt"
printf 'line one\nline two\nline three\n' >"$TARGET_FILE"
MARKER="$TMP/pwned"

# run_hook <json-input> [extra env assignments for `env`...] - invoke
# read-guard.sh with a scrubbed environment (no live loom session, no
# inherited debug flags) plus a private TMPDIR so the read ledger this run
# writes never touches the real one.
run_hook() {
	local input="$1"
	shift
	printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORK_DIR -u LOOM_STAGE_ID \
			-u LOOM_HOOK_DEBUG -u COMMIT_FILTER_DEBUG \
			TMPDIR="$TMP" "$@" bash "$HOOK"
}

# --- (a) the injection payload itself ---------------------------------------
rm -f "$MARKER"
OFFSET_PAYLOAD="KIND[\$(touch ${MARKER})]"
INJECT_INPUT=$(jq -nc --arg fp "$TARGET_FILE" --arg off "$OFFSET_PAYLOAD" \
	'{tool_name:"Read",tool_input:{file_path:$fp, offset:$off, limit:10}}')

set +e
STDERR_A=$(run_hook "$INJECT_INPUT" 2>&1 1>/dev/null)
CODE_A=$?
set -e

if [[ $CODE_A -ne 0 ]]; then
	echo "FAIL: (a) expected exit 0 for the injection payload, got $CODE_A"
	echo "stderr: $STDERR_A"
	exit 1
fi
if [[ -e "$MARKER" ]]; then
	echo "FAIL: (a) offset injection executed - $MARKER was created"
	exit 1
fi
if echo "$STDERR_A" | grep -q "syntax error in expression"; then
	echo "FAIL: (a) arithmetic syntax error leaked for the injection payload"
	echo "stderr: $STDERR_A"
	exit 1
fi

# --- (b) a merely malformed (non-numeric, non-malicious) offset -------------
INPUT_B=$(jq -nc --arg fp "$TARGET_FILE" '{tool_name:"Read",tool_input:{file_path:$fp, offset:"1 2", limit:10}}')

set +e
STDERR_B=$(run_hook "$INPUT_B" 2>&1 1>/dev/null)
CODE_B=$?
set -e

if [[ $CODE_B -ne 0 ]]; then
	echo "FAIL: (b) expected exit 0 for offset '1 2', got $CODE_B"
	echo "stderr: $STDERR_B"
	exit 1
fi
if echo "$STDERR_B" | grep -q "syntax error in expression"; then
	echo "FAIL: (b) offset '1 2' produced an arithmetic syntax error"
	echo "stderr: $STDERR_B"
	exit 1
fi

# --- (c) a well-formed offset/limit must behave exactly as before -----------
INPUT_C=$(jq -nc --arg fp "$TARGET_FILE" '{tool_name:"Read",tool_input:{file_path:$fp, offset:10, limit:20}}')

set +e
STDERR_C=$(run_hook "$INPUT_C" LOOM_HOOK_DEBUG=1 2>&1 1>/dev/null)
CODE_C=$?
set -e

if [[ $CODE_C -ne 0 ]]; then
	echo "FAIL: (c) expected exit 0 for well-formed offset/limit, got $CODE_C"
	echo "stderr: $STDERR_C"
	exit 1
fi
if ! echo "$STDERR_C" | grep -qF "LINES=10-30"; then
	echo "FAIL: (c) offset=10 limit=20 did not resolve to LINES=10-30"
	echo "stderr: $STDERR_C"
	exit 1
fi

# --- (d) a malformed `pages` value (embedded tab) must be treated as absent -
PAGES_PAYLOAD=$'1\t5'
INPUT_D=$(jq -nc --arg fp "$TARGET_FILE" --arg pg "$PAGES_PAYLOAD" '{tool_name:"Read",tool_input:{file_path:$fp, pages:$pg}}')

set +e
STDERR_D=$(run_hook "$INPUT_D" LOOM_HOOK_DEBUG=1 2>&1 1>/dev/null)
CODE_D=$?
set -e

if [[ $CODE_D -ne 0 ]]; then
	echo "FAIL: (d) expected exit 0 for a malformed pages value, got $CODE_D"
	echo "stderr: $STDERR_D"
	exit 1
fi
if ! echo "$STDERR_D" | grep -qF "KIND=full"; then
	echo "FAIL: (d) a malformed pages value was not treated as absent (expected KIND=full)"
	echo "stderr: $STDERR_D"
	exit 1
fi

# --- (e) a well-formed `pages` range must resolve to LINES=<pages> ----------
INPUT_E=$(jq -nc --arg fp "$TARGET_FILE" '{tool_name:"Read",tool_input:{file_path:$fp, pages:"10-20"}}')

set +e
STDERR_E=$(run_hook "$INPUT_E" LOOM_HOOK_DEBUG=1 2>&1 1>/dev/null)
CODE_E=$?
set -e

if [[ $CODE_E -ne 0 ]]; then
	echo "FAIL: (e) expected exit 0 for a well-formed pages range, got $CODE_E"
	echo "stderr: $STDERR_E"
	exit 1
fi
if ! echo "$STDERR_E" | grep -qF "LINES=10-20"; then
	echo "FAIL: (e) pages='10-20' did not resolve to LINES=10-20"
	echo "stderr: $STDERR_E"
	exit 1
fi

echo "PASS"
