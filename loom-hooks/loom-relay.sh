#!/usr/bin/env bash
# loom-relay.sh - PostToolUse:Bash relay from a sandboxed loom command to the
# daemon inbox (doc/plans/PLAN-loom-state-confinement.md section 7).
#
# A confined session cannot write .loom/work. A loom command that has to record
# something leaves a ticket in $LOOM_SCRATCH_DIR and prints one
#   LOOM_RELAY_V1 kind=<kind> id=<id> sha256=<hex> bytes=<n>
# line as the last line of its stdout. This hook runs outside the sandbox. It
# works out which request kinds the Bash command could have produced, then
# hands the payload to `loom hook relay`, which proves the session, verifies
# each ticket against its line and writes the inbox entry.
#
# Registered in every session capsule (PostToolUse, matcher Bash); never in the
# global hook table and never for codex.
#
# Input:  the PostToolUse JSON payload on stdin.
# Output: at most one hookSpecificOutput.additionalContext object.
# Exit:   always 0. Once a relay line is in the output, every path says what
#         happened to it.

# First statement, before any external command runs: resolve commands through
# the PATH loom pinned for hooks, never through inherited PATH directories a
# sandboxed session can write.
PATH="${LOOM_HOOK_PATH:-$PATH}"

# emit <message> - Print the one additionalContext object and exit 0.
emit() {
	jq -nc --arg message "$1" \
		'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
	exit 0
}

# say <message> - emit <message> when HAS_LINE is set, otherwise exit 0
# silently: a large output that was merely persisted to a file is too common
# to comment on, and neither is a payload that reached this point only
# because its output mentions " memory " - it has no relay line to comment on
# either.
say() {
	[[ $HAS_LINE -eq 1 ]] && emit "$1"
	exit 0
}

# drop_control_kinds <csv> - The kinds in <csv> a subagent may relay. A
# `commit` line is admitted like the other control kinds and never swept.
drop_control_kinds() {
	local kind kept="" IFS=,
	for kind in $1; do
		relay_kind_is_control "$kind" || kept=${kept:+$kept,}$kind
	done
	printf '%s' "$kept"
}

[[ -n "${LOOM_SESSION_ID:-}" && -n "${LOOM_SCRATCH_DIR:-}" ]] || exit 0

source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
INPUT_JSON=$(loom_read_input 1 2>/dev/null)

# Fast path, pure bash: almost every Bash call ends here. A `loom memory`
# write command must reach the helper even when its relay line never lands in
# this payload (redirected stdout, piped through `tail`, run from a script
# file, run in the background, or past the helper's per-call line cap): once
# --allowed-kinds names memory or telemetry, the helper also relays leftover
# on-disk tickets of those kinds - but only if it gets invoked at all. So a
# payload merely mentioning " memory " in its command or output proceeds too.
case "$INPUT_JSON" in
*"LOOM_RELAY_V1 "* | *persistedOutputPath* | *"Full output saved to: "* | *" memory "*) ;;
*) exit 0 ;;
esac
HAS_LINE=0
[[ "$INPUT_JSON" == *"LOOM_RELAY_V1 "* ]] && HAS_LINE=1

# _relay_kinds.sh is sourced here, after the fast path above: sourcing it at the
# top would add a file read to every Bash call.
if ! source "${BASH_SOURCE[0]%/*}/_common.sh" 2>/dev/null ||
	! source "${BASH_SOURCE[0]%/*}/_relay_kinds.sh" 2>/dev/null || ! command -v jq &>/dev/null; then
	# Without jq there is no safe way to build JSON: a fixed message, verbatim.
	[[ $HAS_LINE -eq 1 ]] &&
		printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"LOOM relay: unavailable because jq or the loom hook library (_common.sh, _relay_kinds.sh) is missing, so nothing was relayed. Stop and report it; do not retry."}}'
	exit 0
fi

# One jq call. The single-line fields come first and the command, which may
# span lines, last; the trailing "." keeps empty trailing fields from being
# eaten by command substitution.
FIELDS=$(printf '%s' "$INPUT_JSON" | jq -r '
	def one_line: (. // "") | tostring | gsub("[\r\n]"; " ");
	(.tool_name | one_line), (.agent_type | one_line), (.transcript_path | one_line),
	(.tool_input.command // "" | tostring), "."' 2>/dev/null) ||
	say "LOOM relay: the hook payload did not parse, so nothing was relayed. Stop and report it; do not retry."
FIELDS=${FIELDS%$'\n.'}
TOOL_NAME=${FIELDS%%$'\n'*}
FIELDS=${FIELDS#*$'\n'}
AGENT_TYPE=${FIELDS%%$'\n'*}
FIELDS=${FIELDS#*$'\n'}
TRANSCRIPT_PATH=${FIELDS%%$'\n'*}
COMMAND=${FIELDS#*$'\n'}
[[ "$TOOL_NAME" == Bash ]] || exit 0

loom_codex_forwarder_verdict "$AGENT_TYPE" "$TRANSCRIPT_PATH"
case $? in
0) say "LOOM relay: output from a codex forwarder is never relayed, so nothing in it was recorded. Run the loom command from the main agent or a Claude subagent." ;;
2) say "LOOM relay: this subagent's transcript could not be read to rule out a codex forwarder, so nothing was relayed. Run the loom command from the main agent." ;;
esac

if ! loom_tokenize_command "$(strip_embedded_content "$COMMAND")"; then
	say "LOOM relay: this Bash command could not be parsed, so nothing in its output was relayed. Rerun the loom command alone, as its own Bash call, with its stdout unfiltered."
fi
ALLOWED=$(relay_allowed_kinds)
if [[ -n "$AGENT_TYPE" ]]; then
	ALLOWED=$(drop_control_kinds "$ALLOWED")
fi
# A persisted output of a command that ran no relaying loom command could only
# hold echoed lines, which are never relayed: not worth reading the file.
[[ -z "$ALLOWED" && $HAS_LINE -eq 0 ]] && exit 0

LOOM_CLI=${LOOM_BIN:-loom}
command -v "$LOOM_CLI" &>/dev/null ||
	say "LOOM relay: the loom binary ($LOOM_CLI) was not found, so nothing was relayed. Stop and report it; do not retry."

if command -v gtimeout &>/dev/null; then
	BOUND=(gtimeout 10)
elif command -v timeout &>/dev/null; then
	BOUND=(timeout 10)
else
	BOUND=()
fi
REPLY=$(printf '%s' "$INPUT_JSON" |
	LOOM_HOOK_CONTEXT=1 ${BOUND[@]+"${BOUND[@]}"} "$LOOM_CLI" hook relay --allowed-kinds "$ALLOWED" 2>/dev/null)
STATUS=$?
if [[ $STATUS -ne 0 ]]; then
	say "LOOM relay: loom hook relay failed (exit $STATUS), so a request in this output may not have been relayed. Stop and report it; do not retry."
fi
[[ -n "$REPLY" ]] && printf '%s\n' "$REPLY"
exit 0
