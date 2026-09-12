#!/usr/bin/env bash
# A blocking guard (plans-path-guard.sh) must fail closed - exit 2, not
# exit 0 - when jq is not on PATH, since it can no longer parse its payload.
set -euo pipefail
HOOK="$(dirname "$0")/../plans-path-guard.sh"
source "$(dirname "$0")/_path_without.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
NOJQ_PATH=$(path_without jq)
trap 'rm -rf "$TMP" "$NOJQ_PATH"' EXIT

INPUT='{"tool_name":"Write","tool_input":{"file_path":"doc/plans/PLAN-x.md"}}'

# Control: with the normal PATH this payload is allowed - proves the helper
# below, not some unrelated cause, is what changes the outcome.
set +e
CONTROL_OUTPUT=$(printf '%s' "$INPUT" | bash "$HOOK" 2>"$TMP/control.stderr")
CONTROL_CODE=$?
set -e

if [[ $CONTROL_CODE -ne 0 ]]; then
	echo "FAIL: control case (normal PATH) expected exit 0, got $CONTROL_CODE"
	exit 1
fi
if [[ -n "$CONTROL_OUTPUT" ]]; then
	echo "FAIL: control case expected empty stdout, got: $CONTROL_OUTPUT"
	exit 1
fi

set +e
OUTPUT=$(printf '%s' "$INPUT" | PATH="$NOJQ_PATH" bash "$HOOK" 2>"$TMP/nojq.stderr")
CODE=$?
set -e
STDERR=$(cat "$TMP/nojq.stderr")

if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 when jq is missing, got $CODE"
	exit 1
fi
if [[ "$STDERR" != *"jq is not installed"* ]]; then
	echo "FAIL: expected stderr to mention 'jq is not installed', got: $STDERR"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected empty stdout, got: $OUTPUT"
	exit 1
fi

echo "PASS"
