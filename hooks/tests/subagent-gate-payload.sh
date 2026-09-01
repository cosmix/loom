#!/usr/bin/env bash
# subagent-gate-payload.sh - loom_is_subagent gates on a LIVE loom session
# FIRST (LOOM_MAIN_AGENT_PID set and a live process-tree ancestor), and only
# once that passes does it trust the hook payload's .agent_type /
# .transcript_path to decide main-vs-subagent, falling back to the
# process-tree walk only when the payload answers neither field.
#
# Case (c) is the false-positive regression this fixes: a Bash-tool shell's
# cmdline often mentions a ~/.claude/ path (e.g. sourcing a shell-snapshot
# file), which the OLD process-tree walk misread as an intervening Claude
# process and blocked the MAIN agent's own commit. LOOM_MAIN_AGENT_PID is
# exported to a live ancestor for that case (and its subagent-verify-guard
# mirror, case (e)) so the old path would have engaged had the payload
# verdict not already settled it as "main".
#
# Case (g) is the scoping guarantee the loom-session gate exists for: outside
# a live loom session (LOOM_MAIN_AGENT_PID unset), a subagent-shaped payload
# must NOT be blocked - these hooks install globally at
# ~/.claude/hooks/loom/, so an unrelated Claude Code session (a Task subagent
# in a non-loom repo, or an agent-team teammate that is never in the main
# agent's process tree) must never be hard-blocked with no escape hatch.
#
# Cases (h)/(i) pin that `loom_payload_agent_verdict`'s "main" verdict is a
# POSITIVE identification (session_id present, no /subagents/ component,
# basename == "${session_id}.jsonl"), never granted by elimination: an
# unrecognized transcript shape, or a main-shaped path missing session_id,
# must both come back "unknown" rather than "main" - the fix for a review
# finding where "any non-empty transcript_path that isn't the recognized
# subagent shape" silently turned both guards into no-ops for any
# unanticipated subagent transcript layout.
#
# Self-contained: runs in a scratch temp dir, with LOOM_WORK_DIR/LOOM_STAGE_ID
# explicitly unset, so neither the integration-verify carve-out nor any
# worktree-isolation logic can engage on a real repo's state directory
# (knowledge mistakes.md, "Hook Tests Must Scrub the Env the Hook Gates
# On"). LOOM_MAIN_
# AGENT_PID is likewise scrubbed by `run_hook` before any caller-supplied
# value is layered on - the same lesson, applied to the one variable this
# change made load-bearing for a passing/failing case (g).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
COMMIT_FILTER="$ROOT/hooks/commit-filter.sh"
VERIFY_GUARD="$ROOT/hooks/subagent-verify-guard.sh"
COMMON="$ROOT/hooks/_common.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

run_hook() {
	local hook="$1" input="$2"
	shift 2
	# -u LOOM_MAIN_AGENT_PID: this suite runs from a real Claude Code session,
	# and if that session is itself inside a live loom stage, the wrapper
	# (loom/src/orchestrator/terminal/native/wrapper.rs) has already exported
	# LOOM_MAIN_AGENT_PID as a live ancestor of every Bash-tool descendant -
	# including this test process. Left inherited, case (g) below (which
	# asserts "no loom session -> no blocking") would silently pass or fail
	# depending on whether this suite happens to run inside a stage. Callers
	# that need a live ancestor pass LOOM_MAIN_AGENT_PID=$$ explicitly via "$@".
	(cd "$TMP" && printf '%s' "$input" | env -u LOOM_WORK_DIR -u LOOM_STAGE_ID -u LOOM_MAIN_AGENT_PID "$@" bash "$hook" 2>/dev/null)
}

expect_exit() {
	local desc="$1" want="$2" hook="$3" input="$4"
	shift 4
	set +e
	run_hook "$hook" "$input" "$@"
	local code=$?
	set -e
	if [[ $code -ne $want ]]; then
		echo "FAIL: $desc - expected exit $want, got exit $code"
		exit 1
	fi
}

COMMIT_COMMAND='git commit -m x'
TEST_COMMAND='cargo test'

# Shared main-session identity for cases (c)/(e): the transcript path's
# basename is built FROM this session id, so the agreement the main-shaped
# rule requires is obvious by construction rather than two hand-typed
# strings that merely happen to match.
MAIN_SESSION_ID="cccccccc-cccc-cccc-cccc-cccccccccccc"
MAIN_TRANSCRIPT_PATH="/h/.claude/projects/p/${MAIN_SESSION_ID}.jsonl"

# (a) agent_type identifies the subagent once a LIVE LOOM_MAIN_AGENT_PID
# ancestor is established, even though the process tree has no separate,
# intervening Claude process at all (the subagent runs IN-PROCESS).
INPUT_A=$(jq -nc --arg cmd "$COMMIT_COMMAND" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"loom-software-engineer",transcript_path:"/h/.claude/projects/p/s.jsonl"}')
expect_exit "(a) commit-filter blocks by agent_type" 2 "$COMMIT_FILTER" "$INPUT_A" "LOOM_MAIN_AGENT_PID=$$"

# (b) empty agent_type, but transcript_path names a subagents/ transcript -
# same live-ancestor precondition as (a).
INPUT_B=$(jq -nc --arg cmd "$COMMIT_COMMAND" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"",transcript_path:"/h/.claude/projects/p/subagents/agent-abc.jsonl"}')
expect_exit "(b) commit-filter blocks by transcript_path shape" 2 "$COMMIT_FILTER" "$INPUT_B" "LOOM_MAIN_AGENT_PID=$$"

# (c) THE REGRESSION UNDER TEST: a main-shaped transcript_path (basename ==
# "${session_id}.jsonl", no /subagents/ component) must win even with a LIVE
# LOOM_MAIN_AGENT_PID ancestor exported.
INPUT_C=$(jq -nc --arg cmd "$COMMIT_COMMAND" --arg tp "$MAIN_TRANSCRIPT_PATH" --arg sid "$MAIN_SESSION_ID" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"",transcript_path:$tp,session_id:$sid}')
expect_exit "(c) commit-filter allows main agent despite live LOOM_MAIN_AGENT_PID" \
	0 "$COMMIT_FILTER" "$INPUT_C" "LOOM_MAIN_AGENT_PID=$$"

# (d) subagent-verify-guard blocks a full-suite run by agent_type alone (no
# transcript_path field at all), again gated on a live LOOM_MAIN_AGENT_PID.
INPUT_D=$(jq -nc --arg cmd "$TEST_COMMAND" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"loom-software-engineer"}')
expect_exit "(d) subagent-verify-guard blocks by agent_type" 2 "$VERIFY_GUARD" "$INPUT_D" "LOOM_MAIN_AGENT_PID=$$"

# (e) main-shaped transcript_path plus empty agent_type, with a LIVE
# LOOM_MAIN_AGENT_PID exported - must allow, mirroring case (c).
INPUT_E=$(jq -nc --arg cmd "$TEST_COMMAND" --arg tp "$MAIN_TRANSCRIPT_PATH" --arg sid "$MAIN_SESSION_ID" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"",transcript_path:$tp,session_id:$sid}')
expect_exit "(e) subagent-verify-guard allows main agent despite live LOOM_MAIN_AGENT_PID" \
	0 "$VERIFY_GUARD" "$INPUT_E" "LOOM_MAIN_AGENT_PID=$$"

# (g) THE SCOPING GUARANTEE: outside a live loom session (LOOM_MAIN_AGENT_PID
# unset), a subagent-shaped payload must NOT be blocked - the loom-session
# gate runs before the payload is ever consulted. Mirrored across both hooks.
INPUT_G=$(jq -nc --arg cmd "$COMMIT_COMMAND" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"loom-software-engineer",transcript_path:"/h/.claude/projects/p/s.jsonl"}')
expect_exit "(g) commit-filter allows a subagent-shaped payload with no live loom session" \
	0 "$COMMIT_FILTER" "$INPUT_G"

INPUT_G_VERIFY=$(jq -nc --arg cmd "$TEST_COMMAND" \
	'{tool_name:"Bash",tool_input:{command:$cmd},agent_type:"loom-software-engineer"}')
expect_exit "(g) subagent-verify-guard allows a subagent-shaped payload with no live loom session" \
	0 "$VERIFY_GUARD" "$INPUT_G_VERIFY"

# (f) unit check of loom_cmdline_is_claude: a Bash-tool shell whose cmdline
# merely MENTIONS a ~/.claude/ path must not count as Claude Code, while the
# real interpreter+script/binary forms still must.
source "$COMMON"

ZSH_SHELL_CMDLINE='/bin/zsh -c source /home/user/.claude/shell-snapshots/snapshot-zsh-abc123.sh ; eval fake'
if loom_cmdline_is_claude "$ZSH_SHELL_CMDLINE"; then
	echo "FAIL: (f) a Bash-tool shell sourcing a ~/.claude/ snapshot must not be classified as Claude Code"
	exit 1
fi

if ! loom_cmdline_is_claude "claude --resume x"; then
	echo "FAIL: (f) 'claude --resume x' must be classified as Claude Code"
	exit 1
fi

if ! loom_cmdline_is_claude "node /x/@anthropic-ai/claude-code/cli.js"; then
	echo "FAIL: (f) the node cli.js launcher must be classified as Claude Code"
	exit 1
fi

# (h)/(i) unit checks of loom_payload_agent_verdict: "main" must be a
# POSITIVE identification, never granted by elimination. Asserted on the
# verdict directly (like (f) above) rather than a hook exit code, because
# an "unknown" verdict's actual effect depends on the live process-tree
# fallback, not on anything this function alone determines.

# (h) UNRECOGNIZED SUBAGENT TRANSCRIPT SHAPE: neither */subagents/agent-*.jsonl
# nor basename-matching the (present) session_id - must be "unknown", not
# waved through as "main" just because it isn't the recognized subagent shape.
INPUT_H=$(jq -nc \
	'{agent_type:"",transcript_path:"/h/.claude/projects/p/agents/agent-x.jsonl",session_id:"deadbeef-realsession"}')
VERDICT_H=$(loom_payload_agent_verdict "$INPUT_H")
if [[ "$VERDICT_H" != "unknown" ]]; then
	echo "FAIL: (h) unrecognized transcript shape must verdict 'unknown', got '$VERDICT_H'"
	exit 1
fi

# (i) MAIN-SHAPED PATH, session_id ABSENT: nothing to compare the basename
# against, so this must be "unknown" too - not "main" by elimination.
INPUT_I=$(jq -nc --arg tp "$MAIN_TRANSCRIPT_PATH" \
	'{agent_type:"",transcript_path:$tp}')
VERDICT_I=$(loom_payload_agent_verdict "$INPUT_I")
if [[ "$VERDICT_I" != "unknown" ]]; then
	echo "FAIL: (i) main-shaped path with no session_id must verdict 'unknown', got '$VERDICT_I'"
	exit 1
fi

echo "PASS"
