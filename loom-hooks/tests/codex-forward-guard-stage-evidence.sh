#!/usr/bin/env bash
# The forwarding policy applies only inside a loom stage, and only unforgeable
# signals may decide that: the hook's own environment, an ancestor's
# environment, or sandbox confinement. Outside a stage the stock Codex plugin
# must stay usable, so the guard allows every payload.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-stage-evidence.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
HOME_DIR="$d/home"
mkdir -p "$HOME_DIR"
BASH_BIN=$(command -v bash)
JQ_BIN=$(command -v jq)
# env -i leaves the guard nothing to resolve jq or timeout with.
CLEAN_PATH="$(dirname "$JQ_BIN"):/usr/bin:/bin"

payload_for() {
	jq -nc --arg tool "$1" --arg agent_type "$2" \
		'{tool_name:$tool,tool_input:{file_path:"/tmp/x.rs",old_string:"a",new_string:"b"},
		  agent_type:$agent_type,agent_id:"rescue-agent",session_id:"parent-session",
		  tool_use_id:"tool-1",cwd:"/tmp"}'
}

# A PATH that can still source the hook's siblings but cannot resolve jq.
NOJQ_DIR="$d/nojq"
mkdir -p "$NOJQ_DIR"
for tool in dirname cat tr date uname ps; do
	tool_path=$(command -v "$tool" 2>/dev/null) || continue
	ln -s "$tool_path" "$NOJQ_DIR/$tool"
done

RESCUE_EDIT=$(payload_for Edit codex:codex-rescue)
NO_METADATA=$(jq -nc '{tool_name:"Edit",tool_input:{file_path:"/tmp/x.rs"},agent_type:"",transcript_path:""}')

# run_guard <payload> [VAR=VALUE ...] - guard under a scrubbed environment plus
# the named variables, so only what the case builds can be evidence.
run_guard() {
	local input="$1"
	shift
	CODE=0
	printf '%s' "$input" | env -i PATH="$CLEAN_PATH" HOME="$HOME_DIR" "$@" \
		"$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
}

assert_code() {
	local want="$1" label="$2"
	[[ $CODE -eq $want ]] || {
		printf 'FAIL: %s: expected exit %s, got %s\n' "$label" "$want" "$CODE"
		tail -n 20 "$d/stderr"
		exit 1
	}
}

assert_silent() {
	local label="$1"
	[[ ! -s "$d/stdout" && ! -s "$d/stderr" ]] || {
		printf 'FAIL: %s: expected no output\n' "$label"
		tail -n 20 "$d/stdout" "$d/stderr"
		exit 1
	}
}

assert_stderr() {
	local label="$1" needle="$2"
	rg -qF "$needle" "$d/stderr" || {
		printf 'FAIL: %s: stderr does not mention %s\n' "$label" "$needle"
		tail -n 20 "$d/stderr"
		exit 1
	}
}

# The no-evidence cases are honest only when this test process is itself
# outside any stage. The check reads `ps` and probes confinement separately
# from the guard's own library, so a defect in that library cannot quietly
# excuse the cases that depend on evidence being absent.
own_stage_evidence() {
	local pid=$$ depth=0 name comm=""
	for name in LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR; do
		if [[ -n "${!name:-}" ]]; then
			printf 'the test process environment (%s)' "$name"
			return 0
		fi
	done
	while ((depth < 12)); do
		if ps eww -o command= -p "$pid" 2>/dev/null |
			rg -q '(^| )LOOM_(STAGE_ID|SESSION_ID|WORK_DIR)=.'; then
			printf 'the environment of ancestor pid %s' "$pid"
			return 0
		fi
		pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ') || true
		[[ "$pid" =~ ^[0-9]+$ ]] || break
		((pid > 1)) || break
		depth=$((depth + 1))
	done
	if [[ -r /proc/1/comm ]]; then
		read -r comm </proc/1/comm || true
		if [[ "$comm" == bwrap ]]; then
			printf 'a bubblewrap sandbox around the test'
			return 0
		fi
	fi
	if [[ "$(uname -s)" == Darwin ]] &&
		! sandbox-exec -p '(version 1)(allow default)' /usr/bin/true >/dev/null 2>&1; then
		printf 'a Seatbelt sandbox around the test'
		return 0
	fi
	return 1
}

AMBIENT=$(own_stage_evidence || true)
if [[ -n "$AMBIENT" ]]; then
	printf 'SKIP: stage evidence present in the test'"'"'s own ancestry (%s)\n' "$AMBIENT"
else
	# No evidence: a rescue shim doing its own editing is none of this guard's
	# business outside a stage.
	run_guard "$RESCUE_EDIT"
	assert_code 0 'no evidence, forwarder Edit'
	assert_silent 'no evidence, forwarder Edit'

	# The fail-closed metadata check is part of the same policy, so it too is
	# inert outside a stage.
	run_guard "$NO_METADATA"
	assert_code 0 'no evidence, no classification metadata'
	assert_silent 'no evidence, no classification metadata'

	# A missing jq leaves the payload unreadable, which fails closed only where
	# there is a policy to fail closed about.
	CODE=0
	printf '%s' "$RESCUE_EDIT" | env -i PATH="$NOJQ_DIR" HOME="$HOME_DIR" \
		"$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	assert_code 0 'no evidence, jq unavailable'
	assert_silent 'no evidence, jq unavailable'
fi

# E1: the hook's own environment.
run_guard "$RESCUE_EDIT" LOOM_SESSION_ID=session-1
assert_code 2 'E1 environment'
assert_stderr 'E1 environment' 'forwarders may use Bash only'
if rg -qF 'classified as a loom stage by' "$d/stderr"; then
	printf 'FAIL: E1 environment: the guard own environment needs no evidence note\n'
	exit 1
fi

CODE=0
printf '%s' "$RESCUE_EDIT" | env -i PATH="$NOJQ_DIR" HOME="$HOME_DIR" \
	LOOM_SESSION_ID=session-1 "$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
assert_code 2 'E1 environment, jq unavailable'
assert_stderr 'E1 environment, jq unavailable' 'jq is not installed'

# E2: an ancestor's environment, with the guard's own environment scrubbed.
# This is the nested-session masquerade: `env -i claude ...` launched from a
# stage gives the nested session's hooks nothing of loom's, but the stage
# identity is still above them in the process tree.
cat >"$d/ancestor.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
code=0
printf '%s' "\$1" | env -i PATH="$CLEAN_PATH" HOME="$HOME_DIR" \\
	"$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || code=\$?
# No exec above: this process has to stay alive as the guard's parent.
exit "\$code"
EOF
CODE=0
LOOM_STAGE_ID=ancestor-stage "$BASH_BIN" "$d/ancestor.sh" "$RESCUE_EDIT" || CODE=$?
assert_code 2 'E2 ancestor environment'
assert_stderr 'E2 ancestor environment' 'forwarders may use Bash only'
assert_stderr 'E2 ancestor environment' 'ancestor pid'

if [[ -n "$AMBIENT" ]]; then
	printf 'SKIP: stage evidence present in the test'"'"'s own ancestry (%s)\n' "$AMBIENT"
else
	# E2 negative: LOOM_TERMINAL travels further than a stage, so it is not
	# evidence of one.
	CODE=0
	LOOM_TERMINAL=x "$BASH_BIN" "$d/ancestor.sh" "$RESCUE_EDIT" || CODE=$?
	assert_code 0 'E2 negative, LOOM_TERMINAL ancestor'
	assert_silent 'E2 negative, LOOM_TERMINAL ancestor'
fi

# E3 on Linux: Claude Code's Bash sandbox is bubblewrap with its own pid
# namespace, where the ancestry walk cannot reach the host session.
if [[ "$(uname -s)" != Linux ]]; then
	printf 'SKIP: the bubblewrap confinement case is Linux-only\n'
elif ! BWRAP_BIN=$(command -v bwrap); then
	printf 'SKIP: bwrap is not installed, so real confinement cannot be built\n'
elif ! "$BWRAP_BIN" --ro-bind / / --proc /proc --dev /dev --unshare-pid \
	/bin/true 2>"$d/bwrap.err"; then
	printf 'SKIP: unprivileged bubblewrap was refused: %s\n' "$(tail -n 1 "$d/bwrap.err")"
else
	CODE=0
	printf '%s' "$RESCUE_EDIT" | env -i "$BWRAP_BIN" --ro-bind / / --proc /proc \
		--dev /dev --unshare-pid --setenv PATH "$CLEAN_PATH" --setenv HOME "$HOME_DIR" \
		"$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	assert_code 2 'E3 bubblewrap confinement'
	assert_stderr 'E3 bubblewrap confinement' 'sandbox confinement (bubblewrap)'
fi

# E3 on macOS: a Seatbelt profile is inherited by every descendant, so it still
# answers for a process that double-forked out of the ancestry chain.
if [[ "$(uname -s)" != Darwin ]]; then
	printf 'SKIP: the Seatbelt confinement case is macOS-only\n'
else
	CODE=0
	printf '%s' "$RESCUE_EDIT" | env -i PATH="$CLEAN_PATH" HOME="$HOME_DIR" \
		/usr/bin/sandbox-exec -p '(version 1)(allow default)' \
		"$BASH_BIN" "$HOOK" >"$d/stdout" 2>"$d/stderr" || CODE=$?
	assert_code 2 'E3 Seatbelt confinement'
	assert_stderr 'E3 Seatbelt confinement' 'sandbox confinement (seatbelt)'
fi

printf '%s\n' PASS
