#!/usr/bin/env bash
# Git subcommand recognition over tokens from loom_tokenize_command.

loom_git_runs_commit() {
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
