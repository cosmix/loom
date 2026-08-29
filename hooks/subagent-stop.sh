#!/usr/bin/env bash
# subagent-stop.sh - Claude Code SubagentStop hook for loom
#
# Called when a Task-tool subagent finishes. Loom previously registered no
# hook event that fires on subagent completion, so a subagent that finished
# without delivering a chat notification was completely invisible to the
# orchestrator. This is the only positive, push-based completion signal.
#
# Second job: refresh the PARENT session's heartbeat. SessionStart/PostToolUse
# heartbeats are written on the parent's OWN tool loop, but the parent runs no
# tools of its own while blocked on a Task call - so the heartbeat goes stale
# and HeartbeatWatcher::check_session_hung reports the stage hung exactly when
# it is behaving correctly. SubagentStop fires in the PARENT's hook context,
# so refreshing the heartbeat here closes that starvation window.
#
# Input: JSON from stdin, e.g.
#   {"transcript_path": ".../subagents/agent-<agentId>.jsonl", "session_id": "...", ...}
# (the finishing subagent's OWN transcript - see hooks/_common.sh's
# loom_payload_agent_verdict documentation, ~lines 1010-1039, for this shape)
#
# Environment variables (set by loom worktree settings, PARENT session's own):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The parent session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Writes a completion record to .work/subagents/<stage-id>/<agentId>.json
#   2. Refreshes .work/heartbeat/<stage-id>.json so the parent does not look hung
#
# Must never block and never fail the session: every path below ends in
# `exit 0`, and every skip explains itself on the debug channel (a bare
# `|| exit 0` that says nothing is a known defect class in this repo's
# bridge hooks - see doc/loom/knowledge/mistakes/completion-broker-credential.md).

set -euo pipefail
umask 077

# loom_debug and loom_payload_agent_verdict - reused rather than re-deriving
# the subagent transcript-shape classification here.
source "$(dirname "$0")/_common.sh"

# Read stdin JSON. Cross-platform timeout: gtimeout (macOS+coreutils), timeout
# (Linux), or plain cat.
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Validate required environment variables. This hook installs globally and
# runs on every Claude Code session, loom or not - a missing LOOM_STAGE_ID
# means we are simply not inside a loom stage, which is normal.
if [[ -z "${LOOM_STAGE_ID:-}" ]]; then
	loom_debug "subagent-stop: skipping - LOOM_STAGE_ID unset (not a loom session)"
	exit 0
fi
if [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	loom_debug "subagent-stop: skipping - LOOM_SESSION_ID or LOOM_WORK_DIR unset"
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-stop: skipping - LOOM_STAGE_ID has unsafe characters: $LOOM_STAGE_ID"
	exit 0
	;;
esac

if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	loom_debug "subagent-stop: skipping - work directory does not exist: ${LOOM_WORK_DIR}"
	exit 0
fi

if ! command -v jq &>/dev/null; then
	loom_debug "subagent-stop: skipping - jq not available, cannot parse payload"
	exit 0
fi

TRANSCRIPT_PATH=$(printf '%s' "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
if [[ -z "$TRANSCRIPT_PATH" ]]; then
	loom_debug "subagent-stop: skipping - payload has no transcript_path"
	exit 0
fi

# Confirm this payload really is subagent-shaped using the shared classifier
# (defense in depth - a SubagentStop payload should always classify as
# "subagent", but a future Claude Code transcript-layout change must be a
# clean skip here, not a garbage completion record).
VERDICT=$(loom_payload_agent_verdict "$INPUT_JSON")
if [[ "$VERDICT" != "subagent" ]]; then
	loom_debug "subagent-stop: skipping - loom_payload_agent_verdict returned '${VERDICT}' (not 'subagent') for transcript_path=${TRANSCRIPT_PATH}"
	exit 0
fi

# Extract the agent id from the transcript path. Same shape documented at
# hooks/_common.sh's loom_payload_agent_verdict: a subagent's transcript
# lives at .../subagents/agent-<agentId>.jsonl.
case "$TRANSCRIPT_PATH" in
*/subagents/agent-*.jsonl)
	AGENT_ID="${TRANSCRIPT_PATH##*/}"
	AGENT_ID="${AGENT_ID#agent-}"
	AGENT_ID="${AGENT_ID%.jsonl}"
	;;
*)
	loom_debug "subagent-stop: skipping - transcript_path is not subagent-shaped: $TRANSCRIPT_PATH"
	exit 0
	;;
esac

if [[ -z "$AGENT_ID" ]]; then
	loom_debug "subagent-stop: skipping - extracted empty agent id from $TRANSCRIPT_PATH"
	exit 0
fi

case "$AGENT_ID" in
*[!A-Za-z0-9._-]* | "")
	loom_debug "subagent-stop: skipping - AGENT_ID has unsafe characters: $AGENT_ID"
	exit 0
	;;
esac

# Ensure directories exist with plain mkdir + shell redirection - NOT loom's
# Rust safe-read/write path. In a worktree, .work is a SYMLINK to the main
# repo's .work, and loom's safe_open_dirfd opens roots O_NOFOLLOW (refusing a
# symlinked root); a sandboxed hook also sees deny-listed files as zero-byte
# char devices. That exact combination caused the completion-broker credential
# outage (doc/loom/knowledge/mistakes/completion-broker-credential.md) - do
# not route this write through a Rust CLI path.
SUBAGENTS_DIR="${LOOM_WORK_DIR}/subagents/${LOOM_STAGE_ID}"
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p -m 700 "$SUBAGENTS_DIR" "$HEARTBEAT_DIR" 2>/dev/null || {
	loom_debug "subagent-stop: skipping - cannot create .work/subagents or .work/heartbeat"
	exit 0
}
chmod 700 "$SUBAGENTS_DIR" "$HEARTBEAT_DIR" 2>/dev/null || true

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Write the completion record. Built via `jq -n --arg` so a value containing a
# quote/backslash can never produce malformed JSON; a symlinked target path is
# refused - matching post-tool-use.sh's heartbeat-write guard, the target must
# never be written through.
COMPLETION_FILE="${SUBAGENTS_DIR}/${AGENT_ID}.json"
if [[ ! -L "$COMPLETION_FILE" ]]; then
	COMPLETION_JSON=$(jq -n \
		--arg stage_id "$LOOM_STAGE_ID" \
		--arg session_id "$LOOM_SESSION_ID" \
		--arg agent_id "$AGENT_ID" \
		--arg timestamp "$TIMESTAMP" \
		--arg transcript_path "$TRANSCRIPT_PATH" \
		'{stage_id: $stage_id, session_id: $session_id, agent_id: $agent_id, timestamp: $timestamp, transcript_path: $transcript_path}' \
		2>/dev/null || true)
	if [[ -n "$COMPLETION_JSON" ]]; then
		printf '%s\n' "$COMPLETION_JSON" >"$COMPLETION_FILE"
		chmod 600 "$COMPLETION_FILE" 2>/dev/null || true
	else
		loom_debug "subagent-stop: skipping completion record write - jq -n failed for agent $AGENT_ID"
	fi
else
	loom_debug "subagent-stop: skipping completion record write - $COMPLETION_FILE is a symlink"
fi

# Refresh the parent session's heartbeat using the same schema
# session-start.sh/post-tool-use.sh write, so HeartbeatWatcher does not flag
# the stage hung while the parent was legitimately blocked waiting on this
# subagent.
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
if [[ ! -L "$HEARTBEAT_FILE" ]]; then
	HEARTBEAT_JSON=$(jq -n \
		--arg stage_id "$LOOM_STAGE_ID" \
		--arg session_id "$LOOM_SESSION_ID" \
		--arg timestamp "$TIMESTAMP" \
		--arg activity "subagent ${AGENT_ID} finished" \
		'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp, context_percent: null, last_tool: null, activity: $activity}' \
		2>/dev/null || true)
	if [[ -n "$HEARTBEAT_JSON" ]]; then
		printf '%s\n' "$HEARTBEAT_JSON" >"$HEARTBEAT_FILE"
		chmod 600 "$HEARTBEAT_FILE" 2>/dev/null || true
	else
		loom_debug "subagent-stop: skipping heartbeat refresh - jq -n failed"
	fi
else
	loom_debug "subagent-stop: skipping heartbeat refresh - $HEARTBEAT_FILE is a symlink"
fi

exit 0
