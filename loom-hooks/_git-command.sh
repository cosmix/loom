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
					# git.c's global options that may take their value as the next word.
					-C | -c | --git-dir | --work-tree | --namespace | --attr-source | --config-env | --super-prefix)
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

# --- Scratch-repository exemption -------------------------------------------
#
# loom_git_targets_scratch_only - 0 when every git segment of the tokenized
# command works on a repository under LOOM_SCRATCH_DIR, so commit-filter.sh may
# lift its `git commit` / `git add` rules for it. The caller tokenized the
# command (TOKENS_OK=1) and holds the hook payload in INPUT_JSON; the helpers
# of _containment.sh and _bounded.sh are already sourced. Anything it cannot
# prove is no exemption. The checks, in order:
#   - no token is `env`, starts with GIT_, holds a backtick, names a `.git`
#     path component, or holds a `$` other than a leading LOOM_SCRATCH_DIR read;
#   - every non-git command word is one of a few that cannot move a directory
#     or plant a symlink (cd mkdir true : echo printf touch ls pwd cat), so the
#     repository a later segment reaches is the one inspected at hook time;
#   - each git segment passes _loom_git_scratch_segment;
#   - at least one git segment exists (`cd <scratch> && loom stage complete`
#     is never exempt).
# Known limit: repository state is read at hook time, so a script or a git
# hook the command runs can still reach elsewhere; this is the same
# best-effort boundary as the rest of commit-filter.sh.
loom_git_targets_scratch_only() {
	[[ "${TOKENS_OK:-0}" == 1 ]] || return 1
	local n=${#LOOM_TOKENS[@]} i s=0 j seps base chain=0 gits=0 cont=0 cmd_git=0
	_LOOM_GIT_INITS=()
	_LOOM_GIT_NINIT=0
	_loom_git_scratch_tokens_safe "$n" || return 1
	loom_track_cwd "$(jq -r '.cwd // empty' <<<"${INPUT_JSON:-}" 2>/dev/null || true)"
	for ((i = 0; i <= n; i++)); do
		((i == n)) || [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]] || continue
		# A segment after a redirection is its target plus more arguments of the
		# same command, never a command of its own.
		if ((cont == 0)); then
			cmd_git=0
			if ((i > s)) && j=$(loom_tokens_command_word_index "$s"); then
				base="${LOOM_TOKENS[$j]##*/}"
				case "$base" in
				git)
					_loom_git_scratch_segment "$s" "$j" "$i" "$chain" || return 1
					cmd_git=1
					gits=$((gits + 1))
					;;
				cd | mkdir | true | : | echo | printf | touch | ls | pwd | cat) ;;
				*) return 1 ;;
				esac
			fi
		fi
		seps=""
		((i == n)) || loom_token_sep "$i" seps
		cont=0
		case "$seps" in
		*[\<\>]*)
			# A redirected git command hides its later words from the segment scan.
			((cmd_git == 0)) || return 1
			cont=1
			;;
		'&&') ;;
		*) chain=$((chain + 1)) ;;
		esac
		s=$((i + 1))
	done
	((gits > 0))
}

# _loom_git_scratch_tokens_safe <n> - the token-level conditions of
# loom_git_targets_scratch_only over LOOM_TOKENS[0..n).
_loom_git_scratch_tokens_safe() {
	local i tok rest v1='$LOOM_SCRATCH_DIR' v2='${LOOM_SCRATCH_DIR}'
	for ((i = 0; i < $1; i++)); do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		env | GIT_* | *'`'* | .git | .git/* | */.git | */.git/*) return 1 ;;
		"$v1"*) rest="${tok#"$v1"}" ;;
		"$v2"*) rest="${tok#"$v2"}" ;;
		*) rest="$tok" ;;
		esac
		[[ "$rest" != *'$'* ]] || return 1
	done
	return 0
}

# _loom_git_scratch_segment <start> <git-index> <end> <chain> - the git
# segment LOOM_TOKENS[start..end), whose command word sits at <git-index>,
# targets a scratch repository. It carries no repository override (-c,
# --git-dir, --work-tree, --namespace, --config-env, --separate-git-dir, a
# second -C) and no wrapper option or user switch before `git`. Its repository
# is its -C argument normalized against its cwd, else its cwd (a counting
# `git init <dir>` targets <dir>). That repository is class scratch, and either
# git reports its toplevel, git dir and common dir all in scratch, or no
# repository exists there yet and this segment, or an earlier segment of the
# same `&&` chain, is a counting `git init` of it.
_loom_git_scratch_segment() {
	local s="$1" j="$2" e="$3" chain="$4" k ncap=0 dir="" tgt init=0 x
	local cwd="${LOOM_TOKEN_CWD[$j]:-}"
	[[ -n "$cwd" ]] || return 1
	for ((k = s; k < j; k++)); do
		case "${LOOM_TOKENS[$k]}" in -* | sudo | doas | xargs) return 1 ;; esac
	done
	for ((k = j + 1; k < e; k++)); do
		case "${LOOM_TOKENS[$k]}" in
		-c | --git-dir* | --work-tree* | --namespace* | --config-env* | --separate-git-dir*) return 1 ;;
		-C) ncap=$((ncap + 1)) ;;
		esac
	done
	((ncap <= 1)) || return 1
	k=$((j + 1))
	while ((k < e)) && [[ "${LOOM_TOKENS[$k]}" == -* ]]; do
		case "${LOOM_TOKENS[$k]}" in
		-C | --attr-source | --super-prefix)
			((k + 1 < e)) || return 1
			[[ "${LOOM_TOKENS[$k]}" != -C ]] || dir="${LOOM_TOKENS[$((k + 1))]}"
			k=$((k + 2))
			;;
		*) k=$((k + 1)) ;;
		esac
	done
	if [[ -n "$dir" ]]; then dir=$(loom_normalize_word "$cwd" "$dir") || return 1; else dir="$cwd"; fi
	if ((k < e)) && [[ "${LOOM_TOKENS[$k]}" == init ]] &&
		tgt=$(_loom_git_init_target "$dir" $((k + 1)) "$e"); then
		dir="$tgt"
		init=1
	fi
	[[ "$dir" != *$'\n'* && "$(loom_path_class "$dir")" == scratch ]] || return 1
	_loom_git_repo_in_scratch "$dir" && return 0
	_loom_git_no_repo "$dir" || return 1
	if ((init)); then
		_LOOM_GIT_INITS[$_LOOM_GIT_NINIT]="$chain|$dir"
		_LOOM_GIT_NINIT=$((_LOOM_GIT_NINIT + 1))
		return 0
	fi
	for ((x = 0; x < _LOOM_GIT_NINIT; x++)); do
		[[ "${_LOOM_GIT_INITS[$x]}" != "$chain|$dir" ]] || return 0
	done
	return 1
}

# _loom_git_init_target <repo-dir> <first-arg> <end> - print the directory a
# `git init` with arguments LOOM_TOKENS[first-arg..end) creates, run in
# <repo-dir>. Returns 1 for any option other than -q, --quiet, -b and
# --initial-branch (--separate-git-dir, --bare and --template change where the
# repository lives), and for more than one directory argument.
_loom_git_init_target() {
	local dir="$1" k="$2" e="$3" tok arg=""
	while ((k < e)); do
		tok="${LOOM_TOKENS[$k]}"
		case "$tok" in
		-q | --quiet | --initial-branch=*) k=$((k + 1)) ;;
		-b | --initial-branch)
			((k + 1 < e)) || return 1
			k=$((k + 2))
			;;
		-*) return 1 ;;
		*)
			[[ -z "$arg" ]] || return 1
			arg="$tok"
			k=$((k + 1))
			;;
		esac
	done
	if [[ -z "$arg" ]]; then
		printf '%s' "$dir"
		return 0
	fi
	loom_normalize_word "$dir" "$arg"
}

# _loom_git_repo_in_scratch <dir> - git, ignoring the hook's own GIT_DIR,
# GIT_WORK_TREE and GIT_COMMON_DIR, names a toplevel, git dir and common dir
# for <dir> that are all class scratch. Checking the git dirs as well defeats a
# `.git` gitfile in scratch pointing into the main repository, where git
# reports the scratch toplevel but `git commit` writes to the main repository.
_loom_git_repo_in_scratch() {
	local out p abs count=0
	out=$(loom_run_bounded 2 env -u GIT_DIR -u GIT_WORK_TREE -u GIT_COMMON_DIR \
		git -C "$1" rev-parse --show-toplevel --absolute-git-dir --git-common-dir \
		2>/dev/null </dev/null) || return 1
	while IFS= read -r p; do
		abs=$(loom_normalize_path "$1" "$p") || return 1
		[[ "$(loom_path_class "$abs")" == scratch ]] || return 1
		count=$((count + 1))
	done <<<"$out"
	((count == 3))
}

# _loom_git_no_repo <dir> - the nearest existing ancestor of <dir> (itself
# when it exists) is class scratch and git finds no repository there, looking
# no higher than the scratch root. Only git's own "not a repository" status
# (128) counts: a timeout is no proof.
_loom_git_no_repo() {
	local a="$1" root rc=0
	while [[ -n "$a" && ! -d "$a" ]]; do a="${a%/*}"; done
	[[ -n "$a" && "$(loom_path_class "$a")" == scratch ]] || return 1
	root=$(_loom_scratch_root) || return 1
	loom_run_bounded 2 env -u GIT_DIR -u GIT_WORK_TREE -u GIT_COMMON_DIR \
		GIT_CEILING_DIRECTORIES="$root" git -C "$a" rev-parse --git-dir \
		>/dev/null 2>&1 </dev/null || rc=$?
	((rc == 128))
}
