#!/usr/bin/env bash
# Semantic `loom subagents list` polling and forward-receipt guidance.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
hook="$root/loom-hooks/poll-guard.sh"
d=$(mktemp -d "${TMPDIR:-/tmp}/pg.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT

work="$d/work"
stage="pg-stage"
session="pg-session"
mkdir -p "$work"

fail() {
	printf 'FAIL: %s\n' "$1"
	exit 1
}

run_hook() {
	local command="$1" agent="$2"
	jq -nc --arg command "$command" --arg agent "$agent" --arg session "$session" \
		'{tool_name:"Bash",tool_input:{command:$command},agent_id:$agent,session_id:$session}' |
		LOOM_WORK_DIR="$work" LOOM_STAGE_ID="$stage" LOOM_SESSION_ID="$session" bash "$hook"
}

third_output() {
	local command="$1" agent="$2" output=""
	for _ in 1 2 3; do output=$(run_hook "$command" "$agent"); done
	printf '%s' "$output"
}

assert_counted() {
	local label="$1" command="$2" output
	output=$(third_output "$command" "count-$label")
	[[ "$output" == *"run 3 times"* ]] || fail "$label was not counted: $output"
}

assert_not_counted() {
	local label="$1" command="$2" output
	for _ in 1 2 3 4 5; do
		output=$(run_hook "$command" "skip-$label")
		[[ -z "$output" ]] || fail "$label was counted: $output"
	done
}

assert_owned_watch() {
	local label="$1" command="$2" output
	output=$(run_hook "$command" "watch-$label")
	[[ -z "$output" ]] || fail "$label first watch was not silent: $output"
	output=$(run_hook "$command" "watch-$label")
	[[ "$output" == *AlreadyWaiting* && "$output" == *'exit 4'* ]] || fail "$label repeated watch omitted guidance: $output"
}

assert_counted plain-list 'loom subagents list --json'
assert_counted head-pipeline 'loom subagents list | head -n 3'
assert_counted tail-pipeline 'loom subagents list | tail -n 3'
assert_counted rg-pipeline 'loom subagents list | rg running'

assert_not_counted wait 'loom subagents wait --receipt aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa --timeout 1'
assert_owned_watch owned 'loom subagents watch --worker claude:a1 --timeout 1'
assert_not_counted redirection "loom subagents list > $d/list.txt"
assert_not_counted quoted-prompt "codex-forward.sh task 'loom subagents list' --write"

receipts="$work/subagents/$stage/forward-receipts.jsonl"
id_a=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
id_b=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

output=$(third_output 'loom subagents list' unresolved-zero)
[[ "$output" == *"forward state is unresolved"* && "$output" == *"repeated list polling will not resolve it"* ]] || fail "zero active receipts: $output"
[[ "$output" == *'loom subagents watch --worker <kind>:<id> ... --timeout 3600'* ]] || fail "zero active receipts omitted owned watch: $output"

mkdir -p "$(dirname "$receipts")"
printf '{"receipt_id":"%s","loom_session_id":"%s","backend":"companion","backend_id":"job-a","state":"running"}\n' "$id_a" "$session" >"$receipts"
output=$(third_output 'loom subagents list' one-active)
[[ "$output" == *"loom subagents wait --receipt $id_a --timeout 3600"* ]] || fail "single active receipt: $output"

printf '{"receipt_id":"%s","loom_session_id":"%s","backend":"companion","backend_id":"job-a","state":"running"}\n' "$id_a" "$session" >"$receipts"
printf '{"receipt_id":"%s","loom_session_id":"%s","backend":"companion","backend_id":"job-b","state":"queued"}\n' "$id_b" "$session" >>"$receipts"
output=$(third_output 'loom subagents list' many-active)
[[ "$output" == *"forward state is unresolved"* ]] || fail "several active receipts: $output"

printf '{"receipt_id":"%s","loom_session_id":"foreign","backend":"companion","backend_id":"job-a","state":"running"}\n' "$id_a" >"$receipts"
printf '{"receipt_id":"%s","loom_session_id":"%s","backend":"companion","backend_id":"job-b","state":"running"}\n' "$id_b" "$session" >>"$receipts"
output=$(third_output 'loom subagents list' foreign-session)
[[ "$output" == *"loom subagents wait --receipt $id_b --timeout 3600"* ]] || fail "foreign receipt was not ignored: $output"

printf '{not-json}\n' >"$receipts"
output=$(third_output 'loom subagents list' malformed)
[[ "$output" == *"forward state is unresolved"* ]] || fail "malformed receipt did not fail open: $output"

printf 'PASS\n'
