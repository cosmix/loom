#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
WORKTREE="$TMP/repo/.worktrees/build-api"
MAIN_REPO="$TMP/repo"
mkdir -p "$TMP/bin" "$WORKTREE" "$TMP/home"

# The hook only compares paths; the binary is never run.
cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$TMP/bin/loom"

PINNED="$TMP/bin/loom stage complete build-api"
HOOK_RC=0
HOOK_OUTPUT=""

run_payload() {
	local payload=$1
	set +e
	HOOK_OUTPUT=$(printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP/home" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="${LOOM_CONTROL_TEST_BIN_OVERRIDE:-$TMP/bin/loom}" \
			LOOM_STAGE_ID="${TEST_STAGE_ID:-build-api}" LOOM_SESSION_ID="${TEST_SESSION_ID:-session-123}" \
			LOOM_SESSION_TYPE="${TEST_SESSION_TYPE:-}" LOOM_SCRATCH_DIR="${TEST_SCRATCH_DIR:-}" \
			LOOM_WORKTREE_PATH="${TEST_WORKTREE_OVERRIDE:-$WORKTREE}" bash "$HOOK" 2>&1)
	HOOK_RC=$?
	set -e
}

run_pre() {
	local payload
	payload=$(jq -n --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}')
	run_payload "$payload"
}

assert_rc() {
	[[ "$HOOK_RC" == "$1" ]] || { echo "expected rc $1, got $HOOK_RC: $HOOK_OUTPUT" >&2; exit 1; }
}

assert_output() {
	[[ "$HOOK_OUTPUT" == *"$1"* ]] || { echo "missing output '$1': $HOOK_OUTPUT" >&2; exit 1; }
}

expect_pre_blocked() {
	run_pre "$1"
	assert_rc 2
	assert_output 'LOOM_CONTROL_ERROR:'
}

run_pre "$PINNED"
assert_rc 0

# A stale PostToolUse registration is inert: the hook carries no verdict.
payload=$(jq -n --arg command "$PINNED" \
	'{tool_name:"Bash",tool_input:{command:$command},tool_response:{stdout:"done",is_error:false}}')
run_payload "$payload"
assert_rc 0
[[ -z "$HOOK_OUTPUT" ]] || { echo "PostToolUse payload emitted output: $HOOK_OUTPUT" >&2; exit 1; }

# Fail-closed regressions plus the detector matrix: every invalid attempt is
# rejected before the command runs.
symlink_bin="$TMP/bin/loom-link"
ln -s "$TMP/bin/loom" "$symlink_bin"
INVALID_COMMANDS=(
	'loom stage complete build-api'
	"loom() { \"$TMP/bin/loom\" \"\$@\"; }; loom stage complete build-api"
	"alias loom='$TMP/bin/loom'; loom stage complete build-api"
	"$symlink_bin stage complete build-api"
	"env -u RUSTC_WRAPPER $PINNED"
	"NAME=value $PINNED"
	"$PINNED | cat"
	"$PINNED ; true"
	"$PINNED && true"
	"$PINNED > out"
	"$PINNED &"
	"$PINNED"$'\n'"true"
	"bash -c '$PINNED'"
	"sh -lc '$PINNED'"
	'$LOOM_BIN stage complete build-api'
	"$TMP/bin/loom stage \"complete\" build-api"
	"$TMP/bin/loom stage com\\plete build-api"
	"$TMP/bin/loom stage compl\"\"ete build-api"
	"$TMP/bin/loom stage com\\"$'\n'"plete build-api"
	"$PINNED --force"
	"$PINNED unexpected"
	'loom stage complete build-api "'
)
for invalid_command in "${INVALID_COMMANDS[@]}"; do
	expect_pre_blocked "$invalid_command"
done

LOOM_CONTROL_TEST_BIN_OVERRIDE="$symlink_bin" run_pre "$symlink_bin stage complete build-api"
assert_rc 2
assert_output 'LOOM_CONTROL_ERROR:'

for session_type in knowledge merge base-conflict; do
	TEST_SESSION_TYPE="$session_type" TEST_WORKTREE_OVERRIDE="$MAIN_REPO" \
		run_pre 'loom stage complete build-api'
	assert_rc 0
	[[ -z "$HOOK_OUTPUT" ]] || { echo "$session_type main-repo session emitted a pin" >&2; exit 1; }
done

nested_main="$TMP/outer/.worktrees/container/repo"
real_nested_worktree="$nested_main/.worktrees/build-api"
mkdir -p "$real_nested_worktree"
TEST_WORKTREE_OVERRIDE="$nested_main" run_pre 'loom stage complete build-api'
assert_rc 0
[[ -z "$HOOK_OUTPUT" ]] || { echo "nested main-repo path emitted a pin" >&2; exit 1; }
TEST_WORKTREE_OVERRIDE="$real_nested_worktree" run_pre 'loom stage complete build-api'
assert_rc 2
assert_output 'retry with the pinned command'

production_hook="$TMP/installed/loom-control-complete.sh"
mkdir -p "$(dirname "$production_hook")"
cp "$HOOK" "$production_hook"
cp "$ROOT/loom-hooks/_common.sh" "$(dirname "$production_hook")/_common.sh"
cp "$ROOT/loom-hooks/_bounded.sh" "$(dirname "$production_hook")/_bounded.sh"
cp "$ROOT/loom-hooks/_post-tool-heartbeat.sh" "$(dirname "$production_hook")/_post-tool-heartbeat.sh"
set +e
production_output=$(jq -n --arg command "$PINNED" '{tool_name:"Bash",tool_input:{command:$command}}' |
	env PATH="/usr/bin:/bin" HOME="$TMP/empty-home" LOOM_CONTROL_TESTING=1 \
		LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api LOOM_SESSION_ID=session-123 \
		LOOM_WORKTREE_PATH="$WORKTREE" bash "$production_hook" 2>&1)
rc=$?
set -e
[[ "$rc" == 2 ]] || { echo "test binary override escaped repository harness" >&2; exit 1; }
[[ "$production_output" == *LOOM_CONTROL_ERROR:* ]] || { echo "production rejection lacked a reason" >&2; exit 1; }

echo PASS
