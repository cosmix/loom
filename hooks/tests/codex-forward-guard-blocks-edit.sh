#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/subagents"
TRANSCRIPT="$TMP/subagents/agent-aForwarder-abc123.jsonl"
printf '%s\n' '{"message":{"role":"user","content":"LOOM-CODEX-FORWARD-ONLY\n--model gpt-5.6-luna --effort xhigh\ntask text"}}' >"$TRANSCRIPT"
INPUT=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs","old_string":"a","new_string":"b"},"transcript_path":"%s"}' "$TRANSCRIPT")
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 for Edit by a sentinel-carrying forwarder, got exit $CODE"
    exit 1
fi
