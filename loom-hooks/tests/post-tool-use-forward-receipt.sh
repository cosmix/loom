#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/ptufr.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/post-tool-use.sh"
mkdir -p "$d/bin" "$d/work"
CALLS="$d/forward-calls"

cat >"$d/bin/loom" <<'STUB'
#!/usr/bin/env bash
if [[ "$1" == "hook" && "$2" == "forward-receipt" ]]; then
	printf '%s\n' "$*" >>"$FORWARD_CALLS"
fi
exit 0
STUB
chmod +x "$d/bin/loom"

run_hook() {
	local payload="$1"
	printf '%s' "$payload" | env \
		-u LOOM_SESSION_TYPE -u LOOM_MAIN_AGENT_PID \
		PATH="$d/bin:/usr/bin:/bin" FORWARD_CALLS="$CALLS" \
		LOOM_STAGE_ID="stage-one" LOOM_SESSION_ID="session-one" \
		LOOM_WORK_DIR="$d/work" bash "$HOOK" >/dev/null
}

TRANSCRIPT="$d/parent/subagents/agent-forwarder.jsonl"
FORWARD="/hooks/codex-forward.sh task 'work' --model gpt-5.6-terra --effort xhigh --write"
run_hook "$(jq -nc --arg command "$FORWARD" --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:$command},transcript_path:$transcript}')"
run_hook "$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:"printf ordinary"},transcript_path:$transcript}')"
run_hook "$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Edit",tool_input:{file_path:"src/lib.rs"},transcript_path:$transcript}')"

if [[ ! -f "$CALLS" ]] || [[ $(wc -l <"$CALLS") -ne 1 ]]; then
	echo "FAIL: forwarding Bash should invoke forward-receipt exactly once"
	exit 1
fi
EXPECTED="hook forward-receipt --transcript $TRANSCRIPT"
if [[ "$(<"$CALLS")" != "$EXPECTED" ]]; then
	echo "FAIL: unexpected forward-receipt argv: $(<"$CALLS")"
	exit 1
fi

echo "PASS"
