#!/usr/bin/env bash
# A subagent's tool call must tag its heartbeat `subagent:true`; the main
# agent's own tool calls must tag `subagent:false`. The stale-input-wait
# reconciler relies on this to tell a subagent's progress apart from the main
# agent's, since only the latter proves a WaitingForInput wait is stale.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
POST_HOOK="$SCRIPT_DIR/../post-tool-use.sh"
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/heartbeat-subagent-flag.XXXXXX") && [[ -n "$TMP" ]] || {
	echo "FAIL: could not create scratch directory"
	exit 1
}
trap '[[ -n "${TMP:-}" ]] && rm -rf -- "$TMP"' EXIT

stage_owner() {
	local workdir="$1" session="$2"
	mkdir -p "$workdir/stages" "$workdir/heartbeat"
	printf '%s\n' '---' 'id: test-stage' "session: $session" 'description: test' '---' \
		'# Stage' \
		>"$workdir/stages/01-test-stage.md"
}

WORK="$TMP/work"
stage_owner "$WORK" writer

# A main-agent tool call writes subagent:false.
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"true"},"session_id":"cc","transcript_path":""}' |
	env LOOM_WORK_DIR="$WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=writer \
	bash "$POST_HOOK"
if ! jq -e '.subagent == false' "$WORK/heartbeat/test-stage.json" >/dev/null 2>&1; then
	echo "FAIL: main-agent tool call did not write subagent:false: $(cat "$WORK/heartbeat/test-stage.json")"
	exit 1
fi

# A subagent tool call (identified by a non-empty agent_type) writes subagent:true.
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"true"},"session_id":"cc","transcript_path":"","agent_type":"worker"}' |
	env LOOM_WORK_DIR="$WORK" LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=writer \
	bash "$POST_HOOK"
if ! jq -e '.subagent == true' "$WORK/heartbeat/test-stage.json" >/dev/null 2>&1; then
	echo "FAIL: subagent tool call did not write subagent:true: $(cat "$WORK/heartbeat/test-stage.json")"
	exit 1
fi

echo "PASS"
