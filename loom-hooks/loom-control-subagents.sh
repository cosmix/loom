#!/usr/bin/env bash
# Trusted PreToolUse pin for the relayed native worker commands
# `loom subagents start`, `message` and `cancel`. Each writes a
# `subagents:control` relay ticket, so this hook admits exactly one spelling of
# them (BIN subagents VERB, then `--flag value` pairs, then an optional `2>&1`),
# rewrites it to the trusted absolute loom binary, and refuses every other
# spelling and every caller other than the stage's main agent. It carries no
# verdict and writes no heartbeat: a worker command is not a completion.
#
# trusted_loom_candidate and resolve_trusted_loom are duplicated from
# loom-control-complete.sh on purpose: a shared helper would have to change that
# hook too. Keep the two copies identical.

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail
# loom_tokenize_command, strip_embedded_content, loom_record_block and the
# caller-identity helpers.
source "$(dirname "$0")/_common.sh"

read_input() {
	source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
	loom_read_input 1 2>/dev/null || true
}

# fail_closed <reason> [<prefix>] - Print the refusal (after <prefix>) and exit 2.
fail_closed() {
	printf '%sLOOM_CONTROL_ERROR: %s\n' "${2:-}" "$1" >&2
	exit 2
}

# block_not_pinned <reason> - Record the refused attempt in the stage's block
# ledger, then refuse it.
block_not_pinned() {
	loom_record_block loom-control-subagents not-pinned Bash "$COMMAND"
	fail_closed "$1" "[loom-control-subagents/not-pinned] "
}

command -v jq &>/dev/null || fail_closed "$(loom_jq_missing_message loom-control-subagents.sh)"

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

# is_worker_verb <word> - Substring on purpose: an obfuscated spelling of a verb
# is detected here and then refused as not exact.
is_worker_verb() {
	local word
	word=$(lower "$1")
	[[ "$word" == *start* || "$word" == *message* || "$word" == *cancel* ]]
}

raw_has_worker_indicators() {
	local value
	value=$(lower "$1")
	[[ "$value" == *loom* && "$value" == *subagents* ]] && is_worker_verb "$value"
}

# scan_worker_segment <start> <end> - Set ATTEMPT_* and return 0 when the command
# segment LOOM_TOKENS[<start>..<end>) runs `<bin> subagents <worker verb>`.
# The command word comes from loom_tokens_command_word_index, the resolver
# loom-relay.sh classifies `subagents:control` with, so every spelling that
# relays the kind (NAME= prefixes, env/time/command/exec/nohup/timeout/nice/
# setsid/stdbuf/sudo/xargs wrappers, `{`, `if`, `!`) is detected here; anything
# before the command word is then refused as not exact.
scan_worker_segment() {
	local start=$1 end=$2 i=$1 k base cw
	local assignment=false
	while ((i < end)) && [[ "${LOOM_TOKENS[$i]}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do
		assignment=true
		i=$((i + 1))
	done
	((i < end)) || return 1
	base=$(lower "${LOOM_TOKENS[$i]##*/}")
	if [[ "$base" == sh || "$base" == bash || "$base" == zsh ]]; then
		for ((k = i + 1; k < end; k++)); do
			case "${LOOM_TOKENS[$k]}" in -c | -lc | -cl) ATTEMPT_SHELL=true ;; esac
		done
	fi
	cw=$(loom_tokens_command_word_index "$start") || return 1
	((cw + 2 < end)) || return 1
	[[ "$(lower "${LOOM_TOKENS[$((cw + 1))]}")" == subagents ]] || return 1
	is_worker_verb "${LOOM_TOKENS[$((cw + 2))]}" || return 1
	ATTEMPT_BIN=${LOOM_TOKENS[$cw]}
	ATTEMPT_ASSIGN=$assignment
	[[ "$base" != env ]] || ATTEMPT_ENV=true
	((cw == i)) || ATTEMPT_WRAPPER=${LOOM_TOKENS[$i]}
	return 0
}

# detect_worker_command <cmd> - Set ATTEMPT_* and return 0 when a command
# position of <cmd>, heredoc bodies and commit messages stripped, holds the
# worker shape; an untokenizable <cmd> takes the substring test. One trailing
# `2>&1`, the one redirection the grammar admits, does not count as a separator.
detect_worker_command() {
	local cmd n i start
	ATTEMPT_TOKENIZE_FAILED=false ATTEMPT_SEPARATOR=false ATTEMPT_SHELL=false
	ATTEMPT_ASSIGN=false ATTEMPT_ENV=false ATTEMPT_WRAPPER="" ATTEMPT_BIN=""
	cmd=$(strip_embedded_content "${1% 2>&1}")
	# Bash removes a backslash-newline pair before parsing; the tokenizer keeps it.
	cmd=${cmd//$'\\\n'/}
	if ! loom_tokenize_command "$cmd"; then
		ATTEMPT_TOKENIZE_FAILED=true
		raw_has_worker_indicators "$1"
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
		scan_worker_segment "$start" "$i" && return 0
		start=$((i + 1))
	done
	return 1
}

# The accepted grammar, matched against the raw command bytes:
#   COMMAND := BIN " subagents " VERB (" " FLAG " " VALUE)+ [" 2>&1"]
#   BIN     := "loom" | the trusted absolute path ($LOOM_BIN)
#   VALUE   := WORD | '...' (inert) | "..." (no $ ` \ !)
# A value is followed by a space or the end, so `'a'b` and `x$(y)` never match.
WORD_RE='[A-Za-z0-9._/@%+=:,][A-Za-z0-9._/@%+=:,-]*'
VALUE_RE="$WORD_RE|'[^']*'"'|"[^"\$`!]*"'
FLAG_RE='^ (--[a-z][a-z-]*) ('"$VALUE_RE"')(( .*)?)$'
UNIT_RE='^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$'
FLAG_KEYS="provider unit brief model effort worker invocation delivery message_id text file reason"

grammar_fail() { GRAMMAR_ERROR=$1; return 1; }

byte_length() {
	local LC_ALL=C
	printf '%s' "${#1}"
}

# flag_key <verb> <flag> - The variable stem <flag> fills for <verb>; fail for a
# flag <verb> does not take. Only the clap long names (native_args.rs) count.
flag_key() {
	case "$1:$2" in
	start:--provider) echo provider ;;
	start:--unit-id) echo unit ;;
	start:--brief) echo brief ;;
	start:--model) echo model ;;
	start:--effort) echo effort ;;
	message:--worker | cancel:--worker) echo worker ;;
	message:--invocation | cancel:--invocation) echo invocation ;;
	message:--delivery) echo delivery ;;
	message:--message-id) echo message_id ;;
	message:--text) echo text ;;
	message:--file) echo file ;;
	cancel:--reason) echo reason ;;
	*) return 1 ;;
	esac
}

# kind_of <key> - `word`, `quoted`, or empty when the flag was not given.
kind_of() {
	local name="KIND_$1"
	printf '%s' "${!name:-}"
}

arg_of() {
	local name="ARG_$1"
	printf '%s' "${!name:-}"
}

# parse_flags <verb> <rest> - Consume `--flag value` pairs into ARG_<key> (the
# value without its quotes) and KIND_<key>; only one trailing ` 2>&1` may remain.
parse_flags() {
	local verb=$1 rest=$2 flag value key
	for key in $FLAG_KEYS; do unset "ARG_$key" "KIND_$key"; done
	while [[ "$rest" =~ $FLAG_RE ]]; do
		flag=${BASH_REMATCH[1]} value=${BASH_REMATCH[2]} rest=${BASH_REMATCH[3]}
		[[ "$verb:$flag" != cancel:--stage ]] ||
			grammar_fail "--stage is the operator form of cancel; a stage session names --invocation" || return 1
		key=$(flag_key "$verb" "$flag") || grammar_fail "unknown flag $flag for loom subagents $verb" || return 1
		[[ -z "$(kind_of "$key")" ]] || grammar_fail "flag $flag given twice" || return 1
		case "$value" in
		\'* | \"*)
			printf -v "ARG_$key" '%s' "${value:1:${#value}-2}"
			printf -v "KIND_$key" '%s' quoted
			;;
		*)
			printf -v "ARG_$key" '%s' "$value"
			printf -v "KIND_$key" '%s' word
			;;
		esac
	done
	[[ -z "$rest" || "$rest" == ' 2>&1' ]] ||
		grammar_fail "only --flag value pairs and one trailing 2>&1 may follow loom subagents $verb: no pipes, separators, redirections, substitutions, unquoted variables or expansion characters in double quotes"
}

# need <key> <flag> [<regex> <what>] - <flag> was given; with <regex>, as a WORD
# matching it.
need() {
	[[ -n "$(kind_of "$1")" ]] || grammar_fail "missing $2" || return 1
	(($# > 2)) || return 0
	[[ "$(kind_of "$1")" == word && "$(arg_of "$1")" =~ $3 ]] || grammar_fail "$2 must be $4"
}

# need_path <key> <flag> - <flag> names an absolute path without a `..` component.
need_path() {
	local value
	value=$(arg_of "$1")
	[[ -n "$(kind_of "$1")" ]] || grammar_fail "missing $2" || return 1
	[[ "$value" == /* && "$value/" != *'/../'* ]] || grammar_fail "$2 must be an absolute path without a .. component"
}

need_worker_and_invocation() {
	need worker --worker "${UNIT_RE/^/^codex:}" "codex:<unit-id>" &&
		need invocation --invocation '^inv-[0-9a-f]{32}$' "inv- and 32 lowercase hex digits"
}

validate_start() {
	need provider --provider '^codex$' "codex" &&
		need unit --unit-id "$UNIT_RE" "a unit id (letters, digits, _ and -, at most 64)" &&
		need_path brief --brief &&
		need model --model '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$' "a model name" &&
		need effort --effort '^(low|medium|high|xhigh)$' "low, medium, high or xhigh"
}

validate_message() {
	need_worker_and_invocation &&
		need delivery --delivery '^(steer|next-turn)$' "steer or next-turn" &&
		need message_id --message-id "$UNIT_RE" "a message id (letters, digits, _ and -, at most 64)" || return 1
	case "$(kind_of text):$(kind_of file)" in
	:) grammar_fail "one of --text or --file is required" ;;
	?*:?*) grammar_fail "--text and --file cannot be given together" ;;
	:*) need_path file --file ;;
	*)
		local text
		text=$(arg_of text)
		((${#text} <= 16384)) || grammar_fail "--text exceeds 16384 characters"
		;;
	esac
}

validate_cancel() {
	need_worker_and_invocation && need reason --reason
}

# worker_grammar_matches - Return 0, with PINNED_COMMAND set to the command on
# the trusted binary, when COMMAND is exactly the accepted grammar; otherwise
# set GRAMMAR_ERROR and return 1.
worker_grammar_matches() {
	local bin rest verb
	GRAMMAR_ERROR="" PINNED_COMMAND=""
	[[ "$COMMAND_HAS_NUL" == false && "$COMMAND" != *$'\n'* && "$COMMAND" != *$'\r'* ]] ||
		grammar_fail "newline, carriage return and NUL bytes are not allowed" || return 1
	(($(byte_length "$COMMAND") <= 8192)) || grammar_fail "the command exceeds 8192 bytes" || return 1
	for bin in loom "$LOOM_BIN"; do
		[[ "$COMMAND" == "$bin subagents "* ]] || continue
		rest=${COMMAND#"$bin subagents "}
		verb=${rest%% *}
		case "$verb" in start | message | cancel) ;; *) grammar_fail "the verb must be start, message or cancel" || return 1 ;; esac
		rest=${rest#"$verb"}
		parse_flags "$verb" "$rest" || return 1
		case "$verb" in start) validate_start ;; message) validate_message ;; *) validate_cancel ;; esac || return 1
		PINNED_COMMAND="$LOOM_BIN subagents $verb$rest"
		return 0
	done
	grammar_fail "the command must begin with loom subagents, unquoted, one space between words"
}

rejection_reason() {
	local trusted=$1 heredoc_re='(^|[^<])<<-?[[:space:]]*[A-Za-z_]'
	[[ ! "$COMMAND" =~ $heredoc_re ]] || { printf 'a heredoc is not allowed'; return; }
	[[ "$ATTEMPT_TOKENIZE_FAILED" == false ]] || { printf 'the command could not be tokenized safely'; return; }
	[[ "$ATTEMPT_SHELL" == false ]] || { printf 'a shell -c wrapper is not allowed'; return; }
	[[ "$ATTEMPT_ENV" == false ]] || { printf 'an env wrapper is not allowed'; return; }
	[[ "$ATTEMPT_ASSIGN" == false ]] || { printf 'a NAME=value prefix is not allowed'; return; }
	[[ -z "$ATTEMPT_WRAPPER" ]] || { printf 'a wrapper or shell keyword (%s) before loom is not allowed' "$ATTEMPT_WRAPPER"; return; }
	[[ "$ATTEMPT_SEPARATOR" == false ]] || { printf 'pipelines, separators, redirections other than one trailing 2>&1, backgrounding, and newlines are not allowed'; return; }
	[[ "$ATTEMPT_BIN" != *'$'* ]] || { printf 'a variable binary path is not allowed'; return; }
	if [[ "$ATTEMPT_BIN" != loom && "$ATTEMPT_BIN" != /* ]]; then
		printf 'a relative loom path is not allowed'
		return
	fi
	if [[ "$ATTEMPT_BIN" == /* && "$ATTEMPT_BIN" != "$trusted" ]]; then
		printf 'a symlink or other binary is not allowed'
		return
	fi
	printf '%s' "${GRAMMAR_ERROR:-the command does not match the accepted grammar}"
}

# caller_is_main - the payload positively names the main agent (its transcript
# is <session_id>.jsonl), or names no transcript at all and the process walk
# (loom_is_subagent) finds no subagent. An unrecognised transcript shape never
# counts: main is not granted by elimination (_identity.sh).
caller_is_main() {
	[[ -z "$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null)" ]] || return 1
	case "$(loom_payload_agent_verdict "$INPUT_JSON")" in
	main) return 0 ;;
	subagent) return 1 ;;
	esac
	[[ -z "$(printf '%s' "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null)" ]] || return 1
	! loom_is_subagent "$INPUT_JSON"
}

# emit_pinned_allow - Print the PreToolUse allow that runs PINNED_COMMAND.
# updatedInput replaces the whole tool input, so only the model's timeout and
# description carry over; run_in_background and dangerouslyDisableSandbox never
# ride on an explicit hook allow.
emit_pinned_allow() {
	local out
	out=$(jq -c --arg pinned "$PINNED_COMMAND" '{hookSpecificOutput: {
		hookEventName: "PreToolUse",
		permissionDecision: "allow",
		permissionDecisionReason: "loom: native worker command pinned to the trusted loom binary",
		updatedInput: ({command: $pinned}
			+ (.tool_input | {timeout, description} | with_entries(select(.value != null)))),
		additionalContext: "LOOM: ran the native worker command through the trusted loom binary. Wait for its request in your NEXT Bash call with the loom request status line it printed."}}' <<<"$INPUT_JSON") ||
		block_not_pinned "the allow for the native worker command could not be built; retry it unchanged"
	printf '%s\n' "$out"
}

INPUT_JSON=$(read_input)
[[ -n "$INPUT_JSON" ]] || exit 0
[[ "$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty')" == Bash ]] || exit 0

STAGE_ID=${LOOM_STAGE_ID:-}
SESSION_ID=${LOOM_SESSION_ID:-}
WORKTREE_PATH=${LOOM_WORKTREE_PATH:-}
[[ -n "$STAGE_ID" && -n "$SESSION_ID" ]] || exit 0
# Membership, not presence (same rule as loom_current_worktree in _common.sh):
# only a stage session, which runs in `<repo>/.worktrees/<stage-id>`, relays
# worker control. Anchored at the end so a repo under an outer `.worktrees/<id>/`
# does not count.
[[ "$WORKTREE_PATH" =~ /\.worktrees/[^/]+/?$ ]] || exit 0
CHECKOUT_ROOT=$WORKTREE_PATH
case "$STAGE_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper stage identity" ;; esac
case "$SESSION_ID" in *[!A-Za-z0-9_-]* | '') fail_closed "invalid wrapper session identity" ;; esac

COMMAND=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty')
# Command substitution drops NUL bytes, so the raw payload is asked directly.
COMMAND_HAS_NUL=$(printf '%s' "$INPUT_JSON" | jq -r '(.tool_input.command // "") | tostring | explode | any(. == 0)')
detect_worker_command "$COMMAND" || exit 0

caller_is_main ||
	block_not_pinned "only the stage's main agent may start, message or cancel a native Codex worker"
LOOM_BIN=$(resolve_trusted_loom) || fail_closed "no trusted loom binary was found (LOOM_BIN or a fixed install location)"
# The pinned command is built by plain concatenation, so the path must need no quoting.
[[ "$LOOM_BIN" =~ ^/[A-Za-z0-9._/@%+=:,-]+$ ]] ||
	fail_closed "the trusted loom binary path $LOOM_BIN holds characters that would need shell quoting"
# A PreToolUse payload carries no result; a payload that does is a stale
# PostToolUse registration, which this hook does not serve.
[[ "$(printf '%s' "$INPUT_JSON" | jq -r 'has("tool_result") or has("tool_response")')" != true ]] || exit 0
if worker_grammar_matches; then
	emit_pinned_allow
	exit 0
fi
REASON=$(rejection_reason "$LOOM_BIN")
block_not_pinned "a native worker command must be one exact spelling: loom subagents start|message|cancel followed by --flag value pairs and at most one trailing 2>&1 ($REASON)"
