#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-wrapper.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

WRAPPER="$(cd "$(dirname "$0")/.." && pwd)/codex-forward.sh"
HOME_DIR="$d/home"
BIN_DIR="$d/bin"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
COMPANION="$COMPANION_DIR/codex-companion.mjs"
PLUGIN_DATA="$HOME_DIR/.codex/plugin-data"
WORK_DIR="$d/work"
CAPTURE_LAUNCH="$d/launch-argv"
CALLS="$d/calls"
ENV_CALLS="$d/env-calls"
STATUS_CALLS="$d/status-calls"
NODE_CALLED="$d/node-called"
INVOCATION=inv-0123456789abcdef0123456789abcdef
SESSION_ID="loom.v1:stage-one:loom-session:unit-a:$INVOCATION"
STARTS="$WORK_DIR/subagents/stage-one/starts.jsonl"
mkdir -p "$BIN_DIR" "$COMPANION_DIR" "$PLUGIN_DATA/state/workspace-hash/jobs" \
	"$WORK_DIR/subagents/stage-one"
printf '%s\n' '// pinned fixture' >"$COMPANION"
printf '%s\n' '{}' >"$PLUGIN_DATA/state/workspace-hash/jobs/job-exact.json"
jq -nc '{agent_id:"wrapper-forwarder",agent_type:"loom-codex-forwarder",stage_id:"stage-one",
	loom_session_id:"loom-session",parent_session_id:"parent-session",
	ts:"2000-01-01T00:00:00.000Z"}' >"$STARTS"

cat >"$BIN_DIR/sandbox-exec" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB

cat >"$BIN_DIR/node" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
touch "$NODE_CALLED"
jq -nc --args '$ARGS.positional' -- "$@" >>"$CALLS"
companion=$1
shift
subcommand=$1
shift
job_id=job-exact
printf '%s|%s|%s\n' "$subcommand" "${CODEX_COMPANION_SESSION_ID:-}" "${CLAUDE_PLUGIN_DATA:-}" >>"$ENV_CALLS"

case "$subcommand" in
task)
	printf '%q\n' "$companion" task "$@" >"$CAPTURE_LAUNCH"
	if [[ "${SCENARIO:-completed}" == invalid-id ]]; then
		job_id='bad/id'
	fi
	jq -nc --arg id "$job_id" '{jobId:$id,status:"queued",title:"Codex Task",summary:"Task",logFile:"/private/job.log"}'
	;;
status)
	[[ $# -eq 5 && "$1" == "$job_id" && "$2" == --wait && "$3" == --json &&
		"$4" == --timeout-ms && "$5" == 540000 ]] || exit 92
	printf '%s\n' "$*" >>"$STATUS_CALLS"
	case "${SCENARIO:-completed}" in
	wait-fail) printf '%s\n' 'simulated wait failure' >&2; exit 7 ;;
	wait-timeout) status=running; phase=working; timed_out=true ;;
	running) status=running; phase=working; timed_out=false ;;
	queued) status=queued; phase=queued; timed_out=false ;;
	failed) status=failed; phase=failed; timed_out=false ;;
	cancelled) status=cancelled; phase=cancelled; timed_out=false ;;
	*) status=completed; phase=done; timed_out=false ;;
	esac
	jq -nc --arg id "$job_id" --arg status "$status" --arg phase "$phase" \
		--argjson timed_out "$timed_out" \
		'{workspaceRoot:"/workspace",job:{id:$id,status:$status,phase:$phase},waitTimedOut:$timed_out,timeoutMs:540000}'
	;;
result)
	[[ $# -eq 2 && "$1" == "$job_id" && "$2" == --json ]] || exit 93
	case "${SCENARIO:-completed}" in
	failed) status=failed; phase=failed; rendered='provider failure' ;;
	cancelled) status=cancelled; phase=cancelled; rendered='provider cancelled' ;;
	*) status=completed; phase=done; rendered='provider final message' ;;
	esac
	jq -nc --arg id "$job_id" --arg status "$status" --arg phase "$phase" --arg rendered "$rendered" \
		'{job:{id:$id,status:$status,phase:$phase},storedJob:{id:$id,status:$status,phase:$phase,rendered:$rendered}}'
	;;
cancel)
	[[ $# -eq 2 && "$1" == "$job_id" && "$2" == --json ]] || exit 95
	jq -nc --arg id "$job_id" '{job:{id:$id,status:"cancelled",phase:"cancelled"}}'
	;;
*) exit 94 ;;
esac
STUB
chmod +x "$BIN_DIR/node" "$BIN_DIR/sandbox-exec"

prompt=$'literal; operator\nsecond line with $HOME and `ticks`'
START='LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"job-exact"}'
SEPARATOR='--- LOOM-FORWARD-OUTPUT ---'

run_companion() {
	local scenario="$1" stdout="$2" stderr="$3" home="${4:-$HOME_DIR}"
	: >"$CALLS"
	: >"$ENV_CALLS"
	: >"$STATUS_CALLS"
	RUN_STATUS=0
	HOME="$home" PATH="$BIN_DIR:$PATH" SCENARIO="$scenario" NODE_CALLED="$NODE_CALLED" \
		CAPTURE_LAUNCH="$CAPTURE_LAUNCH" CALLS="$CALLS" ENV_CALLS="$ENV_CALLS" STATUS_CALLS="$STATUS_CALLS" \
		CLAUDE_PLUGIN_DATA="$d/attacker-selected-root" LOOM_WORK_DIR="$WORK_DIR" \
		LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=loom-session \
		bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write \
		--unit-id unit-a --invocation-id "$INVOCATION" >"$stdout" 2>"$stderr" || RUN_STATUS=$?
}

assert_line() {
	local file="$1" number="$2" expected="$3" actual
	actual=$(sed -n "${number}p" "$file")
	[[ "$actual" == "$expected" ]] || {
		printf '%s\n' "FAIL: $file line $number was '$actual', expected '$expected'"
		exit 1
	}
}

assert_terminal() {
	local scenario="$1" marker_outcome="$2" evidence_state="$3" exit_code="$4"
	local stdout="$d/${scenario}.stdout" stderr="$d/${scenario}.stderr"
	run_companion "$scenario" "$stdout" "$stderr"
	[[ $RUN_STATUS -eq $exit_code && ! -s "$stderr" ]]
	assert_line "$stdout" 1 "$START"
	assert_line "$stdout" 2 \
		"LOOM-FORWARD-END {\"v\":1,\"backend\":\"companion\",\"job_id\":\"job-exact\",\"outcome\":\"$marker_outcome\",\"exit_code\":$exit_code}"
	assert_line "$stdout" 3 "$SEPARATOR"
	rg -qFx "unit: unit-a" "$stdout"
	rg -qFx "invocation: $INVOCATION" "$stdout"
	rg -qFx "state: $evidence_state" "$stdout"
	[[ $(wc -l <"$STATUS_CALLS") -eq 1 ]]
	[[ $(wc -l <"$CALLS") -eq 3 ]]
	jq -se 'map(.[1]) == ["task", "status", "result"]' "$CALLS" >/dev/null
}

# Terminal states map once, and every companion call receives the fixed identity roots.
OUT="$d/completed.stdout"
ERR="$d/completed.stderr"
run_companion completed "$OUT" "$ERR"
[[ $RUN_STATUS -eq 0 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$START"
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-exact","outcome":"succeeded","exit_code":0}'
assert_line "$OUT" 3 "$SEPARATOR"
rg -qFx "record: $PLUGIN_DATA/state/workspace-hash/jobs/job-exact.json" "$OUT"
rg -qFx 'state: succeeded' "$OUT"
[[ $(wc -l <"$ENV_CALLS") -eq 3 ]]
rg -qFx "task|$SESSION_ID|$PLUGIN_DATA" "$ENV_CALLS"
rg -qFx "status|$SESSION_ID|$PLUGIN_DATA" "$ENV_CALLS"
rg -qFx "result|$SESSION_ID|$PLUGIN_DATA" "$ENV_CALLS"
[[ $(wc -l <"$STATUS_CALLS") -eq 1 ]]
rg -qFx 'job-exact --wait --json --timeout-ms 540000' "$STATUS_CALLS"

[[ $(wc -l <"$CAPTURE_LAUNCH") -eq 10 ]]
assert_line "$CAPTURE_LAUNCH" 2 task
assert_line "$CAPTURE_LAUNCH" 4 --background
assert_line "$CAPTURE_LAUNCH" 5 --json
assert_line "$CAPTURE_LAUNCH" 6 --write
assert_line "$CAPTURE_LAUNCH" 7 --model
assert_line "$CAPTURE_LAUNCH" 8 gpt-5.6-terra
assert_line "$CAPTURE_LAUNCH" 9 --effort
assert_line "$CAPTURE_LAUNCH" 10 xhigh
rg -qF '=== TASK ===' "$CAPTURE_LAUNCH"
rg -qF 'literal;' "$CAPTURE_LAUNCH"
[[ ! -e "$d/operator" ]]

assert_terminal failed failed failed 1
assert_terminal cancelled canceled cancelled 1

# A deadline snapshot cancels the still-running job, reports timed_out, and exits 124.
for scenario in wait-timeout running queued; do
	OUT="$d/${scenario}.stdout"
	ERR="$d/${scenario}.stderr"
	run_companion "$scenario" "$OUT" "$ERR"
	[[ $RUN_STATUS -eq 124 && ! -s "$ERR" ]]
	assert_line "$OUT" 1 "$START"
	assert_line "$OUT" 2 \
		'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-exact","outcome":"timed_out","exit_code":124}'
	assert_line "$OUT" 3 "$SEPARATOR"
	rg -qF 'exceeded the 540000 ms unit deadline and was cancelled' "$OUT"
	rg -qFx 'exit: 124' "$OUT"
	rg -qFx 'state: timed_out' "$OUT"
	[[ $(wc -l <"$STATUS_CALLS") -eq 1 ]]
	[[ $(wc -l <"$CALLS") -eq 3 ]]
	jq -se 'map(.[1]) == ["task", "status", "cancel"]' "$CALLS" >/dev/null
	jq -se '[.[] | select(.[1] == "cancel")] | length == 1 and (.[0][2:] == ["job-exact", "--json"])' \
		"$CALLS" >/dev/null
done

# A wait command failure retains active ownership without manufacturing terminal evidence.
OUT="$d/wait-fail.stdout"
ERR="$d/wait-fail.stderr"
run_companion wait-fail "$OUT" "$ERR"
[[ $RUN_STATUS -eq 7 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$START"
assert_line "$OUT" 2 "$SEPARATOR"
! rg -q '^LOOM-FORWARD-END ' "$OUT"
rg -qF 'simulated wait failure' "$OUT"
rg -qFx 'state: unknown' "$OUT"

# Only the pinned companion is supported, and root failures happen before node launch.
UNSUPPORTED_HOME="$d/unsupported-home"
mkdir -p "$UNSUPPORTED_HOME/.claude/plugins/cache/openai-codex/codex/1.0.7/scripts"
printf '%s\n' '// unsupported' >"$UNSUPPORTED_HOME/.claude/plugins/cache/openai-codex/codex/1.0.7/scripts/codex-companion.mjs"
rm -f "$NODE_CALLED"
OUT="$d/unsupported.stdout"
ERR="$d/unsupported.stderr"
run_companion completed "$OUT" "$ERR" "$UNSUPPORTED_HOME"
[[ $RUN_STATUS -eq 1 && ! -e "$NODE_CALLED" ]]
! rg -q '^LOOM-FORWARD-START ' "$OUT"
rg -qF 'Supported codex companion 1.0.6 is missing or unsafe' "$OUT"

UNWRITABLE_HOME="$d/unwritable-home"
UNWRITABLE_COMPANION="$UNWRITABLE_HOME/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
mkdir -p "$UNWRITABLE_COMPANION" "$UNWRITABLE_HOME/.codex"
printf '%s\n' '// pinned fixture' >"$UNWRITABLE_COMPANION/codex-companion.mjs"
printf '%s\n' blocker >"$UNWRITABLE_HOME/.codex/plugin-data"
rm -f "$NODE_CALLED"
OUT="$d/unwritable.stdout"
ERR="$d/unwritable.stderr"
run_companion completed "$OUT" "$ERR" "$UNWRITABLE_HOME"
[[ $RUN_STATUS -eq 1 && ! -e "$NODE_CALLED" ]]
! rg -q '^LOOM-FORWARD-START ' "$OUT"
rg -qF 'canonical plugin data state root is not writable' "$OUT"

# Direct lane behavior remains a joined foreground child with the existing markers.
DIRECT_BIN="$d/direct-bin"
mkdir -p "$DIRECT_BIN"
cat >"$DIRECT_BIN/sandbox-exec" <<'STUB'
#!/usr/bin/env bash
exit 71
STUB
cat >"$DIRECT_BIN/codex" <<'STUB'
#!/usr/bin/env bash
printf '%q\n' "$@" >"$DIRECT_ARGV"
[[ /dev/stdin -ef /dev/null ]] || exit 66
[[ "${DIRECT_SCENARIO:-success}" == missing-thread ]] || printf '%s\n' '{"type":"thread.started","thread_id":"thread-exact"}'
printf '%s\n' '{"type":"turn.started","turn_id":"turn-exact"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"direct final"}}'
if [[ "${DIRECT_SCENARIO:-success}" == failed ]]; then
	printf '%s\n' '{"type":"turn.failed","turn_id":"turn-exact"}'
	exit 9
fi
printf '%s\n' '{"type":"turn.completed","turn_id":"turn-exact"}'
STUB
chmod +x "$DIRECT_BIN/codex" "$DIRECT_BIN/sandbox-exec"

run_direct() {
	local scenario="$1" stdout="$2" stderr="$3"
	RUN_STATUS=0
	HOME="$HOME_DIR" PATH="$DIRECT_BIN:$PATH" DIRECT_SCENARIO="$scenario" DIRECT_ARGV="$d/direct-argv" \
		LOOM_WORK_DIR="$WORK_DIR" LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=loom-session \
		bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write \
		--unit-id unit-a --invocation-id "$INVOCATION" <"$d/stdin-open" >"$stdout" 2>"$stderr" || RUN_STATUS=$?
}
printf '%s\n' 'caller stdin' >"$d/stdin-open"
OUT="$d/direct.stdout"; ERR="$d/direct.stderr"
run_direct success "$OUT" "$ERR"
[[ $RUN_STATUS -eq 0 && ! -s "$ERR" ]]
assert_line "$OUT" 1 'LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-exact"}'
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"succeeded","exit_code":0}'
assert_line "$OUT" 3 "$SEPARATOR"
rg -qFx 'thread: thread-exact' "$OUT"

OUT="$d/direct-failed.stdout"; ERR="$d/direct-failed.stderr"
run_direct failed "$OUT" "$ERR"
[[ $RUN_STATUS -eq 9 && ! -s "$ERR" ]]
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"failed","exit_code":9}'

# Missing guard-injected identity remains an argv error with no protocol output.
OUT="$d/usage.stdout"; ERR="$d/usage.stderr"; status=0
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=loom-session \
	bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$OUT" 2>"$ERR" || status=$?
[[ $status -eq 2 && ! -s "$OUT" ]]

printf '%s\n' PASS
