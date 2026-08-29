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

# --- Persisted-output recovery -----------------------------------------
#
# Claude Code truncates large tool output into a "<persisted-output>" wrapper
# naming the file it saved the FULL output to, previewing only the first 2KB.
# A marker line past that preview must still be recoverable from the named
# file - but ONLY when that file resolves to a path the sandboxed stage agent
# itself could never have written (under $HOME/.claude/projects/**/tool-results,
# a regular file, not a symlink). Each case below gets its own throwaway
# $HOME so the sandbox's real ~/.claude/projects is never touched.

persisted_home() {
	local dir="$TMP/homes/$1"
	mkdir -p "$dir"
	printf '%s' "$dir"
}

persisted_case_capture() {
	local command=$1 output=$2 home=$3 payload
	payload=$(jq -n \
		--arg command "$command" --arg output "$output" \
		'{tool_name:"Bash",tool_input:{command:$command},tool_result:{output:$output,is_error:false}}')
	printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" HOME="$home" \
		BROKER_LOG="$LOG" LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" \
		LOOM_STAGE_ID="build-api" LOOM_SESSION_ID="session-123" \
		LOOM_WORKTREE_PATH="$WORKTREE" bash "$HOOK"
}

# Mimics Claude Code's own wrapper: names the persisted file, and its 2KB
# preview deliberately does NOT contain the marker (it sits further down in
# the real file, past what the preview shows).
wrapper_output() {
	local path=$1
	printf '<persisted-output>\nOutput too large (43.5KB). Full output saved to: %s\n\nPreview (first 2KB):\nsome earlier verify output\n...\n</persisted-output>\n' "$path"
}

log_lines() { [[ -e "$LOG" ]] && wc -l <"$LOG" | tr -d ' ' || echo 0; }

# 1. Marker present ONLY in the persisted file -> broker IS invoked.
home1=$(persisted_home case1)
persisted_dir1="$home1/.claude/projects/proj/sess/tool-results"
mkdir -p "$persisted_dir1"
persisted_file1="$persisted_dir1/out.txt"
printf 'earlier verify output\n%s\nlater verify output\n' "$MARKER" >"$persisted_file1"
before=$(log_lines)
persisted_case_capture "$PINNED" "$(wrapper_output "$persisted_file1")" "$home1" >/dev/null
after=$(log_lines)
[[ "$after" == "$((before + 1))" ]] || { echo "marker-in-persisted-file case did not invoke the broker" >&2; exit 1; }
rg -q '^stage complete build-api --session session-123$' "$LOG"

# 2. Persisted path present but the file does NOT contain the marker ->
# broker NOT invoked, message says so.
home2=$(persisted_home case2)
persisted_dir2="$home2/.claude/projects/proj/sess/tool-results"
mkdir -p "$persisted_dir2"
persisted_file2="$persisted_dir2/out.txt"
printf 'earlier verify output\nverification failed\nlater verify output\n' >"$persisted_file2"
before=$(log_lines)
out2=$(persisted_case_capture "$PINNED" "$(wrapper_output "$persisted_file2")" "$home2")
after=$(log_lines)
[[ "$after" == "$before" ]] || { echo "no-marker-in-persisted-file case reached the broker" >&2; exit 1; }
ctx2=$(printf '%s' "$out2" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ "$ctx2" == *"persisted-output"* ]] || { echo "no-marker-in-persisted-file message does not mention the persisted file: $ctx2" >&2; exit 1; }
[[ "$ctx2" == *"did not contain the marker"* ]] || { echo "no-marker-in-persisted-file message does not say the marker was absent: $ctx2" >&2; exit 1; }

# 3. Persisted path OUTSIDE $HOME/.claude/projects/ -> rejected, broker NOT
# invoked.
home3=$(persisted_home case3)
outside_dir3="$TMP/outside/tool-results"
mkdir -p "$outside_dir3"
outside_file3="$outside_dir3/out.txt"
printf '%s\n' "$MARKER" >"$outside_file3"
before=$(log_lines)
out3=$(persisted_case_capture "$PINNED" "$(wrapper_output "$outside_file3")" "$home3")
after=$(log_lines)
[[ "$after" == "$before" ]] || { echo "outside-projects persisted path reached the broker" >&2; exit 1; }
ctx3=$(printf '%s' "$out3" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ "$ctx3" == *"did not validate"* ]] || { echo "outside-projects message does not say validation failed: $ctx3" >&2; exit 1; }

# 4. Persisted path IS a symlink -> rejected, broker NOT invoked (even though
# its target is a real file that does contain the marker).
home4=$(persisted_home case4)
persisted_dir4="$home4/.claude/projects/proj/sess/tool-results"
real_dir4="$TMP/outside/real4"
mkdir -p "$persisted_dir4" "$real_dir4"
real_file4="$real_dir4/out.txt"
printf '%s\n' "$MARKER" >"$real_file4"
symlink_file4="$persisted_dir4/out.txt"
ln -s "$real_file4" "$symlink_file4"
before=$(log_lines)
out4=$(persisted_case_capture "$PINNED" "$(wrapper_output "$symlink_file4")" "$home4")
after=$(log_lines)
[[ "$after" == "$before" ]] || { echo "symlinked persisted path reached the broker" >&2; exit 1; }
ctx4=$(printf '%s' "$out4" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ "$ctx4" == *"did not validate"* ]] || { echo "symlinked persisted path message does not say validation failed: $ctx4" >&2; exit 1; }

# 5. Persisted path string-matches the projects root but TRAVERSES OUT via
# ".." to a file outside it that genuinely contains the marker -> rejected,
# broker NOT invoked. `[[ "$path" == "$PROJECTS_ROOT"/* ]]` is a glob against
# the raw string, not a resolved path, so `.../projects/../../../../tmp/x`
# satisfies it while resolving somewhere else entirely - the same directory
# class the sandboxed stage agent can genuinely write to itself.
home5=$(persisted_home case5)
persisted_dir5="$home5/.claude/projects"
mkdir -p "$persisted_dir5"
# The traversal target's directory MUST itself contain a "tool-results"
# segment - otherwise the pre-existing "*/tool-results/*" guard rejects the
# path for an unrelated reason and the traversal case never actually
# exercises the bug this test targets.
real_dir5="$TMP/outside/real5/tool-results"
mkdir -p "$real_dir5"
real_file5="$real_dir5/out.txt"
printf '%s\n' "$MARKER" >"$real_file5"
# Compute the ".." depth from $persisted_dir5 back to $TMP in pure bash, so
# this does not silently drift if persisted_home()'s nesting ever changes.
relative5="${persisted_dir5#"$TMP"/}"
depth5=1
rest5="$relative5"
while [[ "$rest5" == */* ]]; do
	depth5=$((depth5 + 1))
	rest5="${rest5#*/}"
done
dotdots5=""
for ((i = 0; i < depth5; i++)); do dotdots5+="../"; done
traversal_path5="${persisted_dir5}/${dotdots5}outside/real5/tool-results/out.txt"
before=$(log_lines)
out5=$(persisted_case_capture "$PINNED" "$(wrapper_output "$traversal_path5")" "$home5")
after=$(log_lines)
[[ "$after" == "$before" ]] || { echo "path-traversal persisted path reached the broker" >&2; exit 1; }
ctx5=$(printf '%s' "$out5" | jq -r '.hookSpecificOutput.additionalContext // empty')
[[ "$ctx5" == *"did not validate"* ]] || { echo "path-traversal message does not say validation failed: $ctx5" >&2; exit 1; }

# 6. Persisted path contains a legitimate filename with two dots (not a `..`
# path SEGMENT) -> still ACCEPTED, broker invoked. Proves the traversal guard
# above is segment-aware rather than a blanket "contains .." substring ban,
# which would wrongly reject real filenames like this one.
home6=$(persisted_home case6)
persisted_dir6="$home6/.claude/projects/proj/sess/tool-results"
mkdir -p "$persisted_dir6"
persisted_file6="$persisted_dir6/out..1.txt"
printf '%s\n' "$MARKER" >"$persisted_file6"
before=$(log_lines)
persisted_case_capture "$PINNED" "$(wrapper_output "$persisted_file6")" "$home6" >/dev/null
after=$(log_lines)
[[ "$after" == "$((before + 1))" ]] || { echo "two-dot legitimate filename case did not invoke the broker" >&2; exit 1; }
rg -q '^stage complete build-api --session session-123$' "$LOG"

# 7. Existing behaviour unchanged: marker inline in the tool result -> broker
# invoked. Already covered above (the original "valid route" assertion); no
# duplicate case added here.
