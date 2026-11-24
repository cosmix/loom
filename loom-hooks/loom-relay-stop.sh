#!/usr/bin/env bash
# loom-relay-stop.sh - Stop hook for stranded control relay tickets and
# unattended idle stops in stage sessions.
#
# A stranded control ticket may have been refused, missed by the relay hook,
# or delivered without being unlinked. The hook gives recovery instructions at
# most once per ticket, tracked by a `.stop-warned-<id>` marker. With no ticket
# warning to give, it relays any `loom hook idle-stop` block within that
# subcommand's limits of three per session and one per tool call.
#
# Both paths skip a stop with stop_hook_active set to true. Revisit if a Stop
# block is observed to end a session: switch to advisory systemMessage output.
#
# Input:  the Stop JSON payload on stdin.
# Output: at most one {"decision":"block","reason":...} object on stdout.
# Exit:   always 0 (the block is the JSON decision, never exit 2).

PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail

[[ -n ${LOOM_SESSION_ID:-} && -n ${LOOM_SCRATCH_DIR:-} ]] || exit 0
command -v jq &>/dev/null || exit 0

source "$(dirname "${BASH_SOURCE[0]}")/_relay_kinds.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"

INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)
if [[ "$(printf '%s' "$INPUT_JSON" | jq -r '.stop_hook_active // false' 2>/dev/null || true)" == true ]]; then
	exit 0
fi

# rerun_sentence <kind> - How to file the request again, per kind.
rerun_sentence() {
	case "$1" in
	complete | freeze-contracts) printf '%s' 'rerun it and follow its `Host job:` line' ;;
	*) printf '%s' 'rerun it alone, in the foreground, with stdout unfiltered' ;;
	esac
}

# ticket_reason <kind> <id> - The one-line instruction for a stranded ticket.
ticket_reason() {
	local kind=$1 id=$2
	printf 'A %s relay ticket %s is still on disk, so its delivery is uncertain. Run `loom request status %s` alone. If it reports pending relay or not found and you ran that command, the request was NOT filed: %s. If it reports any other state, the request was received; do not rerun it. If a later run printed `LOOM relay: received %s` with another id, or a `LOOM relay:` reply refused it, ignore this ticket.' \
		"$kind" "${id:0:8}" "$id" "$(rerun_sentence "$kind")" "$kind"
}

REASON=""
MARKERS=()
for ticket in "$LOOM_SCRATCH_DIR"/*.req; do
	[[ -f "$ticket" && ! -L "$ticket" ]] || continue
	FIELDS=$(jq -r '(.kind // ""), (.id // "")' "$ticket" 2>/dev/null) || continue
	[[ "$FIELDS" == *$'\n'* ]] || continue
	kind=${FIELDS%%$'\n'*}
	id=${FIELDS#*$'\n'}
	[[ "$id" =~ ^[A-Za-z0-9_-]+$ ]] || continue
	relay_kind_is_control "$kind" || continue
	marker="$LOOM_SCRATCH_DIR/.stop-warned-$id"
	if [[ -e "$marker" || -L "$marker" ]]; then continue; fi
	REASON="${REASON:+$REASON$'\n'}$(ticket_reason "$kind" "$id")"
	MARKERS+=("$marker")
done

if [[ -n "$REASON" ]]; then
	for marker in "${MARKERS[@]}"; do
		: >"$marker" 2>/dev/null || true
	done
	jq -nc --arg reason "$REASON" '{decision: "block", reason: $reason}' 2>/dev/null || true
elif [[ -n "${LOOM_STAGE_ID:-}" ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null; then
	# Keep the reason JSON-encoded through Bash to preserve all string characters.
	if IDLE_REASON_JSON=$(printf '%s' "$INPUT_JSON" |
		loom_run_bounded 5 "${LOOM_BIN:-loom}" hook idle-stop 2>/dev/null |
		jq -ces '
			select(length == 1) | .[0]
			| select(type == "object" and .decision == "block")
			| .reason | select(type == "string" and length > 0)
		' 2>/dev/null); then
		jq -nc --arg reason "$IDLE_REASON_JSON" '{decision: "block", reason: ($reason | fromjson)}' 2>/dev/null || true
	fi
fi
exit 0
