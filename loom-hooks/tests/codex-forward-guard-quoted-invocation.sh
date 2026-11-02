#!/usr/bin/env bash
# The stall monitor reads a minted invocation id in a launcher's argv as proof of
# an authorized forward, so no other Bash text may carry that literal.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/cfw-quoted.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
MINTED=inv-0123456789abcdef0123456789abcdef
OTHER=inv-ffffffffffffffffffffffffffffffff
mkdir -p "$TMP/work/subagents/stage-1"
LEDGER="$TMP/work/subagents/stage-1/codex.jsonl"
printf '{"v":2,"invocation_id":"%s"}\n' "$MINTED" >"$LEDGER"
MAIN="$TMP/session-main.jsonl"
: >"$MAIN"

run_guard() {
	local tool="$1" command="$2" code=0
	shift 2
	jq -nc --arg tool "$tool" --arg command "$command" --arg transcript "$MAIN" \
		'{tool_name:$tool,tool_input:{command:$command},agent_type:"",transcript_path:$transcript}' |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_SESSION_TYPE "$@" bash "$HOOK" >"$TMP/out" 2>"$TMP/err" || code=$?
	return "$code"
}

IN_STAGE=(LOOM_STAGE_ID=stage-1 LOOM_SESSION_ID=session-1 LOOM_WORK_DIR="$TMP/work")

expect_blocked() {
	local code=0 tool="${2:-Bash}"
	run_guard "$tool" "$1" "${IN_STAGE[@]}" || code=$?
	if [[ $code -ne 2 ]]; then
		echo "FAIL: expected exit 2 for: $tool $1 (got $code)"
		exit 1
	fi
	grep -qF "$MINTED" "$TMP/err" || { echo "FAIL: refusal does not name the id"; exit 1; }
}

expect_allowed() {
	local code=0
	run_guard "$1" "$2" "${@:3}" || code=$?
	if [[ $code -ne 0 ]]; then
		echo "FAIL: expected exit 0 for: $1 $2 (got $code)"
		cat "$TMP/err"
		exit 1
	fi
}

# Every way of carrying the literal in the launcher's argv is refused.
expect_blocked ": $MINTED; sleep 999"
expect_blocked "bash -c 'sleep 999' --invocation-id $MINTED"
expect_blocked "sleep 999 # --invocation-id $MINTED"
expect_blocked "printf %s $OTHER $MINTED"

# Monitor and PowerShell take a tool_input.command run as a shell command under
# the agent, so they could place the literal in a launcher's argv just as well.
expect_blocked "sleep 999 # --invocation-id $MINTED" Monitor
expect_blocked "sleep 999 # --invocation-id $MINTED" PowerShell

# An id this stage never minted, and a computed id, reach no launcher as a
# literal of this stage. Ordinary commands stay untouched.
expect_allowed Bash "rg $OTHER payload_tests.rs" "${IN_STAGE[@]}"
expect_allowed Bash "id=\$(printf %s inv-0123456789abcdef 0123456789abcdef); echo \"\$id\"" "${IN_STAGE[@]}"
expect_allowed Bash "cargo test" "${IN_STAGE[@]}"

# The rule binds a stage's command tools only: text an Edit carries reaches no
# launcher, a Monitor WebSocket watch has no command, and an ordinary session
# mints nothing.
expect_allowed Edit "$MINTED" "${IN_STAGE[@]}"
code=0
jq -nc --arg transcript "$MAIN" --arg url "wss://example.invalid/$MINTED" \
	'{tool_name:"Monitor",tool_input:{ws:{url:$url}},agent_type:"",transcript_path:$transcript}' |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_SESSION_TYPE "${IN_STAGE[@]}" bash "$HOOK" >"$TMP/out" 2>"$TMP/err" || code=$?
[[ $code -eq 0 ]] || { echo "FAIL: a Monitor WebSocket watch was refused (got $code)"; exit 1; }
expect_allowed Bash "echo $MINTED" PATH="$PATH"
expect_allowed Monitor "echo $MINTED" PATH="$PATH"

# A ledger the guard does not trust cannot create a refusal.
ln -s "$LEDGER" "$TMP/work/subagents/stage-1/linked.jsonl"
mv "$LEDGER" "$TMP/real.jsonl"
ln -s "$TMP/real.jsonl" "$LEDGER"
expect_allowed Bash "echo $MINTED" "${IN_STAGE[@]}"
echo PASS
