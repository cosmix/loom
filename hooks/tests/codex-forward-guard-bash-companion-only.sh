#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/subagents"
TRANSCRIPT="$TMP/subagents/agent-aForwarder-abc123.jsonl"
printf '%s\n' '{"message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY\n--model gpt-5.6-luna --effort xhigh\ntask text"}}' >"$TRANSCRIPT"

# The single companion invocation is allowed
COMPANION_CMD='node /home/u/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs task hello --write'
INPUT=$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"},"transcript_path":"%s"}' "$COMPANION_CMD" "$TRANSCRIPT")
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for the codex-companion.mjs call, got exit $CODE"
    exit 1
fi

# Any other Bash command is blocked
INPUT=$(printf '{"tool_name":"Bash","tool_input":{"command":"cargo check --lib"},"transcript_path":"%s"}' "$TRANSCRIPT")
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 for a non-companion Bash command, got exit $CODE"
    exit 1
fi
