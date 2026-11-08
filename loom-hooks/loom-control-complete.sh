#!/usr/bin/env bash
# Trusted PreToolUse pin for the one exact completion command a session may run.
# The command relays a host job (`loom stage complete`), so this hook admits the
# pinned command, rewrites the main agent's plain spelling of it (bare `loom`,
# optional `2>&1`, one `| tail`/`| head` line count) to the pinned command,
# refuses every other spelling and writes the completion heartbeat the stall
# monitor reads; it carries no verdict.

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail
# loom_tokenize_command: the shared argv tokenizer git-add-guard.sh scans with.
source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_post-tool-heartbeat.sh"

read_input() {
	source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
	loom_read_input 1 2>/dev/null || true
}

# fail_closed <reason> [<prefix>] - Print the refusal (after <prefix>) and exit 2.
fail_closed() {
	printf '%sLOOM_CONTROL_ERROR: %s\n' "${2:-}" "$1" >&2
	exit 2
}

# block_not_pinned <reason> - Record a refused completion attempt in the stage's
# block ledger, then refuse it.
block_not_pinned() {
	loom_record_block loom-control-complete not-pinned Bash "$COMMAND"
	fail_closed "$1" "[loom-control-complete/not-pinned] "
}

command -v jq &>/dev/null || fail_closed "$(loom_jq_missing_message loom-control-complete.sh)"

# trusted_loom_candidate <path> - Print <path> resolved when it is an absolute,
# executable regular file (not a symlink) outside /tmp and outside the checkout
# this session can write ($CHECKOUT_ROOT); return 1 otherwise.
trusted_loom_candidate() {
	local candidate=$1 resolved
	[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || return 1
	resolved=$(cd "$(dirname "$candidate")" 2>/dev/null && pwd -P)/$(basename "$candidate")
	case "$resolved" in /tmp/* | /private/tmp/* | /var/tmp/* | "$CHECKOUT_ROOT"/*) return 1 ;; esac
	printf '%s\n' "$resolved"
}

resolve_trusted_loom() {
	local candidate
	if [[ ${LOOM_CONTROL_TESTING:-} == 1 && -d "$(dirname "$0")/tests" ]]; then
		candidate=${LOOM_CONTROL_TEST_BIN:-}
		[[ "$candidate" == /* && -f "$candidate" && -x "$candidate" && ! -L "$candidate" ]] || return 1
		printf '%s\n' "$candidate"
		return 0
	fi
	# The binary loom exported for this session (LOOM_BIN) comes first, held to
	# the same checks as the fixed install locations after it.
	for candidate in \
		"${LOOM_BIN:-}" \
		"${HOME:-}/.local/bin/loom" \
		"${HOME:-}/.cargo/bin/loom" \
		/usr/local/bin/loom \
		/opt/homebrew/bin/loom; do
		trusted_loom_candidate "$candidate" && return 0
	done
	return 1
}

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

raw_has_completion_indicators() {
	local value
	value=$(lower "$1")
	[[ "$value" == *loom* && "$value" == *stage* && "$value" == *complete* ]]
}

scan_completion_segment() {
	local start=$1 end=$2 i=$1 token base
	local assignment=false env_wrapper=false
	while ((i < end)) && [[ "${LOOM_TOKENS[$i]}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do
		assignment=true
		i=$((i + 1))
	done
	((i < end)) || return 1
	base=$(lower "${LOOM_TOKENS[$i]##*/}")
	if [[ "$base" == sh || "$base" == bash || "$base" == zsh ]]; then
		local k
		for ((k = i + 1; k < end; k++)); do
			case "${LOOM_TOKENS[$k]}" in -c | -lc | -cl) ATTEMPT_SHELL=true ;; esac
		done
	fi
	if [[ "$base" == env ]]; then
		env_wrapper=true
		i=$((i + 1))
		while ((i < end)); do
			token=${LOOM_TOKENS[$i]}
			case "$token" in
			-u | --unset) i=$((i + 2)) ;;
			--unset=* | -i | --ignore-environment | --) i=$((i + 1)) ;;
			[A-Za-z_]*=*) i=$((i + 1)) ;;
			*) break ;;
			esac
		done
	fi
	((i + 2 < end)) || return 1
	[[ "$(lower "${LOOM_TOKENS[$((i + 1))]}")" == stage ]] || return 1
	[[ "$(lower "${LOOM_TOKENS[$((i + 2))]}")" == *complete* ]] || return 1
	ATTEMPT_BIN=${LOOM_TOKENS[$i]}
	ATTEMPT_VERB=${LOOM_TOKENS[$((i + 2))]}
	ATTEMPT_ID=${LOOM_TOKENS[$((i + 3))]:-}
	ATTEMPT_ARGC=$((end - i))
	ATTEMPT_ASSIGN=$assignment
	ATTEMPT_ENV=$env_wrapper
	return 0
}

# detect_completion <cmd> - Set ATTEMPT_* and return 0 when a command position of
# <cmd> holds the completion shape; an untokenizable <cmd> takes the substring test.
detect_completion() {
	local cmd=$1 tokenized_cmd n i start
	ATTEMPT_TOKENIZE_FAILED=false ATTEMPT_SEPARATOR=false ATTEMPT_SHELL=false
	ATTEMPT_ASSIGN=false ATTEMPT_ENV=false ATTEMPT_BIN="" ATTEMPT_VERB=""
	ATTEMPT_ID="" ATTEMPT_ARGC=0
	# Bash removes a backslash-newline pair before parsing. The shared
	# permissive tokenizer preserves the newline as part of the word, so apply
	# that one shell-mandated splice before detector tokenization. Authorization
	# still compares the original bytes against PINNED_COMMAND below.
	tokenized_cmd=${cmd//$'\\\n'/}
	if ! loom_tokenize_command "$tokenized_cmd"; then
		ATTEMPT_TOKENIZE_FAILED=true
		raw_has_completion_indicators "$cmd"
		return
	fi
	n=${#LOOM_TOKENS[@]}
	((n >= 3)) || return 1
	for ((i = 0; i < n; i++)); do
		[[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]] && ATTEMPT_SEPARATOR=true
	done
	start=0
	while ((start < n)); do
		while ((start < n)) && [[ "${LOOM_TOKENS[$start]}" == "%%SEP%%" ]]; do start=$((start + 1)); done
		((start < n)) || break
		i=$start
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
		scan_completion_segment "$start" "$i" && return 0
		start=$((i + 1))
	done
	return 1
}

# Heredoc bodies are data, so their prose must not read as a completion
# attempt. strip_embedded_content does not parse shell, so a stripped body is
# trusted only when every check below holds; in doubt the raw command decides.
# strip_heredoc_bodies <cmd> - Print <cmd> without heredoc bodies. Fail when
# nothing was stripped, a body swallowed the appended end line (it has no
# terminator), or the quote-blind -m/--message rewrite fired.
strip_heredoc_bodies() {
	local out end='#loom-heredoc-end'
	[[ "$1" == *'<<'* ]] || return 1
	out=$(strip_embedded_content "$1"$'\n'"$end")
	[[ "$out" == *$'\n'"$end" ]] || return 1
	out=${out%$'\n'"$end"}
	case "$out" in "$1" | *'-m ""'* | *"-m ''"* | *'--message'[=\ ]'""'* | *'--message'[=\ ]"''"*) return 1 ;; esac
	printf '%s' "$out"
}

# heredoc_openers_are_plain <stripped> - A line with `<<` holds one unquoted opener,
# <<'WORD' or <<"WORD" (literal body, ended where awk ends it), and ends unquoted.
heredoc_openers_are_plain() {
	local line prior="" pre post q="'" re
	re="^(.*)<<[[:space:]]*(${q}[A-Za-z_][A-Za-z0-9_]*${q}|\"[A-Za-z_][A-Za-z0-9_]*\")([[:space:];&|)>].*)?\$"
	while IFS= read -r line; do
		if [[ "$line" =~ $re ]]; then
			pre=${BASH_REMATCH[1]} post=${BASH_REMATCH[3]}
			case "$pre" in *'<<'* | *'<' | *'\') return 1 ;; esac
			[[ "$post" != *'<<'* ]] || return 1
			loom_tokenize_command "$prior$pre" && loom_tokenize_command "$prior$line" || return 1
		elif [[ "$line" == *'<<'* ]]; then
			return 1
		fi
		prior+="$line"$'\n'
	done <<<"$1"
}

# heredoc_readers_are_inert <stripped> - No command runs its input as code.
# `<` `>` become blanks, so `cat > f <<'EOF'` is the one command `cat f EOF`.
heredoc_readers_are_inert() {
	local i tok at_cmd=1
	loom_tokenize_command "${1//[<>]/ }" || return 1
	for ((i = 0; i < ${#LOOM_TOKENS[@]}; i++)); do
		tok=${LOOM_TOKENS[$i]}
		[[ "$tok" == '%%SEP%%' ]] && at_cmd=1 && continue
		((at_cmd)) && [[ ! "$tok" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]] || continue
		at_cmd=0
		case "${tok##*/}:${LOOM_TOKENS[$((i + 1))]:-}" in
		cat:* | tee:* | wc:* | head:* | tail:* | cd:* | mkdir:* | touch:* | echo:* | printf:* | true:* | ::*) ;;
		loom:knowledge | loom:memory | git:commit) ;;
		*) return 1 ;;
		esac
	done
}

# heredoc_bodies_are_inert <cmd> <stripped> - Nothing awk or the tokenizer misreads
# (comment, backtick, $'', ${}, arithmetic, line splice) remains, and with $( <( >(
# no stripped line holds `)` (bash ends a body at `EOF)`).
heredoc_bodies_are_inert() {
	local all=${1//[^)]/} kept=${2//[^)]/}
	case "$2" in *'#'* | *'`'* | *"\$'"* | *'${'* | *'(('* | *'$['* | *'\'$'\n'* | *'\') return 1 ;; esac
	case "$2" in *'$('* | *'<('* | *'>('*) ((${#all} == ${#kept})) || return 1 ;; esac
	heredoc_openers_are_plain "$2" && heredoc_readers_are_inert "$2"
}

# is_completion_command <cmd> - Decide on <cmd> with inert heredoc bodies stripped. In
# doubt the raw decision stands; if the stripped command feeds a shell, an
# interpreter (python, perl, node, ...), eval, source or xargs (or will not
# tokenize), the raw substring test counts as well.
is_completion_command() {
	local cmd=$1 stripped fed=true
	stripped=$(strip_heredoc_bodies "$cmd") || { detect_completion "$cmd"; return; }
	detect_completion "$stripped" && return 0
	heredoc_bodies_are_inert "$cmd" "$stripped" && return 1
	loom_tokenize_command "$stripped" && ! loom_tokens_invoke '\.' &&
		! loom_tokens_word_matches '^(.*/)?(bash|sh|zsh|dash|ksh|mksh|fish|csh|tcsh|busybox|eval|source|xargs|python[0-9.]*|pypy[0-9.]*|perl[0-9.]*|ruby|node|nodejs|deno|bun|php[0-9.]*|lua[0-9.]*|tclsh|pwsh|osascript|Rscript)$' && fed=false
	detect_completion "$cmd" && return 0
	[[ "$fed" == true ]] && raw_has_completion_indicators "$cmd" || return 1
	ATTEMPT_SHELL=true
}

completion_rejection_reason() {
	local trusted=$1 heredoc_re='(^|[^<])<<-?[[:space:]]*[A-Za-z_]'
	[[ ! "$COMMAND" =~ $heredoc_re ]] || { printf '%s' "an unquoted heredoc delimiter expands \$(...) in its body; quote the delimiter: <<'EOF'"; return; }
	[[ "$ATTEMPT_TOKENIZE_FAILED" == false ]] || { printf 'the command could not be tokenized safely'; return; }
	[[ "$ATTEMPT_SHELL" == false ]] || { printf 'a shell -c wrapper is not allowed'; return; }
	[[ "$ATTEMPT_ENV" == false ]] || { printf 'an env wrapper is not allowed'; return; }
	[[ "$ATTEMPT_ASSIGN" == false ]] || { printf 'a NAME=value prefix is not allowed'; return; }
	[[ "$ATTEMPT_SEPARATOR" == false ]] || { printf 'pipelines, separators, redirections, backgrounding, and newlines are not allowed'; return; }
	[[ "$ATTEMPT_BIN" != *'$'* ]] || { printf 'a variable binary path is not allowed'; return; }
	if [[ "$ATTEMPT_BIN" != /* ]]; then
		printf 'a relative loom path is not allowed'
		return
	fi
	[[ "$ATTEMPT_BIN" == "$trusted" ]] || { printf 'a symlink or other binary is not allowed'; return; }
	if [[ "$ATTEMPT_ARGC" != 4 || "$ATTEMPT_ID" != "$STAGE_ID" ]]; then
		printf 'extra flags, arguments, or a different stage id are not allowed'
		return
	fi
	printf 'quoted, concatenated, escaped, or line-spliced command bytes are not allowed'
}

# completion_rewrite_matches - Return 0 when COMMAND's tokens are exactly
# `BIN stage complete STAGE_ID [2>&1] [| tail|head -N | -n N]`, BIN being the
# word `loom` or the resolved trusted path in LOOM_BIN. Any quote, escape,
# expansion or newline in the raw bytes refuses, so the tokens are the argv.
completion_rewrite_matches() {
	local n i=4 t1 t2 t3
	case "$COMMAND" in *\\* | *\'* | *\"* | *\$* | *\`* | *$'\n'*) return 1 ;; esac
	loom_tokenize_command "$COMMAND" || return 1
	n=${#LOOM_TOKENS[@]}
	((n >= 4)) || return 1
	[[ "${LOOM_TOKENS[0]}" == loom || "${LOOM_TOKENS[0]}" == "$LOOM_BIN" ]] || return 1
	[[ "${LOOM_TOKENS[1]}" == stage && "${LOOM_TOKENS[2]}" == complete ]] || return 1
	[[ "${LOOM_TOKENS[3]}" == "$STAGE_ID" ]] || return 1
	if ((i + 2 < n)) && [[ "${LOOM_TOKENS[$i]}" == 2 && "${LOOM_TOKENS[$((i + 1))]}" == '%%SEP%%' &&
		"${LOOM_TOKEN_SEPS[$((i + 1))]:-}" == '>&' && "${LOOM_TOKENS[$((i + 2))]}" == 1 ]]; then
		i=$((i + 3))
	fi
	if ((i + 2 < n)) && [[ "${LOOM_TOKENS[$i]}" == '%%SEP%%' && "${LOOM_TOKEN_SEPS[$i]:-}" == '|' ]]; then
		t1=${LOOM_TOKENS[$((i + 1))]} t2=${LOOM_TOKENS[$((i + 2))]} t3=${LOOM_TOKENS[$((i + 3))]:-}
		[[ "$t1" == tail || "$t1" == head ]] || return 1
		if [[ "$t2" =~ ^-[0-9]+$ ]]; then
			i=$((i + 3))
		elif ((i + 3 < n)) && [[ "$t2" == -n && "$t3" =~ ^[0-9]+$ ]]; then
			i=$((i + 4))
		else
			return 1
		fi
	fi
	((i == n))
}

# emit_pinned_allow <reason> - Write the completion heartbeat and print the
# PreToolUse allow, giving <reason>, that runs PINNED_COMMAND. updatedInput
# replaces the whole tool input, so only the model's timeout and description
# carry over; run_in_background and dangerouslyDisableSandbox never ride on an
# explicit hook allow.
emit_pinned_allow() {
	local out
	out=$(jq -c --arg pinned "$PINNED_COMMAND" --arg reason "$1" '{hookSpecificOutput: {
		hookEventName: "PreToolUse",
		permissionDecision: "allow",
		permissionDecisionReason: $reason,
		updatedInput: ({command: $pinned}
			+ (.tool_input | {timeout, description} | with_entries(select(.value != null)))),
		additionalContext: ("LOOM: ran the pinned completion command " + $pinned + "; its output is not piped. Read the Host job line.")}}' <<<"$INPUT_JSON") ||
		block_not_pinned "the completion allow could not be built; retry with the pinned command: $PINNED_COMMAND"
	loom_write_completion_heartbeat >/dev/null || true
	printf '%s\n' "$out"
}

INPUT_JSON=$(read_input)
[[ -n "$INPUT_JSON" ]] || exit 0
[[ "$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty')" == Bash ]] || exit 0

STAGE_ID=${LOOM_STAGE_ID:-}
SESSION_ID=${LOOM_SESSION_ID:-}
WORKTREE_PATH=${LOOM_WORKTREE_PATH:-}
[[ -n "$STAGE_ID" && -n "$SESSION_ID" ]] || exit 0
# Membership, not presence (same rule as loom_current_worktree in _common.sh).
# A loom worktree is `<repo>/.worktrees/<stage-id>`. Anchored at the end: the
# wrapper exports the worktree ROOT, and a repo that itself lives under an
# outer `.worktrees/<id>/` must not count as a worktree.
#
# The one main-repo session this hook also serves is a CONFINED Knowledge
# session (LOOM_SCRATCH_DIR set): it cannot write the state directory, so it
# completes through the daemon too. A legacy Knowledge session still completes
# in-process, and Merge, base-conflict and adjudication sessions never complete
# a stage here, so the hook stays out of their way rather than pin their command.
if [[ "$WORKTREE_PATH" =~ /\.worktrees/[^/]+/?$ ]]; then
	CHECKOUT_ROOT=$WORKTREE_PATH
elif [[ ${LOOM_SESSION_TYPE:-} == knowledge && -n ${LOOM_SCRATCH_DIR:-} ]]; then
	# The main project root: <root>/.loom/work, or the legacy <root>/.work.
	case "${LOOM_WORK_DIR:-}" in
	/*/.loom/work) CHECKOUT_ROOT=${LOOM_WORK_DIR%/.loom/work} ;;
	/*/.work) CHECKOUT_ROOT=${LOOM_WORK_DIR%/.work} ;;
	*) CHECKOUT_ROOT="" ;;
	esac
else
	exit 0
fi
case "$STAGE_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper stage identity" ;; esac
case "$SESSION_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper session identity" ;; esac

COMMAND=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty')
if ! is_completion_command "$COMMAND"; then
	exit 0
fi

# A binary inside the checkout this session writes is never trusted, so that
# checkout must be known before any binary is.
[[ -n "$CHECKOUT_ROOT" ]] ||
	fail_closed "this knowledge session's LOOM_WORK_DIR names no loom state directory, so no loom binary can be trusted"
LOOM_BIN=$(resolve_trusted_loom) || fail_closed "no trusted loom binary was found (LOOM_BIN or a fixed install location)"
PINNED_COMMAND="$LOOM_BIN stage complete $STAGE_ID"
# A PreToolUse payload carries no result; a payload that does is a stale
# PostToolUse registration, which this hook no longer serves.
[[ "$(printf '%s' "$INPUT_JSON" | jq -r 'has("tool_result") or has("tool_response")')" != true ]] || exit 0
# A subagent never completes a stage: its pinned form gets no allow (commit-filter.sh
# refuses it) and its plain spelling keeps the refusal.
AGENT_VERDICT=$(loom_payload_agent_verdict "$INPUT_JSON")
if [[ "$COMMAND" == "$PINNED_COMMAND" ]]; then
	# No permission rule matches the absolute pinned path (the session grants
	# `Bash(loom *)`), so without this allow auto mode's classifier judges, and
	# can refuse, the main agent's completion.
	if [[ "$AGENT_VERDICT" != subagent ]]; then
		emit_pinned_allow "loom: pinned completion command"
		exit 0
	fi
	loom_write_completion_heartbeat || true
	exit 0
fi
if [[ "$AGENT_VERDICT" != subagent ]] && completion_rewrite_matches; then
	emit_pinned_allow "loom: completion rewritten to the pinned command"
	exit 0
fi
if [[ "$COMMAND" == "loom stage complete $STAGE_ID" ]]; then
	block_not_pinned "retry with the pinned command: $PINNED_COMMAND"
fi
REASON=$(completion_rejection_reason "$LOOM_BIN")
block_not_pinned "completion must be one exact pinned command: $PINNED_COMMAND ($REASON)"
