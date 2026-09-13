#!/usr/bin/env bash
# _codex_forward.sh - Shared Codex forwarding command parser.

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
