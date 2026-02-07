#!/usr/bin/env bash
# pre-compact.sh - Claude Code PreCompact hook for loom
#
# Implements block-then-allow pattern for context compaction:
# - First attempt: Blocks compaction, creates handoff, asks agent to dump context
# - Second attempt: Allows compaction after capturing updated state
#
# Input: JSON from stdin (if any - hook doesn't need it)
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Exit codes:
#   0 = Allow compaction
#   2 = Block compaction (non-zero, non-1 to avoid hook failure)

set -euo pipefail

# Drain stdin to prevent blocking
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	gtimeout 1 cat >/dev/null 2>&1 || true
elif command -v timeout &>/dev/null; then
	timeout 1 cat >/dev/null 2>&1 || true
else
	cat >/dev/null 2>&1 || true
fi

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	echo "Error: Missing required environment variables" >&2
	exit 1
fi

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	echo "Warning: Work directory does not exist: ${LOOM_WORK_DIR}" >&2
	exit 0 # Exit gracefully
fi

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || {
	echo "Warning: Cannot create hooks directory" >&2
	exit 0
}

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Check for compaction-pending flag file
PENDING_DIR="${LOOM_WORK_DIR}/compaction-pending"
PENDING_FLAG="${PENDING_DIR}/${LOOM_SESSION_ID}"
RECOVERY_DIR="${LOOM_WORK_DIR}/compaction-recovery"

if [[ -f "$PENDING_FLAG" ]]; then
	# SECOND compaction attempt - flag exists, allow compaction
	rm -f "$PENDING_FLAG"

	# Create handoff (captures updated memory)
	HANDOFF_FILE=""
	if command -v loom &>/dev/null; then
		if HANDOFF_OUTPUT=$(loom handoff create --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger precompact 2>&1); then
			HANDOFF_FILE=$(echo "$HANDOFF_OUTPUT" | grep -oE '[^/]+\.md$' || echo "")
		fi
	fi

	# Create recovery marker for post-tool-use hook
	mkdir -p "$RECOVERY_DIR" 2>/dev/null || true
	touch "${RECOVERY_DIR}/${LOOM_SESSION_ID}" 2>/dev/null || true

	# Build payload JSON
	if [[ -n "$HANDOFF_FILE" ]]; then
		PAYLOAD="{\"type\":\"PreCompact\",\"phase\":\"allow\",\"handoff_file\":\"${HANDOFF_FILE}\"}"
	else
		PAYLOAD="{\"type\":\"PreCompact\",\"phase\":\"allow\"}"
	fi

	cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"PreCompact","payload":${PAYLOAD}}
EOF

	exit 0
else
	# FIRST compaction attempt - block and capture state
	mkdir -p "$PENDING_DIR" 2>/dev/null || true
	touch "$PENDING_FLAG"

	# Create initial handoff
	HANDOFF_FILE=""
	if command -v loom &>/dev/null; then
		if HANDOFF_OUTPUT=$(loom handoff create --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger precompact 2>&1); then
			HANDOFF_FILE=$(echo "$HANDOFF_OUTPUT" | grep -oE '[^/]+\.md$' || echo "")
		fi
	fi

	# Build payload JSON
	if [[ -n "$HANDOFF_FILE" ]]; then
		PAYLOAD="{\"type\":\"PreCompact\",\"phase\":\"block\",\"handoff_file\":\"${HANDOFF_FILE}\"}"
	else
		PAYLOAD="{\"type\":\"PreCompact\",\"phase\":\"block\"}"
	fi

	cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"PreCompact","payload":${PAYLOAD}}
EOF

	# Instruct agent to dump context before compaction proceeds
	cat >&2 <<'INTERCEPT'

CONTEXT COMPACTION INTERCEPTED
Before compaction, record your working state:
  loom memory note "CONTEXT DUMP: Working on [TASK]. Next: [NEXT]. Key context: [INFO]"
After recording, continue work. Compaction will proceed on next cycle.

INTERCEPT

	exit 2
fi
