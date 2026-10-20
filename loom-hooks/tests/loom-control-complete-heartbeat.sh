#!/usr/bin/env bash
# The pinned `loom stage complete` is one long foreground call. When the
# PreToolUse branch admits it, the session's heartbeat must say a completion
# is running (the daemon then allows a longer silence). An observation or a
# subagent's heartbeat keeps that mark; a main-agent progress heartbeat, the
# command's own PostToolUse among them, replaces it. Nothing else writes it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../loom-control-complete.sh"
POST_HOOK="$SCRIPT_DIR/../post-tool-use.sh"
STALL_LINE="$SCRIPT_DIR/../../loom/src/orchestrator/monitor/stall_line.rs"
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-control-complete-heartbeat.XXXXXX") && [[ -n "$TMP" ]] || {
	echo "FAIL: could not create scratch directory"
	exit 1
}
trap '[[ -n "${TMP:-}" ]] && rm -rf -- "$TMP"' EXIT

ACTIVITY="Stage completion running"
WORKTREE="$TMP/repo/.worktrees/build-api"
WORK="$TMP/repo/.loom/work"
HEARTBEAT="$WORK/heartbeat/build-api.json"
mkdir -p "$TMP/bin" "$WORKTREE" "$WORK/stages" "$WORK/heartbeat"
printf '%s\n' '---' 'id: build-api' 'session: session-123' '---' '# Stage' >"$WORK/stages/01-build-api.md"
printf '#!/usr/bin/env bash\nprintf "LOOM_CONTROL_OUTCOME accepted\\n"\n' >"$TMP/bin/loom"
chmod +x "$TMP/bin/loom"
PINNED="$TMP/bin/loom stage complete build-api"

fail() {
	echo "FAIL: $1"
	exit 1
}

rg -Fq "COMPLETION_ACTIVITY: &str = \"$ACTIVITY\"" "$STALL_LINE" ||
	fail "the daemon's COMPLETION_ACTIVITY no longer matches '$ACTIVITY'"

# run_pre <command> [session] - Run the PreToolUse branch; sets HOOK_RC.
run_pre() {
	local payload
	payload=$(jq -n --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}')
	set +e
	printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api \
			LOOM_SESSION_ID="${2:-session-123}" LOOM_WORKTREE_PATH="$WORKTREE" \
			LOOM_WORK_DIR="$WORK" bash "$HOOK" >/dev/null 2>&1
	HOOK_RC=$?
	set -e
}

write_prior() {
	jq -n --arg session "$1" '{stage_id:"build-api", session_id:$session,
		timestamp:"2026-10-20T10:00:00.000Z", progress_at:"2026-10-20T10:00:00.000Z",
		activity_kind:"progress", context_tokens:4242, transcript_path:"/t.jsonl",
		last_tool:"Read", activity:"Tool executed: Read", subagent:false}' >"$HEARTBEAT"
}

# An admitted pinned command marks the completion as running, as progress,
# and keeps the context reading the heartbeat already carried.
write_prior session-123
run_pre "$PINNED"
[[ "$HOOK_RC" == 0 ]] || fail "pinned command was not admitted (rc $HOOK_RC)"
jq -e --arg activity "$ACTIVITY" '.activity == $activity and .session_id == "session-123"
	and .last_tool == "Bash" and .activity_kind == "progress" and .progress_at == .timestamp
	and .timestamp != "2026-10-20T10:00:00.000Z" and .context_tokens == 4242
	and .transcript_path == "/t.jsonl" and .subagent == false' "$HEARTBEAT" >/dev/null ||
	fail "admitted completion did not mark the heartbeat: $(cat "$HEARTBEAT")"

# A rejected completion attempt leaves the heartbeat alone.
write_prior session-123
run_pre "loom stage complete build-api"
[[ "$HOOK_RC" == 2 ]] || fail "unpinned command was not rejected (rc $HOOK_RC)"
jq -e '.activity == "Tool executed: Read"' "$HEARTBEAT" >/dev/null ||
	fail "rejected completion rewrote the heartbeat"

# Neither does an unrelated command.
run_pre "cargo test"
[[ "$HOOK_RC" == 0 ]] || fail "unrelated command was blocked"
jq -e '.activity == "Tool executed: Read"' "$HEARTBEAT" >/dev/null ||
	fail "unrelated command rewrote the heartbeat"

# A session that no longer owns the stage cannot claim the heartbeat.
write_prior session-123
run_pre "$PINNED" session-old
jq -e '.session_id == "session-123" and .activity == "Tool executed: Read"' "$HEARTBEAT" >/dev/null ||
	fail "a superseded session marked the heartbeat"

# The heartbeat is never written through a symlink.
printf 'outside\n' >"$TMP/outside"
rm -f "$HEARTBEAT"
ln -s "$TMP/outside" "$HEARTBEAT"
run_pre "$PINNED"
[[ "$HOOK_RC" == 0 ]] || fail "a symlinked heartbeat blocked the completion"
[[ "$(cat "$TMP/outside")" == outside ]] || fail "the heartbeat followed a symlink"
rm -f "$HEARTBEAT"

# Without a state directory the completion still runs; nothing is written.
set +e
jq -n --arg command "$PINNED" '{tool_name:"Bash",tool_input:{command:$command}}' |
	env PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP" LOOM_CONTROL_TESTING=1 \
		LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api \
		LOOM_SESSION_ID=session-123 LOOM_WORKTREE_PATH="$WORKTREE" bash "$HOOK" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" == 0 && ! -e "$HEARTBEAT" ]] || fail "missing LOOM_WORK_DIR blocked or wrote (rc $rc)"

# run_post <command> [agent_type] - Run PostToolUse as the main agent, or as
# a subagent of the given type.
run_post() {
	jq -n --arg command "$1" --arg agent "${2:-}" \
		'{tool_name:"Bash",tool_input:{command:$command},transcript_path:""}
		 + (if $agent == "" then {} else {agent_type:$agent} end)' |
		env LOOM_WORK_DIR="$WORK" LOOM_STAGE_ID=build-api LOOM_SESSION_ID=session-123 \
			bash "$POST_HOOK" >/dev/null 2>&1
}

# expect_activity <activity> <jq condition> <message>
expect_activity() {
	jq -e --arg activity "$1" ".activity == \$activity and $2" "$HEARTBEAT" >/dev/null ||
		fail "$3: $(cat "$HEARTBEAT")"
}

# The command's own PostToolUse heartbeat ends the allowance.
run_pre "$PINNED"
expect_activity "$ACTIVITY" true "admitted completion did not mark a fresh heartbeat"
run_post "$PINNED"
expect_activity "Tool executed: Bash" true "PostToolUse did not replace the completion mark"

# While the completion runs, a background observation or a subagent's tool
# call keeps the mark; the next main-agent progress write ends it.
run_pre "$PINNED"
run_post "sleep 30"
expect_activity "$ACTIVITY" '.activity_kind == "observation"' "an observation erased the completion mark"
run_post "cargo build" general-purpose
expect_activity "$ACTIVITY" '.subagent == true' "a subagent write erased the completion mark"
run_post "cargo build"
expect_activity "Tool executed: Bash" '.subagent == false' "a main-agent progress write kept the completion mark"

# Without a mark, an observation records its own activity.
run_post "sleep 30"
expect_activity "Tool executed: Bash" '.activity_kind == "observation"' "an observation claimed a completion mark"

echo "PASS"
