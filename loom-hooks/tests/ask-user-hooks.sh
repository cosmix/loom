#!/usr/bin/env bash
# Regression coverage for the AskUserQuestion pre/post hooks: they must act
# only on a real AskUserQuestion call and record every trigger, real or
# spurious, before touching the stage.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
pre_hook="$root/loom-hooks/ask-user-pre.sh"
post_hook="$root/loom-hooks/ask-user-post.sh"

source "$(dirname "$0")/_path_without.sh"
NOJQ_PATH=$(path_without jq)
trap 'rm -rf "${NOJQ_PATH:-}"' EXIT

fail() {
	printf 'FAIL: %s\n' "$1"
	exit 1
}

with_fixture() (
	local name="$1" body="$2"
	local fixture_dir
	fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/${name}.XXXXXX") && [[ -n "$fixture_dir" ]] || fail "could not create $name scratch directory"
	trap '[[ -n "${fixture_dir:-}" ]] && rm -rf -- "$fixture_dir"' EXIT

	# A stub loom binary that only records what it was called with.
	cat >"$fixture_dir/loom" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$(dirname "$0")/loom.calls"
exit 0
STUB
	chmod +x "$fixture_dir/loom"

	mkdir -p "$fixture_dir/work"
	"$body" "$fixture_dir"
)

# Runs a hook with the given stdin payload against an isolated fixture.
# OSTYPE is overridden so neither the macOS nor the Linux notification branch
# matches - this suite only checks the stage transition and the events trail.
# Optional 4th and 5th arguments set the stage id and the hook PATH.
run_hook() {
	local hook="$1" fixture="$2" stdin="$3" stage="${4:-stage-x}" hook_path="${5:-$PATH}"
	set +e
	printf '%s' "$stdin" |
		OSTYPE=unmatched \
		LOOM_BIN="$fixture/loom" \
		LOOM_HOOK_PATH="$hook_path" \
		LOOM_WORK_DIR="$fixture/work" \
		LOOM_STAGE_ID="$stage" \
		LOOM_SESSION_ID="session-x" \
		bash "$hook" >"$fixture/stdout" 2>"$fixture/stderr"
	HOOK_CODE=$?
	set -e
}

assert_exit_zero() {
	local label="$1" fixture="$2"
	[[ $HOOK_CODE -eq 0 ]] || fail "$label exited $HOOK_CODE: $(cat "$fixture/stderr")"
}

assert_no_loom_call() {
	local label="$1" fixture="$2"
	[[ ! -s "$fixture/loom.calls" ]] || fail "$label unexpectedly called loom: $(cat "$fixture/loom.calls")"
}

assert_loom_called_with() {
	local label="$1" fixture="$2" expected="$3"
	local calls
	calls=$(cat "$fixture/loom.calls" 2>/dev/null || true)
	[[ "$calls" == *"$expected"* ]] || fail "$label did not call '$expected': got '$calls'"
}

# Reads the single events.jsonl line and checks it against the given jq
# filter/expected-value pairs.
assert_events_line() {
	local label="$1" fixture="$2" events="$fixture/work/hooks/events.jsonl" line count
	shift 2
	[[ -f "$events" ]] || fail "$label did not write events.jsonl"
	count=$(wc -l <"$events")
	[[ "$count" -eq 1 ]] || fail "$label wrote $count events.jsonl lines, expected 1"
	line=$(<"$events")
	echo "$line" | jq -e . >/dev/null 2>&1 || fail "$label events.jsonl line is not valid JSON: $line"
	while [[ $# -gt 0 ]]; do
		local filter="$1" expected="$2" actual
		actual=$(printf '%s' "$line" | jq -r "$filter")
		[[ "$actual" == "$expected" ]] || fail "$label expected $filter == '$expected', got '$actual': $line"
		shift 2
	done
}

fixture_case1_pre_ignores_other_tools() {
	local fixture="$1"
	run_hook "$pre_hook" "$fixture" '{"tool_name":"Bash","hook_event_name":"PreToolUse"}'
	assert_exit_zero "pre/other-tool" "$fixture"
	assert_no_loom_call "pre/other-tool" "$fixture"
	assert_events_line "pre/other-tool" "$fixture" \
		'.payload.acted' 'false' \
		'.payload.tool_name' 'Bash'
}

fixture_case2_pre_records_a_real_question() {
	local fixture="$1"
	local stdin='{"tool_name":"AskUserQuestion","hook_event_name":"PreToolUse","session_id":"cc-1","tool_use_id":"toolu_1","tool_input":{"questions":[],"metadata":{"source":"unit"}}}'
	run_hook "$pre_hook" "$fixture" "$stdin"
	assert_exit_zero "pre/real-question" "$fixture"
	assert_loom_called_with "pre/real-question" "$fixture" "stage waiting stage-x"
	assert_events_line "pre/real-question" "$fixture" \
		'.payload.phase' 'pre' \
		'.payload.source' 'unit' \
		'.payload.claude_session_id' 'cc-1' \
		'.payload.acted' 'true'
}

fixture_case3_pre_fails_open_on_empty_stdin() {
	local fixture="$1"
	run_hook "$pre_hook" "$fixture" ''
	assert_exit_zero "pre/empty-stdin" "$fixture"
	assert_loom_called_with "pre/empty-stdin" "$fixture" "stage waiting stage-x"
}

fixture_case4_post_resumes_on_a_real_answer() {
	local fixture="$1"
	run_hook "$post_hook" "$fixture" '{"tool_name":"AskUserQuestion","hook_event_name":"PostToolUse"}'
	assert_exit_zero "post/real-answer" "$fixture"
	assert_loom_called_with "post/real-answer" "$fixture" "stage resume stage-x"
	assert_events_line "post/real-answer" "$fixture" \
		'.payload.phase' 'post' \
		'.payload.acted' 'true'
}

fixture_case5_post_ignores_other_tools() {
	local fixture="$1"
	run_hook "$post_hook" "$fixture" '{"tool_name":"Edit","hook_event_name":"PostToolUse"}'
	assert_exit_zero "post/other-tool" "$fixture"
	assert_no_loom_call "post/other-tool" "$fixture"
	assert_events_line "post/other-tool" "$fixture" \
		'.payload.acted' 'false' \
		'.payload.tool_name' 'Edit'
}

fixture_case6_pre_fails_open_on_malformed_stdin() {
	local fixture="$1"
	run_hook "$pre_hook" "$fixture" '{not json'
	assert_exit_zero "pre/malformed-stdin" "$fixture"
	assert_loom_called_with "pre/malformed-stdin" "$fixture" "stage waiting stage-x"
	assert_events_line "pre/malformed-stdin" "$fixture" \
		'.payload.phase' 'pre' \
		'.payload.acted' 'true'
}

fixture_case7_pre_fails_open_without_jq() {
	local fixture="$1"
	set +e
	printf '%s' '{"tool_name":"Bash"}' |
		OSTYPE=unmatched \
		LOOM_BIN="$fixture/loom" \
		LOOM_HOOK_PATH="$NOJQ_PATH" \
		LOOM_WORK_DIR="$fixture/work" \
		LOOM_STAGE_ID="stage-x" \
		LOOM_SESSION_ID="session-x" \
		bash "$pre_hook" >"$fixture/stdout" 2>"$fixture/stderr"
	HOOK_CODE=$?
	set -e
	assert_exit_zero "pre/no-jq" "$fixture"
	assert_loom_called_with "pre/no-jq" "$fixture" "stage waiting stage-x"
	assert_events_line "pre/no-jq" "$fixture" \
		'.payload.phase' 'pre' \
		'.payload.acted' 'true'
}

# A stage id outside [A-Za-z0-9_-] must not reach the events line, a loom call
# or a notification: the hook leaves without acting, with or without jq.
fixture_case8_hostile_stage_id_is_refused() {
	local fixture="$1" hook hook_path
	for hook in "$pre_hook" "$post_hook"; do
		for hook_path in "$PATH" "$NOJQ_PATH"; do
			run_hook "$hook" "$fixture" '{"tool_name":"AskUserQuestion"}' 'a"b' "$hook_path"
			assert_exit_zero "hostile-stage-id/${hook##*/}" "$fixture"
			assert_no_loom_call "hostile-stage-id/${hook##*/}" "$fixture"
			[[ ! -e "$fixture/work/hooks/events.jsonl" ]] ||
				fail "hostile-stage-id/${hook##*/} wrote events.jsonl: $(cat "$fixture/work/hooks/events.jsonl")"
		done
	done
}

# The macOS notification text travels as its own osascript argument, never
# inside the AppleScript source.
fixture_case9_notification_message_is_an_argument() {
	local fixture="$1" args_log="$1/osascript.args"
	mkdir -p "$fixture/bin"
	cat >"$fixture/bin/osascript" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$@" >"$(dirname "$0")/../osascript.args"
STUB
	chmod +x "$fixture/bin/osascript"
	set +e
	printf '%s' '{"tool_name":"AskUserQuestion"}' |
		OSTYPE=darwin23 LOOM_HOOK_PATH="$fixture/bin:$PATH" LOOM_BIN="$fixture/loom" \
			LOOM_WORK_DIR="$fixture/work" LOOM_STAGE_ID=stage-x LOOM_SESSION_ID=session-x \
			bash "$pre_hook" >"$fixture/stdout" 2>"$fixture/stderr"
	HOOK_CODE=$?
	set -e
	assert_exit_zero "pre/darwin" "$fixture"
	[[ -s "$args_log" ]] || fail "pre/darwin did not call osascript"
	# Every argument that follows -e is AppleScript source; none may hold the text.
	if grep -x -A1 -- '-e' "$args_log" | grep -q 'needs your input'; then
		fail "the notification text is inside an AppleScript source argument: $(cat "$args_log")"
	fi
	grep -qx 'loom stage stage-x needs your input' "$args_log" ||
		fail "the message is not a separate osascript argument: $(cat "$args_log")"
}

with_fixture ask-user-hooks-case1 fixture_case1_pre_ignores_other_tools
with_fixture ask-user-hooks-case2 fixture_case2_pre_records_a_real_question
with_fixture ask-user-hooks-case3 fixture_case3_pre_fails_open_on_empty_stdin
with_fixture ask-user-hooks-case4 fixture_case4_post_resumes_on_a_real_answer
with_fixture ask-user-hooks-case5 fixture_case5_post_ignores_other_tools
with_fixture ask-user-hooks-case6 fixture_case6_pre_fails_open_on_malformed_stdin
with_fixture ask-user-hooks-case7 fixture_case7_pre_fails_open_without_jq

with_fixture ask-user-hooks-case8 fixture_case8_hostile_stage_id_is_refused
with_fixture ask-user-hooks-case9 fixture_case9_notification_message_is_an_argument

printf 'PASS\n'
