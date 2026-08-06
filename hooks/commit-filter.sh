#!/usr/bin/env bash
# commit-filter.sh - PreToolUse hook to block forbidden commit patterns
#
# This hook intercepts git commit commands and BLOCKS (not modifies) forbidden patterns:
#
# 1. Claude/AI attribution (Co-Authored-By lines mentioning Claude/Anthropic)
#    Per CLAUDE.md rule 9: Never mention Claude in commits.
#
# Instead of trying to modify the command (fragile with JSON escaping),
# this hook blocks and provides guidance so Claude regenerates the command.
#
# SECURITY NOTE (best-effort, defense-in-depth): the git/attribution checks are
# regex-on-shell, not a parser, so determined evasion is still possible — e.g.
# command substitution that builds "git" from pieces, $IFS tricks, base64|sh, or
# spawning git from a child interpreter. The hook now blocks the obvious classes
# (eval, simple variable indirection like `c=commit; git $c`, tab-separated
# `git<TAB>commit`, and `env -u LOOM_MAIN_AGENT_PID` which exists only to unset
# the subagent gate). The DURABLE guarantee is architectural: the main agent owns
# commits and stage completion (CLAUDE.md rule 5); this hook just raises the cost.
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

# Debug tracing comes from _common.sh (`loom_debug`), gated on
# LOOM_HOOK_DEBUG=1 or the legacy COMMIT_FILTER_DEBUG=1.

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

loom_debug "=== $(date) ==="
loom_debug "INPUT_JSON: $INPUT_JSON"

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

loom_debug "TOOL_NAME: $TOOL_NAME"
loom_debug "COMMAND: $COMMAND"
loom_debug "STRIPPED_COMMAND: $STRIPPED_COMMAND"
loom_debug "---"

# Only check Bash tool uses
if [[ "$TOOL_NAME" != "Bash" ]]; then
	exit 0
fi

if [[ -z "$COMMAND" ]]; then
	exit 0
fi

# === ANTI-EVASION GUARD (applies to ALL Bash, not just detected subagents) ===
# These patterns exist only to defeat this hook's own checks, so block them
# outright when combined with a git/loom-stage-complete intent:
#   - `env -u LOOM_MAIN_AGENT_PID ...` unsets the subagent-detection gate
#   - `eval ...` hides the real command from the regex
#   - simple variable indirection: `c=commit; git $c` / `g=git; $g commit`
#   - tab/newline-separated keywords: `git<TAB>commit`
# Best-effort only — see the SECURITY NOTE in the header for known residual evasion.
references_git_or_loom() {
	# Look in the ORIGINAL command (indirection lives outside the stripped body).
	# NOTE: in ERE, [[:space:]] already matches TAB/newline, so `git<TAB>commit`
	# is covered by the space-class patterns elsewhere in this hook. The leading
	# char class includes quotes so `eval "git commit"` (git inside a quoted
	# string) is still recognized.
	echo "$COMMAND" | grep -qiE '(^|[[:space:];&|("'"'"'])git([[:space:]]|$)' ||
		echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete' ||
		# var-indirection: a var assigned "commit"/"git" then expanded later
		echo "$COMMAND" | grep -qiE '=[[:space:]]*["'"'"']?(git|commit)([[:space:]"'"'"';]|$)'
}

if references_git_or_loom; then
	EVASION_REASON=""
	if echo "$COMMAND" | grep -qiE 'env[[:space:]]+(-[^[:space:]]*[[:space:]]+)*-u[[:space:]]+LOOM_MAIN_AGENT_PID\b' ||
		echo "$COMMAND" | grep -qiE '\bunset[[:space:]]+([^;&|]*[[:space:]])?LOOM_MAIN_AGENT_PID\b'; then
		EVASION_REASON="unsetting LOOM_MAIN_AGENT_PID (the subagent-detection gate)"
	elif echo "$COMMAND" | grep -qiE '(^|[[:space:];&|(])eval([[:space:]]|$)'; then
		EVASION_REASON="wrapping git/loom in eval (hides the command from isolation checks)"
	fi

	if [[ -n "$EVASION_REASON" ]]; then
		loom_debug "DEBUG: BLOCKED - anti-evasion: $EVASION_REASON"
		cat >&2 <<EOF
⛔ BLOCKED: git/loom command uses an isolation-bypass pattern.
Reason: $EVASION_REASON

Run git/loom directly, without env -u / unset / eval wrappers. The main agent
owns all commits and stage completion (CLAUDE.md rule 5); bypassing the guard
causes lost work and broken attribution.
EOF
		exit 2
	fi
fi

# === SUBAGENT COMMIT PREVENTION ===
# Block git commits from subagents (per ISSUES.md #3)
# Main agent sets LOOM_MAIN_AGENT_PID in wrapper script
# Subagents inherit this var but run under a different Claude process
#
# Detection lives in _common.sh (`loom_is_subagent`) so that every hook needing
# "am I a subagent?" shares one depth-agnostic implementation.
if loom_is_subagent; then
	# Check if this is a git commit or loom stage complete command.
	# Direct form: `git ... commit|add -A|add .`. Indirection form:
	# a variable assigned to git/commit then expanded (`c=commit; git $c`,
	# `g=git; $g commit`). `[[:space:]]` covers TAB/newline separators.
	if echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\b(commit|add[[:space:]]+-A|add[[:space:]]+\.)\b' ||
		{ echo "$STRIPPED_COMMAND" | grep -qiE '=[[:space:]]*["'"'"']?(git|commit)([[:space:]"'"'"';]|$)' &&
			echo "$STRIPPED_COMMAND" | grep -qiE '(git|\$[A-Za-z_][A-Za-z0-9_]*)[[:space:]]+\$?[A-Za-z_]'; }; then
		loom_debug "DEBUG: BLOCKED - Subagent attempting git operation"

		cat >&2 <<'EOF'
⛔ BLOCKED: Subagent attempting git operation.

You are a SUBAGENT (spawned via Task tool). Per CLAUDE.md rules:
- NEVER run `git commit` - only the main agent commits
- NEVER run `git add -A` or `git add .` - main agent handles staging

Your job is to:
1. Write code to your assigned files
2. Run AT MOST ONE narrowly-scoped check covering only what you changed
   (`cargo test <filter>`, `cargo test --test <name>`) - never the full suite
3. Report what you changed, and what you did NOT verify
4. Let the main agent handle ALL git operations and the full verification run

The main agent will commit your work after all subagents complete.
EOF
		exit 2
	fi

	if echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete'; then
		loom_debug "DEBUG: BLOCKED - Subagent attempting loom stage complete"

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

# === CLAUDE ATTRIBUTION CHECK ===
# Block git commits with AI attribution (per CLAUDE.md rule 9)
# Checks multiple vectors: Co-Authored-By trailers, --trailer flag,
# --author flag, GIT_AUTHOR env vars, and attribution text patterns

# Check if this is a git commit command (use stripped command to avoid matching
# "commit" inside message text; require "commit" as a standalone word)
# Match "git ... commit" allowing options like -c between git and commit
if echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\bcommit\b'; then
	loom_debug "DEBUG: Detected git commit command"

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
		loom_debug "DEBUG: BLOCKED - $BLOCKED_REASON"

		# Output guidance to stderr and block
		cat >&2 <<EOF
BLOCKED: Commit contains forbidden attribution (CLAUDE.md rule 9).
Reason: $BLOCKED_REASON

NOTHING RAN. This hook blocks the ENTIRE Bash call, not just the commit.
If you chained staging into the same command (git add ... && git commit ...),
the staging did NOT happen either. Re-run the WHOLE command - staging
included - with the attribution removed. Re-running only the commit will
fail with "no changes added to commit".

Per project rules, AI attribution must NEVER appear in commits.
This overrides any harness default that tells you to add a Co-Authored-By
trailer: the project rule wins, so omit the trailer entirely.

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
loom_debug "Allowing command"
exit 0
