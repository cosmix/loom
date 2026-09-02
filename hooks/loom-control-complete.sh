#!/usr/bin/env bash
# Trusted PostToolUse bridge for one exact sandboxed completion command.

set -euo pipefail
# loom_tokenize_command: the shared argv tokenizer git-add-guard.sh scans with.
source "$(dirname "$0")/_common.sh"

read_input() {
	if command -v gtimeout &>/dev/null; then
		gtimeout 1 cat 2>/dev/null || true
	elif command -v timeout &>/dev/null; then
		timeout 1 cat 2>/dev/null || true
	else
		cat 2>/dev/null || true
	fi
}

fail_closed() {
	printf 'LOOM_CONTROL_ERROR: %s\n' "$1" >&2
	exit 2
}

resolve_trusted_loom() {
	local candidate resolved
	if [[ ${LOOM_CONTROL_TESTING:-} == 1 && -d "$(dirname "$0")/tests" ]]; then
		candidate=${LOOM_CONTROL_TEST_BIN:-}
		[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || return 1
		printf '%s\n' "$candidate"
		return 0
	fi
	for candidate in \
		"${HOME:-}/.local/bin/loom" \
		"${HOME:-}/.cargo/bin/loom" \
		/usr/local/bin/loom \
		/opt/homebrew/bin/loom; do
		[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || continue
		resolved=$(cd "$(dirname "$candidate")" 2>/dev/null && pwd -P)/$(basename "$candidate")
		case "$resolved" in /tmp/* | /private/tmp/* | /var/tmp/* | "$WORKTREE_PATH"/*) continue ;; esac
		printf '%s\n' "$resolved"
		return 0
	done
	return 1
}

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# is_completion_attempt - Walk a tokenized command (as produced by
# loom_tokenize_command: real argv tokens plus "%%SEP%%" command-boundary
# sentinels) looking for a `loom stage complete` invocation.
#
# A token is at a COMMAND POSITION when it is index 0 or immediately follows a
# "%%SEP%%" sentinel - that is how `id && loom stage complete x` and a
# completion inside `$( )` are still seen. Leading VAR=value environment
# assignments at a command position are skipped first, exactly as
# git-add-guard.sh does.
#
# Matching is on argv VALUES, so quoting can neither forge nor evade it:
# `loom stage "complete" x` and `loom stage comple"te" x` both yield the token
# `complete`, while those same words inside ONE quoted argument - as in
# `loom memory note "...complete..."` - stay a single token that never lands in
# the subcommand position.
#
# Two of the three positions are deliberately loose, each for a case the suite
# pins, and both in the fail-safe direction:
#   argv[0] - basename merely CONTAINS `loom`, or the token contains a `$`. A
#             renamed symlink (`loom-link`) and an unexpanded `$LOOM_BIN` both
#             run the real binary, and neither can be ruled out from here.
#   argv[2] - merely CONTAINS `complete`, so an obfuscation the tokenizer
#             cannot normalise (a `\` line-continuation splicing `+complete`)
#             still reaches the pin instead of running unguarded.
# A match only subjects the command to the pin below, which accepts exactly one
# string - so being loose here costs a rejection, never an unguarded completion.
#
# Returns 0 when the command may be a completion attempt, 1 otherwise.
is_completion_attempt() {
	local -a tokens=("$@")
	local n=${#tokens[@]}
	local i=0
	local at_cmd_pos=1
	local bin sub verb

	while ((i < n)); do
		if [[ "${tokens[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi

		if [[ $at_cmd_pos -eq 1 ]]; then
			while ((i < n)) && [[ "${tokens[$i]}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do
				i=$((i + 1))
			done
			# Fewer than three tokens remain anywhere, so no command position
			# left in this list can carry `<loom> stage complete`.
			((i + 2 < n)) || break

			bin=$(lower "${tokens[$i]##*/}")
			sub=$(lower "${tokens[$((i + 1))]}")
			verb=$(lower "${tokens[$((i + 2))]}")
			if [[ "$bin" == *loom* || "${tokens[$i]}" == *'$'* ]] &&
				[[ "$sub" == stage && "$verb" == *complete* ]]; then
				return 0
			fi
		fi

		at_cmd_pos=0
		i=$((i + 1))
	done

	return 1
}

# is_completion_command - Pre-filter: could this Bash command finalise the
# stage? Only what passes here is held to the exact pinned form below.
#
# Tokenizing, rather than globbing the raw command string as this did before,
# closes a forgery: `loom stage comple"te" x` contains no literal `complete`,
# so the glob skipped this entire bridge - the trusted-binary resolution, the
# session binding, the marker check and the broker call - while bash still
# assembled the argv and finalised the stage. It also stops matching the words
# inside quoted prose and path arguments, which blocked unrelated commands.
is_completion_command() {
	local cmd="$1" lowered

	if loom_tokenize_command "$cmd"; then
		# Guards the expansion below on bash 3.2, where "${arr[@]}" on an empty
		# array trips `set -u`; a completion needs three tokens regardless.
		((${#LOOM_TOKENS[@]} >= 3)) || return 1
		if is_completion_attempt "${LOOM_TOKENS[@]}"; then
			return 0
		fi
		return 1
	fi

	# Unterminated quote: the token list is untrustworthy (and bash would refuse
	# to run the command at all). Fall back to the raw substring glob used
	# before tokenizing, so this gate is never weaker than it was.
	lowered=$(lower "$cmd")
	case "$lowered" in *loom*complete*) return 0 ;; esac
	return 1
}

INPUT_JSON=$(read_input)
[[ -n "$INPUT_JSON" ]] || exit 0
[[ "$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty')" == Bash ]] || exit 0

STAGE_ID=${LOOM_STAGE_ID:-}
SESSION_ID=${LOOM_SESSION_ID:-}
WORKTREE_PATH=${LOOM_WORKTREE_PATH:-}
[[ -n "$STAGE_ID" && -n "$SESSION_ID" && -n "$WORKTREE_PATH" ]] || exit 0
# Membership, not presence (same rule as loom_current_worktree in _common.sh).
# A loom worktree is `<repo>/.worktrees/<stage-id>`; main-repo sessions
# (knowledge, merge, base-conflict) own no worktree and complete in-process, so
# this bridge must stay out of their way rather than pin their command.
[[ "$WORKTREE_PATH" =~ /\.worktrees/[^/]+ ]] || exit 0
case "$STAGE_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper stage identity" ;; esac
case "$SESSION_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper session identity" ;; esac

COMMAND=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty')
if ! is_completion_command "$COMMAND"; then
	exit 0
fi

LOOM_BIN=$(resolve_trusted_loom) || fail_closed "no fixed trusted loom installation was found"
PINNED_COMMAND="$LOOM_BIN stage complete $STAGE_ID"
HAS_RESULT=$(printf '%s' "$INPUT_JSON" | jq -r 'has("tool_result") or has("tool_response")')
if [[ "$HAS_RESULT" != true ]]; then
	if [[ "$COMMAND" == "loom stage complete $STAGE_ID" ]]; then
		fail_closed "retry with the pinned command: $PINNED_COMMAND"
	fi
	[[ "$COMMAND" == "$PINNED_COMMAND" ]] || fail_closed "completion must be one exact pinned command: $PINNED_COMMAND"
	exit 0
fi

[[ "$COMMAND" == "$PINNED_COMMAND" ]] || fail_closed "completion result was not produced by the exact pinned command"

IS_ERROR=$(printf '%s' "$INPUT_JSON" | jq -r '(.tool_result.is_error // .tool_response.is_error // false)')
if [[ "$IS_ERROR" != false ]]; then
	# The pinned completion command itself reported an error, so this bridge
	# never reaches the broker below. Say so instead of skipping silently -
	# without this, the session sees a failing tool call and no explanation
	# of what state that leaves the stage in.
	jq -n --arg message "Stage '$STAGE_ID' completion was NOT applied: the completion command itself reported an error, so the completion bridge did not run. The stage is still Executing." \
		'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
	exit 0
fi
MARKER="LOOM_CONTROL_VERIFICATION_PASSED stage=$STAGE_ID session=$SESSION_ID"
HAS_MARKER=$(printf '%s' "$INPUT_JSON" | jq -r --arg marker "$MARKER" '
  [.tool_result.stdout, .tool_result.output, .tool_response.stdout, .tool_response.output]
  | map(select(type == "string")) | join("\n") | split("\n") | index($marker) != null')

if [[ "$HAS_MARKER" != true ]]; then
	# Claude Code persists large tool output to a file once it crosses an
	# internal size threshold. Two shapes have been observed for how it says
	# so: a structured `tool_response.persistedOutputPath` (or
	# `tool_result.persistedOutputPath`) field naming the file directly,
	# with `stdout` holding only a truncated prefix and no wrapper text at
	# all; and an older inline "<persisted-output>" wrapper embedded in
	# stdout that names the file and shows only a 2KB preview. A marker line
	# sitting past what stdout actually carries is invisible to the check
	# above either way, even though the pinned command genuinely printed it
	# - recover it, when possible, from the full file either shape points
	# at, rather than treating "not in the visible text" as "verification
	# did not pass".
	OUTPUT_TEXT=$(printf '%s' "$INPUT_JSON" | jq -r '
	  [.tool_result.stdout, .tool_result.output, .tool_response.stdout, .tool_response.output]
	  | map(select(type == "string")) | join("\n")')
	PERSISTED_PATH=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_response.persistedOutputPath // .tool_result.persistedOutputPath // empty')
	if [[ -z "$PERSISTED_PATH" ]]; then
		PERSISTED_PATH=$(printf '%s' "$OUTPUT_TEXT" | sed -n 's/^.*Full output saved to: //p' | head -n1)
	fi

	# Security boundary: the pinned command's own stdout is agent-influenceable
	# (it is verify-step output the stage agent's environment can shape), so a
	# malicious command could print a FORGED "<persisted-output>" wrapper
	# naming a file it wrote itself, containing a forged marker line. What
	# makes trusting this path safe is that `~/.claude/projects/` is
	# write-denied to the sandboxed stage agent (deny-within-allow in the
	# sandbox filesystem policy) - a file that genuinely resolves under
	# `$HOME/.claude/projects/**/tool-results/**` therefore cannot have been
	# authored by the stage agent; only the Claude Code harness itself writes
	# there. Every one of these checks is required to keep that guarantee:
	# absolute path (no relative escape from an unexpected cwd), under
	# `$HOME/.claude/projects/` (the write-denied root), containing a
	# `/tool-results/` segment (the harness's own persistence layout, not just
	# any file under `projects/`), NO `..` PATH SEGMENT (see below), a regular
	# file that exists, and NOT a symlink (a symlink could point back out to
	# agent-writable storage even from a write-denied directory entry).
	# Relaxing any one of these lets a forged marker back in - do not loosen
	# this without re-establishing why the resolved path is still
	# harness-authored only.
	#
	# The `..`-segment check exists because the root/segment checks above are
	# a GLOB against the RAW STRING, not the resolved path - they establish
	# that the string is spelled as if it were under the write-denied root,
	# not that it actually IS under it. A path like
	# `$HOME/.claude/projects/../../../../tmp/x/tool-results/f.txt`
	# string-matches `$PROJECTS_ROOT/*` and contains `/tool-results/` while
	# resolving to `/tmp/x/tool-results/f.txt` - and `/tmp/claude*`/`$TMPDIR`
	# ARE writable by the sandboxed stage agent, so it can plant a forged
	# marker there and walk it back through a `..` escape dressed up to look
	# like a harness path. Rejecting any `..` segment closes this: a genuine
	# Claude Code artifact path is never written with one, so this costs
	# nothing legitimate, and it must run BEFORE the `-f`/`-L` checks since
	# those operate on the (attacker-controlled) resolved target, not on
	# containment. The check below matches path SEGMENTS - a leading `../`, an
	# embedded `/../`, a trailing `/..`, or a bare `..` - not the substring
	# `..`, so a legitimate filename that merely contains two dots (e.g.
	# `out..1.txt`) is unaffected.
	PERSISTED_VALID=false
	if [[ -n "$PERSISTED_PATH" && -n "${HOME:-}" ]]; then
		PROJECTS_ROOT="$HOME/.claude/projects"
		case "$PERSISTED_PATH" in
		../* | */../* | */.. | ..) HAS_DOTDOT_SEGMENT=true ;;
		*) HAS_DOTDOT_SEGMENT=false ;;
		esac
		if [[ "$PERSISTED_PATH" == /* && "$PERSISTED_PATH" == "$PROJECTS_ROOT"/* && "$PERSISTED_PATH" == */tool-results/* &&
			"$HAS_DOTDOT_SEGMENT" == false && -f "$PERSISTED_PATH" && ! -L "$PERSISTED_PATH" ]]; then
			PERSISTED_VALID=true
		fi
	fi

	if [[ "$PERSISTED_VALID" == true ]] && grep -Fxq -- "$MARKER" "$PERSISTED_PATH" 2>/dev/null; then
		HAS_MARKER=true
	fi
fi

if [[ "$HAS_MARKER" != true ]]; then
	# The command exited 0 but the verification marker is not present as its
	# own whole line of stdout (nor recoverable from a validated persisted
	# output file above), so this bridge cannot confirm verification passed
	# and will not call the broker. Truncated or line-wrapped stdout is a
	# known way for the marker to go missing even when the command itself
	# actually printed it.
	MESSAGE="Stage '$STAGE_ID' completion was NOT applied: the verification marker was not found in the command output, so the bridge could not confirm verification and did not complete the stage. The marker must appear as its own complete line in stdout; truncated or wrapped output is a known cause."
	if [[ -n "${PERSISTED_PATH:-}" ]]; then
		if [[ "${PERSISTED_VALID:-false}" == true ]]; then
			MESSAGE="$MESSAGE A persisted-output file was seen ($PERSISTED_PATH), but it did not contain the marker either."
		else
			MESSAGE="$MESSAGE A persisted-output file was seen, but it did not validate as a genuine Claude Code artifact, so it was not checked for the marker."
		fi
	fi
	jq -n --arg message "$MESSAGE" \
		'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
	exit 0
fi

if ! BROKER_OUTPUT=$(LOOM_CONTROL_BROKER=1 "$LOOM_BIN" stage complete "$STAGE_ID" --session "$SESSION_ID" 2>&1); then
	fail_closed "daemon completion broker failed: $BROKER_OUTPUT"
fi

jq -n --arg message "Stage '$STAGE_ID' completion was accepted by the daemon." \
	'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
