#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/subagents"
TRANSCRIPT="$TMP/subagents/agent-aForwarder-abc123.jsonl"
printf '%s\n' '{"message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY\n--model gpt-5.6-luna --effort xhigh\ntask text"}}' >"$TRANSCRIPT"

# The single forwarding-wrapper invocation is allowed. Keep the tilde literal:
# the guard validates the command before Bash performs path expansion.
COMPANION_CMD="~/.claude/hooks/loom/codex-forward.sh task 'hello; literal operator' --model gpt-5.6-terra --effort xhigh --write"
INPUT=$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"},"transcript_path":"%s"}' "$COMPANION_CMD" "$TRANSCRIPT")
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for the forwarding-wrapper call, got exit $CODE"
    exit 1
fi

# A forwarding command followed by a shell separator is not authorization
INPUT=$(printf '{"tool_name":"Bash","tool_input":{"command":"%s; touch escaped"},"transcript_path":"%s"}' "$COMPANION_CMD" "$TRANSCRIPT")
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for a forwarding command with a shell separator, got exit $CODE"
    exit 1
fi

# Any other Bash command is blocked
INPUT=$(printf '{"tool_name":"Bash","tool_input":{"command":"cargo check --lib"},"transcript_path":"%s"}' "$TRANSCRIPT")
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 for a non-companion Bash command, got exit $CODE"
    exit 1
fi
