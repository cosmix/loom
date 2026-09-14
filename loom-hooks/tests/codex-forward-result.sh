#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/codex-result.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOK=$(cd "$(dirname "$0")/.." && pwd)/codex-forward-result.sh
HOME_DIR="$d/home"
WORK_DIR="$d/work"
STAGE=stage-one
SESSION=session-one
PARENT=parent-uuid
FORWARDER=forwarder-7
TOOL=tool-direct
UNIT=unit-a
INVOCATION=inv-0123456789abcdef0123456789abcdef
MODEL=gpt-5.6-terra
EFFORT=xhigh
AUTH_TS=2026-09-14T12:00:00.000Z
TERMINAL_AT=2026-09-14T12:01:02.345Z
COMMAND="~/.claude/hooks/loom/codex-forward.sh task 'direct result' --model $MODEL --effort $EFFORT --write --unit-id $UNIT --invocation-id $INVOCATION"
PERSISTED="$HOME_DIR/.claude/projects/project/tool-results/direct.txt"
JOURNAL="$WORK_DIR/subagents/$STAGE/lifecycle.jsonl"

mkdir -p "$HOME_DIR/.claude/projects/project/tool-results" "$WORK_DIR/stages" \
	"$WORK_DIR/subagents/$STAGE"
cat >"$WORK_DIR/stages/$STAGE.md" <<EOF
---
id: $STAGE
session: $SESSION
---
EOF
jq -nc --arg ts "$AUTH_TS" --arg stage "$STAGE" --arg session "$SESSION" \
	--arg parent "$PARENT" --arg forwarder "$FORWARDER" --arg tool "$TOOL" \
	--arg unit "$UNIT" --arg invocation "$INVOCATION" --arg model "$MODEL" --arg effort "$EFFORT" \
	'{v:2,ts:$ts,stage_id:$stage,session_id:$session,parent_session_id:$parent,
	 forwarder_agent_id:$forwarder,tool_use_id:$tool,unit_id:$unit,invocation_id:$invocation,
	 model:$model,effort:$effort,workspace_root:"/",companion_version:"1.0.6",
	 companion_path:"/unused/codex-companion.mjs",state_root:"/unused/state"}' \
	>"$WORK_DIR/subagents/$STAGE/codex.jsonl"

cat >"$PERSISTED" <<EOF
LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-exact"}
LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"succeeded","exit_code":0}
--- LOOM-FORWARD-OUTPUT ---
{"type":"thread.started","thread_id":"thread-exact"}
{"type":"turn.started","turn_id":"turn-exact"}
{"type":"turn.completed","turn_id":"turn-exact"}
LOOM-CODEX-DIRECT-RESULT {"v":1,"state":"succeeded","thread_id":"thread-exact","turn_id":"turn-exact","terminal_at":"$TERMINAL_AT","exit_code":0,"ownership_retained":false}
note: the outer sandbox refuses a nested Seatbelt profile; running codex exec with --sandbox danger-full-access (the outer sandbox is the boundary)
--- LOOM-CODEX-EVIDENCE ---
exit: 0
mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)
thread: thread-exact
unit: $UNIT
invocation: $INVOCATION
state: succeeded
EOF

payload() {
	local output="$1" error="$2" status="$3" persisted="${4:-}"
	jq -nc --arg command "$COMMAND" --arg tool "$TOOL" --arg agent "$FORWARDER" \
		--arg session "$PARENT" --arg output "$output" --arg persisted "$persisted" \
		--argjson error "$error" --argjson status "$status" '
		{tool_name:"Bash",tool_input:{command:$command,timeout:600000},tool_use_id:$tool,
		 agent_id:$agent,session_id:$session,cwd:"/",
		 tool_result:({output:$output,is_error:$error,exit_code:$status} +
		  (if $persisted == "" then {} else {persistedOutputPath:$persisted} end))}'
}

run_hook() {
	local input="$1"
	printf '%s' "$input" | HOME="$HOME_DIR" LOOM_WORK_DIR="$WORK_DIR" \
		LOOM_STAGE_ID="$STAGE" LOOM_SESSION_ID="$SESSION" bash "$HOOK" >"$d/hook.out"
	[[ ! -s "$d/hook.out" ]]
}

# The full output is accepted only through the harness-owned persisted-output path.
ACK="Full output saved to: $PERSISTED"
run_hook "$(payload "$ACK" false 0 "$PERSISTED")"
[[ -f "$JOURNAL" && $(wc -l <"$JOURNAL") -eq 2 ]]

EXPECTED="$d/expected.jsonl"
cat >"$EXPECTED" <<'EOF'
{"version":1,"event_id":"sha256:a795a3c76bbfffa7e3f029b578d369e304cb77543d0c482fa25b8f66f1f0d7d6","producer":"codex_direct","identity":{"kind":"codex","stage_id":"stage-one","loom_session_id":"session-one","parent_session_id":"parent-uuid","forwarder_agent_id":"forwarder-7","unit_id":"unit-a","invocation_id":"inv-0123456789abcdef0123456789abcdef","workspace_root":"/","execution":{"mode":"direct","thread_id":"thread-exact","tool_use_id":"tool-direct"}},"observed_at":"2026-09-14T12:00:00.000Z","state":"running","evidence":{"evidence_kind":"authorization","requested_model":"gpt-5.6-terra","requested_effort":"xhigh","invocation_id":"inv-0123456789abcdef0123456789abcdef","job_id":null,"thread_id":"thread-exact","turn_id":null,"tool_use_id":"tool-direct","terminal_at":null,"outcome":"running","detail":null}}
{"version":1,"event_id":"sha256:cdeb7f7c6f6660a4524b508aefd6a0dff4b9621caccba67e065edc5c67f38c91","producer":"codex_direct","identity":{"kind":"codex","stage_id":"stage-one","loom_session_id":"session-one","parent_session_id":"parent-uuid","forwarder_agent_id":"forwarder-7","unit_id":"unit-a","invocation_id":"inv-0123456789abcdef0123456789abcdef","workspace_root":"/","execution":{"mode":"direct","thread_id":"thread-exact","tool_use_id":"tool-direct"}},"observed_at":"2026-09-14T12:01:02.345Z","state":"completed","evidence":{"evidence_kind":"observation","requested_model":"gpt-5.6-terra","requested_effort":"xhigh","invocation_id":"inv-0123456789abcdef0123456789abcdef","job_id":null,"thread_id":"thread-exact","turn_id":"turn-exact","tool_use_id":"tool-direct","terminal_at":"2026-09-14T12:01:02.345Z","outcome":"succeeded","detail":null}}
EOF
cmp "$EXPECTED" "$JOURNAL"

# A harness acknowledgement without the final foreground result establishes nothing.
before=$(wc -l <"$JOURNAL")
run_hook "$(payload 'Command running in background with ID bash-early' false 0)"
[[ $(wc -l <"$JOURNAL") -eq $before ]]

# An agent-controlled symlink cannot masquerade as persisted harness output.
ln -s "$PERSISTED" "$HOME_DIR/.claude/projects/project/tool-results/forged.txt"
run_hook "$(payload 'Full output saved to: forged' false 0 \
	"$HOME_DIR/.claude/projects/project/tool-results/forged.txt")"
[[ $(wc -l <"$JOURNAL") -eq $before ]]

# The "Full output saved to:" text fallback (no persistedOutputPath field at
# all) runs the same extraction as the JSON field and is held to the same
# trust check: a path named only in the inline text still fails closed with
# no journal growth when it is outside the harness-owned tool-results tree.
outside_dir="$d/outside/tool-results"
mkdir -p "$outside_dir"
outside_path="$outside_dir/untrusted.txt"
printf 'untrusted content\n' >"$outside_path"
run_hook "$(payload "Full output saved to: $outside_path" false 0)"
[[ $(wc -l <"$JOURNAL") -eq $before ]]

printf '%s\n' PASS
