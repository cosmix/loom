#!/usr/bin/env bash
# A judge (LOOM_SESSION_TYPE=adjudication) writes its own heartbeat file,
# never the stage session's, and does so even though the stage's session:
# frontmatter always names the stage agent, never the judge. Without
# LOOM_SESSION_TYPE set, the ordinary ownership gate must still apply.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
POST_HOOK="$SCRIPT_DIR/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# Stage file whose frontmatter session: names the STAGE agent - deliberately
# not the judge, since a judge is never the stage's own session.
stage_owner() {
	local workdir="$1" session="$2"
	mkdir -p "$workdir/stages" "$workdir/heartbeat"
	printf '%s\n' '---' "session: $session" 'description: test' '---' \
		'# Stage' >"$workdir/stages/01-test-stage.md"
}

# A judge writes heartbeat/<stage>.adjudication.json, bypassing the ownership
# gate, and never touches heartbeat/<stage>.json.
JUDGE_WORK="$TMP/judge"
stage_owner "$JUDGE_WORK" stage-agent-session
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"echo judge"}}' |
	env LOOM_SESSION_TYPE=adjudication LOOM_WORK_DIR="$JUDGE_WORK" \
	LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=judge-session \
	bash "$POST_HOOK"
if [[ ! -f "$JUDGE_WORK/heartbeat/test-stage.adjudication.json" ]]; then
	echo "FAIL: judge heartbeat file was not written"
	exit 1
fi
if [[ "$(jq -r '.session_id' "$JUDGE_WORK/heartbeat/test-stage.adjudication.json")" != judge-session ]]; then
	echo "FAIL: judge heartbeat file has the wrong session_id"
	exit 1
fi
if [[ "$(jq -r '.stage_id' "$JUDGE_WORK/heartbeat/test-stage.adjudication.json")" != test-stage ]]; then
	echo "FAIL: judge heartbeat file has the wrong stage_id"
	exit 1
fi
if [[ -e "$JUDGE_WORK/heartbeat/test-stage.json" ]]; then
	echo "FAIL: judge write also created the stage session's heartbeat file"
	exit 1
fi

# Without LOOM_SESSION_TYPE, the ordinary ownership gate still refuses a
# session that does not match the stage's session: field, and no heartbeat
# file is written.
GATED_WORK="$TMP/gated"
stage_owner "$GATED_WORK" stage-agent-session
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"echo gated"}}' |
	env LOOM_WORK_DIR="$GATED_WORK" LOOM_STAGE_ID=test-stage \
	LOOM_SESSION_ID=judge-session bash "$POST_HOOK"
if [[ -e "$GATED_WORK/heartbeat/test-stage.json" ]]; then
	echo "FAIL: ownership gate did not refuse a non-owning session"
	exit 1
fi
if [[ -e "$GATED_WORK/heartbeat/test-stage.adjudication.json" ]]; then
	echo "FAIL: judge heartbeat file was written without LOOM_SESSION_TYPE"
	exit 1
fi

echo "PASS"
