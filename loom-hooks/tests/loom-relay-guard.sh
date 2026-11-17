#!/usr/bin/env bash
# loom-relay-guard.sh: a Bash call that would hide the LOOM_RELAY_V1 line of a
# guarded loom command (backgrounded, redirected, captured, piped into anything
# but a closing `tail -N`, or piped through a filter on the relay line) is
# refused with exit 2 and a block-ledger line; every other spelling passes.
# Every case runs under `env -i`: an inherited LOOM_WORK_DIR or LOOM_STAGE_ID
# would write the live block ledger.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-relay-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-relayguard.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scratch" "$TMP/work"
LEDGER="$TMP/work/heartbeat/stage-a.blocks.jsonl"

FAILED=0

# payload <command> [background] - A PreToolUse Bash payload.
payload() {
	jq -nc --arg c "$1" --arg b "${2:-}" \
		'{tool_name: "Bash", tool_input: ({command: $c}
			+ (if $b == "true" then {run_in_background: true}
			   elif $b == "string" then {run_in_background: "true"} else {} end))}'
}

# run_guard <payload> - Print the guard's exit status; its stderr lands in
# $TMP/stderr. The environment is the guard's own and nothing inherited.
run_guard() {
	local rc=0
	printf '%s' "$1" | env -i HOME="$TMP" PATH="$PATH" \
		LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$TMP/scratch" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID=stage-a \
		bash "$HOOK" 2>"$TMP/stderr" >/dev/null || rc=$?
	printf '%s' "$rc"
}

# blocked <command> [background] - The guard must exit 2 and say why.
blocked() {
	local got
	got=$(run_guard "$(payload "$1" "${2:-}")")
	if [[ "$got" != 2 ]]; then
		echo "FAIL: [$1] bg='${2:-}': expected exit 2, got $got" >&2
		FAILED=1
	elif ! grep -q 'LOOM_RELAY_V1 line on its stdout' "$TMP/stderr"; then
		echo "FAIL: [$1]: block text missing: $(cat "$TMP/stderr")" >&2
		FAILED=1
	elif ! grep -v '^[[:space:]]*$' "$TMP/stderr" | head -1 | grep -q '^\[loom-relay-guard/relay-[a-z]*\] BLOCKED:'; then
		echo "FAIL: [$1]: first stderr line lacks the [guard/rule] prefix: $(cat "$TMP/stderr")" >&2
		FAILED=1
	fi
}

# passes <command> - The guard must let the call through silently.
passes() {
	local got
	got=$(run_guard "$(payload "$1")")
	if [[ "$got" != 0 ]]; then
		echo "FAIL: [$1]: expected exit 0, got $got: $(cat "$TMP/stderr")" >&2
		FAILED=1
	fi
}

INCIDENT="loom stage dispute-contract s --contract c --reason r 2>&1 | grep -v '^LOOM_RELAY' | tail -15"
HEREDOC_COMMIT=$'loom stage commit s -m "$(cat <<\'EOF\'\nmsg\nEOF\n)" 2>&1 | rg x'

# Rule relay-stdout.
blocked "$INCIDENT"
blocked 'loom stage commit s -m x | rg -o y'
blocked "$HEREDOC_COMMIT"
blocked 'loom stage block s "x" > /dev/null'
blocked 'loom stage block s x &'
blocked 'loom stage block s x &>f'
blocked 'loom stage block s x 1>f'
blocked 'loom stage block s x >> f'
blocked 'loom stage block s x | tee f'
blocked 'loom check integration-verify > o'
blocked 'loom check integration-verify' true
blocked 'loom check integration-verify' string
blocked 'loom stage dispute-contract s --contract c --reason r' true
blocked 'loom stage commit s -m x 2>&1 | tail -3 | rg x'
blocked 'loom stage commit s -m x | tail -3 > f'
blocked 'loom stage commit s -m x | tail -n 3 > f'
blocked 'loom stage commit s -m x | tail -0'
blocked 'loom stage commit s -m x | tail -f'
blocked 'loom stage commit s -m x | tail -3 f'
blocked 'loom stage commit s -m x | tail -n3'
blocked 'loom stage commit s -m x | tail --lines=3'
blocked 'loom stage commit s -m x | head -3'
blocked 'x=$(loom stage commit s)'
blocked "bash -c 'loom stage block s x' | rg y"
blocked "bash -c 'loom stage block s x'"
blocked '(loom stage block s x)'
blocked 'echo `loom stage commit s`'
blocked '/abs/bin/loom stage commit s | rg x'
blocked 'cd d && loom stage block s x | head'
blocked 'env A=1 loom stage dispute-criteria s --criterion-index 0 --reason r | grep x'
blocked 'loom stage dispute-criteria s --field impact-tests --test t --reason r | head'
blocked 'loom stage block s x; loom stage commit s | rg y'
# A numeric argument before `>` is not a descriptor; only a glued `2>` is.
blocked 'loom check s --criterion 2 > log 2>&1'
blocked 'loom check s --criterion 3 >> log'
blocked 'loom stage dispute-criteria s --reason r --criterion-index 0 > out'
# A capture anywhere in the segment, not just right before the command.
blocked 'out=$(cd loom && loom stage commit s -m x)'
blocked 'x=`true; loom stage block s x`'
blocked $'x=$(\nloom stage commit s\n)'
blocked "bash -c 'cd d && loom stage block s x'"
# A closing `}` / `done` keeps the pipe or redirect that follows it.
blocked '{ loom stage commit s; } | rg x'
blocked "{ loom stage commit s; } 2>&1 | grep -v '^LOOM'"
blocked 'for i in 1; do loom stage block s x; done > f'
# Another command between the invocation and the closer keeps the group's fate.
blocked '{ loom stage commit s; echo ok; } | rg x'
blocked '{ loom stage block s x && true; } > f'
blocked 'if loom check s; then echo ok; fi > log'
blocked 'if x; then loom stage commit s; else :; fi | head'
blocked 'for s in a; do loom stage block $s x; echo; done > f'
blocked 'while true; do loom stage block s x; break; done | tee f'
blocked 'if x; then { loom stage commit s; echo; } 2>&1 | rg y; fi'
blocked $'{ loom stage commit s\necho ok\n} | rg x'
# Rule relay-filter.
blocked 'loom memory note a 2>&1 | grep -v LOOM_RELAY'
blocked 'loom memory note a | rg -v RELAY_V1'
blocked '{ loom memory note a; } 2>&1 | grep -v LOOM_RELAY'
blocked '{ loom memory note a; echo; } 2>&1 | grep -v LOOM_RELAY'
blocked 'for i in 1; do loom memory note a; { echo; }; done | grep -v LOOM_RELAY'
blocked 'loom knowledge context --query x | sed -n "/^LOOM/p"'

# Allowed spellings.
passes 'loom stage commit s -m x'
passes 'loom stage commit s -m x 2>&1 | tail -3'
passes 'loom stage commit s -m x 2>&1 | tail -n 3'
passes 'loom stage commit s -m x |& tail -3'
passes 'loom stage commit s -m x 2>/dev/null'
passes 'loom stage commit s -m x ; echo done | rg x'
passes 'loom stage commit s -m x && git log | tail -3'
passes 'rg LOOM_RELAY_V1 loom/src'
passes 'loom stage complete s 2>&1 | head -5'
passes 'loom memory note a | rg x'
passes 'echo LOOM_RELAY'
passes 'loom stage commit s -m "feat: a | b LOOM_RELAY"'
passes $'loom stage commit s -m "$(cat <<\'EOF\'\nfeat: a | b LOOM_RELAY\nEOF\n)"'
passes $'loom stage commit s -m "$(cat <<\'EOF\'\nfeat: a | b LOOM_RELAY\nEOF\n)" 2>&1 | tail -3'
passes $'loom stage commit s -m x\ngit log | head'
passes 'loom stage block s x && echo done | grep LOOM_RELAY'
passes 'loom memory note a'
passes 'loom check s --criterion 2 2>&1 | tail -3'
passes 'loom check s --criterion 2 2>/dev/null'
passes 'loom stage commit s -m x 2>>f'
passes '{ loom stage commit s; } 2>&1 | tail -3'
passes 'for i in 1; do loom stage block s x; done'
passes 'echo $(date); loom stage commit s'
passes 'if loom check s; then echo ok; fi'
passes '{ loom stage commit s; echo ok; }'
passes 'for s in a; do loom stage block $s x; done'
passes '{ echo a; } | rg x; loom stage commit s'
passes 'if true; then echo; fi > f; loom stage commit s 2>&1 | tail -3'
passes 'loom stage commit s; { echo; } | rg x'
passes '{ loom stage commit s; echo ok; } 2>&1 | tail -3'
passes '{ loom memory note a; echo hi | grep LOOM_RELAY; }'
passes '{ echo a; } | grep LOOM_RELAY; loom memory note a'

# The payload must be a Bash tool call, and the env gate decides.
got=$(run_guard '{"tool_name":"Read","tool_input":{"command":"loom stage commit s | rg x"}}')
[[ "$got" == 0 ]] || { echo "FAIL: non-Bash payload: exit $got" >&2; FAILED=1; }
got=$(printf '%s' "$(payload 'loom stage commit s | rg x')" | env -i HOME="$TMP" PATH="$PATH" \
	bash "$HOOK" >/dev/null 2>&1 && echo 0 || echo $?)
[[ "$got" == 0 ]] || { echo "FAIL: env gate unset: exit $got" >&2; FAILED=1; }
got=$(printf '%s' "$(payload 'loom stage commit s | rg x')" | env -i HOME="$TMP" PATH="$PATH" \
	LOOM_SESSION_ID=session-1 bash "$HOOK" >/dev/null 2>&1 && echo 0 || echo $?)
[[ "$got" == 0 ]] || { echo "FAIL: scratch gate unset: exit $got" >&2; FAILED=1; }

# A blocked call appends exactly one block-ledger line.
rm -rf "$TMP/work/heartbeat"
run_guard "$(payload "$INCIDENT")" >/dev/null
if [[ ! -f "$LEDGER" ]] || [[ "$(wc -l <"$LEDGER" | tr -d ' ')" != 1 ]]; then
	echo "FAIL: expected one ledger line in $LEDGER" >&2
	FAILED=1
elif [[ "$(jq -r '.guard + "/" + .rule' "$LEDGER")" != "loom-relay-guard/relay-stdout" ]]; then
	echo "FAIL: ledger line names the wrong guard or rule: $(cat "$LEDGER")" >&2
	FAILED=1
fi

[[ $FAILED -eq 0 ]] || exit 1
echo "loom-relay-guard: ok"
