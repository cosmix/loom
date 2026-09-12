#!/usr/bin/env bash
# The SUBAGENT branch of the governor must resolve its ceiling from
# `[context] subagent_ceiling_tokens` (default 120000) ONLY - never from the
# stage's own `context_ceiling_tokens` frontmatter, which is a MAIN-session
# tier a subagent has no business inheriting (several subagents can run well
# past it while the main session is nowhere near its own limit).
#
# The canonical pair declares a main ceiling (100000) far ABOVE the resident
# count and a subagent ceiling (500) that the resident count sits AT. If shell
# selects the wrong half, the hard block below never fires.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="test-stage"
WORKDIR="$TMP/work"
mkdir -p "$WORKDIR/stages" "$WORKDIR/heartbeat"
printf '%s\n' '100000:500' \
	>"$WORKDIR/heartbeat/${STAGE_ID}.test-session.context-ceilings"

TRANSCRIPT="$TMP/subagent-transcript.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	printf '%s\n' '{"type":"assistant","message":{"usage":{"input_tokens":500,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}'
} >"$TRANSCRIPT"

# The parent already owns the stage heartbeat. A team teammate is not in the
# parent's process tree, but its harness payload still identifies it as a
# subagent; it must carry these values forward rather than replacing them
# with its own 500-token transcript.
PARENT_TRANSCRIPT="$TMP/parent-transcript.jsonl"
jq -n \
	--arg stage_id "$STAGE_ID" \
	--arg session_id "test-session" \
	--arg transcript_path "$PARENT_TRANSCRIPT" \
	'{stage_id:$stage_id,session_id:$session_id,timestamp:"2026-08-30T00:00:00.000Z",
	  context_tokens:321,transcript_path:$transcript_path,last_tool:null,activity:"parent"}' \
	>"$WORKDIR/heartbeat/${STAGE_ID}.json"

# `agent_type` is authoritative for this per-session hook even when
# LOOM_MAIN_AGENT_PID is deliberately NOT an ancestor. That is the process
# shape of an agent-team teammate; the global enforcement hooks keep their
# stricter ancestry-first scoping separately.
INPUT=$(jq -nc --arg tp "$TRANSCRIPT" \
	'{tool_name:"Bash",tool_input:{command:"echo hi"},agent_type:"loom-software-engineer",transcript_path:$tp}')

set +e
STDERR_OUT=$(printf '%s' "$INPUT" |
	 env -u LOOM_WORKTREE_PATH \
		LOOM_MAIN_AGENT_PID="99999999" \
		LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$WORKDIR" \
		bash "$HOOK" 2>&1 1>/dev/null)
CODE=$?
set -e

if [[ $CODE -ne 2 ]]; then
	echo "FAIL: subagent at 100% of its OWN ceiling - expected exit 2, got $CODE (stderr: $STDERR_OUT)"
		echo "      (a stray exit 0 here means the main half of the canonical pair leaked into the subagent branch)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"SUBAGENT CEILING REACHED"* ]]; then
	echo "FAIL: expected 'SUBAGENT CEILING REACHED' on stderr, got: $STDERR_OUT"
	exit 1
fi

GOT_PARENT_TOKENS=$(jq -r '.context_tokens' "$WORKDIR/heartbeat/${STAGE_ID}.json")
GOT_PARENT_TRANSCRIPT=$(jq -r '.transcript_path' "$WORKDIR/heartbeat/${STAGE_ID}.json")
if [[ "$GOT_PARENT_TOKENS" != "321" || "$GOT_PARENT_TRANSCRIPT" != "$PARENT_TRANSCRIPT" ]]; then
	echo "FAIL: teammate overwrote parent heartbeat: tokens=$GOT_PARENT_TOKENS transcript=$GOT_PARENT_TRANSCRIPT"
	exit 1
fi

echo "PASS"
