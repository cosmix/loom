#!/usr/bin/env bash
# post-tool-use-commit-reminder-tokenized.sh - regression test for the
# post-commit knowledge/memory reminder in post-tool-use.sh. It used to
# regex-match the raw command with GNU-only `\s`/`\S`, which both failed to
# fire at all on a non-GNU grep and fired on any prose mentioning "git
# commit". It now strips heredoc bodies (strip_embedded_content) and
# tokenizes what remains (loom_tokenize_command + loom_tokens_cmd_has_arg),
# exactly as commit-filter.sh's is_git_commit_command() does.
#
# Three cases:
#   (a) a heredoc BODY whose text happens to start a line with "git commit"
#       (e.g. writing a doc/test fixture) must NOT fire the reminder - the
#       heredoc's content is stripped before tokenizing, so `git` never sits
#       at a real command position.
#   (b) a real `git commit -m ...` must still fire.
#   (c) a real `git -C <dir> commit -m ...` must still fire.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../post-tool-use.sh"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# run_hook <command-string> - invoke post-tool-use.sh as a Bash tool call
# with a scrubbed environment (no live loom session, no inherited debug
# flags) and echo stderr only.
run_hook() {
	local command="$1"
	local input
	input=$(jq -nc --arg c "$command" '{tool_name:"Bash",tool_input:{command:$c}}')
	printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_HOOK_DEBUG -u COMMIT_FILTER_DEBUG \
			LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$TMP/work" \
			bash "$HOOK" 2>&1 1>/dev/null
}

mkdir -p "$TMP/work"

# --- (a) heredoc body false positive ----------------------------------------
HEREDOC_CMD=$'cat <<EOF > doc.md\ngit commit -m "example"\nEOF'
STDERR_A=$(run_hook "$HEREDOC_CMD")
if echo "$STDERR_A" | grep -q "POST-COMMIT REMINDER"; then
	echo "FAIL: (a) a heredoc body mentioning 'git commit' fired the reminder"
	echo "stderr: $STDERR_A"
	exit 1
fi

# --- (b) a real git commit must still fire ----------------------------------
STDERR_B=$(run_hook 'git commit -m wip')
if ! echo "$STDERR_B" | grep -q "POST-COMMIT REMINDER"; then
	echo "FAIL: (b) a real 'git commit' did not fire the reminder"
	echo "stderr: $STDERR_B"
	exit 1
fi

# --- (c) a real `git -C <dir> commit` must still fire -----------------------
STDERR_C=$(run_hook 'git -C /some/path commit -m wip')
if ! echo "$STDERR_C" | grep -q "POST-COMMIT REMINDER"; then
	echo "FAIL: (c) a real 'git -C <dir> commit' did not fire the reminder"
	echo "stderr: $STDERR_C"
	exit 1
fi

echo "PASS"
