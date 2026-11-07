#!/usr/bin/env bash
# worktree-isolation-git-target.sh - a GLOBAL git -C / --git-dir /
# --work-tree (between `git` and the subcommand) is allowed only when every
# resolved target lies in the current worktree or the session scratch
# directory. The token check matched `-C` anywhere in the segment, so
# `git grep -C 3 x`, `git log -C` and `git rev-parse --git-dir` blocked, as
# did a scratch clone under $LOOM_SCRATCH_DIR. Resolution is physical and
# fails closed: an unknown directory, a `$` other than an expandable
# $LOOM_SCRATCH_DIR, or a symlink out of scratch blocks.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$(cd "$SCRIPT_DIR/.." && pwd)/worktree-isolation.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-gittarget.XXXXXX")
TMP=$(cd -P "$TMP" && pwd -P)
BSD_DIR=""
trap 'rm -rf "$TMP" ${BSD_DIR:+"$BSD_DIR"}' EXIT
PATH_PREFIX="$PATH"
if [[ "${LOOM_HOOK_TEST_BSD:-}" == "1" ]]; then
    source "$SCRIPT_DIR/_bsd_path.sh"
    BSD_DIR=$(bsd_shim_dir)
    PATH_PREFIX="$BSD_DIR:$PATH"
fi

REPO="$TMP/repo"
WT="$REPO/.worktrees/stage"
S="$TMP/scratch"
mkdir -p "$REPO/.loom/work" "$REPO/.git" "$REPO/.worktrees/stage-x" \
    "$WT/loom" "$WT/a/b" "$WT/.loom" "$TMP/tmpdir" "$TMP/work" "$S/clone"
chmod 700 "$S" "$TMP/tmpdir"
ln -s ../../../.loom/work "$WT/.loom/work"
ln -s "$REPO" "$S/link"

FAILED=0
fail() {
    echo "FAIL: $1"
    FAILED=1
}

# run_hook <dir> <command> [VAR=value ...] - run the hook from <dir> (also the
# payload cwd) in the scrubbed hook environment; sets CODE, OUT and ERR.
run_hook() {
    local dir="$1" cmd="$2" payload
    shift 2
    payload=$(jq -nc --arg c "$cmd" --arg d "$dir" '{tool_name:"Bash",tool_input:{command:$c},cwd:$d}')
    CODE=0
    (cd "$dir" && printf '%s' "$payload" | env -u LOOM_SCRATCH_DIR -u LOOM_WORKTREE_PATH \
        -u LOOM_MAIN_AGENT_PID -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_WORK_DIR \
        TMPDIR="$TMP/tmpdir" PATH="$PATH_PREFIX" "$@" bash "$HOOK" >"$TMP/out" 2>"$TMP/err") ||
        CODE=$?
    OUT=$(cat "$TMP/out")
    ERR=$(cat "$TMP/err")
}

allow() {
    run_hook "$@"
    [[ $CODE -eq 0 ]] || fail "expected an allow from $1: $2 (exit $CODE) $ERR"
}

block() {
    local first
    run_hook "$@"
    first=$(printf '%s\n' "$ERR" | awk 'NF { print; exit }')
    if [[ $CODE -ne 2 ]]; then
        fail "expected a block from $1: $2 (exit $CODE) $OUT"
    elif [[ "$first" != "[worktree-isolation/git-override]"* ]]; then
        fail "block of $2 lacks the git-override tag: $first"
    fi
}

SCR="LOOM_SCRATCH_DIR=$S"

# --- allowed ---
allow "$WT" 'git -C . status'
allow "$WT" "git -C $WT/loom log"
allow "$WT" "git -C $S/clone status" "$SCR"
allow "$WT" 'git -C "$LOOM_SCRATCH_DIR/clone" status' "$SCR"
allow "$WT" 'git rev-parse --git-dir'
allow "$WT" 'git log -C'
allow "$WT" 'git grep -C 3 x'
allow "$WT" 'git log --work-tree'
allow "$WT/a/b" 'git -C ../.. status'
allow "$WT" 'git -c x=y --no-pager -C loom --git-dir=.git log'

# --- blocked ---
block "$WT" "git -C $REPO status"
block "$WT" 'git -C ../stage-x log'
block "$WT" "git --git-dir=$REPO/.git log"
block "$WT" "git -C $WT/.loom/work log"
block "$WT" 'git -C .loom/work/../.. status'
block "$WT/a/b" 'git -C ../.. -C ../.. rev-parse --show-toplevel'
block "$WT" "git -c x=y -C $REPO status"
block "$WT" "git --no-pager -C $REPO log"
block "$WT" "git --namespace ns -C $REPO log"
block "$WT" "git -C $S/link status" "$SCR"
block "$WT" 'LOOM_SCRATCH_DIR=/abs/repo git -C "$LOOM_SCRATCH_DIR" status' "$SCR"
block "$WT" "export LOOM_SCRATCH_DIR=$REPO; git -C \"\$LOOM_SCRATCH_DIR\" status" "$SCR"
block "$WT" 'git -C "$LOOM_SCRATCH_DIR/clone" status'
block "$WT" "git -C $S/clone status"
block "$WT" 'git -C "$HOME" status'
block "$WT" 'GIT_DIR=x git log'
block "$WT" 'eval "git log"'
block "$WT" 'cd "$X" && git -C . log'
block "$WT" "(cd $WT/a && :); git -C . log"
block "$WT" 'git -C'
block "$WT" "git -C . --work-tree=$REPO status"
# The tokenizer keeps glob, brace and escape text literal; bash expands it, so
# each form blocks even where its literal text would name scratch.
ANSI="\$'\\x2e\\x2e'"
block "$WT" "git -C $S/cl* status" "$SCR"
block "$WT" "git -C $S/clon? status" "$SCR"
block "$WT" "git -C $S/[c]lone status" "$SCR"
block "$WT" "git -C $S/{clone,x} status" "$SCR"
block "$WT" "git -C '$S/clone\\x' status" "$SCR"
block "$WT" "git -C $S/$ANSI/repo status" "$SCR"
block "$WT" "git --git-dir=$S/cl*/.git log" "$SCR"

# One block appends one ledger line carrying the rule.
run_hook "$WT" "git -C $REPO status" LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID=stage LOOM_SESSION_ID=x
if ! jq -e 'select(.guard == "worktree-isolation" and .rule == "git-override")' \
    "$TMP/work/heartbeat/stage.blocks.jsonl" >/dev/null 2>&1; then
    fail "the git-override block wrote no ledger line (exit $CODE)"
fi

if [[ $FAILED -ne 0 ]]; then exit 1; fi
echo PASS
