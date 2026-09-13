#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-agent-type.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
HOME_DIR="$d/home"
WORK_DIR="$d/work"
WORKSPACE="$d/workspace"
STAGE_ID=agent-stage
LOOM_SESSION=loom-session
PARENT_SESSION=parent-session
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
LEDGER="$WORK_DIR/subagents/$STAGE_ID/codex.jsonl"
STARTS="$WORK_DIR/subagents/$STAGE_ID/starts.jsonl"
COMMAND="~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write"
mkdir -p "$COMPANION_DIR" "$WORKSPACE/.git" "$WORK_DIR/subagents/$STAGE_ID"
printf '%s\n' '// pinned fixture' >"$COMPANION_DIR/codex-companion.mjs"

write_start() {
	jq -nc --arg agent_id "$1" --arg agent_type "$2" \
		--arg stage "$STAGE_ID" --arg session "$LOOM_SESSION" --arg parent "$PARENT_SESSION" \
		'{agent_id:$agent_id,agent_type:$agent_type,stage_id:$stage,loom_session_id:$session,
		 parent_session_id:$parent,ts:"2000-01-01T00:00:00.000Z"}' >>"$STARTS"
}
write_start primary-forwarder loom-codex-forwarder
write_start rescue-forwarder codex:codex-rescue
write_start outside-forwarder codex:codex-rescue

payload_for() {
	jq -nc --arg tool "$1" --arg command "$2" --arg agent_type "$3" \
		--arg agent_id "$4" --arg tool_use_id "$5" --arg session_id "$PARENT_SESSION" \
		--arg cwd "$WORKSPACE" \
		'{tool_name:$tool,tool_input:{command:$command,timeout:600000},agent_type:$agent_type,agent_id:$agent_id,session_id:$session_id,tool_use_id:$tool_use_id,cwd:$cwd}'
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
	local agent_type="$1" agent_id="$2" tool_use_id="$3" before=0 invocation unit expected
	[[ ! -f "$LEDGER" ]] || before=$(wc -l <"$LEDGER")
	run_guard "$(payload_for Bash "$COMMAND" "$agent_type" "$agent_id" "$tool_use_id")" "$HOME_DIR" 1
	[[ $CODE -eq 0 && ! -s "$d/stderr" && $(wc -l <"$d/stdout") -eq 1 ]]
	[[ $(wc -l <"$LEDGER") -eq $((before + 1)) ]]
	invocation=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$d/stdout")
	unit="fwd-$agent_id"
	expected="$COMMAND --unit-id $unit --invocation-id $invocation"
	jq -e --arg command "$expected" \
		'.hookSpecificOutput == {hookEventName:"PreToolUse",permissionDecision:"allow",updatedInput:{command:$command,timeout:600000}}' \
		"$d/stdout" >/dev/null
	tail -n 1 "$LEDGER" | jq -e --arg agent_id "$agent_id" \
		--arg tool_use_id "$tool_use_id" --arg unit "$unit" --arg invocation "$invocation" '
		.v == 2 and .stage_id == "agent-stage" and .session_id == "loom-session" and
		.parent_session_id == "parent-session" and .forwarder_agent_id == $agent_id and
		.tool_use_id == $tool_use_id and .unit_id == $unit and .invocation_id == $invocation and
		.model == "gpt-5.6-terra" and .effort == "xhigh"' >/dev/null
}

# Both authoritative forwarder types are restricted to the wrapper.
for spec in 'loom-codex-forwarder primary-forwarder primary-tool' \
	'codex:codex-rescue rescue-forwarder rescue-tool'; do
	read -r agent_type agent_id tool_use_id <<<"$spec"
	expect_allow "$agent_type" "$agent_id" "$tool_use_id"
done

# A payload identity without its exact start-ledger row is never authorized.
before=$(wc -l <"$LEDGER")
run_guard "$(payload_for Bash "$COMMAND" loom-codex-forwarder unstarted-forwarder unstarted-tool)" "$HOME_DIR" 1
[[ $CODE -eq 2 && ! -s "$d/stdout" && $(wc -l <"$LEDGER") -eq $before ]]
rg -qF 'forwarder identity does not match exactly one SubagentStart row' "$d/stderr"

run_guard "$(payload_for Edit ignored loom-codex-forwarder primary-forwarder edit-tool)" "$HOME_DIR" 1
[[ $CODE -eq 2 ]]
run_guard "$(payload_for Bash 'cargo check --lib' codex:codex-rescue rescue-forwarder cargo-tool)" "$HOME_DIR" 1
[[ $CODE -eq 2 ]]

# The same exact call is rejected without a complete active-stage identity.
OUTSIDE_INPUT=$(payload_for Bash "$COMMAND" codex:codex-rescue outside-forwarder outside-tool)
run_guard "$OUTSIDE_INPUT" "$HOME_DIR" 0
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)' "$d/stderr"

# A complete stage fixture still fails closed when companion 1.0.6 is absent.
NO_COMPANION_HOME="$d/no-companion-home"
mkdir -p "$NO_COMPANION_HOME"
run_guard "$OUTSIDE_INPUT" "$NO_COMPANION_HOME" 1
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'supported codex companion 1.0.6 is missing or unsafe' "$d/stderr"

# Any other authoritative agent type is untouched.
run_guard "$(payload_for Edit ignored loom-software-engineer software-agent other-tool)" "$HOME_DIR" 1
[[ $CODE -eq 0 && ! -s "$d/stdout" && ! -s "$d/stderr" ]]

printf '%s\n' PASS
