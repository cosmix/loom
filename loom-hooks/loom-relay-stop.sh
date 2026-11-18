#!/usr/bin/env bash
# loom-relay-stop.sh - Stop hook that tells a session once when a control relay
# ticket is still on disk.
#
# A control ticket (block, commit, dispute, handoff, verdict, merge, freeze,
# complete, ...) is never swept from disk by a later call. One that is still in
# $LOOM_SCRATCH_DIR when the agent stops was either relayed with its unlink
# failing, refused (a subagent's control kind), or never reached the relay hook
# because the call's output was filtered, redirected or backgrounded. The agent
# may believe the request was filed, so the hook blocks the stop once per ticket
# with the recovery steps: check `loom request status`, rerun only when the
# request was not filed.
#
# Why blocking once is safe: commit-guard.sh is advisory because a Stop hook that
# blocked on every stop fired during subagent waits and ended sessions before they
# committed. This hook blocks at most once per ticket (a `.stop-warned-<id>`
# marker next to the ticket) and never when stop_hook_active is true, so a lead
# waiting on subagents gets one extra instruction and no loop. A subagent's refused
# control ticket sits in the shared scratch too, hence the "ignore this ticket"
# clause. Revisit: if a Stop block is observed to end a session, switch to the
# advisory systemMessage output.
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

[[ -n "$REASON" ]] || exit 0
for marker in "${MARKERS[@]}"; do
	: >"$marker" 2>/dev/null || true
done
jq -nc --arg reason "$REASON" '{decision: "block", reason: $reason}'
exit 0
