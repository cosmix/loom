#!/usr/bin/env bash
# _codex_forward.sh - Shared Codex forwarding command parser and stage probe.

if [[ "${_LOOM_CODEX_FORWARD_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_CODEX_FORWARD_LOADED=1

source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"

parse_shell_words() {
	local input="$1" state=plain word="" char="" started=0 i
	PARSED_WORDS=()

	for ((i = 0; i < ${#input}; i++)); do
		char=${input:i:1}
		case "$state" in
		plain)
			case "$char" in
			' ')
				if [[ $started -eq 1 ]]; then
					PARSED_WORDS+=("$word")
					word=""
					started=0
				fi
				;;
			"'") state=single; started=1 ;;
			'"') state=double; started=1 ;;
			'\') state=escape; started=1 ;;
			$'\n' | $'\r' | $'\t' | $'\v' | $'\f' | ';' | '|' | '&' | '<' | '>' | '`' | '$' | '(' | ')' | '#' | '*' | '?' | '[' | ']' | '{' | '}') return 1 ;;
			*) word+="$char"; started=1 ;;
			esac
			;;
		single)
			if [[ "$char" == "'" ]]; then state=plain; else word+="$char"; fi
			;;
		double)
			case "$char" in
			'"') state=plain ;;
			'\') state=double_escape ;;
			$'\n' | $'\r' | $'\t' | $'\v' | $'\f' | '$' | '`') return 1 ;;
			*) word+="$char" ;;
			esac
			;;
		escape)
			case "$char" in $'\n' | $'\r' | $'\t' | $'\v' | $'\f') return 1 ;; esac
			word+="$char"
			state=plain
			;;
		double_escape)
			case "$char" in
			'"' | '\') word+="$char"; state=double ;;
			*) return 1 ;;
			esac
			;;
		esac
	done

	[[ "$state" == plain ]] || return 1
	if [[ $started -eq 1 ]]; then PARSED_WORDS+=("$word"); fi
}

# Stage evidence.
#
# codex-forward-guard.sh only has a forwarding policy to enforce inside a loom
# stage, so "is this a stage?" decides whether it enforces anything at all.
# The answer therefore rests only on signals an agent running inside a stage
# cannot shed: the hook's own environment, an ancestor's initial environment,
# and sandbox confinement. The payload cwd, the prompt sentinel, the
# transcript and anything under the worktree are agent-controlled and prove
# nothing. There is deliberately no environment override below - a variable
# able to hide evidence would be the bypass this probe exists to prevent - and
# the state these functions memoize is reset here so an inherited value cannot
# pre-seed the answer.
_LOOM_STAGE_EVIDENCE_DONE=0
_LOOM_PLATFORM_CACHE=""
LOOM_STAGE_EVIDENCE=""

# The three names loom exports into a stage session. The other LOOM_*
# variables travel further than a stage, so they are not evidence.
_loom_stage_env_evidence() {
	local name
	for name in LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR; do
		if [[ -n "${!name:-}" ]]; then
			printf 'env %s' "$name"
			return 0
		fi
	done
	return 1
}

# Every probe binary below is named by absolute path: a nested session
# controls PATH, so a probe resolved through it could be answered by a stub.
_loom_platform() {
	if [[ -z "$_LOOM_PLATFORM_CACHE" ]]; then
		if [[ -x /usr/bin/uname ]]; then
			_LOOM_PLATFORM_CACHE=$(/usr/bin/uname -s 2>/dev/null || true)
		elif [[ -x /bin/uname ]]; then
			_LOOM_PLATFORM_CACHE=$(/bin/uname -s 2>/dev/null || true)
		fi
		[[ -n "$_LOOM_PLATFORM_CACHE" ]] || _LOOM_PLATFORM_CACHE=unknown
	fi
	printf '%s' "$_LOOM_PLATFORM_CACHE"
}

# A name matches only at the start of an environment entry, so a value that
# merely mentions LOOM_STAGE_ID is not evidence.
_loom_pid_env_has_stage_var() {
	local pid="$1" entry text hit=1
	case "$(_loom_platform)" in
	Darwin)
		text=$(/bin/ps eww -o command= -p "$pid" 2>/dev/null || true)
		case " $text" in
		*" LOOM_STAGE_ID="?* | *" LOOM_SESSION_ID="?* | *" LOOM_WORK_DIR="?*) hit=0 ;;
		esac
		;;
	*)
		[[ -r "/proc/$pid/environ" ]] || return 1
		while IFS= read -r -d '' entry; do
			case "$entry" in
			LOOM_STAGE_ID=?* | LOOM_SESSION_ID=?* | LOOM_WORK_DIR=?*)
				hit=0
				break
				;;
			esac
		done <"/proc/$pid/environ" 2>/dev/null || true
		;;
	esac
	return "$hit"
}

_loom_parent_pid() {
	local pid="$1" key rest value=""
	case "$(_loom_platform)" in
	Darwin)
		value=$(/bin/ps -o ppid= -p "$pid" 2>/dev/null || true)
		read -r value <<<"$value" || true
		;;
	*)
		[[ -r "/proc/$pid/status" ]] || return 1
		while read -r key rest; do
			if [[ "$key" == PPid: ]]; then
				value=$rest
				break
			fi
		done <"/proc/$pid/status" 2>/dev/null || true
		;;
	esac
	[[ "$value" =~ ^[0-9]+$ ]] || return 1
	printf '%s' "$value"
}

# An ancestor's initial environment still carries the stage identity when the
# hook's own process was launched with a scrubbed one, which is what a nested
# `env -i claude ...` started from inside a stage produces. An ancestor whose
# environment cannot be read belongs to another user, and a loom session runs
# as the user, so it is skipped rather than counted.
_loom_ancestor_stage_evidence() {
	local pid="$$" depth=0
	while ((depth < 12)); do
		if _loom_pid_env_has_stage_var "$pid"; then
			printf 'ancestor pid %s environment' "$pid"
			return 0
		fi
		pid=$(_loom_parent_pid "$pid") || return 1
		((pid > 1)) || return 1
		depth=$((depth + 1))
	done
	return 1
}

# A sandbox profile is inherited by every descendant and cannot be dropped,
# which is what catches a process that left the ancestry chain above: on macOS
# a double-forked process reparents to launchd, yet stays inside the stage's
# Seatbelt profile.
_loom_confinement_evidence() {
	local comm=""
	case "$(_loom_platform)" in
	Darwin)
		# A nested Seatbelt profile is refused, so the probe failing means this
		# process is already inside one. codex-forward.sh runs the same probe to
		# pick its lane; its copy is not shared here because it resolves
		# sandbox-exec through PATH so its own tests can stub it, and a
		# stub-able probe would defeat this one.
		[[ -x /usr/bin/sandbox-exec && -x /usr/bin/true ]] || return 1
		if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true >/dev/null 2>&1; then
			return 1
		fi
		printf 'sandbox confinement (seatbelt)'
		;;
	*)
		# Claude Code's Linux Bash sandbox is bubblewrap with its own pid
		# namespace, where the walk above cannot reach the host session. Pid 1
		# of that namespace is bwrap itself; an ordinary session sees init.
		[[ -r /proc/1/comm ]] || return 1
		read -r comm </proc/1/comm 2>/dev/null || true
		[[ "$comm" == bwrap ]] || return 1
		printf 'sandbox confinement (bubblewrap)'
		;;
	esac
}

# loom_stage_evidence - Answer "is this hook running inside a loom stage?"
#
# Sets LOOM_STAGE_EVIDENCE to a readable name for the signal that answered yes,
# or to the empty string when none did, and returns 0 for yes, 1 for no. The
# signals are tried cheapest first and the answer is computed once, because the
# later probes spawn processes: callers consult it only on a path that is about
# to block or rewrite a call. A probe that errors contributes no evidence.
loom_stage_evidence() {
	if [[ "$_LOOM_STAGE_EVIDENCE_DONE" != 1 ]]; then
		_LOOM_STAGE_EVIDENCE_DONE=1
		LOOM_STAGE_EVIDENCE=$(_loom_stage_env_evidence || _loom_ancestor_stage_evidence ||
			_loom_confinement_evidence || true)
	fi
	[[ -n "$LOOM_STAGE_EVIDENCE" ]]
}
