#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"

# agent_type identifies the forwarder even with no transcript_path at all
INPUT='{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs"},"agent_type":"loom-codex-forwarder"}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for Edit with agent_type=loom-codex-forwarder, got exit $CODE"
    exit 1
fi

# The plugin wrapper is pinned too - a direct spawn must still forward
INPUT='{"tool_name":"Bash","tool_input":{"command":"cargo check --lib"},"agent_type":"codex:codex-rescue"}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for non-companion Bash with agent_type=codex:codex-rescue, got exit $CODE"
    exit 1
fi

# The companion call itself passes the primary gate
INPUT='{"tool_name":"Bash","tool_input":{"command":"node /x/scripts/codex-companion.mjs task hello --write"},"agent_type":"codex:codex-rescue"}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for the companion call with agent_type=codex:codex-rescue, got exit $CODE"
    exit 1
fi

# Any other agent_type is untouched
INPUT='{"tool_name":"Edit","tool_input":{"file_path":"/tmp/x.rs"},"agent_type":"loom-software-engineer"}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 0 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 0 for Edit with agent_type=loom-software-engineer, got exit $CODE"
    exit 1
fi
