#!/usr/bin/env bash
# commit-filter.sh - PreToolUse hook to block forbidden commit patterns
#
# This hook intercepts git commit commands and BLOCKS (not modifies) forbidden patterns:
#
# 1. Claude/AI attribution (Co-Authored-By lines mentioning Claude/Anthropic)
#    Per CLAUDE.md rule 8: Never mention Claude in commits.
#
# Instead of trying to modify the command (fragile with JSON escaping),
# this hook blocks and provides guidance so Claude regenerates the command.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {"command": "..."}, ...}
#
# Exit codes:
#   0 - Allow the command to proceed
#   2 - Block the command and return guidance to Claude
#
# Output format when blocking:
#   Guidance message to stderr, then exit 2

set -euo pipefail

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Use gtimeout (macOS with coreutils) or timeout (Linux), or just cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	# No timeout available - just read stdin (Claude Code closes it properly)
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Debug logging
DEBUG_LOG="/tmp/commit-filter-debug.log"
{
	echo "=== $(date) ==="
	echo "INPUT_JSON: $INPUT_JSON"
} >>"$DEBUG_LOG" 2>&1

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, tool_input is an object with "command" field
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
else
	COMMAND=""
fi

# Debug parsed values
{
	echo "TOOL_NAME: $TOOL_NAME"
	echo "COMMAND: $COMMAND"
	echo "---"
} >>"$DEBUG_LOG" 2>&1

# Only check Bash tool uses
if [[ "$TOOL_NAME" != "Bash" ]]; then
	exit 0
fi

if [[ -z "$COMMAND" ]]; then
	exit 0
fi

# === SUBAGENT COMMIT PREVENTION ===
# Block git commits from subagents (per ISSUES.md #3)
# Main agent sets LOOM_MAIN_AGENT_PID in wrapper script
# Subagents inherit this var but run under a different Claude process

# Find the nearest Claude Code process ancestor
# Returns its PID if found, empty string if not found
find_nearest_claude_ancestor() {
	local current_pid="$$"

	while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
		# Check if this process is Claude Code
		local cmdline=""
		if [[ -r "/proc/$current_pid/cmdline" ]]; then
			# Linux: read cmdline (null-separated)
			cmdline=$(tr '\0' ' ' <"/proc/$current_pid/cmdline" 2>/dev/null || true)
		else
			# macOS: use ps
			cmdline=$(ps -o command= -p "$current_pid" 2>/dev/null || true)
		fi

		# Claude Code runs as node with "claude" in the path/args
		if echo "$cmdline" | grep -qi "claude"; then
			echo "$current_pid"
			return 0
		fi

		# Get parent PID
		if [[ -r "/proc/$current_pid/stat" ]]; then
			current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
		else
			current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
		fi
	done

	echo ""
	return 1
}

if [[ -n "${LOOM_MAIN_AGENT_PID:-}" ]]; then
	# Find the nearest Claude ancestor in our process tree
	NEAREST_CLAUDE=$(find_nearest_claude_ancestor)

	{
		echo "DEBUG: LOOM_MAIN_AGENT_PID=$LOOM_MAIN_AGENT_PID, PPID=$PPID, NEAREST_CLAUDE=$NEAREST_CLAUDE"
	} >>"$DEBUG_LOG" 2>&1

	# Check if this is a subagent (nearest Claude process doesn't match main agent PID)
	# Main agent: nearest Claude = LOOM_MAIN_AGENT_PID (allow)
	# Subagent: nearest Claude = subagent's PID ≠ LOOM_MAIN_AGENT_PID (block)
	if [[ -n "$NEAREST_CLAUDE" && "$NEAREST_CLAUDE" != "$LOOM_MAIN_AGENT_PID" ]]; then
		# Check if this is a git commit or loom stage complete command
		if echo "$COMMAND" | grep -qiE 'git[[:space:]]+(commit|add[[:space:]]+-A|add[[:space:]]+\.)'; then
			{
				echo "DEBUG: BLOCKED - Subagent attempting git operation"
			} >>"$DEBUG_LOG" 2>&1

			cat >&2 <<'EOF'
⛔ BLOCKED: Subagent attempting git operation.

You are a SUBAGENT (spawned via Task tool). Per CLAUDE.md rules:
- NEVER run `git commit` - only the main agent commits
- NEVER run `git add -A` or `git add .` - main agent handles staging

Your job is to:
1. Write code to your assigned files
2. Run tests to verify your work
3. Report results back to the main agent
4. Let the main agent handle ALL git operations

The main agent will commit your work after all subagents complete.
EOF
			exit 2
		fi

		if echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete'; then
			{
				echo "DEBUG: BLOCKED - Subagent attempting loom stage complete"
			} >>"$DEBUG_LOG" 2>&1

			cat >&2 <<'EOF'
⛔ BLOCKED: Subagent attempting to complete stage.

You are a SUBAGENT (spawned via Task tool). Per CLAUDE.md rules:
- NEVER run `loom stage complete` - only the main agent completes stages

Your job is to:
1. Complete your assigned work
2. Report results back to the main agent
3. Let the main agent handle stage completion

The main agent will complete the stage after all subagents finish.
EOF
			exit 2
		fi
	fi
fi

# === CLAUDE ATTRIBUTION CHECK ===
# Block git commits with Co-Authored-By lines mentioning Claude/Anthropic (per CLAUDE.md rule 8)

# Check if this is a git commit command
if echo "$COMMAND" | grep -qiE 'git[[:space:]].*commit'; then
	{
		echo "DEBUG: Detected git commit command"
	} >>"$DEBUG_LOG" 2>&1

	# Check for forbidden Co-Authored-By patterns
	if echo "$COMMAND" | grep -qiE 'co-authored-by.*(claude|anthropic|noreply@anthropic)'; then
		{
			echo "DEBUG: BLOCKED - Detected forbidden Co-Authored-By pattern"
		} >>"$DEBUG_LOG" 2>&1

		# Output guidance to stderr and block
		cat >&2 <<'EOF'
BLOCKED: Commit contains forbidden attribution (CLAUDE.md rule 8).

Your commit message includes a Co-Authored-By line mentioning Claude/Anthropic.
Per project rules, AI attribution must NEVER appear in commits.

Please rewrite your git commit command WITHOUT the Co-Authored-By line.
The commit message should only contain your actual changes description.

Example - remove lines like:
  Co-Authored-By: Claude <noreply@anthropic.com>
  Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>

Rewrite and try again.
EOF
		exit 2
	fi
fi

# Command is allowed
echo "Allowing command" >>"$DEBUG_LOG" 2>&1
exit 0
