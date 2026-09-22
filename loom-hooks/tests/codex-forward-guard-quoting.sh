#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-quoting.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
FORWARD_LIB="$(dirname "$HOOK")/_codex_forward.sh"
HOME_DIR="$d/home"
WORK_DIR="$d/work"
WORKSPACE="$d/workspace"
STAGE_ID=quote-stage
LOOM_SESSION=loom-session
PARENT_SESSION=parent-session
AGENT_ID=quote-forwarder
UNIT_ID=fwd-quote-forwarder
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
LEDGER="$WORK_DIR/subagents/$STAGE_ID/codex.jsonl"
STARTS="$WORK_DIR/subagents/$STAGE_ID/starts.jsonl"
mkdir -p "$COMPANION_DIR" "$WORKSPACE/.git" "$WORK_DIR/subagents/$STAGE_ID"
printf '%s\n' '// pinned fixture' >"$COMPANION_DIR/codex-companion.mjs"
jq -nc --arg agent_id "$AGENT_ID" --arg stage "$STAGE_ID" --arg session "$LOOM_SESSION" \
	--arg parent "$PARENT_SESSION" \
	'{agent_id:$agent_id,agent_type:"loom-codex-forwarder",stage_id:$stage,
	 loom_session_id:$session,parent_session_id:$parent,ts:"2000-01-01T00:00:00.000Z"}' >"$STARTS"

payload_for() {
	jq -nc --arg command "$1" --arg tool_use_id "$2" --arg agent_id "$AGENT_ID" \
		--arg session_id "$PARENT_SESSION" --arg cwd "$WORKSPACE" \
		'{tool_name:"Bash",tool_input:{command:$command,timeout:600000},agent_type:"loom-codex-forwarder",agent_id:$agent_id,session_id:$session_id,tool_use_id:$tool_use_id,cwd:$cwd}'
}

run_guard() {
	local input="$1" home="$2" active="$3"
	CODE=0
	if [[ "$active" == 1 ]]; then
		printf '%s' "$input" | HOME="$home" LOOM_WORK_DIR="$WORK_DIR" \
			LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$LOOM_SESSION" \
			bash "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	else
		# Stage evidence without a resolvable stage: the guard enforces and
		# blocks on the incomplete identity. With no evidence at all it would
		# have no policy to apply and would allow the call.
		printf '%s' "$input" | HOME="$home" LOOM_SESSION_ID="$LOOM_SESSION" \
			bash "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	fi
}

expect_allow() {
	local command="$1" tool_use_id="$2" label="$3" before=0 invocation expected model effort
	[[ ! -f "$LEDGER" ]] || before=$(wc -l <"$LEDGER")
	run_guard "$(payload_for "$command" "$tool_use_id")" "$HOME_DIR" 1
	[[ $CODE -eq 0 && ! -s "$d/stderr" && $(wc -l <"$d/stdout") -eq 1 ]] || {
		printf '%s\n' "FAIL: expected allow for $label, got exit $CODE"
		exit 1
	}
	[[ $(wc -l <"$LEDGER") -eq $((before + 1)) ]]
	invocation=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$d/stdout")
	expected="$command --unit-id $UNIT_ID --invocation-id $invocation"
	parse_shell_words "$command"
	model=${PARSED_WORDS[4]}
	effort=${PARSED_WORDS[6]}
	jq -e --arg command "$expected" \
		'.hookSpecificOutput == {hookEventName:"PreToolUse",permissionDecision:"allow",updatedInput:{command:$command,timeout:600000}}' \
		"$d/stdout" >/dev/null
	tail -n 1 "$LEDGER" | jq -e --arg tool_use_id "$tool_use_id" --arg invocation "$invocation" \
		--arg model "$model" --arg effort "$effort" '
		.v == 2 and .stage_id == "quote-stage" and .session_id == "loom-session" and
		.parent_session_id == "parent-session" and .forwarder_agent_id == "quote-forwarder" and
		.tool_use_id == $tool_use_id and .unit_id == "fwd-quote-forwarder" and
		.invocation_id == $invocation and .model == $model and .effort == $effort' >/dev/null
}

expect_block() {
	local command="$1" tool_use_id="$2" label="$3"
	run_guard "$(payload_for "$command" "$tool_use_id")" "$HOME_DIR" 1
	[[ $CODE -eq 2 ]] || {
		printf '%s\n' "FAIL: expected block for $label, got exit $CODE"
		exit 1
	}
}

# Source the shared parser so the apostrophe case asserts its decoded argv word too.
source "$FORWARD_LIB"
[[ "$(type -t parse_shell_words || true)" == function ]]

# Apostrophes, escaped dollars, escaped double quotes/backslashes, and an
# expanded scratch-HOME wrapper path all round-trip byte-for-byte in updatedInput.
CMD="~/.claude/hooks/loom/codex-forward.sh task 'fix the reader'\''s zone' --model gpt-5.6-terra --effort xhigh --write"
expect_allow "$CMD" apostrophe-tool "apostrophe-via-'\'' idiom"
parse_shell_words "$CMD"
[[ "${PARSED_WORDS[2]}" == "fix the reader's zone" ]]

CMD='~/.claude/hooks/loom/codex-forward.sh task cost\ is\ \$5 --model gpt-5.6-terra --effort xhigh --write'
expect_allow "$CMD" dollar-tool 'backslash-escaped dollar'

CMD='~/.claude/hooks/loom/codex-forward.sh task "say \"hi\" then \\ done" --model gpt-5.6-terra --effort xhigh --write'
expect_allow "$CMD" double-quote-tool 'double-quoted escapes'

CMD="$HOME_DIR/.claude/hooks/loom/codex-forward.sh task hello --model gpt-6-luna --effort xhigh --write"
expect_allow "$CMD" expanded-home-tool 'HOME-expanded wrapper path'

# Unquoted operators, incomplete escaping, wrong arity, and another path stay blocked.
expect_block '~/.claude/hooks/loom/codex-forward.sh task $(whoami) --model gpt-5.6-terra --effort xhigh --write' substitution-tool 'command substitution'
expect_block '~/.claude/hooks/loom/codex-forward.sh task hello; rm -rf / --model gpt-5.6-terra --effort xhigh --write' separator-tool 'command chaining'
expect_block '~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write\' backslash-tool 'trailing backslash'
expect_block '~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh' arity-tool 'missing write flag'
expect_block "$HOME_DIR/.claude/hooks/loom/evil.sh task hello --model gpt-5.6-terra --effort xhigh --write" path-tool 'non-wrapper path'

# The same syntactically valid quoted call is blocked outside a stage and with no companion.
CMD="~/.claude/hooks/loom/codex-forward.sh task 'still literal' --model gpt-5.6-terra --effort xhigh --write"
OUTSIDE_INPUT=$(payload_for "$CMD" outside-tool)
run_guard "$OUTSIDE_INPUT" "$HOME_DIR" 0
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)' "$d/stderr"
NO_COMPANION_HOME="$d/no-companion-home"
mkdir -p "$NO_COMPANION_HOME"
run_guard "$OUTSIDE_INPUT" "$NO_COMPANION_HOME" 1
[[ $CODE -eq 2 && ! -s "$d/stdout" ]]
rg -qF 'supported codex companion 1.0.6 is missing or unsafe' "$d/stderr"

printf '%s\n' PASS
