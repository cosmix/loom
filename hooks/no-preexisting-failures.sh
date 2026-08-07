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
#   0 - Always. Advisory only.
#
# Output format when warning:
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "LOOM_HOOK_WARN: ..."}}

set -euo pipefail

source "$(dirname "$0")/_common.sh"

debug() {
	[[ "${LOOM_HOOK_DEBUG:-}" == "1" || "${NO_PREEXISTING_FAILURES_DEBUG:-}" == "1" ]] || return 0
	echo "$@" >&2
}

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

debug "=== no-preexisting-failures: tool=$TOOL_NAME ==="

# Each pattern names a way of saying "this red result is not mine to fix".
# Anchored on a failure word so that "pre-existing behaviour" or "pre-existing
# file" - both legitimate and common - do not trip it.
MATCHED=""
FAILWORD='(fail|fails|failed|failing|failure|failures|broken|breakage|red|error|errors)'

check() {
	[[ -n "$MATCHED" ]] && return 0
	if echo "$HAYSTACK" | grep -qiE "$1"; then
		MATCHED="$2"
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
	debug "no excuse phrasing found"
	exit 0
fi

debug "WARN: $MATCHED"

read -r -d '' MSG <<'EOF' || true
LOOM_HOOK_WARN: STOP - you are about to record a failure as somebody else's problem.

CLAUDE.md rule 15: "Nothing is 'pre-existing' - every warning and failure you
see is your responsibility." A red gate is red regardless of which commit
turned it red. `git log main..HEAD` can prove you did not WRITE the bug; it
cannot prove the bug is not real, and it never makes the gate green.

Before using this phrasing, do the work it is standing in for:
  1. Diagnose it to a ROOT CAUSE - an actual mechanism, not a category like
     "environmental", "flaky", or "sandbox". If you cannot name the mechanism,
     you have not finished investigating.
  2. Reproduce it in isolation, minimally. A one-file or one-syscall repro
     usually turns "flaky" into a specific race with a specific fix.
  3. Fix it, or - if it is genuinely outside this stage - say precisely WHAT
     is broken, WHY it is out of scope, and WHO should fix it. Never silence,
     skip, or delete a test to get to green.

Every failure filed under this excuse in this project so far has been genuine.
The most recent was a spurious ENOENT from a racing O_CREAT, dismissed for
weeks as environmental; it was corrupting concurrent log appends in production.

If you are quoting this rule, writing a prevention note, or naming the
anti-pattern in a review, carry on - this hook is advisory and blocks nothing.
EOF

jq -nc --arg ctx "$MSG" \
	'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'

exit 0
