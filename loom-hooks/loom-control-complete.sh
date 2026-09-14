#!/usr/bin/env bash
# Trusted PostToolUse bridge for one exact sandboxed completion command.

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

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

is_completion_command() {
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

completion_rejection_reason() {
	local trusted=$1
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
# The one main-repo session this bridge also serves is a CONFINED Knowledge
# session (LOOM_SCRATCH_DIR set): it cannot write the state directory, so it
# completes through the broker too (plan section 9). A legacy Knowledge session
# still completes in-process, and Merge, base-conflict and adjudication
# sessions never complete a stage here, so the bridge stays out of their way
# rather than pin their command.
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

collect_output_text() {
	printf '%s' "$INPUT_JSON" | jq -r '
      [.tool_result.stdout, .tool_result.output, .tool_response.stdout, .tool_response.output]
      | map(select(type == "string")) | join("\n")'
}

find_persisted_path() {
	local path
	path=$(printf '%s' "$INPUT_JSON" | jq -r \
		'.tool_response.persistedOutputPath // .tool_result.persistedOutputPath // empty')
	if [[ -z "$path" ]]; then
		path=$(loom_saved_output_path "$OUTPUT_TEXT")
	fi
	printf '%s' "$path"
}

# The raw path must name a harness-owned regular file beneath
# ~/.claude/projects/**/tool-results, without traversal or symlink indirection.
persisted_path_is_valid() {
	local path=$1 projects_root has_dotdot
	[[ -n "$path" && -n "${HOME:-}" ]] || return 1
	projects_root="$HOME/.claude/projects"
	case "$path" in
	../* | */../* | */.. | ..) has_dotdot=true ;;
	*) has_dotdot=false ;;
	esac
	[[ "$path" == /* && "$path" == "$projects_root"/* && "$path" == */tool-results/* &&
		"$has_dotdot" == false && -f "$path" && ! -L "$path" ]]
}

fail_outcome() {
	local message=$1 detail=${2:-}
	[[ -z "$detail" ]] || message="$message: $detail"
	fail_closed "$message"
}

handle_broker_outcome() {
	local token=$1 detail=$2
	if [[ "$TOOL_STATUS" == failed && ("$token" == accepted || "$token" == accepted_reconciled) ]]; then
		fail_closed "completion command failed; the broker returned an invalid acceptance outcome"
	fi
	case "$token" in
	accepted)
		jq -n --arg message "Stage '$STAGE_ID' completion was accepted by the daemon." \
			'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
		;;
	accepted_reconciled)
		jq -n --arg message "Stage '$STAGE_ID' completion was accepted by the daemon, reconciled from durable daemon state after a lost acknowledgement." \
			'{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
		;;
	tool_failed_recorded)
		fail_closed "completion command failed; diagnostic evidence was recorded; fix the failing check and rerun the pinned command"
		;;
	evidence_missing_recorded)
		fail_closed "the output carried no valid verification evidence record; a diagnostic was recorded"
		;;
	evidence_record_failed)
		fail_outcome "verification evidence could not be recorded durably" "$detail"
		;;
	daemon_rejected)
		fail_outcome "the daemon rejected the verified completion" "$detail"
		;;
	verified_pending_ack)
		fail_closed "verification passed but the daemon acknowledgement was lost; completion is pending; do not rerun blindly, check loom status"
		;;
	uncertain)
		fail_outcome "completion state is uncertain" "$detail"
		;;
	*) fail_closed "daemon completion broker failed: unrecognized outcome '$token'; $BROKER_OUTPUT" ;;
	esac
}

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
HAS_RESULT=$(printf '%s' "$INPUT_JSON" | jq -r 'has("tool_result") or has("tool_response")')
if [[ "$HAS_RESULT" != true ]]; then
	if [[ "$COMMAND" == "loom stage complete $STAGE_ID" ]]; then
		fail_closed "retry with the pinned command: $PINNED_COMMAND"
	fi
	if [[ "$COMMAND" != "$PINNED_COMMAND" ]]; then
		REASON=$(completion_rejection_reason "$LOOM_BIN")
		fail_closed "completion must be one exact pinned command: $PINNED_COMMAND ($REASON)"
	fi
	exit 0
fi

[[ "$COMMAND" == "$PINNED_COMMAND" ]] || fail_closed "completion result was not produced by the exact pinned command"

IS_ERROR=$(printf '%s' "$INPUT_JSON" | jq -r '(.tool_result.is_error // .tool_response.is_error // false)')
if [[ "$IS_ERROR" == true ]]; then TOOL_STATUS=failed; else TOOL_STATUS=ok; fi
OUTPUT_TEXT=$(collect_output_text)
PERSISTED_PATH=$(find_persisted_path)
PERSISTED_VALID=false
persisted_path_is_valid "$PERSISTED_PATH" && PERSISTED_VALID=true
[[ -z "$PERSISTED_PATH" || "$PERSISTED_VALID" == true ]] ||
	fail_closed "persisted tool output path is not trusted"

BROKER_RC=0
if [[ "$PERSISTED_VALID" == true ]]; then
	BROKER_OUTPUT=$(LOOM_CONTROL_BROKER=1 LOOM_CONTROL_TOOL_STATUS="$TOOL_STATUS" \
		"$LOOM_BIN" stage complete "$STAGE_ID" --session "$SESSION_ID" <"$PERSISTED_PATH") || BROKER_RC=$?
else
	BROKER_OUTPUT=$(printf '%s' "$OUTPUT_TEXT" | LOOM_CONTROL_BROKER=1 \
		LOOM_CONTROL_TOOL_STATUS="$TOOL_STATUS" "$LOOM_BIN" stage complete "$STAGE_ID" \
		--session "$SESSION_ID") || BROKER_RC=$?
fi

OUTCOME_LINE=$(printf '%s\n' "$BROKER_OUTPUT" | sed -n '/^LOOM_CONTROL_OUTCOME /p' | tail -n1)
[[ -n "$OUTCOME_LINE" ]] || fail_closed "daemon completion broker failed: $BROKER_OUTPUT"
OUTCOME_BODY=${OUTCOME_LINE#LOOM_CONTROL_OUTCOME }
OUTCOME_TOKEN=${OUTCOME_BODY%% *}
if [[ "$OUTCOME_BODY" == "$OUTCOME_TOKEN" ]]; then
	OUTCOME_DETAIL=""
else
	OUTCOME_DETAIL=${OUTCOME_BODY#"$OUTCOME_TOKEN" }
fi
handle_broker_outcome "$OUTCOME_TOKEN" "$OUTCOME_DETAIL"
