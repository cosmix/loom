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
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
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

invoke_hook() {
	local payload
	payload=$(jq -n --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}')
	rm -f "$LOG"
	set +e
	HOOK_OUTPUT=$(printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID="build-api" \
			LOOM_SESSION_ID="session-123" LOOM_WORKTREE_PATH="$WORKTREE" \
			bash "$HOOK" 2>&1)
	HOOK_RC=$?
	set -e
}

expect_reaches_pin() {
	local command=$1 label=$2
	invoke_hook "$command"
	[[ "$HOOK_RC" == 2 ]] || {
		echo "FAIL: $label returned $HOOK_RC instead of 2: $HOOK_OUTPUT" >&2
		exit 1
	}
	[[ "$HOOK_OUTPUT" == *LOOM_CONTROL_ERROR:* ]] || {
		echo "FAIL: $label had no reason-bearing error: $HOOK_OUTPUT" >&2
		exit 1
	}
	[[ ! -e "$LOG" ]] || { echo "FAIL: $label reached the broker" >&2; exit 1; }
}

expect_allowed() {
	local command=$1 label=$2
	invoke_hook "$command"
	[[ "$HOOK_RC" == 0 ]] || {
		echo "FAIL: $label was blocked: $command: $HOOK_OUTPUT" >&2
		exit 1
	}
	[[ ! -e "$LOG" ]] || { echo "FAIL: $label reached the broker" >&2; exit 1; }
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

# Every syntactic way of attempting completion other than the exact pinned
# bytes is held and rejected. These cases exercise command positions,
# wrappers, shell operators, byte obfuscation, and conservative parse failure.
expect_reaches_pin "env -u RUSTC_WRAPPER $PINNED" "an env -u wrapper"
expect_reaches_pin "NAME=value $PINNED" "a NAME=value prefix"
expect_reaches_pin "$PINNED | cat" "a pipeline"
expect_reaches_pin "$PINNED ; true" "a command separator"
expect_reaches_pin "$PINNED && true" "an and-list"
expect_reaches_pin "$PINNED > out" "a redirection"
expect_reaches_pin "$PINNED &" "background execution"
expect_reaches_pin "$PINNED"$'\n'"true" "an embedded newline"
expect_reaches_pin "bash -c '$PINNED'" "a bash -c wrapper"
expect_reaches_pin "sh -lc '$PINNED'" "an sh -lc wrapper"
expect_reaches_pin '$LOOM_BIN stage complete build-api' "a variable binary"
expect_reaches_pin "$TMP/bin/loom stage \"complete\" build-api" "a quoted verb"
expect_reaches_pin "$TMP/bin/loom stage com\\plete build-api" "an escaped verb"
expect_reaches_pin "$TMP/bin/loom stage compl\"\"ete build-api" "a concatenated verb"
expect_reaches_pin "$TMP/bin/loom stage com\\"$'\n'"plete build-api" "a backslash-newline splice"
expect_reaches_pin "$PINNED --force" "an extra flag"
expect_reaches_pin "$PINNED unexpected" "an extra argument"
expect_reaches_pin 'loom stage complete build-api "' "an unterminated quote"

# 3. THE FALSE POSITIVES: the words inside ONE quoted argument, and this hook's
# own filename as a path argument, are not completion attempts. The raw glob
# blocked both; tokenizing must not.
expect_allowed 'loom memory note "gotcha: never run loom stage complete early"' \
	"a memory note quoting the verb"
expect_allowed "loom memory note 'mistake: stage complete ran before the subagents returned'" \
	"a single-quoted memory note"
expect_allowed 'rg -n "pre_tool_hooks" loom-hooks/loom-control-complete.sh' \
	"an rg naming this hook's file"
expect_allowed 'cat loom-hooks/loom-control-complete.sh' "a path argument naming this hook"
expect_allowed 'git commit -m "fix: complete the loom stage guard"' \
	"a commit message quoting the words"
expect_allowed 'loom stage list' "an unrelated loom stage subcommand"
expect_allowed 'echo done' "an unrelated command"
expect_allowed 'echo "run loom stage complete later"' "quoted completion prose"
expect_allowed 'cat docs/loom-stage-complete.md' "a completion-shaped path"
expect_allowed 'rg "stage complete" loom/' "a quoted search pattern"

# Nothing above may reach the completion broker.
[[ ! -e "$LOG" ]] || {
	echo "FAIL: the pre-filter cases reached the broker: $(cat "$LOG")" >&2
	exit 1
}

echo "PASS"
