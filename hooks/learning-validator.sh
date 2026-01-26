#!/usr/bin/env bash
# learning-validator.sh - Claude Code Stop hook for loom
#
# Called when a Claude Code session is stopping.
# Validates session outcomes and provides guidance.
#
# Checks:
# 1. Memory usage warning (soft) - did agent record any decisions/notes?
# 2. Goal-backward verification stub (for future enforcement)
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# This hook runs alongside commit-guard.sh (which handles uncommitted changes).
# It focuses on learning and outcome validation.

set -euo pipefail

# Drain stdin to prevent blocking (Stop hooks receive JSON from Claude Code)
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
	# Not in a loom session, exit silently
	exit 0
fi

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	# Work directory doesn't exist, exit silently
	exit 0
fi

# Ensure hooks directory exists for logging
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || true

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
PAYLOAD="{\"type\":\"Stop\"}"
cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"Stop","payload":${PAYLOAD}}
EOF

# ============================================================================
# Check 1: Memory usage warning (soft - does not block)
# ============================================================================
check_memory_usage() {
	local memory_dir="${LOOM_WORK_DIR}/memory"

	# Skip if memory directory doesn't exist
	if [[ ! -d "$memory_dir" ]]; then
		return 0
	fi

	# Count memory entries (files with actual content)
	local memory_count=0
	for f in "$memory_dir"/*.md 2>/dev/null; do
		if [[ -f "$f" ]] && grep -q '^## ' "$f" 2>/dev/null; then
			memory_count=$((memory_count + 1))
		fi
	done

	if [[ "$memory_count" -eq 0 ]]; then
		cat >&2 <<'WARN'
┌────────────────────────────────────────────────────────────────────┐
│  ⚠️  No memories recorded this session                              │
│                                                                    │
│  Consider capturing insights for future sessions:                  │
│    loom memory note "observation"                                  │
│    loom memory decision "choice" --context "reasoning"             │
│    loom memory question "open question to investigate"             │
│                                                                    │
│  Memories help future agents avoid repeating mistakes.             │
└────────────────────────────────────────────────────────────────────┘
WARN
	fi
}

# ============================================================================
# Check 2: Goal-backward verification stub (future enforcement)
# ============================================================================
# This will be enabled once 'loom verify --quiet' is implemented.
# For now, goal-backward verification runs during 'loom stage complete'.
#
# check_goal_backward() {
#     if command -v loom &>/dev/null && [[ -n "$LOOM_STAGE_ID" ]]; then
#         # Check if stage has goal-backward checks defined and verify them
#         if ! loom verify "$LOOM_STAGE_ID" --quiet 2>/dev/null; then
#             cat >&2 <<'ERROR'
# ┌────────────────────────────────────────────────────────────────────┐
# │  ✗ Goal-backward verification failed                               │
# │                                                                    │
# │  Some truths, artifacts, or wiring checks are not passing.         │
# │  Run 'loom stage complete' to see detailed failure information.    │
# └────────────────────────────────────────────────────────────────────┘
# ERROR
#             # Note: We don't block here - commit-guard.sh handles blocking
#             # This is informational for when the agent needs guidance
#         fi
#     fi
# }

# Run checks
check_memory_usage

exit 0
