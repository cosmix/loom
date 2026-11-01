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
MODEL=gpt-6.1-terra
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
cp -R "$WORK_DIR" "$d/work-template"

cat >"$PERSISTED" <<EOF
LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-exact"}
LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"succeeded","exit_code":0}
--- LOOM-FORWARD-OUTPUT ---
{"type":"thread.started","thread_id":"thread-exact"}
{"type":"turn.started"}
{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}
LOOM-CODEX-DIRECT-RESULT {"v":1,"state":"succeeded","thread_id":"thread-exact","turn_id":null,"terminal_at":"$TERMINAL_AT","exit_code":0,"ownership_retained":false,"announced_thread_id":"thread-exact"}
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
{"version":1,"event_id":"sha256:a68754e0188b66ee235dbec88ff3866b2e06f1d30e6be1f9ce7a2ab72411b129","producer":"codex_direct","identity":{"kind":"codex","stage_id":"stage-one","loom_session_id":"session-one","parent_session_id":"parent-uuid","forwarder_agent_id":"forwarder-7","unit_id":"unit-a","invocation_id":"inv-0123456789abcdef0123456789abcdef","workspace_root":"/","execution":{"mode":"direct","thread_id":"thread-exact","tool_use_id":"tool-direct"}},"observed_at":"2026-09-14T12:00:00.000Z","state":"running","evidence":{"evidence_kind":"authorization","requested_model":"gpt-6.1-terra","requested_effort":"xhigh","invocation_id":"inv-0123456789abcdef0123456789abcdef","job_id":null,"thread_id":"thread-exact","turn_id":null,"tool_use_id":"tool-direct","terminal_at":null,"outcome":"running","detail":null}}
{"version":1,"event_id":"sha256:88f5598189f9e54be2db3fba94fe1f1f3cfb19dd9ca1525e2bafb16e393c01ae","producer":"codex_direct","identity":{"kind":"codex","stage_id":"stage-one","loom_session_id":"session-one","parent_session_id":"parent-uuid","forwarder_agent_id":"forwarder-7","unit_id":"unit-a","invocation_id":"inv-0123456789abcdef0123456789abcdef","workspace_root":"/","execution":{"mode":"direct","thread_id":"thread-exact","tool_use_id":"tool-direct"}},"observed_at":"2026-09-14T12:01:02.345Z","state":"completed","evidence":{"evidence_kind":"observation","requested_model":"gpt-6.1-terra","requested_effort":"xhigh","invocation_id":"inv-0123456789abcdef0123456789abcdef","job_id":null,"thread_id":"thread-exact","turn_id":null,"tool_use_id":"tool-direct","terminal_at":"2026-09-14T12:01:02.345Z","outcome":"succeeded","detail":null}}
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

# Each case below runs against a fresh work dir. Args: case name, result state,
# exit status, ownership_retained, announced_thread_id (JSON).
run_direct_case() {
	local name="$1" state="$2" status="$3" retained="$4" announced="$5" reason="${6:-Seatbelt}" error=true
	local file="$HOME_DIR/.claude/projects/project/tool-results/$name.txt"
	cp -R "$d/work-template" "$d/work-$name"
	cat >"$file" <<EOF
LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-exact"}
LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"$state","exit_code":$status}
--- LOOM-FORWARD-OUTPUT ---
{"type":"thread.started","thread_id":"thread-exact"}
LOOM-CODEX-DIRECT-RESULT {"v":1,"state":"$state","thread_id":"thread-exact","turn_id":null,"terminal_at":"$TERMINAL_AT","exit_code":$status,"ownership_retained":$retained,"announced_thread_id":$announced}
--- LOOM-CODEX-EVIDENCE ---
exit: $status
mode: direct (codex exec --sandbox danger-full-access; nested $reason refused)
thread: thread-exact
unit: $UNIT
invocation: $INVOCATION
state: $state
EOF
	[[ "$status" != 0 ]] || error=false
	WORK_DIR="$d/work-$name" run_hook "$(payload "Full output saved to: $file" "$error" "$status" "$file")"
}

# An `unknown` END (exit 125) is a recorded non-success; the detail says whether
# the supervisor kept ownership of a live descendant.
for retained in true false; do
	run_direct_case "unknown-$retained" unknown 125 "$retained" '"thread-exact"'
	case_journal="$d/work-unknown-$retained/subagents/$STAGE/lifecycle.jsonl"
	[[ $(wc -l <"$case_journal") -eq 2 ]]
	detail='incomplete event stream'
	[[ "$retained" == false ]] || detail='ownership retained'
	tail -n 1 "$case_journal" | jq -e --arg detail "$detail" '
		.state == "unknown" and .evidence.outcome == "unknown" and
		.evidence.detail == $detail and .identity.execution.thread_id == "thread-exact"' >/dev/null
done

run_direct_case bubblewrap succeeded 0 false '"thread-exact"' bubblewrap
[[ $(wc -l <"$d/work-bubblewrap/subagents/$STAGE/lifecycle.jsonl") -eq 2 ]]
run_direct_case bad-reason succeeded 0 false '"thread-exact"' unknown
[[ ! -s "$d/work-bad-reason/subagents/$STAGE/lifecycle.jsonl" ]]

# A result whose announced START thread differs from its thread records nothing.
run_direct_case announced-mismatch succeeded 0 false '"thread-other"'
[[ ! -s "$d/work-announced-mismatch/subagents/$STAGE/lifecycle.jsonl" ]]

printf '%s\n' PASS
