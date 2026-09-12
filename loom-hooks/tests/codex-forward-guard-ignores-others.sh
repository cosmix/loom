#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/subagents"

# A subagent WITHOUT the sentinel is untouched
PLAIN="$TMP/subagents/agent-aSonnetWorker-def456.jsonl"
printf '%s\n' '{"message":{"role":"user","content":"implement the parser in src/parser.rs"}}' >"$PLAIN"
INPUT=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs"},"transcript_path":"%s"}' "$PLAIN")
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for a plain subagent, got exit $CODE"
    exit 1
fi

# A MAIN session transcript is untouched even when it contains the sentinel
# (the Agent tool_use that spawned a forwarder embeds the prompt verbatim)
MAIN="$TMP/session-main.jsonl"
printf '%s\n' '{"message":{"role":"assistant","content":"spawning LOOM-CODEX-FORWARD-ONLY prompt"}}' >"$MAIN"
INPUT=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs"},"transcript_path":"%s"}' "$MAIN")
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for a main-session transcript, got exit $CODE"
    exit 1
fi

# No classification metadata in the payload: fail closed
INPUT='{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs"}}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 with no agent_type or transcript_path, got exit $CODE"
    exit 1
fi
