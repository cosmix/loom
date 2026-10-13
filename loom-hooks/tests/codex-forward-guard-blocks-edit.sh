#!/usr/bin/env bash
set -euo pipefail
unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw.XXXXXX") && [ -n "$d" ]
trap 'rm -rf "$d"' EXIT
TMP="$d"
mkdir -p "$TMP/subagents"
TRANSCRIPT="$TMP/subagents/agent-aForwarder-abc123.jsonl"
printf '%s\n' '{"message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY\n--model gpt-6-luna --effort xhigh\ntask text"}}' >"$TRANSCRIPT"
INPUT=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs","old_string":"a","new_string":"b"},"transcript_path":"%s"}' "$TRANSCRIPT")
# LOOM_SESSION_ID is the guard's stage evidence: the forwarding policy applies
# only inside a stage, and the sentinel in the transcript is not evidence of
# one because the forwarder writes it.
set +e
echo "$INPUT" | LOOM_SESSION_ID=blocks-edit-session bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for Edit by a sentinel-carrying forwarder, got exit $CODE"
    exit 1
fi
# A nested `env -i claude` gives its hooks no HOME. The block message must not
# trip set -u, whose exit 1 Claude Code treats as a non-blocking error.
set +e
echo "$INPUT" | env -u HOME LOOM_SESSION_ID=blocks-edit-session bash "$HOOK" 2>"$TMP/nohome.err"
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for Edit by a forwarder with HOME unset, got exit $CODE"
    exit 1
fi
# The block message is still printed, minus the full-path line that needs HOME.
# That line starts with the expanded path; the `~/...` example line stays.
if ! grep -q 'BLOCKED' "$TMP/nohome.err"; then
    echo "FAIL: the HOME-unset block message does not say BLOCKED"
    exit 1
fi
if grep -q '^[[:space:]]*/.*/\.claude/hooks/loom/codex-forward\.sh' "$TMP/nohome.err"; then
    echo "FAIL: the HOME-unset block message printed the full wrapper path"
    exit 1
fi
echo "PASS"
