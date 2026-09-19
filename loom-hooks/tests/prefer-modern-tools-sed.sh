#!/usr/bin/env bash
# `sed -i` (in-place edit) points at the Edit tool (CLAUDE.md rule 8); a
# plain `sed` that only prints to stdout is unaffected.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

run_hook() {
	local cmd="$1" input tmp out
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/pmt-sed.XXXXXX")
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

assert_warning "sed -i warns toward the Edit tool" \
	"sed -i 's/a/b/' file.txt" "Edit tool"

assert_warning "sed --in-place also warns" \
	"sed --in-place 's/a/b/' file.txt" "Edit tool"

assert_no_warning "plain sed without -i does not warn" \
	"sed 's/a/b/' file.txt"

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
