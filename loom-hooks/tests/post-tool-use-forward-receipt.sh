#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID LOOM_BIN LOOM_HOOK_PATH
d=$(mktemp -d "${TMPDIR:-/tmp}/ptufr.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/post-tool-use.sh"
mkdir -p "$d/bin" "$d/work"
CALLS="$d/forward-calls"
STDIN="$d/forward-stdin"

cat >"$d/bin/loom" <<'STUB'
#!/usr/bin/env bash
if [[ "$1" == "hook" && "$2" == "forward-receipt" ]]; then
	printf '%s\n' "$*" >>"$FORWARD_CALLS"
	cat >>"$FORWARD_STDIN"
	printf '\n' >>"$FORWARD_STDIN"
fi
exit 0
STUB
chmod +x "$d/bin/loom"

run_hook() {
	local payload="$1"
	printf '%s' "$payload" | env \
		-u LOOM_SESSION_TYPE -u LOOM_MAIN_AGENT_PID -u LOOM_BIN -u LOOM_HOOK_PATH \
		PATH="$d/bin:/usr/bin:/bin" FORWARD_CALLS="$CALLS" FORWARD_STDIN="$STDIN" \
		LOOM_STAGE_ID="stage-one" LOOM_SESSION_ID="session-one" \
		LOOM_WORK_DIR="$d/work" bash "$HOOK" >/dev/null
}

TRANSCRIPT="$d/parent/subagents/agent-forwarder.jsonl"
FORWARD="/hooks/codex-forward.sh task 'work' --model gpt-5.6-terra --effort xhigh --write"
FORWARDING="$(jq -nc --arg command "$FORWARD" --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:$command},transcript_path:$transcript,tool_response:{stdout:"out"}}')"
NO_TRANSCRIPT="$(jq -nc --arg command "$FORWARD" \
	'{tool_name:"Bash",tool_input:{command:$command},tool_response:{stdout:"out"}}')"
run_hook "$FORWARDING"
run_hook "$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:"printf ordinary"},transcript_path:$transcript}')"
run_hook "$(jq -nc --arg transcript "$TRANSCRIPT" \
	'{tool_name:"Edit",tool_input:{file_path:"src/lib.rs"},transcript_path:$transcript}')"

if [[ ! -f "$CALLS" ]] || [[ $(wc -l <"$CALLS") -ne 1 ]]; then
	echo "FAIL: forwarding Bash should invoke forward-receipt exactly once"
	exit 1
fi
if [[ "$(<"$CALLS")" != "hook forward-receipt" ]]; then
	echo "FAIL: unexpected forward-receipt argv: $(<"$CALLS")"
	exit 1
fi
if [[ "$(<"$STDIN")" != "$FORWARDING" ]]; then
	echo "FAIL: forward-receipt stdin differs from the forwarding payload"
	exit 1
fi

run_hook "$NO_TRANSCRIPT"
if [[ $(wc -l <"$CALLS") -ne 2 ]] || [[ "$(tail -n 1 "$STDIN")" != "$NO_TRANSCRIPT" ]]; then
	echo "FAIL: a forwarding payload without transcript_path should still record one call"
	exit 1
fi

echo "PASS"
