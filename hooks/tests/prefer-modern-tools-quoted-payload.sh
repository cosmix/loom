#!/usr/bin/env bash
# prefer-modern-tools-quoted-payload.sh - grep/find inside a quoted payload
# (a codex-forward task prompt, a JS `.find(` call) is not a command
# invocation and must not warn. Covers the false positives reported against
# prefer-modern-tools.sh's old raw-string regex matching, plus a regression
# check that real invocations (including through a wrapper or absolute path)
# still warn, and that 'rg'/'fd' never do.
set -euo pipefail
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"

FAILED=0

# assert_no_warning <label> <command>
assert_no_warning() {
	local label="$1" cmd="$2"
	local input output
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	output=$(echo "$input" | bash "$HOOK")
	if [[ -n "$output" ]]; then
		echo "FAIL: $label: expected no warning, got: $output"
		FAILED=1
	fi
}

# assert_warning <label> <command> <needle>
assert_warning() {
	local label="$1" cmd="$2" needle="$3"
	local input output
	input=$(jq -nc --arg c "$cmd" '{"tool_name":"Bash","tool_input":{"command":$c}}')
	output=$(echo "$input" | bash "$HOOK")
	if ! echo "$output" | grep -q "hookSpecificOutput" || ! echo "$output" | grep -q "$needle"; then
		echo "FAIL: $label: expected warning containing '$needle', got: $output"
		FAILED=1
	fi
}

# --- Reported false positives: no warning -----------------------------------

# The exact reproduction from the report: a codex-forward brief telling the
# agent to use rg and that grep is banned - prose, never an actual grep
# invocation.
assert_no_warning "codex-forward brief discussing grep as prose" \
	"fwd task 'Use rg to find the spec, then grep -n is banned' --write"

# A full codex-forward invocation whose quoted task prompt contains a
# JavaScript `.find(` call and prose about git/grep - none of it a real
# command or argument position. Mirrors common-token-helpers.sh's
# CODEX_FORWARD_CMD fixture.
CODEX_FORWARD_CMD=$'~/.claude/hooks/loom/codex-forward.sh task \'Fix the bug.\nDo NOT run grep at all (no grep, no find).\nconst spec = INDICATOR_PROPERTIES.find((candidate) => candidate.key === key);\n\' --model gpt-5.6-terra --effort xhigh --write'
assert_no_warning "codex-forward prompt with JS .find( call" "$CODEX_FORWARD_CMD"

# A quoted grep followed by a trailing space - the existing
# prefer-modern-tools-quoted.sh test uses `echo "use grep"`, which passes
# only because "grep" sits right before the closing quote (no trailing
# whitespace for the old regex to match). This case has a trailing space
# inside the quotes and must still not warn.
assert_no_warning "quoted grep with trailing space inside the string" \
	'echo "grep -n foo bar"'

# --- Must not regress: real invocations still warn ---------------------------

assert_warning "grep -rn is a real invocation" \
	'grep -rn "pat" src/' "grep"

assert_warning "find . -name is a real invocation" \
	'find . -name "*.txt"' "find"

assert_warning "xargs grep unwraps the xargs wrapper" \
	'xargs grep foo' "grep"

assert_warning "/usr/bin/grep matches by basename" \
	'/usr/bin/grep -n x' "grep"

assert_no_warning "rg is never flagged as grep" \
	'rg -n "pat" src/'

# --- FALLBACK PATH: unterminated quote -> loom_tokenize_command returns 1,
# so uses_grep/uses_find must fall through to the pre-tokenizing regex scan.
# Nothing else in this file exercises that branch, so it would rot unnoticed.
assert_warning "unterminated quote falls back to the regex scan and still warns on grep" \
	'grep -rn "pat src/' "grep"

if [[ $FAILED -ne 0 ]]; then
	exit 1
fi

echo "PASS"
