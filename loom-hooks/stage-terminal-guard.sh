#!/usr/bin/env bash
# PreToolUse hook: Block Write/Edit/Task/Agent once a stage is completed/verified
#
# `loom stage complete` is meant to be a session's LAST act (CLAUDE.md hard
# stop 3, Rule 4): the merge starts from the completed commit, so any edit or
# subagent spawn AFTER completion is lost work. commit-guard.sh (Stop hook)
# is only advisory and can be talked past; this hook is the hard enforcement
# — it blocks the next Write, Edit, Task, or Agent call once the stage's own
# status file says the stage is already completed or verified.
#
# `completed-with-failures` (and any other non-terminal status) MUST allow:
# that status means the agent is still expected to keep fixing and re-run
# `loom stage complete` — blocking it would strand the session.
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message
#
# Environment:
#   LOOM_MERGE_SESSION=1 - merge resolution sessions are exempt (see commit-guard.sh)

set -euo pipefail

debug_log() {
	if [[ "${LOOM_HOOK_DEBUG:-0}" == "1" ]]; then
		printf "[stage-terminal-guard] %s\n" "$*" >&2
	fi
}

# Read JSON input from stdin (drain to prevent blocking, matching sibling
# PreToolUse guards)
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)
debug_log "INPUT_JSON: $INPUT_JSON"

# --- Main hook logic ------------------------------------------------------

main() {
	debug_log "=== stage-terminal-guard starting ==="
	debug_log "CWD: $(pwd)"

	# Merge resolution sessions are exempt - they run on the main repo, own no
	# worktree, and their stage's status transition is exactly what they exist
	# to perform.
	if [[ "${LOOM_MERGE_SESSION:-}" == "1" ]]; then
		debug_log "Merge session detected (LOOM_MERGE_SESSION=1) - allowing"
		exit 0
	fi

    local worktree STAGE_ID work_dir stage_file status
    # Registered globally: an inherited LOOM_WORKTREE_PATH must not block an
    # unrelated session, so the session's own location alone decides.
    worktree=$(loom_cwd_worktree) || exit 0
    worktree=${worktree%/}
    STAGE_ID=${worktree##*/}
    work_dir=$(loom_stage_work_dir "$worktree") || exit 0
    stage_file=$(loom_find_stage_file "$work_dir" "$STAGE_ID") || exit 0
    status=$(loom_stage_status "$stage_file" || true)
	debug_log "Stage '$STAGE_ID' status: $status"

	case "$status" in
	completed | Completed | verified | Verified)
		debug_log "Stage is $status - BLOCKING"
		;;
	*)
		debug_log "Stage status '$status' is not terminal - allowing"
		exit 0
		;;
	esac

	cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - stage already $status
============================================================

BLOCKED: stage '$STAGE_ID' is already $status. \`loom stage complete\` was this session's LAST act -
nothing runs after it. This edit/spawn will NOT be merged: the merge starts from the completed
commit, so post-completion work is LOST WORK. End the session now.
If the stage was completed prematurely (subagents still out, defects unfixed), record the gap:
  loom memory note "completed prematurely: <what is missing>"
then end the session; recovery belongs to the orchestrator (loom stage retry / a new stage),
not to this session.

============================================================

EOF
	exit 2
}

main "$@"
