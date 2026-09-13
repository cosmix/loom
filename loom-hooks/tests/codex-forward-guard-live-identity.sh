#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHILD_TMP="$d/children"
LIVE_WORK="$d/live-work"
mkdir -p "$CHILD_TMP" "$LIVE_WORK"

# Recreate the ambient identity that used to leak into guard tests. Each child
# must clear it before invoking the guard and use only its own cfw.* scratch.
export LOOM_STAGE_ID=live-stage
export LOOM_SESSION_ID=live-session
export LOOM_WORK_DIR="$LIVE_WORK"
export LOOM_SESSION_TYPE=stage
export LOOM_MAIN_AGENT_PID=$$

for test_name in \
	codex-forward-guard-agent-type.sh \
	codex-forward-guard-bash-companion-only.sh \
	codex-forward-guard-blocks-edit.sh \
	codex-forward-guard-ignores-others.sh \
	codex-forward-guard-quoting.sh \
	codex-forward-records-model.sh; do
	output=
	if ! output=$(TMPDIR="$CHILD_TMP" bash "$SCRIPT_DIR/$test_name" 2>&1); then
		printf '%s\n' "FAIL: $test_name failed under a live ambient identity" "$output"
		exit 1
	fi
	if rg --files "$d" | rg -q '(^|/)codex\.jsonl$'; then
		printf '%s\n' "FAIL: $test_name left a codex ledger outside its own temporary directory"
		exit 1
	fi
	if [[ -e "$LIVE_WORK/subagents" ]]; then
		printf '%s\n' "FAIL: $test_name used the ambient live LOOM_WORK_DIR"
		exit 1
	fi
done

printf '%s\n' 'PASS'
