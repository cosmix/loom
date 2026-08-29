#!/usr/bin/env bash
# The SUBAGENT branch of the governor must resolve its ceiling from
# `[context] subagent_ceiling_tokens` (default 120000) ONLY - never from the
# stage's own `context_ceiling_tokens` frontmatter, which is a MAIN-session
# tier a subagent has no business inheriting (several subagents can run well
# past it while the main session is nowhere near its own limit).
#
# The stage frontmatter here declares a ceiling (100000) that is far ABOVE
# the resident count used, and the config declares the real, much smaller
# subagent ceiling (500) that the resident count sits AT. If the stage tier
# ever leaked into the subagent branch, 500 resident tokens would clear
# neither its 100% nor 80% threshold and the hard block below would never
# fire - so this test fails loudly on that regression instead of passing by
# coincidence.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="test-stage"
WORKDIR="$TMP/work"
mkdir -p "$WORKDIR/stages"

cat >"$WORKDIR/config.toml" <<'EOF'
[context]
subagent_ceiling_tokens = 500
EOF
cat >"$WORKDIR/stages/${STAGE_ID}.md" <<'EOF'
---
id: test-stage
context_ceiling_tokens: 100000
---
body
EOF

TRANSCRIPT="$TMP/subagent-transcript.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	printf '%s\n' '{"type":"assistant","message":{"usage":{"input_tokens":500,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}'
} >"$TRANSCRIPT"

# agent_type alone is enough for loom_payload_agent_verdict to classify this
# as a subagent (hooks/_common.sh:1044) once a LIVE LOOM_MAIN_AGENT_PID
# ancestor is established - $$ trivially satisfies is_ancestor for itself.
INPUT=$(jq -nc --arg tp "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:"echo hi"},agent_type:"loom-software-engineer",transcript_path:$tp}')

set +e
STDERR_OUT=$(printf '%s' "$INPUT" |
	env -u LOOM_WORKTREE_PATH \
		LOOM_MAIN_AGENT_PID="$$" \
		LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$WORKDIR" \
		bash "$HOOK" 2>&1 1>/dev/null)
CODE=$?
set -e

if [[ $CODE -ne 2 ]]; then
	echo "FAIL: subagent at 100% of its OWN ceiling - expected exit 2, got $CODE (stderr: $STDERR_OUT)"
	echo "      (a stray exit 0 here means the stage's context_ceiling_tokens leaked into the subagent branch)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"SUBAGENT CEILING REACHED"* ]]; then
	echo "FAIL: expected 'SUBAGENT CEILING REACHED' on stderr, got: $STDERR_OUT"
	exit 1
fi

echo "PASS"
