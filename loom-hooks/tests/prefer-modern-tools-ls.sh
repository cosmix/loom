#!/usr/bin/env bash
# `ls <path>` points at fd (CLAUDE.md rule 8); plain `ls` with only flags and
# no path operand is unaffected.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

run_hook() {
	local cmd="$1" input tmp out
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/pmt-ls.XXXXXX")
	out=$(printf '%s' "$input" | TMPDIR="$tmp" bash "$HOOK")
	rm -rf "$tmp"
	printf '%s' "$out"
}

assert_no_warning() {
	local label="$1" cmd="$2" output
	output=$(run_hook "$cmd")
	if [[ -n "$output" ]]; then
		echo "FAIL: $label: expected no warning, got: $output"
		FAILED=1
	fi
}

assert_warning() {
	local label="$1" cmd="$2" needle="$3" output
	output=$(run_hook "$cmd")
	if ! echo "$output" | grep -q "LOOM_HOOK_WARN" || ! echo "$output" | grep -q "$needle"; then
		echo "FAIL: $label: expected a warning containing '$needle', got: $output"
		FAILED=1
	fi
}

assert_warning "ls with a path operand warns toward fd" \
	"ls src/" "fd"

assert_no_warning "ls with only flags and no path does not warn" \
	"ls -la"

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
