#!/usr/bin/env bash
# grep filtering another command's piped stdout, with no file operand of its
# own, is not the anti-pattern Rule 8 targets - Rule 14 tells agents to pipe
# verbose output through exactly this kind of filter. A real grep still warns
# when it reads a file directly, even if piped onward, or when it is chained
# with `||` rather than a data pipe.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

# run_hook <command> - fresh TMPDIR per call so the once-per-session "tools"
# ledger never carries state between these otherwise-independent assertions.
run_hook() {
	local cmd="$1" input tmp out
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/pmt-pipe.XXXXXX")
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
	local label="$1" cmd="$2" output
	output=$(run_hook "$cmd")
	if ! echo "$output" | grep -q "LOOM_HOOK_WARN"; then
		echo "FAIL: $label: expected a warning, got: $output"
		FAILED=1
	fi
}

# Silent: grep with no file operand, following a single pipe - a pure filter.
assert_no_warning "grep filtering piped stdout with no file operand" \
	'cargo test 2>&1 | grep FAIL'

# Still warns: grep given a file operand, even though it is also piped.
assert_warning "grep with a file operand still warns, even when piped" \
	'echo x | grep pattern file.txt'

# Still warns: a plain file-reading grep, no pipe at all.
assert_warning "grep -rn pattern src/ still warns" \
	'grep -rn pattern src/'

# Still warns: `||` is not a data pipe, so the exemption must not apply.
assert_warning "cmd || grep pattern does not count as a pipeline filter" \
	'false || grep pattern'

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
