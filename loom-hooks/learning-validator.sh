#!/usr/bin/env bash
# learning-validator.sh - Claude Code Stop hook for loom
#
# Called when a Claude Code session is stopping.
# Validates session outcomes and provides guidance.
#
# Checks:
# 1. Memory usage warning (soft) - does this stage's own journal
#    (memory/<stage-id>.md) hold a `###` entry?
# 2. Goal-backward verification stub (for future enforcement)
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the state directory (.loom/work, or the
#                      legacy .work for a workspace that already resolved
#                      to it)
#
# This hook runs alongside commit-guard.sh (which handles uncommitted changes).
# It focuses on learning and outcome validation.

set -uo pipefail
# Note: intentionally NOT using set -e. Any command failure in this advisory
# hook must not cause a non-zero exit that kills the session.

# Drain stdin to prevent blocking (Stop hooks receive JSON from Claude Code)
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
loom_read_input 1 >/dev/null 2>&1 || true

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	# Not in a loom session, exit silently
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

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
	local journal="${LOOM_WORK_DIR}/memory/${LOOM_STAGE_ID}.md"

	if [[ ! -f "$journal" ]] || ! grep -q '^### ' "$journal" 2>/dev/null; then
		cat >&2 <<'WARN'
┌────────────────────────────────────────────────────────────────────┐
│  ⚠️  No memories recorded for this stage                            │
│                                                                    │
│  Consider capturing insights for future sessions:                  │
│    loom memory note "observation"                                  │
│    loom memory decision "choice" --context "reasoning"             │
│    loom memory question "open question to investigate"             │
│                                                                    │
│  Memories help future agents avoid repeating mistakes.             │
│  A knowledge-distill stage records nothing once distilling begins. │
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
