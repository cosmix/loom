#!/usr/bin/env bash
# `head`/`tail` given a real file operand point at the Read tool (CLAUDE.md
# rule 8); used as a pipe filter over another command's stdout (Rule 14's own
# recommended pattern), with no file operand, they must stay silent.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

run_hook() {
	local cmd="$1" input tmp out
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/pmt-headtail.XXXXXX")
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

assert_warning "head with a file argument warns toward the Read tool" \
	"head -n 20 output.log" "Read tool"

assert_warning "tail with a file argument warns toward the Read tool" \
	"tail -f service.log" "Read tool"

assert_no_warning "tail as a pipe filter with no file operand does not warn" \
	"cargo test | tail -50"

assert_no_warning "head as a pipe filter with no file operand does not warn" \
	"cargo build 2>&1 | head -30"

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
