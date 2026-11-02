#!/usr/bin/env bash
# The SubagentStart/SubagentStop pair against the real binary, fed the payloads
# the Claude Code hooks reference documents: SubagentStart carries the PARENT
# session's transcript_path and no worker transcript exists yet; only
# SubagentStop names the worker's own agent_transcript_path.
#
# Hook-only jobs have no build, and an older binary does not speak the current
# delegate input, so either case skips: `cargo build --bin loom` runs this. A job
# that builds first sets LOOM_HOOK_TEST_REQUIRE_BINARY=1, where a skip is a defect.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_HOOK_PATH \
	LOOM_HOOK_CONTEXT LOOM_WORKTREE_PATH
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$ROOT/loom/target/debug/loom"
for source_file in "$ROOT"/loom/src/commands/hook/review_{start,harvest}.rs; do
	if [[ ! -x "$BIN" || "$BIN" -ot "$source_file" ]]; then
		if [[ "${LOOM_HOOK_TEST_REQUIRE_BINARY:-}" == 1 ]]; then
			echo "FAIL: $BIN is missing or older than $source_file"
			exit 1
		fi
		echo "skip: $BIN is missing or older than $source_file"
		exit 0
	fi
done

TMP=$(mktemp -d "${TMPDIR:-/tmp}/subagent-review-real.XXXXXX")
trap 'rm -rf -- "$TMP"' EXIT
# The stop hook refuses paths that cross a symlink.
TMP=$(cd "$TMP" && pwd -P)

fail() {
	echo "FAIL: $1"
	exit 1
}

STAGE=review-stage
SESSION=loom-session
PARENT=parent-uuid
PROJECT="$TMP/project"
WORKTREE="$PROJECT/.worktrees/$STAGE"
WORK="$PROJECT/.loom/work"
CLAUDE="$TMP/claude/-project"
PARENT_TRANSCRIPT="$CLAUDE/$PARENT.jsonl"
REVIEWS="$WORK/reviews/$STAGE"
SKIPS="$WORK/subagents/$STAGE/stop-skips.jsonl"

mkdir -p "$PROJECT" "$WORK/stages" "$CLAUDE/$PARENT/subagents"
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com \
	GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com
git -C "$PROJECT" init -q -b main
printf '.loom/\n.worktrees/\n' >"$PROJECT/.gitignore"
git -C "$PROJECT" add .gitignore
git -C "$PROJECT" commit -q -m init
git -C "$PROJECT" worktree add -q -b "loom/$STAGE" "$WORKTREE"
printf 'change\n' >"$WORKTREE/change.txt"
# A stage session reaches the state directory through this symlink.
mkdir -p "$WORKTREE/.loom"
ln -s ../../../.loom/work "$WORKTREE/.loom/work"

cat >"$WORK/stages/01-$STAGE.md" <<EOF
---
id: $STAGE
name: $STAGE
status: executing
worktree: $STAGE
session: $SESSION
plan_version: 2
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
fix_attempts: 0
max_fix_attempts: null
created_at: 2026-01-01T00:00:00Z
updated_at: 2026-01-01T00:00:00Z
---
EOF
printf '%s\n' '{"type":"parent"}' >"$PARENT_TRANSCRIPT"
# An unheld daemon lock proves no daemon runs, so the delegates compute the
# change fingerprint themselves; a live daemon would answer on its socket.
: >"$WORK/orchestrator.lock"

# What `loom init` captures before any stage can touch the checkout.
directory_identity() {
	local ids=""
	ids=$(stat -c '%d %i' "$1" 2>/dev/null) || ids=$(stat -f '%d %i' "$1")
	jq -nc --arg path "$1" --argjson device "${ids% *}" --argjson inode "${ids#* }" \
		'{path:$path,device:$device,inode:$inode}'
}
jq -nc --argjson checkout "$(directory_identity "$PROJECT")" \
	--argjson git "$(directory_identity "$PROJECT/.git")" \
	'{checkout:$checkout,git_dir:$git,common_dir:$git}' >"$WORK/main-checkout-identity.json"

# run_hook <script> <work-dir> <payload>
run_hook() {
	printf '%s' "$3" | env LOOM_BIN="$BIN" LOOM_WORK_DIR="$2" LOOM_STAGE_ID="$STAGE" \
		LOOM_SESSION_ID="$SESSION" bash "$ROOT/loom-hooks/$1"
}

# start_reviewer <agent> <work-dir>: the documented SubagentStart payload.
start_reviewer() {
	local worker="$CLAUDE/$PARENT/subagents/agent-$1.jsonl"
	[[ ! -e "$worker" ]] || fail "fixture: the worker transcript exists before start"
	run_hook subagent-start.sh "$2" "$(jq -nc --arg agent "$1" --arg session "$PARENT" \
		--arg parent "$PARENT_TRANSCRIPT" \
		'{session_id:$session,transcript_path:$parent,cwd:"/",
		  hook_event_name:"SubagentStart",agent_id:$agent,agent_type:"loom-code-reviewer"}')"
}

# stop_reviewer <agent> <work-dir>: the worker transcript, then SubagentStop.
stop_reviewer() {
	local worker="$CLAUDE/$PARENT/subagents/agent-$1.jsonl"
	jq -nc '{type:"assistant",message:{content:[{type:"text",text:
		"Clean.\n\n```loom-review\n{\"findings\":[],\"suggestions\":[],\"resolved\":[],\"unresolved\":[]}\n```\n"}]}}' \
		>"$worker"
	run_hook subagent-stop.sh "$2" "$(jq -nc --arg agent "$1" --arg session "$PARENT" \
		--arg parent "$PARENT_TRANSCRIPT" --arg worker "$worker" \
		'{session_id:$session,transcript_path:$parent,agent_transcript_path:$worker,
		  hook_event_name:"SubagentStop",agent_id:$agent,agent_type:"loom-code-reviewer"}')"
}

rounds() {
	local count=0 file=""
	for file in "$REVIEWS"/round-*.json; do
		if [[ -f "$file" ]]; then count=$((count + 1)); fi
	done
	printf '%s' "$count"
}

# Canonical state directory: start evidence appears with no worker transcript,
# and the stop that follows records a round.
start_reviewer reviewer-a "$WORK"
START_FILES=("$REVIEWS"/start-*.json)
[[ -f "${START_FILES[0]}" ]] || fail "a documented SubagentStart payload left no start evidence"
jq -e 'has("transcript_path") | not' "${START_FILES[0]}" >/dev/null ||
	fail "start evidence must not record the parent transcript"
stop_reviewer reviewer-a "$WORK"
[[ "$(rounds)" == 1 ]] || fail "the reviewer's stop did not record a round"
[[ ! -e "$SKIPS" ]] || fail "a harvested stop wrote a skip row: $(<"$SKIPS")"

# The worktree's symlinked state directory, as a stage session sees it.
start_reviewer reviewer-b "$WORKTREE/.loom/work"
stop_reviewer reviewer-b "$WORKTREE/.loom/work"
[[ "$(rounds)" == 2 ]] || fail "a symlinked state directory lost the round"

# An edit between start and stop is refused, and the refusal reaches the gate's
# ledger: the immutable start evidence binds the harvest to the reviewed tree.
start_reviewer reviewer-c "$WORK"
printf 'edited after the reviewer started\n' >"$WORKTREE/late.txt"
stop_reviewer reviewer-c "$WORK"
[[ "$(rounds)" == 2 ]] || fail "a harvest certified edits made after the reviewer started"
jq -es 'length == 1 and .[0].agent_id == "reviewer-c" and
	.[0].reason == "review_harvest_failed.worktree_changed"' "$SKIPS" >/dev/null ||
	fail "a failed harvest wrote no stop-skips row"

echo "PASS"
