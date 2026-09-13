#!/usr/bin/env bash
set -euo pipefail
unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
d=$(mktemp -d "${TMPDIR:-/tmp}/cfg.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT
TMP="$d"
mkdir -p "$TMP/subagents"

COMPANION_CMD="~/.claude/hooks/loom/codex-forward.sh task 'hello; literal operator' --model gpt-5.6-terra --effort xhigh --write"

input_for() {
	jq -nc --arg command "$1" --arg transcript "$2" --arg tool_use_id "$3" \
		'{tool_name:"Bash", tool_input:{command:$command}, agent_type:"loom-codex-forwarder", transcript_path:$transcript, tool_use_id:$tool_use_id}'
}

expect_exit() {
	local expected="$1" label="$2" input="$3" code
	set +e
	printf '%s\n' "$input" | HOME=/home/u bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr"
	code=$?
	set -e
	if [[ $code -ne $expected ]]; then
		echo "FAIL: $label: expected exit $expected, got exit $code"
		exit 1
	fi
}

write_bash_tool_use() {
	jq -nc --arg id "$1" --arg command "$2" \
		'{type:"assistant", message:{role:"assistant", content:[{type:"tool_use", id:$id, name:"Bash", input:{command:$command}}]}}' >>"$3"
}

# No earlier forwarding call means the first exact command passes.
FIRST_TRANSCRIPT="$TMP/subagents/agent-first.jsonl"
printf '%s\n' '{"type":"user","message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY"}}' >"$FIRST_TRANSCRIPT"
expect_exit 0 "first forward with no prior tool use" "$(input_for "$COMPANION_CMD" "$FIRST_TRANSCRIPT" "first-forward")"

# A prior exact Bash tool use blocks a second call, including a noop task.
PRIOR_TRANSCRIPT="$TMP/subagents/agent-prior.jsonl"
write_bash_tool_use "first-forward" "$COMPANION_CMD" "$PRIOR_TRANSCRIPT"
NOOP_CMD="~/.claude/hooks/loom/codex-forward.sh task noop --model gpt-5.6-terra --effort xhigh --write"
expect_exit 2 "second forward after a prior forward" "$(input_for "$NOOP_CMD" "$PRIOR_TRANSCRIPT" "second-forward")"
if ! rg -q 'one forward per forwarder.*first forward is already running or finished' "$TMP/stderr"; then
	echo "FAIL: repeated forward did not explain that the first forward owns the result"
	exit 1
fi

# An earlier Bash use with another command is not evidence of forwarding.
OTHER_BASH_TRANSCRIPT="$TMP/subagents/agent-other-bash.jsonl"
write_bash_tool_use "other-bash" "cargo check --lib" "$OTHER_BASH_TRANSCRIPT"
expect_exit 0 "non-forwarding Bash tool use" "$(input_for "$COMPANION_CMD" "$OTHER_BASH_TRANSCRIPT" "forward-after-other")"

# Primary forwarder classification authorizes when its transcript is absent.
MISSING_TRANSCRIPT="$TMP/subagents/agent-missing.jsonl"
expect_exit 0 "missing transcript" "$(input_for "$COMPANION_CMD" "$MISSING_TRANSCRIPT" "missing-forward")"

# A torn JSONL line is ignored rather than crashing the guard.
MALFORMED_TRANSCRIPT="$TMP/subagents/agent-malformed.jsonl"
printf '%s\n' '{"type":"assistant","message":' >"$MALFORMED_TRANSCRIPT"
expect_exit 0 "malformed transcript line" "$(input_for "$COMPANION_CMD" "$MALFORMED_TRANSCRIPT" "malformed-forward")"

# Shell operators and other Bash commands remain blocked by the exact checker.
expect_exit 2 "forwarding command with a shell separator" "$(input_for "$COMPANION_CMD; touch escaped" "$FIRST_TRANSCRIPT" "invalid-forward")"
expect_exit 2 "non-forwarding Bash command" "$(input_for "cargo check --lib" "$FIRST_TRANSCRIPT" "invalid-bash")"

echo "PASS"
