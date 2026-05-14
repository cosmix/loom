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

source "$(dirname "$0")/_common.sh"

debug() {
	[[ "${COMMIT_FILTER_DEBUG:-}" == "1" ]] || return 0
	echo "$@" >&2
}

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

debug "=== $(date) ==="
debug "INPUT_JSON: $INPUT_JSON"

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, tool_input is an object with "command" field
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
else
	COMMAND=""
fi

# Strip embedded content (heredoc bodies, -m messages) for pattern matching
# This prevents false positives from words like "commit" appearing inside messages
STRIPPED_COMMAND=""
if [[ -n "$COMMAND" ]]; then
	STRIPPED_COMMAND=$(strip_embedded_content "$COMMAND")
fi

debug "TOOL_NAME: $TOOL_NAME"
debug "COMMAND: $COMMAND"
debug "STRIPPED_COMMAND: $STRIPPED_COMMAND"
debug "---"

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

# Check if a PID is in our ancestor chain
# Returns 0 if found, 1 if not
is_ancestor() {
	local target_pid="$1"
	local current_pid="$$"

	while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
		if [[ "$current_pid" == "$target_pid" ]]; then
			return 0
		fi

		# Get parent PID
		if [[ -r "/proc/$current_pid/stat" ]]; then
			current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
		else
			current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
		fi
	done

	return 1
}

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

		# Claude Code runs as node with "claude" in the binary/args
		# Exclude matches that are just hook scripts (paths containing .claude/hooks)
		if echo "$cmdline" | grep -qi "claude"; then
			if echo "$cmdline" | grep -q "\.claude/hooks"; then
				# This is a hook script, not Claude Code - skip it
				debug "DEBUG: Skipping PID $current_pid - hook script: $cmdline"
			else
				echo "$current_pid"
				return 0
			fi
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

# Count Claude processes between two PIDs (exclusive of start, inclusive of end)
# Returns the count. If end PID is not found, returns 999.
count_claude_processes_between() {
	local start_pid="$1"
	local end_pid="$2"
	local count=0

	local current_pid="$start_pid"
	# Move to parent first (start is exclusive)
	if [[ -r "/proc/$current_pid/stat" ]]; then
		current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
	else
		current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
	fi

	while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
		if [[ "$current_pid" == "$end_pid" ]]; then
			echo "$count"
			return 0
		fi

		# Check if this process is Claude Code (not a hook script)
		local cmdline=""
		if [[ -r "/proc/$current_pid/cmdline" ]]; then
			cmdline=$(tr '\0' ' ' <"/proc/$current_pid/cmdline" 2>/dev/null || true)
		else
			cmdline=$(ps -o command= -p "$current_pid" 2>/dev/null || true)
		fi

		if echo "$cmdline" | grep -qi "claude" && ! echo "$cmdline" | grep -q "\.claude/hooks"; then
			((count++))
		fi

		# Get parent PID
		if [[ -r "/proc/$current_pid/stat" ]]; then
			current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
		else
			current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
		fi
	done

	echo "999" # End PID not found
	return 0  # Don't return 1 - it triggers set -e and skips attribution check
}

if [[ -n "${LOOM_MAIN_AGENT_PID:-}" ]]; then
	# First, validate that LOOM_MAIN_AGENT_PID is actually in our ancestor chain
	# If it's not, it's stale (from a previous session) and should be ignored
	if ! is_ancestor "$LOOM_MAIN_AGENT_PID"; then
		debug "DEBUG: LOOM_MAIN_AGENT_PID=$LOOM_MAIN_AGENT_PID is NOT in ancestor chain - stale value, ignoring"
	else
		# Find the nearest Claude ancestor in our process tree
		NEAREST_CLAUDE=$(find_nearest_claude_ancestor)

		debug "DEBUG: LOOM_MAIN_AGENT_PID=$LOOM_MAIN_AGENT_PID, PPID=$PPID, NEAREST_CLAUDE=$NEAREST_CLAUDE"

		# Check if this is a subagent
		# Main agent: NEAREST_CLAUDE == LOOM_MAIN_AGENT_PID (same process after exec)
		# Subagent: NEAREST_CLAUDE != LOOM_MAIN_AGENT_PID (different Claude process)
		#
		# IMPORTANT: After `exec claude`, the wrapper PID IS the Claude process PID.
		# So for a main agent, NEAREST_CLAUDE == LOOM_MAIN_AGENT_PID.
		#
		# Process tree for main agent:
		#   wrapper (exec'd) → now Claude (LOOM_MAIN_AGENT_PID == NEAREST_CLAUDE)
		#   Claude count: 0 (same process)
		#
		# Process tree for subagent:
		#   wrapper → Claude (main) → ... → Claude (subagent = NEAREST_CLAUDE)
		#   NEAREST_CLAUDE != LOOM_MAIN_AGENT_PID
		if [[ -n "$NEAREST_CLAUDE" ]]; then
			# Fast path: if NEAREST_CLAUDE == LOOM_MAIN_AGENT_PID, we ARE the main agent
			# This happens because the wrapper uses `exec claude` which replaces the
			# shell process with Claude, inheriting the PID
			if [[ "$NEAREST_CLAUDE" == "$LOOM_MAIN_AGENT_PID" ]]; then
				CLAUDE_COUNT=0
				debug "DEBUG: Fast path - NEAREST_CLAUDE == LOOM_MAIN_AGENT_PID (same process after exec)"
			else
				CLAUDE_COUNT=$(count_claude_processes_between "$NEAREST_CLAUDE" "$LOOM_MAIN_AGENT_PID")
				debug "DEBUG: Claude processes between NEAREST_CLAUDE and LOOM_MAIN_AGENT_PID: $CLAUDE_COUNT"
			fi

			if [[ "$CLAUDE_COUNT" == "0" ]]; then
				# Main agent - no other Claude process between us and the wrapper
				debug "DEBUG: Main agent detected - no intermediate Claude processes"
			else
				# Subagent - there's another Claude process in between (the main agent)
				debug "DEBUG: Subagent detected - $CLAUDE_COUNT intermediate Claude process(es)"

				# Check if this is a git commit or loom stage complete command
				if echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\b(commit|add[[:space:]]+-A|add[[:space:]]+\.)\b'; then
					debug "DEBUG: BLOCKED - Subagent attempting git operation"

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
					debug "DEBUG: BLOCKED - Subagent attempting loom stage complete"

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
	fi
fi

# === CLAUDE ATTRIBUTION CHECK ===
# Block git commits with AI attribution (per CLAUDE.md rule 8)
# Checks multiple vectors: Co-Authored-By trailers, --trailer flag,
# --author flag, GIT_AUTHOR env vars, and attribution text patterns

# Check if this is a git commit command (use stripped command to avoid matching
# "commit" inside message text; require "commit" as a standalone word)
# Match "git ... commit" allowing options like -c between git and commit
if echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\bcommit\b'; then
	debug "DEBUG: Detected git commit command"

	BLOCKED_REASON=""

	# --- Check 1: Co-Authored-By trailer in message body ---
	# Use ORIGINAL command to catch real attribution in heredoc/message bodies
	# and multi-flag formats like: git commit -m "msg" -m "Co-Authored-By: ..."
	# No ^ anchor — Co-Authored-By can appear mid-line in multi-flag commits
	if echo "$COMMAND" | grep -qiE 'Co-Authored-By:.*\b(claude|anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="Co-Authored-By trailer in commit message"
	fi

	# --- Check 2: --trailer flag with attribution ---
	# Catches: --trailer "Co-Authored-By: Claude..." and --trailer="Co-Authored-By: Claude..."
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE -- '--trailer[[:space:]="'"'"']*Co-Authored-By:.*\b(claude|anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="--trailer flag with Co-Authored-By attribution"
	fi

	# --- Check 3: Signed-off-by trailer mentioning Claude/Anthropic ---
	# No ^ anchor — same multi-flag bypass as Check 1
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE 'Signed-off-by:.*\b(claude|anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="Signed-off-by trailer with AI attribution"
	fi

	# --- Check 4: --trailer flag with Signed-off-by attribution ---
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE -- '--trailer[[:space:]="'"'"']*Signed-off-by:.*\b(claude|anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="--trailer flag with Signed-off-by attribution"
	fi

	# --- Check 5: --author flag with Anthropic email ---
	# Catches: --author="Claude <noreply@anthropic.com>" but NOT --author="Claude Shannon <human@example.com>"
	# Only block when an Anthropic email is present (humans named Claude exist)
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE -- '--author[[:space:]="'"'"']*[^"'"'"']*\b(anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="--author flag with Anthropic email"
	fi

	# --- Check 6: GIT_AUTHOR_EMAIL env var with Anthropic domain ---
	# Catches: GIT_AUTHOR_EMAIL="noreply@anthropic.com" but NOT GIT_AUTHOR_NAME="Claude" alone
	# Only check EMAIL (not NAME) to avoid false positives for humans named Claude
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE 'GIT_AUTHOR_EMAIL[[:space:]]*=[[:space:]]*["'"'"']?[^"'"'"']*\b(anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="GIT_AUTHOR_EMAIL with Anthropic domain"
	fi

	# --- Check 7: GIT_COMMITTER_EMAIL env var with Anthropic domain ---
	# Mirrors Check 6 but for the committer identity
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE 'GIT_COMMITTER_EMAIL[[:space:]]*=[[:space:]]*["'"'"']?[^"'"'"']*\b(anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="GIT_COMMITTER_EMAIL with Anthropic domain"
	fi

	# --- Check 8: git -c trailer config injection ---
	# Catches: git -c trailer.co-authored-by.value="Claude <noreply@anthropic.com>" commit ...
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE -- '-c[[:space:]]+trailer\.[^[:space:]]*\b(claude|anthropic|noreply@anthropic)\b'; then
		BLOCKED_REASON="git -c trailer config with AI attribution"
	fi

	# --- Check 9: Attribution text patterns in commit message ---
	# Catches "Generated with Claude Code", "claude.ai/code", "claude.com/claude-code"
	# Uses ORIGINAL command to check inside message bodies
	if [[ -z "$BLOCKED_REASON" ]] && echo "$COMMAND" | grep -qiE 'Generated with.*(Claude Code|claude\.ai|claude\.com)'; then
		BLOCKED_REASON="'Generated with Claude Code' attribution text"
	fi

	if [[ -n "$BLOCKED_REASON" ]]; then
		debug "DEBUG: BLOCKED - $BLOCKED_REASON"

		# Output guidance to stderr and block
		cat >&2 <<EOF
BLOCKED: Commit contains forbidden attribution (CLAUDE.md rule 8).
Reason: $BLOCKED_REASON

Per project rules, AI attribution must NEVER appear in commits.

Please rewrite your git commit command WITHOUT any AI attribution.
Remove ALL of the following if present:
  - Co-Authored-By lines mentioning Claude/Anthropic
  - Signed-off-by lines mentioning Claude/Anthropic
  - --trailer flags adding AI attribution
  - --author flags referencing Claude/Anthropic
  - GIT_AUTHOR_NAME/EMAIL or GIT_COMMITTER_EMAIL environment variables
  - git -c trailer.* config overrides
  - "Generated with Claude Code" or similar text

The commit message should only contain your actual changes description.
Rewrite and try again.
EOF
		exit 2
	fi
fi

# Command is allowed
debug "Allowing command"
exit 0
