#!/usr/bin/env bash
# commit-filter-scratch-repo.sh - in a loom stage session, `git add` and
# `git commit` in a repository under LOOM_SCRATCH_DIR pass commit-filter.sh for
# a subagent and for the main agent (loom_git_targets_scratch_only in
# _git-command.sh). Every way of pointing such a command back at the stage's
# own repository still blocks, as do `loom stage commit` and a subagent's
# `loom stage complete`, the isolation-bypass rule and git-add-guard.sh.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FILTER="$ROOT/loom-hooks/commit-filter.sh"
ADD_GUARD="$ROOT/loom-hooks/git-add-guard.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
TMP=$(cd -P "$TMP" && pwd -P)

git_q() { git -c user.name=t -c user.email=t@example.com -c init.defaultBranch=main "$@" >/dev/null 2>&1; }

# Fixture: repo with a real worktree and its .loom/work symlink, a uid-owned
# mode-700 scratch dir outside it, an empty TMPDIR, and git discovery stopped
# at $TMP so an enclosing checkout never answers for a fixture directory.
REPO="$TMP/repo"
WT="$REPO/.worktrees/st"
S="$TMP/scratch"
mkdir -p "$REPO" "$TMP/tmpdir" "$S"
chmod 700 "$TMP/tmpdir" "$S"
git_q -C "$REPO" init -q
printf 'a\n' >"$REPO/a.md"
git_q -C "$REPO" add a.md
git_q -C "$REPO" commit -qm init
git_q -C "$REPO" worktree add -q -b loom/st .worktrees/st
mkdir -p "$REPO/.loom/work" "$WT/.loom" "$WT/doc"
ln -s ../../../.loom/work "$WT/.loom/work"
mkdir -p "$S/fx/doc" "$S/gf"
printf 'a\n' >"$S/fx/doc/a.md"
git_q -C "$S/fx" init -q
printf 'gitdir: %s\n' "$REPO/.git/worktrees/st" >"$S/gf/.git"
ln -s "$REPO" "$S/link"

# run_hook <hook> <payload> <role> [VAR=value...] - run <hook> from the
# worktree with every ambient LOOM_*/GIT_* gate scrubbed, as a stage session.
# Role `subagent` adds the live LOOM_MAIN_AGENT_PID chain. Sets LAST_STDERR.
run_hook() {
	local hook="$1" input="$2" role="$3" pid=()
	shift 3
	if [[ "$role" == subagent ]]; then pid=(LOOM_MAIN_AGENT_PID=$$); fi
	LAST_STDERR=$(cd "$WT" && printf '%s' "$input" |
		env -u LOOM_WORK_DIR -u LOOM_MAIN_AGENT_PID -u LOOM_HOOK_CONTEXT -u LOOM_SCRATCH_DIR \
			-u LOOM_WORKTREE_PATH -u LOOM_CODEX_FORWARD -u GIT_DIR -u GIT_WORK_TREE \
			TMPDIR="$TMP/tmpdir" GIT_CEILING_DIRECTORIES="$TMP" \
			LOOM_STAGE_ID=st LOOM_SESSION_ID=sess LOOM_SCRATCH_DIR="$S" \
			${pid[@]+"${pid[@]}"} "$@" bash "$hook" 2>&1 >/dev/null)
}

payload() { # <role> <command> [cwd]
	local extra='{}'
	if [[ "$1" == subagent ]]; then
		extra='{agent_type:"loom-software-engineer",transcript_path:"/h/.claude/projects/p/subagents/agent-x.jsonl"}'
	fi
	jq -nc --arg c "$2" --arg d "${3:-$WT}" "{tool_name:\"Bash\",tool_input:{command:\$c},cwd:\$d} + $extra"
}

# expect <want-exit> <role> <command> [hook] [cwd] [VAR=value...]
expect() {
	local want="$1" role="$2" command="$3" hook="${4:-$FILTER}" cwd="${5:-$WT}" code=0
	shift 3
	shift $(($# < 2 ? $# : 2))
	run_hook "$hook" "$(payload "$role" "$command" "$cwd")" "$role" "$@" || code=$?
	if [[ $code -ne $want ]]; then
		echo "FAIL: $role: $command - expected exit $want, got $code: $LAST_STDERR"
		exit 1
	fi
}

# --- allowed for both roles --------------------------------------------------
for role in subagent main; do
	expect 0 "$role" "cd $S/fx && git add doc && git commit -qm x"
	expect 0 "$role" 'cd "$LOOM_SCRATCH_DIR/fx" && git add doc && git commit -qm x'
	expect 0 "$role" "mkdir -p $S/fx2 && cd $S/fx2 && git init -q && git add doc && git commit -qm x"
	expect 0 "$role" "git init -q $S/fx3 && git -C $S/fx3 add doc && git -C $S/fx3 commit -qm x"
	expect 0 "$role" "git add doc && git commit -qm x" "$FILTER" "$S/fx"
	expect 0 "$role" "cd $S/fx && printf 'b\n' > doc/b.md && git add doc && git commit -qm x"
	# `&&` then a newline continues the chain exactly as `&&` does.
	expect 0 "$role" "git init -q $S/fx6 &&"$'\n'"git -C $S/fx6 commit -qm x"
done

# --- blocked for both roles --------------------------------------------------
for role in subagent main; do
	for command in \
		"cd $WT && git add doc && git commit -qm x" \
		"mkdir -p $WT/fx2 && cd $WT/fx2 && git init -q && git add doc && git commit -qm x" \
		'cd "$X" && git commit -qm x' \
		"cd $S/fx && git -C $WT commit -qm x" \
		"git -C $S/fx -C $WT commit -qm x" \
		"cd $S/fx && GIT_DIR=$REPO/.git git commit -qm x" \
		"cd $S/fx && git --work-tree=$WT commit -qm x" \
		"cd $S/fx && git --git-dir=$REPO/.git commit -qm x" \
		"cd $S/fx && git -c core.worktree=$WT commit -qm x" \
		"cd $S/gf && git commit -qm x" \
		"cd $S/link && git commit -qm x" \
		"(cd $S/fx && :); git commit -qm x" \
		"true;\$(cd $S/fx && :);git commit -qm x" \
		"bash -c \"cd $S/fx\"; git commit -qm x" \
		"cd $S/missing && :; git commit -qm x" \
		"cd $S/fx && git commit -qm x; cd $WT && git commit -qm y" \
		"cd $S/fx && git commit -qm x || git -C $WT commit -qm y" \
		"cd $S/fx; git init -q --separate-git-dir=$REPO/.git/x && git commit -qm x" \
		"mkdir -p $S/fx4 && cd $S/fx4 && git commit -qm x" \
		"mkdir -p $S/fx5 && cd $S/fx5 && git init -q; git commit -qm x" \
		"ln -s $REPO $S/new && cd $S/new && git init -q && git commit -qm x" \
		"cd $S/fx && printf 'gitdir: x' > .git && git commit -qm x" \
		"cd $S/fx && export GIT_WORK_TREE=$WT && git commit -qm x" \
		"cd $S/fx && env GIT_DIR=$REPO/.git git commit -qm x" \
		"unset LOOM_MAIN_AGENT_PID; git status"; do
		expect 2 "$role" "$command"
	done
	# Glob, brace and escape text the tokenizer keeps literal names another
	# directory once bash expands it, so it earns no scratch exemption.
	ansi="\$'\\x67x'"
	for target in "$S/g*" "$S/g?" "$S/[g]x" "$S/{gx,gy}" "'$S/g\\x'" "$S/$ansi"; do
		expect 2 "$role" "mkdir -p $S/gx && cd $target && git init -q && git commit -qm x"
		expect 2 "$role" "git init -q $target && git -C $target commit -qm x"
	done
	# A single-quoted or escaped `$` stays literal: bash runs git in
	# <wt>/$LOOM_SCRATCH_DIR/fx, inside the worktree, while $S/fx is a real repo.
	for lit in "'\$LOOM_SCRATCH_DIR/fx'" "\\\$LOOM_SCRATCH_DIR/fx" "'\${LOOM_SCRATCH_DIR}/fx'" \
		"\"\\\$LOOM_SCRATCH_DIR/fx\"" "\$\\LOOM_SCRATCH_DIR/fx" "\"\$\"LOOM_SCRATCH_DIR/fx"; do
		expect 2 "$role" "mkdir -p $lit && git -C $lit commit -qm x"
		expect 2 "$role" "cd $lit && git commit -qm x"
	done
	# Without a valid LOOM_SCRATCH_DIR the scratch path is just outside.
	expect 2 "$role" "cd $S/fx && git commit -qm x" "$FILTER" "$WT" LOOM_SCRATCH_DIR=
	expect 2 "$role" "cd $S/fx && git commit -qm x" "$FILTER" "$WT" LOOM_SCRATCH_DIR=relative
	expect 2 "$role" "cd $S/fx && git add -A" "$ADD_GUARD"
	[[ "$LAST_STDERR" == *"[git-add-guard/dangerous-add]"* ]] || {
		echo "FAIL: $role: git-add-guard block lacks its rule prefix: $LAST_STDERR"
		exit 1
	}
	expect 2 "$role" "cd $S/fx && git commit -qm x -m 'Co-Authored-By: Claude <noreply@anthropic.com>'"
	[[ "$LAST_STDERR" == *"[commit-filter/ai-attribution]"* ]] || {
		echo "FAIL: $role: attribution block lacks its rule prefix: $LAST_STDERR"
		exit 1
	}
done
expect 2 main "cd $WT && git commit -qm x"
[[ "$LAST_STDERR" == *"[commit-filter/no-git-commit]"* ]] || {
	echo "FAIL: main: worktree commit lacks the no-git-commit prefix: $LAST_STDERR"
	exit 1
}

# --- blocked for a subagent (the main agent owns these commands) ------------
for command in \
	"cd $S/fx && c=commit; git \$c -qm x" \
	"cd $S/fx && loom stage commit st -m x" \
	"git -C $S/fx status; loom stage commit st -m x" \
	"cd $S/fx && loom stage complete st"; do
	expect 2 subagent "$command"
done
expect 2 subagent "cd $WT && git commit -qm x"
[[ "$LAST_STDERR" == *"[commit-filter/subagent-git]"* ]] || {
	echo "FAIL: subagent: worktree commit lacks the subagent-git prefix: $LAST_STDERR"
	exit 1
}

echo "PASS: commit-filter-scratch-repo"
