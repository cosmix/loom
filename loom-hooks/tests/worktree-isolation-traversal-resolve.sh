#!/usr/bin/env bash
# worktree-isolation-traversal-resolve.sh - worktree-isolation.sh decides `..`
# traversal by resolving each candidate against the directory its token runs
# in (loom_track_cwd), physically. The lexical `../..` check blocked
# `cd web/src && cat ../../doc/x.md` (inside the worktree) and allowed
# `cat ../sibling/x` (another stage). A `../..` token whose directory cannot
# be known (a subshell, `sh -c`, a cd that may not have run) warns instead of
# guessing, and a block anywhere in the command wins over that warning.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$(cd "$SCRIPT_DIR/.." && pwd)/worktree-isolation.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-traversal.XXXXXX")
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
mkdir -p "$REPO/.loom/work" "$REPO/.git" "$REPO/.worktrees/stage-x" \
    "$WT/web/src" "$WT/web/components" "$WT/scripts" "$WT/doc" "$WT/src" "$WT/a/b" "$WT/.loom" \
    "$TMP/tmpdir" "$TMP/elsewhere/a/b" "$TMP/scratch/a/b"
chmod 700 "$TMP/tmpdir" "$TMP/scratch"
printf 'inside\n' >"$WT/doc/x.md"
ln -s ../../../.loom/work "$WT/.loom/work"
# A directory inside the worktree by name that resolves outside it.
ln -s "$TMP/elsewhere/a/b" "$WT/out"

FAILED=0
fail() {
    echo "FAIL: $1"
    FAILED=1
}

# run_hook <dir> <command> - run the hook from <dir> (also the payload cwd)
# in the scrubbed hook environment; sets CODE, OUT (stdout) and ERR.
# HOOK_SCRATCH, when set, becomes LOOM_SCRATCH_DIR for the scratch cases only.
HOOK_SCRATCH=""
run_hook() {
    local dir="$1" cmd="$2" payload
    payload=$(jq -nc --arg c "$cmd" --arg d "$dir" '{tool_name:"Bash",tool_input:{command:$c},cwd:$d}')
    CODE=0
    (cd "$dir" && printf '%s' "$payload" | env -u LOOM_SCRATCH_DIR -u LOOM_WORKTREE_PATH \
        -u LOOM_MAIN_AGENT_PID -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_WORK_DIR \
        ${HOOK_SCRATCH:+"LOOM_SCRATCH_DIR=$HOOK_SCRATCH"} TMPDIR="$TMP/tmpdir" PATH="$PATH_PREFIX" bash "$HOOK" >"$TMP/out" 2>"$TMP/err") || CODE=$?
    OUT=$(cat "$TMP/out")
    ERR=$(cat "$TMP/err")
}

allow() {
    run_hook "$1" "$2"
    if [[ $CODE -ne 0 || "$OUT" == *LOOM_HOOK_WARN* ]]; then
        fail "expected a silent allow from $1: $2 (exit $CODE) $OUT $ERR"
    fi
}

block() {
    local first
    run_hook "$1" "$2"
    first=$(printf '%s\n' "$ERR" | awk 'NF { print; exit }')
    if [[ $CODE -ne 2 ]]; then
        fail "expected a block from $1: $2 (exit $CODE) $OUT $ERR"
    elif [[ "$first" != "[worktree-isolation/path-traversal]"* ]]; then
        fail "block of $2 lacks the path-traversal tag: $first"
    fi
}

warn() {
    run_hook "$1" "$2"
    if [[ $CODE -ne 0 ]]; then
        fail "expected a warning from $1: $2 (exit $CODE) $ERR"
    elif ! jq -e '.hookSpecificOutput.additionalContext | startswith("LOOM_HOOK_WARN: ")' \
        <<<"$OUT" >/dev/null 2>&1; then
        fail "expected LOOM_HOOK_WARN on stdout from $1: $2, got: $OUT"
    fi
}

# --- allowed: resolves inside the worktree ---
allow "$WT/web/src" 'sed -n 1p ../../doc/x.md'
allow "$WT/web/src" 'git status --short -- . ../components ../../scripts'
allow "$WT" "cd $WT/web/src && cat ../../doc/x.md"
allow "$WT" 'cd web/src && cat ../../doc/x.md'
allow "$WT" 'cat "$X/../y"'
allow "$WT/scripts" 'rg --file=../web/x pat'
allow "$WT" 'cat "..\..\file.txt"'
allow "$WT" 'echo "see ../../../src/y for details"'
# A glob after the last `..` expands inside the resolved directory.
allow "$WT" 'cat src/../*.md'

# --- blocked: resolves outside the worktree ---
block "$WT" 'cat ../../secret'
block "$WT" 'cd web && cat ../../../other/x'
block "$WT" 'cat ../stage-x/x'
block "$WT" 'cp x ../../../../etc/y'
block "$WT/out" 'cat ../../x'
block "$WT" 'cat .loom/work/../../x'
block "$WT" 'cat ../../../x=y'
block "$WT" 'rg --file=../stage-x/f pat'
block "$WT" 'cat $HOME/../../x'
block "$WT" 'cat "../../doc/my notes.md"'
block "$WT" 'cd ..'
# A glob or brace component before `..` expands first (w* is the `work`
# symlink), so `..` follows it physically into the main checkout.
block "$WT" 'cat .loom/w*/../../CLAUDE.md'
block "$WT" 'cat .loom/w?rk/../../x'
block "$WT" 'cat .loom/{work,x}/../../x'
block "$WT" 'cat .loom/w[o]rk/../../x'
block "$WT" 'cat .loom/w*/sub/../../../x'
# A backslash component is shell escape text: bash reads w\ork as `work`.
block "$WT" 'cat .loom/w\ork/../../x'
# A block wins over a warning raised by the same command.
block "$WT" "cat $WT/../stage-x/f; cd \"\$X\" && cat ../../x"

# --- warned: the directory is unknown, so nothing may resolve against <wt>/a/b ---
for rel in ../../../x ../../x; do
    warn "$WT" "cd \"\$X\" && cat $rel"
    warn "$WT" "(cd $WT/a/b && :) ; cat $rel"
    warn "$WT" "true;\$(cd $WT/a/b && :);cat $rel"
    warn "$WT" "bash -c \"cd $WT/a/b\"; cat $rel"
    warn "$WT" "cd $WT/missing && :; cat $rel"
done

# --- scratch: the literal ${LOOM_SCRATCH_DIR} prefix is not a brace glob ---
HOOK_SCRATCH="$TMP/scratch"
allow "$WT" 'cat ${LOOM_SCRATCH_DIR}/a/b/../../c'
block "$WT" 'cat ${LOOM_SCRATCH_DIR}/w*/../../x'
HOOK_SCRATCH=""

if [[ $FAILED -ne 0 ]]; then exit 1; fi
echo PASS
