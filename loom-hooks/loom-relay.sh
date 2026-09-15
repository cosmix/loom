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

# say <message> - emit <message> when the inline output carries a relay line,
# otherwise exit 0 silently: a large output that was merely persisted to a file
# is too common to comment on.
say() {
	[[ $HAS_LINE -eq 1 ]] && emit "$1"
	exit 0
}

# segment_has_arg <index> <arg> - True when a later word of the command segment
# whose command word sits at LOOM_TOKENS[<index>] is exactly <arg>.
segment_has_arg() {
	local k=$(($1 + 1)) n=${#LOOM_TOKENS[@]}
	while ((k < n)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]]; do
		[[ "${LOOM_TOKENS[$k]}" == "$2" ]] && return 0
		k=$((k + 1))
	done
	return 1
}

# relay_kind_at <index> - Echo the request kind written by the loom invocation
# whose command word sits at LOOM_TOKENS[<index>], or nothing (section 5).
relay_kind_at() {
	local j=$1 n=${#LOOM_TOKENS[@]} sub="" verb=""
	((j + 1 < n)) && sub=${LOOM_TOKENS[$((j + 1))]}
	((j + 2 < n)) && verb=${LOOM_TOKENS[$((j + 2))]}
	[[ "$verb" == "%%SEP%%" ]] && verb=""
	case "$sub:$verb" in
	memory:note | memory:decision | memory:change | memory:question | memory:resolve) echo memory ;;
	stage:block) echo block ;;
	stage:dispute-criteria) echo dispute ;;
	stage:merge) segment_has_arg "$j" --resolved && echo merge-resolved ;;
	stage:adjudicate) echo verdict ;;
	knowledge:context) echo telemetry ;;
	handoff:*) echo handoff ;;
	esac
	return 0
}

# relay_allowed_kinds - Echo, comma-separated, the kinds written by every
# segment of LOOM_TOKENS whose effective command word is exactly `loom`. Walks
# command positions the way loom_tokens_invoke does, so VAR= prefixes, wrappers
# and `sh -c` payloads are seen through while quoted prose stays one argument.
relay_allowed_kinds() {
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j kind kinds=""
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i") &&
			[[ "${LOOM_TOKENS[$j]##*/}" == loom ]]; then
			kind=$(relay_kind_at "$j")
			if [[ -n "$kind" && ",$kinds," != *",$kind,"* ]]; then
				kinds=${kinds:+$kinds,}$kind
			fi
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
	printf '%s' "$kinds"
}

# drop_control_kinds <csv> - The kinds in <csv> a subagent may relay.
drop_control_kinds() {
	local kind kept="" IFS=,
	for kind in $1; do
		case "$kind" in
		block | dispute | handoff | merge-resolved | verdict) ;;
		*) kept=${kept:+$kept,}$kind ;;
		esac
	done
	printf '%s' "$kept"
}

[[ -n "${LOOM_SESSION_ID:-}" && -n "${LOOM_SCRATCH_DIR:-}" ]] || exit 0

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null)
else
	INPUT_JSON=$(cat 2>/dev/null)
fi

# Fast path, pure bash: almost every Bash call ends here.
case "$INPUT_JSON" in
*"LOOM_RELAY_V1 "* | *persistedOutputPath* | *"Full output saved to: "*) ;;
*) exit 0 ;;
esac
HAS_LINE=0
[[ "$INPUT_JSON" == *"LOOM_RELAY_V1 "* ]] && HAS_LINE=1

if ! source "${BASH_SOURCE[0]%/*}/_common.sh" 2>/dev/null || ! command -v jq &>/dev/null; then
	# Without jq there is no safe way to build JSON: a fixed message, verbatim.
	[[ $HAS_LINE -eq 1 ]] &&
		printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"LOOM relay: unavailable because jq or the loom hook library (_common.sh) is missing, so nothing was relayed. Stop and report it; do not retry."}}'
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
