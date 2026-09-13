#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-bash-only.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
HOME_DIR="$d/home"
WORK_DIR="$d/work"
WORKSPACE="$d/workspace"
TRANSCRIPTS="$d/subagents"
STAGE_ID=bash-stage
LOOM_SESSION=loom-session
PARENT_SESSION=parent-session
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
LEDGER="$WORK_DIR/subagents/$STAGE_ID/codex.jsonl"
STARTS="$WORK_DIR/subagents/$STAGE_ID/starts.jsonl"
COMPANION_CMD="~/.claude/hooks/loom/codex-forward.sh task 'hello; literal operator' --model gpt-5.6-terra --effort xhigh --write"
mkdir -p "$COMPANION_DIR" "$WORKSPACE/.git" "$TRANSCRIPTS" "$WORK_DIR/subagents/$STAGE_ID"
printf '%s\n' '// pinned fixture' >"$COMPANION_DIR/codex-companion.mjs"

write_start() {
	jq -nc --arg agent_id "$1" --arg stage "$STAGE_ID" --arg session "$LOOM_SESSION" \
		--arg parent "$PARENT_SESSION" \
		'{agent_id:$agent_id,agent_type:"loom-codex-forwarder",stage_id:$stage,
		 loom_session_id:$session,parent_session_id:$parent,ts:"2000-01-01T00:00:00.000Z"}' >>"$STARTS"
}
for agent_id in first prior other-bash missing malformed; do write_start "$agent_id"; done

input_for() {
	jq -nc --arg command "$1" --arg transcript "$2" --arg tool_use_id "$3" \
		--arg session_id "$PARENT_SESSION" --arg cwd "$WORKSPACE" \
		'{tool_name:"Bash",tool_input:{command:$command,timeout:600000},agent_type:"loom-codex-forwarder",transcript_path:$transcript,session_id:$session_id,tool_use_id:$tool_use_id,cwd:$cwd}'
}

run_guard() {
	local input="$1" home="$2" active="$3"
	CODE=0
	if [[ "$active" == 1 ]]; then
		printf '%s' "$input" | HOME="$home" LOOM_WORK_DIR="$WORK_DIR" \
			LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$LOOM_SESSION" \
			bash "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	else
		printf '%s' "$input" | HOME="$home" bash "$HOOK" \
			>"$d/stdout" 2>"$d/stderr" || CODE=$?
	fi
}

expect_allow() {
	local label="$1" command="$2" transcript="$3" tool_use_id="$4"
	local before=0 agent_id unit invocation expected
	[[ ! -f "$LEDGER" ]] || before=$(wc -l <"$LEDGER")
	run_guard "$(input_for "$command" "$transcript" "$tool_use_id")" "$HOME_DIR" 1
	[[ $CODE -eq 0 && ! -s "$d/stderr" && $(wc -l <"$d/stdout") -eq 1 ]] || {
		printf '%s\n' "FAIL: $label: expected an authorized first forward, got exit $CODE"
		exit 1
	}
	[[ $(wc -l <"$LEDGER") -eq $((before + 1)) ]]
	agent_id=${transcript##*/agent-}
	agent_id=${agent_id%.jsonl}
	unit="fwd-$agent_id"
	invocation=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$d/stdout")
	expected="$command --unit-id $unit --invocation-id $invocation"
	jq -e --arg command "$expected" \
		'.hookSpecificOutput == {hookEventName:"PreToolUse",permissionDecision:"allow",updatedInput:{command:$command,timeout:600000}}' \
		"$d/stdout" >/dev/null
	tail -n 1 "$LEDGER" | jq -e --arg agent_id "$agent_id" --arg tool_use_id "$tool_use_id" \
		--arg unit "$unit" --arg invocation "$invocation" '
		.v == 2 and .stage_id == "bash-stage" and .session_id == "loom-session" and
		.parent_session_id == "parent-session" and .forwarder_agent_id == $agent_id and
		.tool_use_id == $tool_use_id and .unit_id == $unit and .invocation_id == $invocation and
		.model == "gpt-5.6-terra" and .effort == "xhigh"' >/dev/null
}

expect_block() {
	local label="$1" input="$2"
	run_guard "$input" "$HOME_DIR" 1
	[[ $CODE -eq 2 ]] || {
		printf '%s\n' "FAIL: $label: expected exit 2, got exit $CODE"
		exit 1
	}
}

write_bash_tool_use() {
	jq -nc --arg id "$1" --arg command "$2" \
		'{type:"assistant",message:{role:"assistant",content:[{type:"tool_use",id:$id,name:"Bash",input:{command:$command}}]}}' >>"$3"
}

# No earlier forwarding call means the first exact command passes.
FIRST_TRANSCRIPT="$TRANSCRIPTS/agent-first.jsonl"
printf '%s\n' '{"type":"user","message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY"}}' >"$FIRST_TRANSCRIPT"
expect_allow 'first forward with no prior tool use' "$COMPANION_CMD" "$FIRST_TRANSCRIPT" first-forward

# A prior exact Bash tool use blocks a second call, including a noop task.
PRIOR_TRANSCRIPT="$TRANSCRIPTS/agent-prior.jsonl"
write_bash_tool_use first-forward "$COMPANION_CMD" "$PRIOR_TRANSCRIPT"
NOOP_CMD="~/.claude/hooks/loom/codex-forward.sh task noop --model gpt-5.6-terra --effort xhigh --write"
expect_block 'second forward after a prior forward' "$(input_for "$NOOP_CMD" "$PRIOR_TRANSCRIPT" second-forward)"
rg -q 'one forward per forwarder.*first forward is already running or finished' "$d/stderr"

# Other prior Bash activity, absent transcripts, and torn JSONL still permit one forward.
OTHER_TRANSCRIPT="$TRANSCRIPTS/agent-other-bash.jsonl"
write_bash_tool_use other-bash 'cargo check --lib' "$OTHER_TRANSCRIPT"
expect_allow 'non-forwarding Bash tool use' "$COMPANION_CMD" "$OTHER_TRANSCRIPT" forward-after-other
expect_allow 'missing transcript' "$COMPANION_CMD" "$TRANSCRIPTS/agent-missing.jsonl" missing-forward
MALFORMED_TRANSCRIPT="$TRANSCRIPTS/agent-malformed.jsonl"
printf '%s\n' '{"type":"assistant","message":' >"$MALFORMED_TRANSCRIPT"
expect_allow 'malformed transcript line' "$COMPANION_CMD" "$MALFORMED_TRANSCRIPT" malformed-forward

# Bash-only and exact-command restrictions still reject other command shapes.
expect_block 'shell separator' "$(input_for "$COMPANION_CMD; touch escaped" "$FIRST_TRANSCRIPT" invalid-forward)"
expect_block 'non-forwarding Bash command' "$(input_for 'cargo check --lib' "$FIRST_TRANSCRIPT" invalid-bash)"

# The same exact call is rejected outside a stage and without companion 1.0.6.
OUTSIDE_INPUT=$(input_for "$COMPANION_CMD" "$FIRST_TRANSCRIPT" outside-tool)
run_guard "$OUTSIDE_INPUT" "$HOME_DIR" 0
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)' "$d/stderr"
NO_COMPANION_HOME="$d/no-companion-home"
mkdir -p "$NO_COMPANION_HOME"
run_guard "$OUTSIDE_INPUT" "$NO_COMPANION_HOME" 1
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'supported codex companion 1.0.6 is missing or unsafe' "$d/stderr"

printf '%s\n' PASS
