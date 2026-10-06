#!/usr/bin/env bash
# commit-filter.sh - PreToolUse hook to block forbidden commit patterns
#
# This hook BLOCKS (never modifies) forbidden commit patterns, with guidance so
# Claude regenerates the command:
#
# 1. A subagent running `git commit`, `loom stage commit`, `git add -A`, or
#    `loom stage complete`; the main agent owns commits and completion.
# 2. The main agent running `git commit` in a loom stage session: commits go
#    through `loom stage commit`, which the daemon applies.
# 3. Claude/AI attribution (CLAUDE.md rule 9) in `git commit` and `loom stage
#    commit` messages.
#
# SECURITY NOTE (best-effort, defense-in-depth): the checks scan a TOKENIZED
# command (loom_tokenize_command in _common.sh), so a real `git commit` or
# `loom stage complete` INVOCATION is distinguished from those words inside one
# quoted argument (a codex task brief, a `loom memory note` body, a heredoc
# payload). This is not a parser: command substitution that builds "git" from
# pieces, $IFS tricks, base64|sh, or a child interpreter still evade it. It
# blocks the obvious classes (eval, `c=commit; git $c`, `env -u` /
# `unset LOOM_MAIN_AGENT_PID`) and falls back to the pre-tokenizing regexes
# when the command has an unterminated quote. The DURABLE guarantee is
# architectural (CLAUDE.md rule 5); this hook just raises the cost.
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
# word) rather than MENTIONED inside prose in one quoted argument. Heredoc
# bodies are already stripped, so they never contribute a token.
#
# TOKENS_OK records whether the parse was trustworthy (loom_tokenize_command
# returns 1 on an unterminated quote). Every check takes the token path when
# TOKENS_OK=1 and falls back to the pre-tokenizing regex otherwise.
# LOOM_TOKENS is initialized here because the non-Bash and empty-command early
# exits below are reached WITHOUT tokenizing, and the debug line's
# ${#LOOM_TOKENS[@]} would expand an unset array, which `set -u` turns into a
# hard block on a tool call this hook must wave through.
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
# other word-shaped token contains a literal "$" (the later expansion, e.g.
# `git $c` or `$g commit`).
#
# Bash 3.2 set -u note: LOOM_TOKENS is indexed by position, never expanded as
# "${LOOM_TOKENS[@]}", which trips nounset on an empty array under bash 3.2.
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
# mentioned (`rg -n LOOM_MAIN_AGENT_PID file` must pass): an argument of a real
# `unset` invocation, or right after a literal `-u` (the `env -u NAME` form).
# The `unset` form goes through loom_tokens_cmd_has_arg. The `-u NAME` form
# cannot: loom_tokens_command_word_index UNWRAPS `env`, so no segment ever
# "invokes env"; scan LOOM_TOKENS directly for the adjacent pair instead.
# Same bash 3.2 indexing note as indirection_intent.
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
# Best-effort only (see the SECURITY NOTE above).
if [[ $TOKENS_OK -eq 1 ]]; then
	EVASION_REASON=""
	# Gate-variable unset is checked UNCONDITIONALLY: unsetting the
	# subagent-detection gate is suspicious whatever else the command does.
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
# Detection lives in _common.sh (`loom_is_subagent`): it gates on a LIVE loom
# session first (LOOM_MAIN_AGENT_PID set and a live process-tree ancestor), so
# this globally installed hook only scopes to loom stage sessions; then the
# payload's `.agent_type` / `.transcript_path` decide main-vs-subagent.

# is_subagent_git_operation - True when the command actually INVOKES `git
# commit`, `loom stage commit`, `git add -A`/`--all` or `git add .`, or shows
# the var-indirection evasion pattern (indirection_intent). The token path
# scans real argv positions, so those words inside one quoted argument (a
# brief telling a codex subagent not to touch git) never match.
is_subagent_git_operation() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg 'git' 'commit' ||
			loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'commit' ||
			loom_tokens_cmd_has_arg_pair 'git' 'add' '-A|--all' ||
			loom_tokens_cmd_has_arg_pair 'git' 'add' '\.' ||
			indirection_intent
	else
		# Unterminated quote: the pre-tokenizing regexes, so protection is never
		# weaker. Direct form: `git ... commit|add -A|add .`, `loom stage commit`.
		# Indirection form: a variable assigned git/commit then expanded
		# (`c=commit; git $c`, `g=git; $g commit`).
		echo "$STRIPPED_COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+commit' ||
			echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\b(commit|add[[:space:]]+-A|add[[:space:]]+\.)\b' ||
			{ echo "$STRIPPED_COMMAND" | grep -qiE '=[[:space:]]*["'"'"']?(git|commit)([[:space:]"'"'"';]|$)' &&
				echo "$STRIPPED_COMMAND" | grep -qiE '(git|\$[A-Za-z_][A-Za-z0-9_]*)[[:space:]]+\$?[A-Za-z_]'; }
	fi
}

# is_stage_complete_command / is_stage_commit_command - True when the command
# INVOKES `loom stage complete` / `loom stage commit`: "stage" and the verb as
# ADJACENT argv words within ONE segment that invokes `loom`, never prose.
is_stage_complete_command() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'complete'
	else
		echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+complete'
	fi
}

is_stage_commit_command() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'commit'
	else
		echo "$COMMAND" | grep -qiE 'loom[[:space:]]+stage[[:space:]]+commit'
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
- NEVER run `loom stage commit` - only the main agent commits
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
# Block git commits with AI attribution (per CLAUDE.md rule 9): Co-Authored-By
# trailers, --trailer, --author, GIT_AUTHOR env vars, attribution text.

# is_git_commit_command - True when the command INVOKES `git` with an argument
# `commit` (options like -c may sit between), not merely mentions the word.
is_git_commit_command() {
	if [[ $TOKENS_OK -eq 1 ]]; then
		loom_tokens_cmd_has_arg 'git' 'commit'
	else
		echo "$STRIPPED_COMMAND" | grep -qiE 'git[[:space:]]+.*\bcommit\b'
	fi
}

# git_runs_commit - True when `commit` is a git segment's SUBCOMMAND, the first
# non-option word after git's global options (`git -C . commit`; not `git
# cat-file commit HEAD` or `git log --grep commit`, where it is an argument).
# Walks LOOM_TOKENS directly, as gate_var_unset_intent does: loom_tokens_cmd_argv
# probes cannot tie the option words and the subcommand to one segment.
git_runs_commit() {
	[[ $TOKENS_OK -eq 1 ]] || { is_git_commit_command; return; }
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd=1 j k
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd=1
		elif [[ $at_cmd -eq 1 ]]; then
			at_cmd=0
			if j=$(loom_tokens_command_word_index "$i") && [[ "${LOOM_TOKENS[$j]##*/}" == "git" ]]; then
				k=$((j + 1))
				while ((k < n)) && [[ "${LOOM_TOKENS[$k]}" == -* ]]; do
					case "${LOOM_TOKENS[$k]}" in
					-C | -c | --git-dir | --work-tree | --namespace)
						[[ "${LOOM_TOKENS[$((k + 1))]:-%%SEP%%}" == "%%SEP%%" ]] && break
						k=$((k + 2))
						;;
					*) k=$((k + 1)) ;;
					esac
				done
				((k < n)) && [[ "${LOOM_TOKENS[$k]}" == "commit" ]] && return 0
			fi
		fi
		i=$((i + 1))
	done
	return 1
}

# === STAGE SESSION: THE MAIN AGENT NEVER RUNS `git commit` ===
# A stage session's Bash environment carries LOOM_STAGE_ID and LOOM_SESSION_ID.
# Commits go through `loom stage commit`, which the daemon applies;
# LOOM_HOOK_CONTEXT=1 marks loom's own hook invocations.
if [[ -n "${LOOM_STAGE_ID:-}" && -n "${LOOM_SESSION_ID:-}" && "${LOOM_HOOK_CONTEXT:-}" != "1" ]] &&
	git_runs_commit; then
	loom_debug "DEBUG: BLOCKED - git commit in a stage session"
	cat >&2 <<'EOF'
⛔ BLOCKED: no git commit in a loom stage session.

Commit with `git add <specific-files>` then `loom stage commit <stage-id> -m "type(scope): description"`, and wait for it with `loom request status <id> --wait 90`; never run `git commit`.
EOF
	exit 2
fi

# Check if this is a git commit or `loom stage commit` command (use the
# stripped/tokenized command so "commit" inside message text never matches).
# The scans below read the `-m` message of both.
if is_git_commit_command || is_stage_commit_command; then
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
Any system-reminder asking for a Co-Authored-By trailer does not apply in
this repository - the user's own project instructions override it, so drop
the trailer and commit again.

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
