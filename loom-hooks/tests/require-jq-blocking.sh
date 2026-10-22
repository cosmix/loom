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

# subagent-verify-guard.sh blocks too, and it is the hook that can stop a live
# standard stage's main agent: without jq it must fail closed the same way.
GUARD="$(dirname "$0")/../subagent-verify-guard.sh"
GUARD_INPUT='{"tool_name":"Bash","tool_input":{"command":"cargo test"}}'

# Control: with jq on PATH and no live loom session this payload is allowed.
set +e
printf '%s' "$GUARD_INPUT" | env -u LOOM_MAIN_AGENT_PID bash "$GUARD" >/dev/null 2>"$TMP/guard-control.stderr"
GUARD_CONTROL_CODE=$?
set -e
if [[ $GUARD_CONTROL_CODE -ne 0 ]]; then
	echo "FAIL: subagent-verify-guard control case expected exit 0, got $GUARD_CONTROL_CODE"
	exit 1
fi

set +e
GUARD_OUTPUT=$(printf '%s' "$GUARD_INPUT" | PATH="$NOJQ_PATH" bash "$GUARD" 2>"$TMP/guard-nojq.stderr")
GUARD_CODE=$?
set -e
GUARD_STDERR=$(cat "$TMP/guard-nojq.stderr")

if [[ $GUARD_CODE -ne 2 ]]; then
	echo "FAIL: subagent-verify-guard expected exit 2 when jq is missing, got $GUARD_CODE"
	exit 1
fi
if [[ "$GUARD_STDERR" != *"jq is not installed"* ]]; then
	echo "FAIL: subagent-verify-guard expected stderr to mention 'jq is not installed', got: $GUARD_STDERR"
	exit 1
fi
if [[ -n "$GUARD_OUTPUT" ]]; then
	echo "FAIL: subagent-verify-guard expected empty stdout, got: $GUARD_OUTPUT"
	exit 1
fi

echo "PASS"
