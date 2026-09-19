#!/usr/bin/env bash
# no-preexisting-failures.sh - PreToolUse hook that pushes back on the
# "pre-existing failure" excuse.
#
# Per CLAUDE.md rule 15 (QUALITY GATES): "Nothing is 'pre-existing' - every
# warning and failure you see is your responsibility."
#
# A red test that predates your branch is still a red gate. The excuse is
# seductive because it is often TRUE and still wrong: `git log main..HEAD`
# showing no commits on the failing file proves you did not introduce it, and
# proves nothing about whether it is a real bug. Every such failure recorded in
# this project so far turned out to be genuine - most recently a spurious ENOENT
# from a racing O_CREAT that reached production log appends, filed for weeks as
# "environmental".
#
# ADVISORY ONLY - never blocks. The phrase has legitimate uses: writing a
# prevention rule, quoting this rule, or naming the anti-pattern in a review.
# Blocking those would be worse than the excuse. This hook exists to make the
# agent stop and justify, not to forbid a word.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "...", "tool_input": {...}, ...}
#
# Exit codes:
#   0 - Always, unless jq is not installed. Advisory only.
#   1 - jq not installed (non-blocking error)
#
# Output format when warning:
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "LOOM_HOOK_WARN: ..."}}

set -euo pipefail

# Debug tracing comes from _common.sh (`loom_debug`), gated on LOOM_HOOK_DEBUG=1.
source "$(dirname "$0")/_common.sh"
loom_warn_no_jq "no-preexisting-failures.sh"

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)

# Scan every free-text field an agent can put an excuse into: shell commands
# (commit messages, `loom memory note`, `loom stage complete`) and file content
# (handoffs, knowledge files, plan prose).
HAYSTACK=$(echo "$INPUT_JSON" | jq -r '
  [ .tool_input.command?
  , .tool_input.content?
  , .tool_input.new_string?
  , (.tool_input.edits? // [] | .[]?.new_string?)
  ] | map(select(. != null)) | join("\n")
' 2>/dev/null || true)

if [[ -z "$HAYSTACK" ]]; then
	exit 0
fi

loom_debug "=== no-preexisting-failures: tool=$TOOL_NAME ==="

# Each pattern names a way of saying "this red result is not mine to fix".
# Anchored on a failure word so that "pre-existing behaviour" or "pre-existing
# file" - both legitimate and common - do not trip it.
MATCHED=""
FAILWORD='(fail|fails|failed|failing|failure|failures|broken|breakage|red|error|errors)'

# _line_is_exempt <line> - True when a line that otherwise matches an excuse
# pattern is still legitimate use: naming this hook's own file, writing out
# the two spellings as a regex alternation, sitting in a Markdown table row
# or blockquote (documenting the pattern, not invoking it), or explaining the
# rule/hook itself. Mirrors the header's own carve-outs for a prevention
# note, quoting the rule, or naming the anti-pattern in a review.
_line_is_exempt() {
	local line="$1" trimmed
	if [[ "$line" == *"no-preexisting-failures.sh"* ]]; then
		return 0
	fi
	if printf '%s' "$line" | grep -qiE 'pre-?existing[^A-Za-z0-9]{0,4}\|[^A-Za-z0-9]{0,4}pre-?existing'; then
		return 0
	fi
	trimmed="${line#"${line%%[![:space:]]*}"}"
	case "$trimmed" in
	'|'* | '>'*) return 0 ;;
	esac
	if printf '%s' "$line" | grep -qiE 'Rule 15|hook'; then
		return 0
	fi
	return 1
}

# check <excuse-pattern> <label> - Find every HAYSTACK line matching
# <excuse-pattern>. Fire <label> only when at least one matching line is NOT
# exempt (_line_is_exempt) - a line quoting this rule, this hook's filename,
# a regex alternation, or a table/blockquote row never fires alone.
check() {
	if [[ -n "$MATCHED" ]]; then
		return 0
	fi
	local pattern="$1" label="$2" line all_exempt=1
	while IFS= read -r line; do
		[[ -n "$line" ]] || continue
		if ! _line_is_exempt "$line"; then
			all_exempt=0
			break
		fi
	done < <(printf '%s\n' "$HAYSTACK" | grep -iE "$pattern" || true)
	if ((all_exempt == 0)); then
		MATCHED="$label"
	fi
}

check "pre-?existing[[:space:]:_-]+([a-z]+[[:space:]]+){0,2}${FAILWORD}" "calling a failure pre-existing"
check "${FAILWORD}[[:space:]]+(that[[:space:]]+)?(are|is|were|was)[[:space:]]+pre-?existing" "calling a failure pre-existing"
check "(already|previously)[[:space:]]+(broken|failing|red)([[:space:]]+on[[:space:]]+main)?" "waving through work that was already red"
check "${FAILWORD}[[:space:]]+((on|in)[[:space:]]+main|before[[:space:]]+(my|this)[[:space:]]+(change|branch|stage))" "attributing a failure to main"
check "(unrelated|not[[:space:]]+related)[[:space:]]+to[[:space:]]+(my|this)[[:space:]]+(change|work|stage|branch)" "declaring a failure out of scope"
check "not[[:space:]]+(caused[[:space:]]+by|introduced[[:space:]]+by|my)[[:space:]]+(this[[:space:]]+)?(change|fault|doing)" "disclaiming authorship of a failure"
check "(known|expected|acceptable|benign|harmless)[[:space:]]+${FAILWORD}" "normalising a failure"
check "(environmental|flaky|transient|intermittent)[[:space:]]+${FAILWORD}" "attributing a failure to the environment"

if [[ -z "$MATCHED" ]]; then
	loom_debug "no excuse phrasing found"
	exit 0
fi

loom_debug "WARN: $MATCHED"

read -r -d '' MSG <<'EOF' || true
LOOM_HOOK_WARN: CLAUDE.md rule 15: "Nothing is 'pre-existing' - every failure you see is yours."
1. Diagnose it to a ROOT CAUSE, not a category like "environmental" or "flaky".
2. Reproduce it minimally, in isolation.
3. Fix it, or say precisely what is broken, why it is out of scope, and who owns it.
Quoting this rule or naming the anti-pattern in review is fine - this hook blocks nothing.
EOF

jq -nc --arg ctx "$MSG" \
	'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'

exit 0
