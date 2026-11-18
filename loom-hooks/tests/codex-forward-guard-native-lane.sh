#!/usr/bin/env bash
# With codex.backend resolving to native (`loom hook codex-backend`, stubbed
# here) no caller may run the forwarding wrapper; the refusal names
# `loom subagents start`. A legacy or unknown backend and other commands keep
# the guard's existing behavior.
set -euo pipefail
unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID LOOM_BIN
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-native.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT
mkdir -p "$d/home"
cat >"$d/loom" <<'STUB'
#!/usr/bin/env bash
[[ "$1 $2" == "hook codex-backend" ]] || exit 0
[[ "$CODEX_BACKEND" != __FAIL__ ]] || exit 1
printf '%s\n' "$CODEX_BACKEND"
STUB
chmod +x "$d/loom"

# shellcheck disable=SC2088 # the literal tilde is the spelling an agent types
FORWARD="~/.claude/hooks/loom/codex-forward.sh task 'Read /x/brief.md' --model gpt-6.1-terra --effort xhigh --write"
payload() {
	jq -nc --arg command "$1" \
		'{tool_name:"Bash",tool_input:{command:$command},agent_type:"",transcript_path:"/tmp/session-main.jsonl"}'
}
run_guard() {
	local backend="$1" command="$2"
	set +e
	OUT=$(payload "$command" | HOME="$d/home" LOOM_SESSION_ID=native-lane-session \
		LOOM_BIN="$d/loom" CODEX_BACKEND="$backend" bash "$HOOK" 2>&1 >/dev/null)
	CODE=$?
	set -e
}

run_guard native "$FORWARD"
if [[ $CODE -ne 2 || "$OUT" != *"loom subagents start --provider codex --unit-id <unit>"* ]]; then
	echo "FAIL: a native backend must refuse a main-session forward with the start command (exit $CODE): $OUT"
	exit 1
fi
# The native lane is the default backend, so the refusal wording is pinned.
if [[ "$OUT" != *"BLOCKED: a codex-forward.sh call is the legacy Codex lane; this stage runs the native lane (codex.backend = native)."* ]]; then
	echo "FAIL: the native refusal must open with the BLOCKED lane message: $OUT"
	exit 1
fi

run_guard native "rg -n codex-forward.sh loom-hooks"
if [[ $CODE -ne 0 ]]; then
	echo "FAIL: a native backend must allow a command that only mentions the wrapper, got exit $CODE: $OUT"
	exit 1
fi

run_guard native "git status --short"
if [[ $CODE -ne 0 || -n "$OUT" ]]; then
	echo "FAIL: a native backend must leave a command that never names the wrapper untouched, got exit $CODE: $OUT"
	exit 1
fi

for backend in legacy __FAIL__; do
	run_guard "$backend" "$FORWARD"
	if [[ $CODE -ne 0 ]]; then
		echo "FAIL: backend '$backend' must keep a main-session forward untouched, got exit $CODE: $OUT"
		exit 1
	fi
done
echo "PASS"
