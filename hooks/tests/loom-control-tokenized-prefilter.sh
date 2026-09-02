#!/usr/bin/env bash
# The completion bridge's pre-filter decides which commands are held to the
# exact pinned form. It used to glob the RAW command string, which was wrong in
# both directions: `loom stage comple"te" x` carries no literal verb, so the
# whole bridge was skipped while bash still ran the completion; and the same
# words appearing inside quoted prose or a path argument matched, blocking
# unrelated commands. It now scans argv VALUES from loom_tokenize_command.
#
# This covers the three properties that pin that fix: the pinned command is
# still accepted, quote-obfuscated forgeries are caught, and the two
# false-positive classes are cured. The pre-existing sibling test covers the
# rest of the bridge (trusted-binary resolution, marker, broker route).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/loom-control-complete.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WORKTREE="$TMP/repo/.worktrees/build-api"
mkdir -p "$TMP/bin" "$WORKTREE"
LOG="$TMP/broker.log"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$BROKER_LOG"
SH
chmod +x "$TMP/bin/loom"

PINNED="$TMP/bin/loom stage complete build-api"

# Exits 0 when the hook allowed the command, non-zero when it blocked it.
invoke_hook() {
	jq -n --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}' |
		env PATH="$TMP/bin:/usr/bin:/bin" \
			BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" \
			LOOM_STAGE_ID="build-api" LOOM_SESSION_ID="session-123" \
			LOOM_WORKTREE_PATH="$WORKTREE" bash "$HOOK" >/dev/null 2>&1
}

expect_reaches_pin() {
	local command=$1 label=$2
	if invoke_hook "$command"; then
		echo "FAIL: $label was waved through instead of being pinned: $command" >&2
		exit 1
	fi
}

expect_allowed() {
	local command=$1 label=$2
	if ! invoke_hook "$command"; then
		echo "FAIL: $label was blocked: $command" >&2
		exit 1
	fi
}

# 1. REGRESSION: the pinned command itself must still reach the pin AND be
# accepted by it. This is the path that has to keep working.
expect_allowed "$PINNED" "the pinned command"

# 2. THE FORGERY: quoting evaded the old raw-string glob while bash still built
# the argv [stage] [complete] [build-api]. Each of these must now reach the pin,
# which rejects them for not being the one exact pinned string.
expect_reaches_pin 'loom stage comple"te" build-api' "a split-quoted verb"
expect_reaches_pin "loom stage comple'te' build-api" "a single-quoted verb fragment"
expect_reaches_pin 'loom stage "complete" build-api' "a fully quoted verb"
expect_reaches_pin 'loom stage co"mpl"ete build-api' "a doubly split verb"
expect_reaches_pin 'loom "stage" complete build-api' "a quoted subcommand"
expect_reaches_pin 'LOOM_FORGE=1 loom stage complete build-api' "a leading env assignment"
expect_reaches_pin "$TMP/bin/loom stage comple\"te\" build-api" "a forged verb on the pinned path"

# 3. THE FALSE POSITIVES: the words inside ONE quoted argument, and this hook's
# own filename as a path argument, are not completion attempts. The raw glob
# blocked both; tokenizing must not.
expect_allowed 'loom memory note "gotcha: never run loom stage complete early"' \
	"a memory note quoting the verb"
expect_allowed "loom memory note 'mistake: stage complete ran before the subagents returned'" \
	"a single-quoted memory note"
expect_allowed 'rg -n "pre_tool_hooks" hooks/loom-control-complete.sh' \
	"an rg naming this hook's file"
expect_allowed 'cat hooks/loom-control-complete.sh' "a path argument naming this hook"
expect_allowed 'git commit -m "fix: complete the loom stage guard"' \
	"a commit message quoting the words"
expect_allowed 'loom stage list' "an unrelated loom stage subcommand"
expect_allowed 'echo done' "an unrelated command"

# Nothing above may reach the completion broker.
[[ ! -e "$LOG" ]] || {
	echo "FAIL: the pre-filter cases reached the broker: $(cat "$LOG")" >&2
	exit 1
}

echo "PASS"
