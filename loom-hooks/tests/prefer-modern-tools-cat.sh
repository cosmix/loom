#!/usr/bin/env bash
# `cat <file>` with exactly one file operand and no redirection points at the
# Read tool (CLAUDE.md rule 8). Two files, or any redirection, is a different
# shape this rule does not cover.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

run_hook() {
	local cmd="$1" input tmp out
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/pmt-cat.XXXXXX")
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

assert_warning "single file operand warns toward the Read tool" \
	"cat notes.txt" "Read tool"

assert_no_warning "two file operands is a different shape, no warning" \
	"cat a.txt b.txt"

assert_no_warning "redirection is a different shape, no warning" \
	"cat notes.txt > copy.txt"

assert_warning "a later command's redirection does not suppress cat's own warning" \
	"cat notes.txt && make > out.log" "Read tool"

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
