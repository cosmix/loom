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
# SECURITY NOTE (best-effort, defense-in-depth): the git/loom-completion
# checks scan a TOKENIZED command (loom_tokenize_command in _common.sh)
# rather than regex-matching the raw string, so a real `git commit` or `loom
# stage complete` INVOCATION is distinguished from those same words sitting
# inside one quoted argument - a codex task brief, a `loom memory note`
# body, a heredoc test payload. This is still not a parser, so determined
# evasion is still possible - e.g. command substitution that builds "git"
# from pieces, $IFS tricks, base64|sh, or spawning git from a child
# interpreter. The hook still blocks the obvious classes via the same token
# scan (eval, simple variable indirection like `c=commit; git $c`, and `env
# -u LOOM_MAIN_AGENT_PID` / `unset LOOM_MAIN_AGENT_PID`, which exist only to
# unset the subagent gate), and falls back to the pre-tokenizing regexes
# verbatim when the command does not tokenize cleanly (an unterminated
# quote - not valid bash anyway, so today's protection is never weaker than
# it was). The DURABLE guarantee is architectural: the main agent owns
# commits and stage completion (CLAUDE.md rule 5); this hook just raises the
# cost.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {"command": "..."}, ...}
#
# Exit codes:
#   0 - Allow the command to proceed
#   2 - Block the command and return guidance to Claude
#   2 - jq not installed (fail closed)
#
# Output format when blocking:
#   Guidance message to stderr, then exit 2

set -euo pipefail

source "$(dirname "$0")/_common.sh"
loom_require_jq "commit-filter.sh"

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

# Tokenize the heredoc/-m-stripped command once, for every check below that
# needs to know whether git/loom is actually INVOKED (a real argv command
# word) rather than merely MENTIONED inside prose sitting in one quoted
# argument (a codex task brief, a `loom memory note` body). Because
# STRIPPED_COMMAND already dropped heredoc bodies entirely, tokenizing it
# also means a heredoc BODY can never contribute a token - fixing the
# "unsetting LOOM_MAIN_AGENT_PID" false positive that a raw regex over the
# unstripped command used to trip on test-data text inside a heredoc.
#
# TOKENS_OK records whether the parse was trustworthy - loom_tokenize_command
# returns 1 on an unterminated quote (not valid bash anyway). Every check
# below takes the token path when TOKENS_OK=1 and falls back to the
# pre-tokenizing regex otherwise, mirroring git-add-guard.sh's own
# tokenize-or-fall-back structure so today's protection is never weaker than
# it was.
# LOOM_TOKENS is initialized here, not just inside loom_tokenize_command: the
# non-Bash and empty-command early exits below are reached WITHOUT tokenizing,
# and the debug line's ${#LOOM_TOKENS[@]} would then expand an unset array,
# which `set -u` turns into a non-zero exit - i.e. a hard block on a tool call
# this hook is supposed to wave through.
LOOM_TOKENS=()
TOKENS_OK=0
if [[ -n "$STRIPPED_COMMAND" ]] && loom_tokenize_command "$STRIPPED_COMMAND"; then
	TOKENS_OK=1
fi

loom_debug "TOOL_NAME: $TOOL_NAME"
loom_debug "COMMAND: $COMMAND"
loom_debug "STRIPPED_COMMAND: $STRIPPED_COMMAND"
loom_debug "TOKENS_OK: $TOKENS_OK (${#LOOM_TOKENS[@]} token(s))"
loom_debug "---"

# Only check Bash tool uses
if [[ "$TOOL_NAME" != "Bash" ]]; then
	exit 0
fi

if [[ -z "$COMMAND" ]]; then
	exit 0
fi

# mentions_git_or_loom_raw - Best-effort RAW substring check (not
# token-based) for whether $STRIPPED_COMMAND mentions git or "loom stage
# complete". Used only as the eval-evasion conjunct below: the token scan
# cannot see inside an eval'd string - `eval "git commit"` hides `git`
# inside ONE quoted token, never at a command position - so the intent half
# necessarily stays a raw match; requiring `eval` itself at a real command
# position (loom_tokens_invoke) is what prose cannot fake.
mentions_git_or_loom_raw() {
	echo "$STRIPPED_COMMAND" | grep -qiE '(^|[[:space:];&|("'"'"'])git([[:space:]]|$)' ||
		echo "$STRIPPED_COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete'
}

# indirection_intent - True when the tokenized (heredoc-stripped) command
# shows the git/commit variable-indirection evasion pattern: a bare argv
# word assigns "git" or "commit" to a variable (`c=commit`, `g=git`) AND some
# other word-shaped token in the command contains a literal "$" (the later
# expansion, e.g. `git $c` or `$g commit`). Replaces the raw regex pair
# previously used at the subagent-git-operation check below, which prose can
# also trigger (a task brief containing `const x = "commit"` matches the
# assignment half alone, with no expansion anywhere near it).
#
# Bash 3.2 set -u note: this indexes LOOM_TOKENS by position up to its
# length rather than expanding "${LOOM_TOKENS[@]}" directly, mirroring the
# helpers in _common.sh, because that expansion trips nounset on an empty
# array under bash 3.2.
indirection_intent() {
	loom_tokens_word_matches '^[A-Za-z_][A-Za-z0-9_]*=["'"'"']?(git|commit)["'"'"']?$' || return 1

	local n=${#LOOM_TOKENS[@]}
	local i tok
	for ((i = 0; i < n; i++)); do
		tok="${LOOM_TOKENS[$i]}"
		loom_token_is_word "$tok" || continue
		[[ "$tok" == *'$'* ]] && return 0
	done
	return 1
}

# gate_var_unset_intent - True when the tokenized command shows the gate
# variable, LOOM_MAIN_AGENT_PID, actually being UNSET rather than merely
# mentioned: either as an argument of a real `unset` invocation, or
# immediately after a literal `-u` flag (the `env -u NAME` form). A bare
# loom_tokens_word_matches check on the variable name is too broad - it
# fires on ANY standalone argv word equal to the gate variable, including
# something as innocuous as `rg -n LOOM_MAIN_AGENT_PID hooks/_common.sh`
# searching this very file for the name.
#
# The `unset` form is expressible via loom_tokens_cmd_has_arg, which already
# unwraps wrapper commands and skips VAR=value prefixes when resolving
# `unset` as the effective command word. The `-u NAME` form is NOT
# expressible the same way: loom_tokens_command_word_index UNWRAPS `env`
# while resolving the effective command word for every other check, so no
# segment ever "invokes env" for loom_tokens_cmd_has_arg_pair to match
# against. Scan LOOM_TOKENS directly for the literal adjacent pair instead.
#
# Bash 3.2 set -u note: indexes LOOM_TOKENS by position rather than
# expanding "${LOOM_TOKENS[@]}" directly, same reason as indirection_intent.
gate_var_unset_intent() {
	local gate_var="LOOM_MAIN_AGENT_PID"

	loom_tokens_cmd_has_arg 'unset' "$gate_var" && return 0

	local n=${#LOOM_TOKENS[@]}
	local i
	for ((i = 0; i + 1 < n; i++)); do
		if [[ "${LOOM_TOKENS[$i]}" == "-u" && "${LOOM_TOKENS[$((i + 1))]}" == "$gate_var" ]]; then
			return 0
		fi
	done
	return 1
}

# block_anti_evasion - Shared exit path for the anti-evasion guard below, so
# the token path and the regex-fallback path emit identical guidance.
block_anti_evasion() {
	local reason="$1"
	cat >&2 <<EOF
⛔ BLOCKED: git/loom command uses an isolation-bypass pattern.
Reason: $reason

Run git/loom directly, without env -u / unset / eval wrappers. The main agent
owns all commits and stage completion (CLAUDE.md rule 5); bypassing the guard
causes lost work and broken attribution.
EOF
	exit 2
}

# === ANTI-EVASION GUARD (applies to ALL Bash, not just detected subagents) ===
# These patterns exist only to defeat this hook's own checks:
#   - `env -u LOOM_MAIN_AGENT_PID ...` / `unset LOOM_MAIN_AGENT_PID` unsets
#     the subagent-detection gate
#   - `eval ...` hides the real command from the scan
# Best-effort only - see the SECURITY NOTE in the header for known residual
# evasion.
if [[ $TOKENS_OK -eq 1 ]]; then
	EVASION_REASON=""
	# Gate-variable unset is checked UNCONDITIONALLY - not gated behind any
	# git/loom "intent" test - because unsetting the subagent-detection gate
	# is suspicious regardless of what else the command does.
	# gate_var_unset_intent requires the variable to actually be the OPERAND
	# of `unset` or of `env -u`, not merely a bare argv word anywhere in the
	# command - so a `rg -n LOOM_MAIN_AGENT_PID hooks/_common.sh` search, a
	# legitimate `$LOOM_MAIN_AGENT_PID` expansion, or prose sitting inside
	# one whitespace-bearing quoted token can never trip this.
	if gate_var_unset_intent; then
		EVASION_REASON="unsetting LOOM_MAIN_AGENT_PID (the subagent-detection gate)"
	elif loom_tokens_invoke 'eval' && mentions_git_or_loom_raw; then
		EVASION_REASON="wrapping git/loom in eval (hides the command from isolation checks)"
	fi

	if [[ -n "$EVASION_REASON" ]]; then
		loom_debug "DEBUG: BLOCKED - anti-evasion: $EVASION_REASON"
		block_anti_evasion "$EVASION_REASON"
	fi
else
	# Fallback: the command has an unterminated quote, so it is not valid
	# bash anyway and loom_tokenize_command could not produce a trustworthy
	# token list. Fall back to the regex patterns this hook used before
	# tokenizing existed, so today's protection is never weaker than it was.
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
			block_anti_evasion "$EVASION_REASON"
		fi
	fi
fi

# === SUBAGENT COMMIT PREVENTION ===
# Block git commits from subagents (per ISSUES.md #3)
#
# Detection lives in _common.sh (`loom_is_subagent`) so that every hook needing
# "am I a subagent?" shares one implementation. It gates on a LIVE loom
# session FIRST (LOOM_MAIN_AGENT_PID set and a live process-tree ancestor) -
# this hook installs globally at ~/.claude/hooks/loom/, so that precondition
# is the only thing scoping the block to a loom stage session rather than
# every Claude Code session on the machine. Only once that gate passes does
# the hook payload's `.agent_type` / `.transcript_path` (from $INPUT_JSON,
# captured above) decide main-vs-subagent outright - it identifies a genuine
# Task-tool subagent even when it runs IN-PROCESS (no separate Claude process
# to find) and is immune to a Bash-tool shell's cmdline merely mentioning a
# ~/.claude/ path. A process-tree walk is only the further fallback for a
# payload that answers neither field.

# is_subagent_git_operation - True when the command actually INVOKES `git
# commit`, `git add -A`/`--all`, or `git add .`, or shows the
# var-indirection evasion pattern (indirection_intent). The token path scans
# real argv positions, so a task brief that merely CONTAINS the words "git
# commit" inside one quoted argument - a HARD CONSTRAINTS bullet telling a
# codex subagent not to touch git, for example - can never match: `git`
# never sits at a command position when it is only a substring of a single
# whitespace-bearing quoted token.
is_subagent_git_operation() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg 'git' 'commit' ||
			loom_tokens_cmd_has_arg_pair 'git' 'add' '-A|--all' ||
			loom_tokens_cmd_has_arg_pair 'git' 'add' '\.' ||
			indirection_intent
	else
		# Fallback: unterminated quote - preserve the original regex
		# behaviour verbatim so today's protection is never weaker than it
		# was.
		#
		# Direct form: `git ... commit|add -A|add .`. Indirection form:
		# a variable assigned to git/commit then expanded (`c=commit; git $c`,
		# `g=git; $g commit`). `[[:space:]]` covers TAB/newline separators.
		echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\b(commit|add[[:space:]]+-A|add[[:space:]]+\.)\b' ||
			{ echo "$STRIPPED_COMMAND" | grep -qiE '=[[:space:]]*["'"'"']?(git|commit)([[:space:]"'"'"';]|$)' &&
				echo "$STRIPPED_COMMAND" | grep -qiE '(git|\$[A-Za-z_][A-Za-z0-9_]*)[[:space:]]+\$?[A-Za-z_]'; }
	fi
}

# is_stage_complete_command - True when the command actually INVOKES `loom
# stage complete` - "stage" and "complete" as ADJACENT argv words within the
# SAME segment that invokes `loom` (loom_tokens_cmd_has_arg_pair) - not
# merely mentions those words inside prose. Previously this checked
# argv[1]=="stage" and argv[2]=="complete" as two INDEPENDENT segment scans
# ANDed together, which could each be satisfied by a DIFFERENT segment -
# `loom stage list && loom log complete` false-positived as a stage
# completion even though neither segment invokes `stage complete` together.
is_stage_complete_command() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'complete'
	else
		echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete'
	fi
}

if loom_is_subagent "$INPUT_JSON"; then
	# Check if this is a git commit or loom stage complete command.
	if is_subagent_git_operation; then
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

	if is_stage_complete_command; then
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

# is_git_commit_command - True when the command actually INVOKES `git
# commit` (allowing options like -c between git and commit), not merely
# contains the word "commit" inside message text elsewhere.
is_git_commit_command() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg 'git' 'commit'
	else
		echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\bcommit\b'
	fi
}

# Check if this is a git commit command (use the stripped/tokenized command
# to avoid matching "commit" inside message text)
if is_git_commit_command; then
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
