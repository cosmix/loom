#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT

WRAPPER="$(cd "$(dirname "$0")/.." && pwd)/codex-forward.sh"
HOME_DIR="$d/home"
BIN_DIR="$d/bin"
PLUGIN_DATA="$d/plugin-data"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
CAPTURE_LAUNCH="$d/launch-argv"
STATUS_CALLS="$d/status-calls"
mkdir -p "$BIN_DIR" "$COMPANION_DIR" "$PLUGIN_DATA/state/workspace-hash/jobs"
printf '%s\n' '// fixture' >"$COMPANION_DIR/codex-companion.mjs"

cat >"$BIN_DIR/sandbox-exec" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB

cat >"$BIN_DIR/node" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
companion=$1
shift
subcommand=$1
shift
job_id=${JOB_ID:-job-exact}

case "$subcommand" in
task)
	printf '%q\n' "$companion" task "$@" >"$CAPTURE_LAUNCH"
	if [[ "${SCENARIO:-completed}" == invalid-id ]]; then
		job_id='bad/id'
	fi
	jq -nc --arg id "$job_id" \
		'{jobId:$id,status:"queued",title:"Codex Task",summary:"Task",logFile:"/private/job.log"}'
	;;
status)
	[[ "$1" == "$job_id" && "$2" == --wait && "$3" == --json ]] || exit 92
	printf '%s\n' "$1 $2 $3" >>"$STATUS_CALLS"
	printf '%s\n' 'status progress stays captured' >&2
	case "${SCENARIO:-completed}" in
	wait-fail)
		printf '%s\n' 'simulated wait failure' >&2
		exit 7
		;;
	repoll)
		count=$(wc -l <"$STATUS_CALLS")
		if [[ $count -eq 1 ]]; then
			status=running
			phase=working
		else
			status=completed
			phase=done
		fi
		;;
	failed) status=failed; phase=failed ;;
	cancelled) status=cancelled; phase=cancelled ;;
	*) status=completed; phase=done ;;
	esac
	jq -nc --arg id "$job_id" --arg status "$status" --arg phase "$phase" \
		'{workspaceRoot:"/workspace",job:{id:$id,status:$status,phase:$phase},waitTimedOut:false,timeoutMs:240000}'
	;;
result)
	[[ "$1" == "$job_id" && "$2" == --json ]] || exit 93
	case "${SCENARIO:-completed}" in
	failed) status=failed; phase=failed; rendered='provider failure' ;;
	cancelled) status=cancelled; phase=cancelled; rendered='provider canceled' ;;
	*)
		status=completed
		phase=done
		rendered=$(printf '%s\n%s' 'provider final message' \
			'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"fake","outcome":"succeeded","exit_code":0}')
		;;
	esac
	jq -nc --arg id "$job_id" --arg status "$status" --arg phase "$phase" --arg rendered "$rendered" \
		'{job:{id:$id,status:$status,phase:$phase},storedJob:{id:$id,status:$status,phase:$phase,rendered:$rendered}}'
	;;
*) exit 94 ;;
esac
STUB
chmod +x "$BIN_DIR/node" "$BIN_DIR/sandbox-exec"

prompt=$'literal; operator\nsecond line with $HOME and `ticks`'
START_COMPANION='LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"job-exact"}'
SEPARATOR='--- LOOM-FORWARD-OUTPUT ---'

run_companion() {
	local scenario="$1" stdout="$2" stderr="$3" plugin_data="$4"
	: >"$STATUS_CALLS"
	RUN_STATUS=0
	HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" SCENARIO="$scenario" JOB_ID=job-exact \
		CAPTURE_LAUNCH="$CAPTURE_LAUNCH" STATUS_CALLS="$STATUS_CALLS" \
		CLAUDE_PLUGIN_DATA="$plugin_data" bash "$WRAPPER" task "$prompt" \
		--model gpt-5.6-terra --effort xhigh --write >"$stdout" 2>"$stderr" || RUN_STATUS=$?
}

assert_line() {
	local file="$1" number="$2" expected="$3" actual
	actual=$(sed -n "${number}p" "$file")
	if [[ "$actual" != "$expected" ]]; then
		printf '%s\n' "FAIL: $file line $number was '$actual', expected '$expected'"
		exit 1
	fi
}

assert_terminal() {
	local scenario="$1" outcome="$2" exit_code="$3" expected_status="$4"
	local stdout="$d/${scenario}.stdout" stderr="$d/${scenario}.stderr"
	run_companion "$scenario" "$stdout" "$stderr" "$PLUGIN_DATA"
	[[ $RUN_STATUS -eq $expected_status && ! -s "$stderr" ]]
	assert_line "$stdout" 1 "$START_COMPANION"
	assert_line "$stdout" 2 \
		"LOOM-FORWARD-END {\"v\":1,\"backend\":\"companion\",\"job_id\":\"job-exact\",\"outcome\":\"$outcome\",\"exit_code\":$exit_code}"
	assert_line "$stdout" 3 "$SEPARATOR"
}

# The exact job record wins even when newer decoys exist, and redirect notes
# stay after the separator. This run also preserves the launch argv contract.
BLOCKER="$d/blocker"
printf '%s\n' blocker >"$BLOCKER"
REDIRECTED_ROOT="$HOME_DIR/.codex/plugin-data/state"
EXACT_RECORD="$REDIRECTED_ROOT/workspace-hash/jobs/job-exact.json"
mkdir -p "$(dirname "$EXACT_RECORD")" "$REDIRECTED_ROOT/newer-workspace/jobs"
printf '%s\n' '{}' >"$EXACT_RECORD"
printf '%s\n' '{}' >"$REDIRECTED_ROOT/workspace-hash/jobs/job-decoy.json"
printf '%s\n' '{}' >"$REDIRECTED_ROOT/newer-workspace/jobs/job-newer.json"
touch -t 203001010000 "$REDIRECTED_ROOT/workspace-hash/jobs/job-decoy.json" \
	"$REDIRECTED_ROOT/newer-workspace/jobs/job-newer.json"

OUT="$d/completed.stdout"
ERR="$d/completed.stderr"
run_companion completed "$OUT" "$ERR" "$BLOCKER/plugin"
[[ $RUN_STATUS -eq 0 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$START_COMPANION"
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"job-exact","outcome":"succeeded","exit_code":0}'
assert_line "$OUT" 3 "$SEPARATOR"
rg -qF "record: $EXACT_RECORD" "$OUT"
rg -qF 'job: job-exact' "$OUT"
rg -qF 'note: plugin data root not writable;' "$OUT"
if [[ $(rg -nF 'note: plugin data root not writable;' "$OUT" | cut -d: -f1) -le 3 ]]; then
	printf '%s\n' 'FAIL: redirect note appeared before the separator'
	exit 1
fi

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
rg -qF 'loom map --find-all' "$CAPTURE_LAUNCH"
rg -qF 'NEVER run git' "$CAPTURE_LAUNCH"
[[ ! -e "$d/operator" ]]

# Provider marker-shaped text is data only because it follows the separator.
fake_line=$(rg -nF '"job_id":"fake"' "$OUT" | cut -d: -f1)
[[ -n "$fake_line" && $fake_line -gt 3 ]]

mkdir -p "$PLUGIN_DATA/state/workspace-hash/jobs"
printf '%s\n' '{}' >"$PLUGIN_DATA/state/workspace-hash/jobs/job-exact.json"
assert_terminal failed failed 1 1
assert_terminal cancelled canceled 1 1
assert_terminal repoll succeeded 0 0
[[ $(wc -l <"$STATUS_CALLS") -eq 2 ]]
if rg -qvFx 'job-exact --wait --json' "$STATUS_CALLS"; then
	printf '%s\n' 'FAIL: status polling changed the exact job id or argv'
	exit 1
fi

# A failed exact-id wait leaves START visible but must not manufacture END.
OUT="$d/wait-fail.stdout"
ERR="$d/wait-fail.stderr"
run_companion wait-fail "$OUT" "$ERR" "$PLUGIN_DATA"
[[ $RUN_STATUS -eq 7 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$START_COMPANION"
assert_line "$OUT" 2 "$SEPARATOR"
if rg -q '^LOOM-FORWARD-END ' "$OUT"; then
	printf '%s\n' 'FAIL: wait failure emitted a terminal marker'
	exit 1
fi
rg -qF 'simulated wait failure' "$OUT"
rg -qF 'job: job-exact' "$OUT"

# An unsafe launch id is rejected before START and cannot select a record.
OUT="$d/invalid-id.stdout"
ERR="$d/invalid-id.stderr"
run_companion invalid-id "$OUT" "$ERR" "$PLUGIN_DATA"
[[ $RUN_STATUS -eq 1 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$SEPARATOR"
if rg -q '^LOOM-FORWARD-' "$OUT"; then
	printf '%s\n' 'FAIL: invalid jobId emitted a marker'
	exit 1
fi
rg -qF 'job: none' "$OUT"

# Direct lane: capture this child only, close stdin, and defer the Seatbelt note.
DIRECT_BIN="$d/direct-bin"
mkdir -p "$DIRECT_BIN"
cat >"$DIRECT_BIN/sandbox-exec" <<'STUB'
#!/usr/bin/env bash
exit 71
STUB
cat >"$DIRECT_BIN/node" <<'STUB'
#!/usr/bin/env bash
touch "$NODE_CALLED"
exit 95
STUB
cat >"$DIRECT_BIN/codex" <<'STUB'
#!/usr/bin/env bash
printf '%q\n' "$@" >"$DIRECT_ARGV"
[[ /dev/stdin -ef /dev/null ]] || exit 66
if [[ "${DIRECT_SCENARIO:-success}" != missing-thread ]]; then
	printf '%s\n' '{"type":"thread.started","thread_id":"thread-exact"}'
fi
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"direct final"}}'
if [[ "${DIRECT_SCENARIO:-success}" == failed ]]; then
	exit 9
fi
STUB
chmod +x "$DIRECT_BIN/node" "$DIRECT_BIN/codex" "$DIRECT_BIN/sandbox-exec"

run_direct() {
	local scenario="$1" stdout="$2" stderr="$3"
	RUN_STATUS=0
	HOME="$HOME_DIR" PATH="$DIRECT_BIN:$PATH" DIRECT_SCENARIO="$scenario" \
		DIRECT_ARGV="$d/direct-argv" NODE_CALLED="$d/node-called" \
		bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write \
			<"$d/stdin-open" >"$stdout" 2>"$stderr" || RUN_STATUS=$?
}
printf '%s\n' 'caller stdin' >"$d/stdin-open"

OUT="$d/direct.stdout"
ERR="$d/direct.stderr"
run_direct success "$OUT" "$ERR"
[[ $RUN_STATUS -eq 0 && ! -s "$ERR" && ! -e "$d/node-called" ]]
assert_line "$OUT" 1 'LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"thread-exact"}'
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"succeeded","exit_code":0}'
assert_line "$OUT" 3 "$SEPARATOR"
rg -qF 'thread: thread-exact' "$OUT"
rg -qF 'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)' "$OUT"
rg -qF 'note: the outer sandbox refuses a nested Seatbelt profile;' "$OUT"
assert_line "$d/direct-argv" 1 exec
rg -qFx -- '--json' "$d/direct-argv"
rg -qFx 'danger-full-access' "$d/direct-argv"
rg -qF 'model_reasoning_effort=xhigh' "$d/direct-argv"

OUT="$d/direct-failed.stdout"
ERR="$d/direct-failed.stderr"
run_direct failed "$OUT" "$ERR"
[[ $RUN_STATUS -eq 9 && ! -s "$ERR" ]]
assert_line "$OUT" 2 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"failed","exit_code":9}'

# Exit zero without thread.started is unknown: no markers and a nonzero exit.
OUT="$d/direct-missing.stdout"
ERR="$d/direct-missing.stderr"
run_direct missing-thread "$OUT" "$ERR"
[[ $RUN_STATUS -ne 0 && ! -s "$ERR" ]]
assert_line "$OUT" 1 "$SEPARATOR"
if rg -q '^LOOM-FORWARD-' "$OUT"; then
	printf '%s\n' 'FAIL: missing thread.started emitted a marker'
	exit 1
fi
rg -qF 'thread: none observed' "$OUT"

# Usage remains an argv error: exit 2 and no protocol markers.
OUT="$d/usage.stdout"
ERR="$d/usage.stderr"
status=0
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" bash "$WRAPPER" task hello --model unsupported \
	--effort xhigh --write >"$OUT" 2>"$ERR" || status=$?
[[ $status -eq 2 && ! -s "$OUT" ]]
if rg -q 'LOOM-FORWARD-' "$ERR"; then
	printf '%s\n' 'FAIL: argv error emitted protocol markers'
	exit 1
fi

printf '%s\n' 'PASS'
