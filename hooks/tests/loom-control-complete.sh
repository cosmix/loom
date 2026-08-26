#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/loom-control-complete.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
# A real loom worktree path: the hook engages on `<repo>/.worktrees/<stage-id>`
# and must stay out of the way of main-repo sessions.
WORKTREE="$TMP/repo/.worktrees/build-api"
MAIN_REPO="$TMP/repo"
mkdir -p "$TMP/bin" "$WORKTREE"
LOG="$TMP/broker.log"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$BROKER_LOG"
SH
chmod +x "$TMP/bin/loom"

invoke_hook() {
	local payload=$1
	local test_bin=${LOOM_CONTROL_TEST_BIN_OVERRIDE:-$TMP/bin/loom}
	local worktree=${LOOM_CONTROL_TEST_WORKTREE:-$WORKTREE}
	printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" \
		BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$test_bin" \
		LOOM_STAGE_ID="build-api" LOOM_SESSION_ID="session-123" \
		LOOM_WORKTREE_PATH="$worktree" bash "$HOOK" >/dev/null
}

post_case() {
	local command=$1 output=$2 is_error=$3 payload
	payload=$(jq -n \
		--arg command "$command" --arg output "$output" --argjson is_error "$is_error" \
		'{tool_name:"Bash",tool_input:{command:$command},tool_result:{output:$output,is_error:$is_error}}')
	invoke_hook "$payload"
}

# invoke_hook_capture / post_case_capture - same as invoke_hook / post_case but
# return the hook's stdout instead of discarding it, so a caller can inspect
# the additionalContext JSON the two former silent skips now emit.
invoke_hook_capture() {
	local payload=$1
	local test_bin=${LOOM_CONTROL_TEST_BIN_OVERRIDE:-$TMP/bin/loom}
	local worktree=${LOOM_CONTROL_TEST_WORKTREE:-$WORKTREE}
	printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" \
		BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$test_bin" \
		LOOM_STAGE_ID="build-api" LOOM_SESSION_ID="session-123" \
		LOOM_WORKTREE_PATH="$worktree" bash "$HOOK"
}

post_case_capture() {
	local command=$1 output=$2 is_error=$3 payload
	payload=$(jq -n \
		--arg command "$command" --arg output "$output" --argjson is_error "$is_error" \
		'{tool_name:"Bash",tool_input:{command:$command},tool_result:{output:$output,is_error:$is_error}}')
	invoke_hook_capture "$payload"
}

pre_case() {
	local command=$1 payload
	jq -n \
		--arg command "$command" \
		'{tool_name:"Bash",tool_input:{command:$command}}'
}

MARKER='LOOM_CONTROL_VERIFICATION_PASSED stage=build-api session=session-123'
invalid_commands=(
	'loom stage complete build-api --no-verify'
	'loom stage complete build-api extra'
	'loom stage complete build-api > result.txt'
	'loom stage complete build-api < input.txt'
	'loom stage complete build-api 2>&1'
	'loom stage complete $(id)'
	'loom stage complete `id`'
	'loom stage complete build-api; id'
	'loom stage complete build-api && id'
	'loom stage complete build-api || id'
	'loom stage complete build-api | id'
	'loom stage complete build-api &'
	$'loom stage complete build-api\nid'
	$'loom\tstage\tcomplete\tbuild-api'
	'loom  stage  complete  build-api'
	$'loom stage \\\n+complete build-api'
	'$LOOM_BIN stage complete build-api'
	'/tmp/loom stage complete build-api'
)
for command in "${invalid_commands[@]}"; do
	payload=$(pre_case "$command")
	if invoke_hook "$payload" 2>/dev/null; then
		echo "invalid command was not blocked: $command" >&2
		exit 1
	fi
done
[[ ! -e "$LOG" ]] || { echo "invalid command reached broker" >&2; exit 1; }

relative_payload=$(pre_case 'loom stage complete build-api')
if invoke_hook "$relative_payload" 2>/dev/null; then
	echo "relative PATH-resolved command was not blocked" >&2
	exit 1
fi
[[ ! -e "$LOG" ]] || { echo "PATH-controlled loom was invoked" >&2; exit 1; }

PINNED="$TMP/bin/loom stage complete build-api"
valid_payload=$(pre_case "$PINNED")
invoke_hook "$valid_payload"
[[ ! -e "$LOG" ]] || { echo "PreToolUse invoked the broker" >&2; exit 1; }

for command in \
	"function $TMP/bin/loom { printf forge; }; $PINNED" \
	"alias loom='$TMP/bin/loom'; $PINNED"; do
	payload=$(pre_case "$command")
	if invoke_hook "$payload" 2>/dev/null; then
		echo "function or alias command was not blocked: $command" >&2
		exit 1
	fi
done
[[ ! -e "$LOG" ]] || { echo "function or alias reached broker" >&2; exit 1; }

symlink_bin="$TMP/bin/loom-link"
ln -s "$TMP/bin/loom" "$symlink_bin"
symlink_payload=$(pre_case "$symlink_bin stage complete build-api")
if LOOM_CONTROL_TEST_BIN_OVERRIDE="$symlink_bin" invoke_hook "$symlink_payload" 2>/dev/null; then
	echo "symlink trusted-binary override was not blocked" >&2
	exit 1
fi

production_hook="$TMP/installed/loom-control-complete.sh"
mkdir -p "$(dirname "$production_hook")"
cp "$HOOK" "$production_hook"
# install.sh ships _common.sh into the same hooks dir; without it the copy dies
# on its `source` and this case would pass for the wrong reason.
cp "$ROOT/hooks/_common.sh" "$(dirname "$production_hook")/_common.sh"
production_payload=$(pre_case "$PINNED")
if printf '%s' "$production_payload" | env PATH="/usr/bin:/bin" HOME="$TMP/empty-home" \
	LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" \
	LOOM_STAGE_ID=build-api LOOM_SESSION_ID=session-123 \
	LOOM_WORKTREE_PATH="$WORKTREE" bash "$production_hook" >/dev/null 2>&1; then
	echo "test override was accepted outside the repository test harness" >&2
	exit 1
fi

# Main-repo sessions (knowledge, merge, base-conflict) own no worktree. They
# complete in-process, so the bridge must DISENGAGE rather than pin their
# command — pinning it used to hand a knowledge stage a stage id that does not
# exist and block the only command that could complete the stage.
for non_worktree in "$MAIN_REPO" "$MAIN_REPO/.worktrees" "$TMP/repo-worktrees-backup"; do
	pass_through=$(pre_case 'loom stage complete build-api')
	if ! LOOM_CONTROL_TEST_WORKTREE="$non_worktree" invoke_hook "$pass_through" 2>/dev/null; then
		echo "main-repo session was blocked by the worktree bridge: $non_worktree" >&2
		exit 1
	fi
done
[[ ! -e "$LOG" ]] || { echo "main-repo session reached broker" >&2; exit 1; }

# is_error=true skips the broker without ever inspecting stdout for the
# marker. That used to be a bare `exit 0` - now it must explain, via
# additionalContext, that the completion command itself errored and the
# stage is still Executing.
is_error_output=$(post_case_capture "$PINNED" "$MARKER" true)
is_error_context=$(printf '%s' "$is_error_output" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ -n "$is_error_context" ]] || { echo "is_error skip produced no additionalContext" >&2; exit 1; }
[[ "$is_error_context" == *"error"* ]] || { echo "is_error skip message does not explain the error: $is_error_context" >&2; exit 1; }
[[ "$is_error_context" == *"Executing"* ]] || { echo "is_error skip message does not say the stage is still Executing: $is_error_context" >&2; exit 1; }

# is_error=false but the marker is absent from stdout ("verification
# failed" instead of the pinned marker line). That was also a bare `exit 0`
# - now it must explain that the marker was not found.
missing_marker_output=$(post_case_capture "$PINNED" 'verification failed' false)
missing_marker_context=$(printf '%s' "$missing_marker_output" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ -n "$missing_marker_context" ]] || { echo "missing-marker skip produced no additionalContext" >&2; exit 1; }
[[ "$missing_marker_context" == *"marker"* ]] || { echo "missing-marker skip message does not mention the marker: $missing_marker_context" >&2; exit 1; }

[[ ! -e "$LOG" ]] || { echo "failed verification reached broker" >&2; exit 1; }

for command in 'loom stage complete build-api' '/tmp/loom stage complete build-api'; do
	if post_case "$command" "$MARKER" false 2>/dev/null; then
		echo "invalid PostToolUse command was not rejected: $command" >&2
		exit 1
	fi
done
[[ ! -e "$LOG" ]] || { echo "invalid PostToolUse command reached broker" >&2; exit 1; }

post_case "$PINNED" "$MARKER" false
[[ "$(wc -l <"$LOG" | tr -d ' ')" == 1 ]] || { echo "valid route did not run once" >&2; exit 1; }
rg -q '^stage complete build-api --session session-123$' "$LOG"
