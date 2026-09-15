#!/usr/bin/env bash
# loom-relay.sh kind derivation: only a command segment whose effective command
# word is exactly `loom`, running a relaying subcommand, contributes its kind -
# seen through VAR= prefixes, wrappers, `&&`, pipes and `sh -c` - while quoted
# prose, heredoc bodies and other commands contribute nothing, and a subagent
# never gets a control kind.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-relay.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-relaytest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STUB="$TMP/bin/loom"
LOG="$TMP/stub.log"
mkdir -p "$TMP/bin" "$TMP/scratch/session-1"
cat >"$STUB" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >"$STUB_LOG"
cat >/dev/null
SH
chmod +x "$STUB"

LINE='LOOM_RELAY_V1 kind=memory id=0123456789abcdef0123456789abcdef sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 bytes=42'

# allowed_for <command> [agent_type] - the --allowed-kinds value the hook
# handed the helper for <command>.
allowed_for() {
	local input argv
	input=$(jq -nc --arg c "$1" --arg o "$LINE" --arg a "${2:-}" \
		'{tool_name: "Bash", tool_input: {command: $c}, tool_response: {stdout: $o}}
		+ (if $a == "" then {} else {agent_type: $a} end)')
	rm -f "$LOG"
	printf '%s' "$input" | env -i HOME="$TMP" PATH="$PATH" STUB_LOG="$LOG" \
		LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$TMP/scratch/session-1" LOOM_BIN="$STUB" \
		bash "$HOOK" >/dev/null
	if [[ ! -e "$LOG" ]]; then
		printf '<helper not called>'
		return 0
	fi
	argv=$(<"$LOG")
	printf '%s' "${argv#hook relay --allowed-kinds }"
}

FAILED=0
# expect <kinds> <command> [agent_type]
expect() {
	local got
	got=$(allowed_for "$2" "${3:-}")
	if [[ "$got" != "$1" ]]; then
		echo "FAIL: [$2] agent='${3:-}': expected '$1', got '$got'" >&2
		FAILED=1
	fi
}

expect memory 'loom memory note "x"'
expect memory 'loom memory decision "chose x" --context "y"'
expect memory 'loom memory change "x"'
expect memory 'loom memory question "x"'
expect memory 'loom memory resolve 0123456789abcdef --outcome applied --target doc/loom/knowledge/mistakes.md'
expect '' 'loom memory pending'
expect '' 'loom memory list'
expect block 'loom stage block stage-a --reason "stuck"'
expect dispute 'loom stage dispute-criteria stage-a --criterion 2 --reason "flaky"'
expect handoff 'loom handoff --stage stage-a --session s --trigger ceiling'
expect merge-resolved 'loom stage merge stage-a --resolved'
expect '' 'loom stage merge stage-a'
expect verdict 'loom stage adjudicate --stage stage-a --dispute 1'
expect telemetry 'loom knowledge context --query "x" --budget-tokens 800'
expect memory 'cd loom && loom memory note x'
expect memory 'FOO=1 loom memory note x'
expect memory 'env FOO=1 timeout 30 loom memory note x'
expect memory "bash -c 'loom memory note x'"
expect memory '/home/u/.local/bin/loom memory note x'
expect memory 'loom memory note x 2>&1 | tail -5'
expect memory,block 'loom memory note x; loom stage block stage-a'
expect '' 'cat output.txt'
expect '' 'echo "loom memory note x"'
expect '' 'loom-link memory note x'
expect '' 'rg -n loom memory'
expect '' $'cat <<\'EOF\'\nloom memory note x\nEOF'
# A subagent never relays a control kind; its memory and telemetry still flow.
expect '' 'loom stage block stage-a --reason r' general-purpose
expect memory 'loom memory note x; loom handoff --trigger ceiling' general-purpose
expect telemetry 'loom knowledge context --query x' general-purpose

[[ $FAILED -eq 0 ]] || exit 1
echo "loom-relay kinds: ok"
