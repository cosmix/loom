#!/usr/bin/env bash
# loom-relay.sh gates: the pure-bash fast path, the jq requirement, the
# LOOM_HOOK_PATH pin, the forwarder and tokenizer refusals, and the helper call
# contract (argv, stdin, LOOM_HOOK_CONTEXT, failure reporting). Every hook run
# gets a scrubbed environment (env -i), so nothing leaks in from a loom session.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-relay.sh"
source "$ROOT/loom-hooks/tests/_path_without.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-relaytest.XXXXXX")
NOJQ=$(path_without jq)
trap 'rm -rf "$TMP" "$NOJQ"' EXIT

STUB="$TMP/bin/loom"
LOG="$TMP/stub.log"
MARK="$TMP/fake-jq-ran"
mkdir -p "$TMP/bin" "$TMP/fakebin" "$TMP/scratch/session-1"
cat >"$STUB" <<'SH'
#!/usr/bin/env bash
printf 'argv=%s context=%s\n' "$*" "${LOOM_HOOK_CONTEXT:-}" >>"$STUB_LOG"
cat >"$STUB_LOG.stdin"
printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"stub reply"}}'
exit "${STUB_EXIT:-0}"
SH
printf '#!/usr/bin/env bash\n: >"%s"\nexit 1\n' "$MARK" >"$TMP/fakebin/jq"
chmod +x "$STUB" "$TMP/fakebin/jq"

LINE='LOOM_RELAY_V1 kind=memory id=0123456789abcdef0123456789abcdef sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 bytes=42'

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# payload <command> <stdout> [agent_type] [transcript_path]
payload() {
	jq -nc --arg c "$1" --arg o "$2" --arg a "${3:-}" --arg t "${4:-}" \
		'{tool_name: "Bash", tool_input: {command: $c}, tool_response: {stdout: $o, stderr: ""}, tool_use_id: "toolu_1"}
		+ (if $a == "" then {} else {agent_type: $a} end)
		+ (if $t == "" then {} else {transcript_path: $t} end)'
}

# run_hook <payload> [VAR=value...] - the hook in a relay-mode session with a
# scrubbed environment; later assignments override the defaults.
run_hook() {
	local input=$1
	shift
	printf '%s' "$input" | env -i HOME="$TMP" PATH="$PATH" STUB_LOG="$LOG" \
		LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$TMP/scratch/session-1" LOOM_BIN="$STUB" \
		${1+"$@"} bash "$HOOK"
}

context_of() { printf '%s' "$1" | jq -r '.hookSpecificOutput.additionalContext // empty'; }
reset() { rm -f "$LOG" "$LOG.stdin" "$MARK"; }

MEMORY=$(payload 'loom memory note "x"' "noted
$LINE")

# 1. With jq removed from PATH, a plain payload exits 0 silently.
out=$(run_hook "$(payload 'ls' 'README.md')" PATH="$NOJQ") || fail "plain payload without jq exited non-zero"
[[ -z "$out" && ! -e "$LOG" ]] || fail "plain payload without jq was not silent: $out"

# 2. Without jq a relay line is answered with a fixed message, never guessed at.
out=$(run_hook "$MEMORY" PATH="$NOJQ") || fail "missing jq exited non-zero"
[[ "$(context_of "$out")" == *"jq"* ]] || fail "missing-jq message: $out"
[[ ! -e "$LOG" ]] || fail "helper ran without jq"

# 3. Outside a relay-mode session the hook is inert.
out=$(run_hook "$MEMORY" LOOM_SCRATCH_DIR=) || fail "no-scratch session exited non-zero"
[[ -z "$out" && ! -e "$LOG" ]] || fail "hook acted without LOOM_SCRATCH_DIR: $out"

# 4. A fake jq first on the inherited PATH never runs once LOOM_HOOK_PATH is set;
#    the helper gets the payload on stdin, its kinds, and LOOM_HOOK_CONTEXT=1.
out=$(run_hook "$MEMORY" PATH="$TMP/fakebin:$PATH" LOOM_HOOK_PATH="$PATH") || fail "pinned PATH run exited non-zero"
[[ ! -e "$MARK" ]] || fail "the fake jq on the inherited PATH ran despite LOOM_HOOK_PATH"
[[ "$(context_of "$out")" == "stub reply" ]] || fail "the helper's reply was not printed: $out"
rg -qx 'argv=hook relay --allowed-kinds memory context=1' "$LOG" || fail "helper argv/context: $(<"$LOG")"
[[ "$(<"$LOG.stdin")" == "$MEMORY" ]] || fail "the helper did not receive the payload on stdin"
reset
# Control: without the pin the same fake jq does run, so the check above bites.
run_hook "$MEMORY" PATH="$TMP/fakebin:$PATH" >/dev/null || fail "unpinned run exited non-zero"
[[ -e "$MARK" ]] || fail "control: the fake jq should run when LOOM_HOOK_PATH is unset"
reset

# 5. Codex forwarder output is never relayed: by agent_type, and by the
#    transcript sentinel when agent_type is absent.
for agent in loom-codex-forwarder codex:codex-rescue; do
	out=$(run_hook "$(payload 'loom memory note x' "$LINE" "$agent")") || fail "forwarder $agent exited non-zero"
	[[ "$(context_of "$out")" == *"codex forwarder"* ]] || fail "forwarder $agent: $out"
	[[ ! -e "$LOG" ]] || fail "the helper ran for forwarder $agent"
done
transcript="$TMP/proj/subagents/agent-1.jsonl"
mkdir -p "${transcript%/*}"
printf '%s\n' '{"message":"LOOM-CODEX-FORWARD-ONLY"}' >"$transcript"
out=$(run_hook "$(payload 'loom memory note x' "$LINE" '' "$transcript")") || fail "sentinel run exited non-zero"
[[ "$(context_of "$out")" == *"codex forwarder"* ]] || fail "sentinel transcript: $out"
[[ ! -e "$LOG" ]] || fail "the helper ran for a sentinel transcript"
printf '%s\n' '{"message":"ordinary subagent"}' >"$transcript"
run_hook "$(payload 'loom memory note x' "$LINE" '' "$transcript")" >/dev/null || fail "plain transcript exited non-zero"
[[ -e "$LOG" ]] || fail "a plain subagent transcript blocked the relay"
reset

# 6. A command the tokenizer cannot parse: no helper call, rerun it alone.
out=$(run_hook "$(payload 'loom memory note "unterminated' "$LINE")") || fail "tokenizer failure exited non-zero"
[[ "$(context_of "$out")" == *"Rerun the loom command alone"* ]] || fail "tokenizer failure: $out"
[[ ! -e "$LOG" ]] || fail "the helper ran for an unparseable command"

# 7. A persisted-output payload reaches the helper without any inline line.
persisted=$(jq -nc '{tool_name: "Bash", tool_input: {command: "loom memory note x"},
	tool_response: {stdout: "preview", persistedOutputPath: "/nowhere/tool-results/out.txt"}}')
run_hook "$persisted" >/dev/null || fail "persisted payload exited non-zero"
[[ -e "$LOG" ]] || fail "a persisted-output payload skipped the helper"
reset

# 8. Other tools are ignored even when their output carries a line.
out=$(run_hook "$(jq -nc --arg o "$LINE" '{tool_name: "Read", tool_input: {file_path: "x"}, tool_response: {stdout: $o}}')") ||
	fail "non-Bash payload exited non-zero"
[[ -z "$out" && ! -e "$LOG" ]] || fail "a non-Bash payload was relayed: $out"

# 9. A failing helper is reported, never silent; so is a missing binary.
out=$(run_hook "$MEMORY" STUB_EXIT=3) || fail "failing helper exited non-zero"
[[ "$(context_of "$out")" == *"exit 3"* ]] || fail "helper failure: $out"
reset
out=$(run_hook "$MEMORY" LOOM_BIN="$TMP/missing/loom") || fail "missing binary exited non-zero"
[[ "$(context_of "$out")" == *"was not found"* ]] || fail "missing binary: $out"

# 10. Every hook that runs loom pins PATH before any other statement.
for hook in loom-relay.sh loom-control-complete.sh ask-user-pre.sh ask-user-post.sh session-end.sh \
	pre-compact.sh user-prompt-context.sh codex-apply-patch.sh post-tool-use.sh _read_discipline.sh \
	_read_ledger.sh spawn-guard.sh subagent-start.sh; do
	first=""
	while IFS= read -r code; do
		[[ -z "$code" || "$code" == \#* ]] && continue
		first=$code
		break
	done <"$ROOT/loom-hooks/$hook"
	[[ "$first" == 'PATH="${LOOM_HOOK_PATH:-$PATH}"' ]] || fail "$hook: first statement is not the LOOM_HOOK_PATH pin: $first"
done

echo "loom-relay gates: ok"
