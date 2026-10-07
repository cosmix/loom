#!/usr/bin/env bash
# commit-filter-session-git-commit.sh - in a loom stage session (LOOM_STAGE_ID
# and LOOM_SESSION_ID both set) commit-filter.sh blocks the main agent's own
# `git commit`: commits go through `loom stage commit`, which the daemon
# applies. The block matches `commit` only as git's SUBCOMMAND (the first
# non-option word after git's global options), so `git cat-file commit HEAD`
# and `git log --grep commit` pass. A subagent may not run `loom stage commit`
# either, and the attribution scan covers its message.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/commit-filter.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# run_hook <payload> [VAR=value...] - Invoke the hook with every gate variable
# scrubbed (this suite may itself run inside a live stage session), then the
# caller's assignments. Sets LAST_STDERR for message assertions.
run_hook() {
	local input="$1"
	shift
	LAST_STDERR=$(cd "$TMP" && printf '%s' "$input" | env -u LOOM_WORK_DIR -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_MAIN_AGENT_PID -u LOOM_HOOK_CONTEXT "$@" bash "$HOOK" 2>&1 >/dev/null)
}

expect_exit() {
	local desc="$1" want="$2" input="$3"
	shift 3
	set +e
	run_hook "$input" "$@"
	local code=$?
	set -e
	if [[ $code -ne $want ]]; then
		echo "FAIL: $desc - expected exit $want, got exit $code: $LAST_STDERR"
		exit 1
	fi
}

expect_stderr_has() {
	local desc="$1" needle="$2"
	if [[ "$LAST_STDERR" != *"$needle"* ]]; then
		echo "FAIL: $desc - stderr lacks '$needle': $LAST_STDERR"
		exit 1
	fi
}

plain_payload() {
	jq -nc --arg c "$1" '{tool_name:"Bash",tool_input:{command:$c}}'
}

subagent_payload() {
	jq -nc --arg c "$1" \
		'{tool_name:"Bash",tool_input:{command:$c},agent_type:"loom-software-engineer",transcript_path:"/h/.claude/projects/p/subagents/agent-x.jsonl"}'
}

SESSION=(LOOM_STAGE_ID=s1 LOOM_SESSION_ID=sess)

# --- the main agent in a stage session -------------------------------------
expect_exit "git commit in a stage session is blocked" \
	2 "$(plain_payload 'git commit -m x')" "${SESSION[@]}"
expect_stderr_has "git commit block" 'loom stage commit'
expect_stderr_has "git commit block" '--wait 90'

expect_exit "git -C . commit in a stage session is blocked" \
	2 "$(plain_payload 'git -C . commit -m x')" "${SESSION[@]}"

# `commit` is an argument there, not git's subcommand.
expect_exit "git cat-file commit HEAD in a stage session is allowed" \
	0 "$(plain_payload 'git cat-file commit HEAD')" "${SESSION[@]}"
expect_exit "git log --grep commit in a stage session is allowed" \
	0 "$(plain_payload 'git log --grep commit')" "${SESSION[@]}"

# Outside a session (either variable missing) the block does not apply.
expect_exit "git commit without session variables is allowed" \
	0 "$(plain_payload 'git commit -m x')"
expect_exit "git commit with only the stage id is allowed" \
	0 "$(plain_payload 'git commit -m x')" LOOM_STAGE_ID=s1

expect_exit "loom stage commit in a stage session is allowed" \
	0 "$(plain_payload 'loom stage commit s1 -m "feat(x): y"')" "${SESSION[@]}"
expect_exit "loom memory note quoting git commit is allowed" \
	0 "$(plain_payload 'loom memory note "never run git commit"')" "${SESSION[@]}"

# --- the attribution scan covers the new command's message ------------------
expect_exit "loom stage commit with an attribution trailer is blocked" \
	2 "$(plain_payload 'loom stage commit s1 -m "feat(x): y" -m "Co-Authored-By: Claude <noreply@anthropic.com>"')" "${SESSION[@]}"

# --- a subagent never commits, with either command --------------------------
expect_exit "loom stage commit by a subagent is blocked" \
	2 "$(subagent_payload 'loom stage commit s1 -m "feat(x): y"')" LOOM_MAIN_AGENT_PID=$$
expect_stderr_has "subagent block" 'loom stage commit'

echo "PASS: commit-filter-session-git-commit"
