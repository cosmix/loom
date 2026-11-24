#!/usr/bin/env bash
# Absolute Bash paths into the main checkout must use the stage's worktree.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$(cd "$SCRIPT_DIR/.." && pwd)/worktree-isolation.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-main-checkout.XXXXXX")
FIXTURE_HOME=""
trap 'rm -rf "$TMP" ${FIXTURE_HOME:+"$FIXTURE_HOME"}' EXIT
FIXTURE_HOME=$(mktemp -d "${TMPDIR:-/tmp}/loom-main-home.XXXXXX")
TMP=$(cd -P "$TMP" && pwd -P)
FIXTURE_HOME=$(cd -P "$FIXTURE_HOME" && pwd -P)

REPO="$TMP/repo"
WT="$REPO/.worktrees/stage"
SCRATCH="$TMP/scratch"
mkdir -p "$WT/src" "$REPO/.worktrees/other-stage" "$REPO/src" "$REPO/.loom/work/signals" \
    "$FIXTURE_HOME/.cargo/bin" "$TMP/tmpdir" "$SCRATCH"
printf 'fn main() {}\n' >"$REPO/src/main.rs"
printf 'fixture signal\n' >"$REPO/.loom/work/signals/s.md"
ln -s "$REPO/src" "$TMP/src-link"

FAILED=0
fail() {
    echo "FAIL: $1"
    FAILED=1
}

# run_hook <dir> <command> [VAR=value ...] - set CODE, OUT and ERR.
run_hook() {
    local dir="$1" cmd="$2" payload
    shift 2
    payload=$(jq -nc --arg c "$cmd" --arg d "$dir" '{tool_name:"Bash",tool_input:{command:$c},cwd:$d}')
    CODE=0
    (cd "$dir" && printf '%s' "$payload" | env -u LOOM_SCRATCH_DIR -u LOOM_WORKTREE_PATH \
        -u LOOM_MAIN_AGENT_PID -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_WORK_DIR \
        -u WORKTREE_ISOLATION_DEBUG HOME="$FIXTURE_HOME" TMPDIR="$TMP/tmpdir" \
        "$@" bash "$HOOK" >"$TMP/out" 2>"$TMP/err") || CODE=$?
    OUT=$(cat "$TMP/out")
    ERR=$(cat "$TMP/err")
}

allow() {
    run_hook "$@"
    if [[ $CODE -ne 0 || "$OUT" == *LOOM_HOOK_WARN* ]]; then
        fail "expected a silent allow from $1: $2 (exit $CODE) $OUT $ERR"
    fi
}

block() {
    local rule="$1" first
    shift
    run_hook "$@"
    first=${ERR%%$'\n'*}
    # The older banners start with a blank line.
    if [[ "$rule" != main-checkout ]]; then first=$(awk 'NF { print; exit }' <<<"$ERR"); fi
    if [[ $CODE -ne 2 ]]; then
        fail "expected a block from $1: $2 (exit $CODE) $OUT $ERR"
    elif [[ "$first" != "[worktree-isolation/$rule]"* ]]; then
        fail "block of $2 lacks the $rule tag: $first"
    fi
}

blocked=(
    "cd $REPO/src && cat main.rs"
    "cat $REPO/src/main.rs"
    "rg -n foo $REPO/src"
    "cargo test --manifest-path=$REPO/Cargo.toml"
    "X=$REPO/src/main.rs cat \"\$X\""
    "cat \"$REPO/src/main.rs\""
    "cat \"$REPO/src/main notes.rs\""
    "cat $TMP/src-link/main.rs"
    "ls $REPO"
)
for cmd in "${blocked[@]}"; do
    block main-checkout "$WT" "$cmd"
    allow "$WT" "$cmd" HOME="$REPO"
done
block main-checkout "$WT" 'cat ~/repo/src/main.rs' HOME="$TMP"
block main-checkout "$WT" "make -C$REPO/src test"
block main-checkout "$WT" "tar -C$REPO -xf x.tar"
block main-checkout "$WT" 'tar -C~/repo/src -xf x.tar' HOME="$TMP"

allow "$WT" "cat $REPO/.loom/work/signals/s.md"
allow "$WT" "cat $WT/src/x.rs"
allow "$WT" 'cat /etc/hostname'
allow "$WT" "ls $FIXTURE_HOME/.cargo/bin"
allow "$WT" "echo \"the brief mentions $REPO/src here\""
allow "$WT" 'git status'
allow "$WT" "cat $SCRATCH/x" LOOM_SCRATCH_DIR="$SCRATCH"
allow "$WT" 'cc -I/usr/include -c x.c'
allow "$WT" "make -C$WT/src test"
allow "$WT" "make -C$SCRATCH test" LOOM_SCRATCH_DIR="$SCRATCH"
allow "$WT" "cat $REPO-neighbor/src/x"
allow "$REPO" "cat $REPO/src/main.rs"

block path-traversal "$WT" "cat $REPO/src/../src/main.rs"
block cross-worktree "$WT" "cat $REPO/.worktrees/other-stage/src/x $REPO/src/main.rs"

if [[ $FAILED -ne 0 ]]; then exit 1; fi
echo PASS
