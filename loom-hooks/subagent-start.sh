#!/usr/bin/env bash
# subagent-start.sh - Claude Code SubagentStart hook for loom
#
# Fires when a Task/Agent subagent starts. Paired with subagent-stop.sh (which
# records completion) and spawn-guard.sh (which records the spawn REQUEST from
# the caller's side) so `loom subagents` can correlate a spawn with its start
# and its completion by agent id.
#
# Input: JSON from stdin, e.g.
#   {"agent_id": "...", "agent_type": "loom-software-engineer",
#    "session_id": "<claude-parent-uuid>",
#    "transcript_path": ".../subagents/agent-<agentId>.jsonl", ...}
#
# Environment variables (set by loom worktree settings, PARENT session's own):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The parent session ID
#   LOOM_WORK_DIR    - Path to the state directory (.loom/work, or the
#                      legacy .work for a workspace that already resolved
#                      to it)
#
# Actions:
#   Appends one record to $LOOM_WORK_DIR/subagents/<stage-id>/starts.jsonl
#
# Must never block: every path below ends in `exit 0`, and every write is
# best-effort so a failure here can never fail the subagent's start.

set -euo pipefail
umask 077

source "$(dirname "$0")/_common.sh"

if [[ -z "${LOOM_WORK_DIR:-}" ]] || [[ -z "${LOOM_STAGE_ID:-}" ]] || \
	[[ -z "${LOOM_SESSION_ID:-}" ]] || [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	loom_debug "subagent-start: skipping - not a loom session or work dir missing"
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-start: skipping - LOOM_STAGE_ID has unsafe characters: $LOOM_STAGE_ID"
	exit 0
	;;
esac
case "$LOOM_SESSION_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-start: skipping - LOOM_SESSION_ID has unsafe characters: $LOOM_SESSION_ID"
	exit 0
	;;
esac

if ! command -v jq &>/dev/null; then
	loom_debug "subagent-start: skipping - jq not available"
	exit 0
fi

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

AGENT_TYPE=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
AGENT_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
PARENT_SESSION_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)

# Fall back to deriving the agent id from transcript_path's basename, the same
# shape subagent-stop.sh already relies on
# (.../subagents/agent-<agentId>.jsonl).
if [[ -z "$AGENT_ID" ]]; then
	TRANSCRIPT_PATH=$(printf '%s' "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
	case "$TRANSCRIPT_PATH" in
	*/subagents/agent-*.jsonl)
		AGENT_ID="${TRANSCRIPT_PATH##*/}"
		AGENT_ID="${AGENT_ID#agent-}"
		AGENT_ID="${AGENT_ID%.jsonl}"
		;;
	esac
fi

case "$AGENT_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-start: skipping - AGENT_ID missing or has unsafe characters: $AGENT_ID"
	exit 0
	;;
esac
case "$PARENT_SESSION_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-start: skipping - payload session_id missing or has unsafe characters: $PARENT_SESSION_ID"
	exit 0
	;;
esac

# Same write discipline as subagent-stop.sh:124-137 - plain mkdir/redirection,
# never a Rust/loom CLI path (the state directory is a SYMLINK inside a
# worktree and loom's safe-write opens roots O_NOFOLLOW), a symlinked target
# refused, every step best-effort.
SUBAGENTS_DIR="${LOOM_WORK_DIR}/subagents/${LOOM_STAGE_ID}"
mkdir -p -m 700 "$SUBAGENTS_DIR" 2>/dev/null || {
	loom_debug "subagent-start: skipping - cannot create $SUBAGENTS_DIR"
	exit 0
}
chmod 700 "$SUBAGENTS_DIR" 2>/dev/null || true

STARTS_FILE="${SUBAGENTS_DIR}/starts.jsonl"
if [[ -L "$STARTS_FILE" ]]; then
	loom_debug "subagent-start: skipping - $STARTS_FILE is a symlink"
	exit 0
fi

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
START_JSON=$(jq -nc \
	--arg agent_id "$AGENT_ID" \
	--arg agent_type "$AGENT_TYPE" \
	--arg stage_id "$LOOM_STAGE_ID" \
	--arg parent_session_id "$PARENT_SESSION_ID" \
	--arg loom_session_id "$LOOM_SESSION_ID" \
	--arg ts "$TIMESTAMP" \
	'{agent_id: $agent_id, agent_type: $agent_type, stage_id: $stage_id, parent_session_id: $parent_session_id, loom_session_id: $loom_session_id, ts: $ts}' \
	2>/dev/null || true)

if [[ -n "$START_JSON" ]]; then
	printf '%s\n' "$START_JSON" >>"$STARTS_FILE" 2>/dev/null ||
		loom_debug "subagent-start: ledger append failed for $STARTS_FILE"
	chmod 600 "$STARTS_FILE" 2>/dev/null || true
else
	loom_debug "subagent-start: skipping - jq -n failed for agent $AGENT_ID"
fi

exit 0
