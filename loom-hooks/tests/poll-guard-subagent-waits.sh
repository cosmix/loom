#!/usr/bin/env bash
# Regression coverage for bounded `loom subagents` observations and owned waits.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
hook="$root/loom-hooks/poll-guard.sh"

fail() {
	printf 'FAIL: %s\n' "$1"
	exit 1
}

with_fixture() (
	local name="$1" body="$2"
	shift 2
	local fixture_dir fixture_work fixture_stage fixture_session fixture_main_pid
	fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/${name}.XXXXXX") && [[ -n "$fixture_dir" ]] || fail "could not create $name scratch directory"
	trap '[[ -n "${fixture_dir:-}" ]] && rm -rf -- "$fixture_dir"' EXIT
	fixture_work="$fixture_dir/work"
	fixture_stage="stage-$name"
	fixture_session="session-$name"
	fixture_main_pid=$BASHPID
	mkdir -p "$fixture_work"
	"$body" "$@"
)

enable_deny() {
	printf '[hooks]\ndeny_enabled = true\n' >"$fixture_work/config.toml"
}

run_hook() {
	local command="$1" agent="$2" payload
	payload=$(jq -nc --arg command "$command" --arg agent "$agent" --arg session "$fixture_session" \
		'{tool_name:"Bash",tool_input:{command:$command},agent_id:$agent,session_id:$session}')
	set +e
	printf '%s' "$payload" |
		env -u LOOM_HOOK_DEBUG -u COMMIT_FILTER_DEBUG -u LOOM_HOOK_PATH \
			LOOM_WORK_DIR="$fixture_work" \
			LOOM_STAGE_ID="$fixture_stage" \
			LOOM_SESSION_ID="$fixture_session" \
			LOOM_MAIN_AGENT_PID="$fixture_main_pid" \
			bash "$hook" >"$fixture_dir/stdout" 2>"$fixture_dir/stderr"
	HOOK_CODE=$?
	set -e
	HOOK_STDOUT=$(<"$fixture_dir/stdout")
	HOOK_STDERR=$(<"$fixture_dir/stderr")
}

assert_silent_allow() {
	local label="$1"
	[[ $HOOK_CODE -eq 0 ]] || fail "$label exited $HOOK_CODE: $HOOK_STDERR"
	[[ -z "$HOOK_STDOUT" && -z "$HOOK_STDERR" ]] || fail "$label was not silent: stdout=$HOOK_STDOUT stderr=$HOOK_STDERR"
}

assert_warn_count() {
	local label="$1" count="$2" key="$3"
	[[ $HOOK_CODE -eq 0 ]] || fail "$label exited $HOOK_CODE: $HOOK_STDERR"
	[[ -z "$HOOK_STDERR" ]] || fail "$label wrote stderr: $HOOK_STDERR"
	[[ "$HOOK_STDOUT" == *"\`${key}\` has run ${count} times"* ]] || fail "$label did not warn at count $count: $HOOK_STDOUT"
}

assert_deny_count() {
	local label="$1" count="$2" key="$3"
	[[ $HOOK_CODE -eq 2 ]] || fail "$label exited $HOOK_CODE instead of 2: stdout=$HOOK_STDOUT stderr=$HOOK_STDERR"
	[[ -z "$HOOK_STDOUT" ]] || fail "$label wrote stdout while denying: $HOOK_STDOUT"
	[[ "$HOOK_STDERR" == *"\`${key}\` has run ${count} times"* ]] || fail "$label did not deny at count $count: $HOOK_STDERR"
}

fixture_t3_list() {
	local n command='loom subagents list' agent=t3-list
	enable_deny
	for ((n = 1; n <= 902; n++)); do
		run_hook "$command" "$agent"
		case "$n" in
		1 | 2) assert_silent_allow "list call $n" ;;
		3 | 4) assert_warn_count "list call $n" "$n" "$command" ;;
		*) assert_deny_count "list call $n" 5 "$command" ;;
		esac
	done
}

fixture_normalized_list() {
	local agent=normalized-list
	local -a commands=(
		'loom subagents list'
		'/usr/local/bin/loom subagents list'
		'env FOO=1 loom subagents list'
		'command loom subagents list'
		'timeout 5 loom subagents list'
	)
	enable_deny
	run_hook "${commands[0]}" "$agent"
	assert_silent_allow 'plain normalized list'
	run_hook "${commands[1]}" "$agent"
	assert_silent_allow 'absolute-path normalized list'
	run_hook "${commands[2]}" "$agent"
	assert_warn_count 'env normalized list' 3 'loom subagents list'
	run_hook "${commands[3]}" "$agent"
	assert_warn_count 'command normalized list' 4 'loom subagents list'
	run_hook "${commands[4]}" "$agent"
	assert_deny_count 'timeout normalized list' 5 'loom subagents list'
}

fixture_harvest() {
	local n command='loom subagents harvest' agent=harvest
	enable_deny
	for ((n = 1; n <= 6; n++)); do
		run_hook "$command" "$agent"
		case "$n" in
		1 | 2) assert_silent_allow "harvest call $n" ;;
		3 | 4) assert_warn_count "harvest call $n" "$n" "$command" ;;
		*) assert_deny_count "harvest call $n" 5 "$command" ;;
		esac
	done
}

fixture_owned_watch_denied() {
	local command='loom subagents watch --worker claude:a1 --worker codex:u1 --timeout 3600'
	local reordered='loom subagents watch --timeout 90 --worker codex:u1 --json --session alternate --worker claude:a1'
	local different='loom subagents watch --worker claude:a1 --worker codex:u2 --timeout 90'
	enable_deny
	run_hook "$command" owned-watch
	assert_silent_allow 'first owned watch'
	run_hook "$reordered" owned-watch
	[[ $HOOK_CODE -eq 2 ]] || fail "repeated owned watch exited $HOOK_CODE instead of 2: $HOOK_STDERR"
	[[ -z "$HOOK_STDOUT" ]] || fail "repeated owned watch wrote stdout: $HOOK_STDOUT"
	[[ "$HOOK_STDERR" == *AlreadyWaiting* && "$HOOK_STDERR" == *'exit 4'* ]] || fail "repeated owned watch omitted AlreadyWaiting/exit 4 guidance: $HOOK_STDERR"
	run_hook "$different" owned-watch
	assert_silent_allow 'different worker set watch'
}

fixture_owned_watch_warned() {
	local command='loom subagents watch --worker claude:a1 --worker codex:u1 --timeout 3600'
	run_hook "$command" owned-watch-warn
	assert_silent_allow 'first owned watch with deny off'
	run_hook "$command" owned-watch-warn
	[[ $HOOK_CODE -eq 0 && -z "$HOOK_STDERR" ]] || fail "deny-off repeated watch was not a warning: code=$HOOK_CODE stderr=$HOOK_STDERR"
	[[ "$HOOK_STDOUT" == *AlreadyWaiting* && "$HOOK_STDOUT" == *'exit 4'* ]] || fail "deny-off repeated watch omitted AlreadyWaiting/exit 4 guidance: $HOOK_STDOUT"
}

fixture_unmatched() {
	local label="$1" command="$2" n agent="skip-$1"
	enable_deny
	for ((n = 1; n <= 5; n++)); do
		run_hook "$command" "$agent"
		assert_silent_allow "$label call $n"
	done
	local ledger="$fixture_work/hooks/polls/$fixture_session/$agent.tsv"
	[[ ! -e "$ledger" ]] || fail "$label created a poll ledger: $ledger"
}

with_fixture poll-guard-t3-list fixture_t3_list
with_fixture poll-guard-normalized-list fixture_normalized_list
with_fixture poll-guard-harvest fixture_harvest
with_fixture poll-guard-owned-watch-denied fixture_owned_watch_denied
with_fixture poll-guard-owned-watch-warned fixture_owned_watch_warned
with_fixture poll-guard-quoted-prose fixture_unmatched quoted-prose 'echo "loom subagents list"'
with_fixture poll-guard-heredoc fixture_unmatched heredoc $'cat <<\'EOF\'\nloom subagents list\nEOF'
with_fixture poll-guard-lookalike fixture_unmatched lookalike 'loomx subagents list'
with_fixture poll-guard-wait fixture_unmatched wait 'loom subagents wait --receipt x'
with_fixture poll-guard-progress-segment fixture_unmatched progress-segment 'loom subagents list && cargo build'

printf 'PASS\n'
