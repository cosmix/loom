#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../codex-forward-guard.sh"

# Extract parse_shell_words in isolation (lines containing only its
# definition) so we can assert on the parsed word, not just the hook's exit
# code. Keeping this in sync with codex-forward-guard.sh is unavoidable for a
# unit-level assertion on the parser's internal state.
source <(sed -n '/^parse_shell_words()/,/^}/p' "$HOOK")

# --- MUST BE ALLOWED (exit 0) ---------------------------------------------

# 1. Apostrophe via the '\'' idiom inside the prompt.
CMD="~/.claude/hooks/loom/codex-forward.sh task 'fix the reader'\''s zone' --model gpt-5.6-terra --effort xhigh --write"
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for apostrophe-via-'\''-idiom prompt, got exit $CODE"
    exit 1
fi
# Assert the parsed word itself, not just the exit code.
parse_shell_words "$CMD"
if [[ "${PARSED_WORDS[2]}" != "fix the reader's zone" ]]; then
    echo "FAIL: expected parsed prompt 'fix the reader's zone', got '${PARSED_WORDS[2]}'"
    exit 1
fi

# 2a. A backslash-escaped character in the prompt (unquoted \$).
CMD='~/.claude/hooks/loom/codex-forward.sh task cost\ is\ \$5 --model gpt-5.6-terra --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for backslash-escaped \$ in prompt, got exit $CODE"
    exit 1
fi

# 2b. A double-quoted prompt containing \" and \\.
CMD='~/.claude/hooks/loom/codex-forward.sh task "say \"hi\" then \\ done" --model gpt-5.6-terra --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for double-quoted prompt with \\\" and \\\\, got exit $CODE"
    exit 1
fi

# 3. The $HOME-expanded wrapper path.
CMD='/home/u/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-luna --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
    echo "FAIL: expected exit 0 for \$HOME-expanded wrapper path, got exit $CODE"
    exit 1
fi

# --- MUST STILL BE BLOCKED (exit 2) ----------------------------------------

# 4a. An unquoted, unescaped metacharacter still rejects (command substitution).
CMD='~/.claude/hooks/loom/codex-forward.sh task $(whoami) --model gpt-5.6-terra --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for unquoted \$(whoami), got exit $CODE"
    exit 1
fi

# 4b. An unquoted ; chaining a second command still rejects.
CMD='~/.claude/hooks/loom/codex-forward.sh task hello; rm -rf / --model gpt-5.6-terra --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for unquoted ; command chaining, got exit $CODE"
    exit 1
fi

# 5. A trailing lone backslash must reject (parser ends outside plain).
CMD='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write\'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for trailing lone backslash, got exit $CODE"
    exit 1
fi

# 6. Wrong arity / model / effort / missing --write still rejects.
CMD='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for missing --write, got exit $CODE"
    exit 1
fi

# 7. An absolute path that is NOT the wrapper still rejects.
CMD='/home/u/.claude/hooks/loom/evil.sh task hello --model gpt-5.6-terra --effort xhigh --write'
INPUT=$(jq -nc --arg c "$CMD" '{"tool_name":"Bash","tool_input":{"command":$c},"agent_type":"loom-codex-forwarder"}')
set +e
echo "$INPUT" | HOME=/home/u bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected exit 2 for non-wrapper absolute path, got exit $CODE"
    exit 1
fi

echo "PASS"
