#!/usr/bin/env bash
# containment-cwd-resolve.sh - the shared resolver in _containment.sh:
# loom_track_cwd (the working directory each token of a command runs in),
# loom_normalize_path (physical normalization, so `<wt>/.loom/work/../x` is
# `<repo>/.loom/x` through the real work symlink) and loom_path_class. Guards
# that judged relative paths against the hook's own directory blocked
# `cd web/src && sed ../../doc/x.md` and allowed the reverse; these cases pin
# the rules every guard now shares, including every case where the directory
# must stay unknown (subshells, pipes, a cd that may not have run).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOKS="$(cd "$SCRIPT_DIR/.." && pwd)"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-cwdresolve.XXXXXX")
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
mkdir -p "$REPO/.loom/work" "$REPO/.git" "$REPO/.worktrees/stage-x/src" \
    "$WT/web/src" "$WT/a/b" "$WT/doc" "$WT/.loom" "$TMP/tmpdir" "$TMP/home" "$S"
chmod 700 "$S" "$TMP/tmpdir"
ln -s ../../../.loom/work "$WT/.loom/work"
ln -s "$S" "$TMP/scratch-link"

FAILED=0
fail() {
    echo "FAIL: $1"
    FAILED=1
}

DRIVER="$TMP/driver.sh"
cat >"$DRIVER" <<'EOF'
set -euo pipefail
source "$1/_common.sh"
mode="$2"
shift 2
case "$mode" in
cwds)
    loom_tokenize_command "$2" || { echo TOKFAIL; exit 0; }
    loom_track_cwd "$1"
    for ((i = 0; i < ${#LOOM_TOKENS[@]}; i++)); do
        printf '%s\t%s\n' "${LOOM_TOKENS[$i]}" "${LOOM_TOKEN_CWD[$i]-UNSET}"
    done
    echo "EXPAND=$LOOM_SCRATCH_EXPAND"
    ;;
norm)
    if [[ -n "${EXPAND:-}" ]]; then LOOM_SCRATCH_EXPAND="$EXPAND"; fi
    if out=$(loom_normalize_path "$1" "$2"); then printf '%s\n' "$out"; else echo "RC=1"; fi
    ;;
class)
    loom_path_class "$1"
    echo
    ;;
esac
EOF

# drv <dir> [VAR=value ...] -- <mode> <args...> - run the driver from <dir>
# with the scrubbed hook environment (common.md test-environment rule).
drv() {
    local dir="$1"
    shift
    local -a vars=()
    while [[ "$1" != "--" ]]; do
        vars+=("$1")
        shift
    done
    shift
    (cd "$dir" && env -u LOOM_SCRATCH_DIR -u LOOM_WORKTREE_PATH -u LOOM_MAIN_AGENT_PID \
        -u LOOM_SCRATCH_EXPAND TMPDIR="$TMP/tmpdir" HOME="$TMP/home" PATH="$PATH_PREFIX" \
        ${vars[@]+"${vars[@]}"} bash "$DRIVER" "$HOOKS" "$@" 2>&1)
}

# cwd_of <label> <command> <token> <expected-cwd> - the cwd loom_track_cwd
# gives the LAST token equal to <token>, run from $WT with .cwd = $WT.
cwd_of() {
    local label="$1" cmd="$2" tok="$3" want="$4" out got="" line
    out=$(drv "$WT" -- cwds "$WT" "$cmd")
    while IFS= read -r line; do
        if [[ "${line%%$'\t'*}" == "$tok" ]]; then got="${line#*$'\t'}"; fi
    done <<<"$out"
    if [[ "$got" != "$want" ]]; then
        fail "$label: token '$tok' cwd '$got', want '$want' (output: $out)"
    fi
}

# expect_eq <label> <got> <want>
expect_eq() {
    [[ "$2" == "$3" ]] || fail "$1: got '$2', want '$3'"
}

# --- loom_track_cwd ---------------------------------------------------------
cwd_of "cd && relative" "cd $WT/web/src && sed -n 1p ../../doc/x.md" "../../doc/x.md" "$WT/web/src"
expect_eq "cd && relative normalizes" \
    "$(drv "$WT" -- norm "$WT/web/src" ../../doc/x.md)" "$WT/doc/x.md"
cwd_of "cd unexpandable target" 'cd "$X" && cat a' "a" ""
cwd_of "cd piped" "cd web | cat ../x" "../x" ""
cwd_of "cd in a subshell" "(cd web && cat x)" "x" ""
cwd_of "subshell taint" "(cd $WT/a/b && :) ; cat ../../x" "../../x" ""
cwd_of "substitution taint" 'true;$(cd /a && :);cat ../x' "../x" ""
cwd_of "sh -c taint" "bash -c \"cd $WT/a/b\"; cat ../../x" "../../x" ""
cwd_of "unverified cd stops at ;" "cd $WT/missing && :; cat ../../x" "../../x" ""
cwd_of "unverified cd holds through &&" "cd $WT/missing && :; cat ../../x" ":" "$WT/missing"
cwd_of "&& chain after an unverified cd" "mkdir -p $S/fx2 && cd $S/fx2 && git init -q" "git" "$S/fx2"
cwd_of "cd ; existing" "cd web; cat x" "x" "$WT/web"
cwd_of "cd ; missing" "cd nowhere; cat x" "x" ""
cwd_of "cd && existing" "cd web && cat x" "x" "$WT/web"
cwd_of "cd behind && then ;" "false && cd web; cat x" "x" ""
cwd_of "cd behind ||" "false || cd web && cat x" "x" ""
cwd_of "cd with an option" "cd -P web && cat x" "x" ""
cwd_of "cd -" "cd - && cat x" "x" ""
cwd_of "bare cd" "cd && cat x" "x" ""
cwd_of "pushd" "pushd web && cat x" "x" ""
cwd_of "cd inside a brace group" "{ cd web; } && cat x" "x" ""
cwd_of "segment before any cd" "cat y && cd web && cat x" "y" "$WT"
cwd_of "sentinel index" "cd web && cat x" "%%SEP%%" ""
cwd_of "unknown stays unknown" "cd web | cat q; cd $WT/doc && cat x" "x" ""
cwd_of "CDPATH mention makes cd unknown" "CDPATH=$WT/a; cd web && cat x" "x" ""
cwd_of "CDPATH after the cd" "cd web && cat x; echo CDPATH" "x" "$WT/web"
cwd_of "glob cd target" "cd we* && cat x" "x" ""
cwd_of "brace cd target" "cd {web,doc} && cat x" "x" ""
cwd_of "cd && newline" "cd web &&"$'\n'"cat x" "x" "$WT/web"
cwd_of "unverified cd && newline" "cd $WT/missing &&"$'\n'":; cat ../../x" ":" "$WT/missing"
out=$(drv "$WT/web" -- cwds "" "cat x")
expect_eq "start from pwd -P" "$(printf '%s' "$out" | sed -n 2p)" "x	$WT/web"
out=$(drv "$WT" -- cwds "$WT/.loom/work" "cat x")
expect_eq "start dir made physical" "$(printf '%s' "$out" | sed -n 2p)" "x	$REPO/.loom/work"

# --- LOOM_SCRATCH_EXPAND -----------------------------------------------------
expand_of() {
    drv "$WT" -- cwds "$WT" "$1" | sed -n 's/^EXPAND=//p'
}
expect_eq "expand: prefix assignment" \
    "$(expand_of 'LOOM_SCRATCH_DIR=/x git -C "$LOOM_SCRATCH_DIR" status')" 0
expect_eq "expand: export" \
    "$(expand_of 'export LOOM_SCRATCH_DIR=/x; git -C "$LOOM_SCRATCH_DIR" status')" 0
expect_eq "expand: defaulting assignment" \
    "$(expand_of ': "${LOOM_SCRATCH_DIR:=/x}"; git -C "$LOOM_SCRATCH_DIR" status')" 0
expect_eq "expand: plain read" \
    "$(expand_of 'git -C "$LOOM_SCRATCH_DIR/clone" status')" 1
# A `$` bash keeps literal: the token reads $LOOM_SCRATCH_DIR, the path does not.
for lit in "'\$LOOM_SCRATCH_DIR/c'" "\\\$LOOM_SCRATCH_DIR/c" "\"\\\$LOOM_SCRATCH_DIR/c\"" \
    "\$\\LOOM_SCRATCH_DIR/c" "\"\$\"LOOM_SCRATCH_DIR/c" "\$'\$LOOM_SCRATCH_DIR/c'"; do
    expect_eq "expand: literal \$ in $lit" "$(expand_of "git -C $lit status")" 0
done
# eval, source and . count only where they run as a command.
expect_eq "expand: . as an argument" \
    "$(expand_of 'git -C "$LOOM_SCRATCH_DIR/clone" grep -n foo -- .')" 1
expect_eq "expand: source as an argument" "$(expand_of 'git -C "$LOOM_SCRATCH_DIR/c" grep source')" 1
for cmd in '. ./x' 'source ./x' 'eval x' 'builtin . ./x' 'if . ./x; then :; fi' '>/dev/null . ./x' \
    'true | . ./x' '{ source ./x; }'; do
    expect_eq "expand: $cmd" "$(expand_of "$cmd && git -C \"\$LOOM_SCRATCH_DIR/c\" status")" 0
done

# --- loom_normalize_path -----------------------------------------------------
expect_eq "norm: / ../.." "$(drv "$WT" -- norm / ../..)" "/"
expect_eq "norm: through the work symlink" \
    "$(drv "$WT" -- norm "$WT" .loom/work/../x)" "$REPO/.loom/x"
expect_eq "norm: missing tail under the work symlink" \
    "$(drv "$WT" -- norm "$WT" .loom/work/new/file)" "$REPO/.loom/work/new/file"
expect_eq "norm: dot and empty components" "$(drv "$WT" -- norm "$WT" ./a//b/.)" "$WT/a/b"
expect_eq "norm: ~/x" "$(drv "$WT" -- norm "$WT" '~/x')" "$TMP/home/x"
expect_eq "norm: ~user" "$(drv "$WT" -- norm "$WT" '~user/x')" "RC=1"
expect_eq "norm: other variable" "$(drv "$WT" -- norm "$WT" '$X/y')" "RC=1"
expect_eq "norm: backtick" "$(drv "$WT" -- norm "$WT" 'a/`id`')" "RC=1"
expect_eq "norm: relative with empty base" "$(drv "$WT" -- norm "" a/b)" "RC=1"
expect_eq "norm: scratch expanded" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$S" EXPAND=1 -- norm "$WT" '$LOOM_SCRATCH_DIR/clone')" "$S/clone"
expect_eq "norm: braced scratch expanded" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$S" EXPAND=1 -- norm "$WT" '${LOOM_SCRATCH_DIR}/clone')" "$S/clone"
expect_eq "norm: scratch with EXPAND=0" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$S" EXPAND=0 -- norm "$WT" '$LOOM_SCRATCH_DIR/clone')" "RC=1"
expect_eq "norm: scratch with EXPAND unset" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$S" -- norm "$WT" '$LOOM_SCRATCH_DIR/clone')" "RC=1"
expect_eq "norm: symlinked scratch dir" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$TMP/scratch-link" EXPAND=1 -- norm "$WT" '$LOOM_SCRATCH_DIR/c')" "RC=1"
expect_eq "norm: scratch name prefix" \
    "$(drv "$WT" LOOM_SCRATCH_DIR="$S" EXPAND=1 -- norm "$WT" '$LOOM_SCRATCH_DIRX/c')" "RC=1"

# --- loom_path_class ---------------------------------------------------------
class_of() {
    drv "$WT" "${@:2}" -- class "$1"
}
expect_eq "class: main work dir" "$(class_of "$REPO/.loom/work/x")" "state"
work_x=$(drv "$WT" -- norm "$WT" .loom/work/x)
expect_eq "class: worktree work symlink" "$(class_of "$work_x")" "state"
expect_eq "class: git dir" "$(class_of "$REPO/.git/config")" "state"
expect_eq "class: sibling" "$(class_of "$REPO/.worktrees/stage-x/src")" "sibling"
expect_eq "class: sibling root" "$(class_of "$REPO/.worktrees/stage-x")" "sibling"
expect_eq "class: worktree" "$(class_of "$WT/web/src/a.ts")" "worktree"
expect_eq "class: worktree root" "$(class_of "$WT")" "worktree"
expect_eq "class: scratch" "$(class_of "$S/clone" LOOM_SCRATCH_DIR="$S")" "scratch"
expect_eq "class: TMPDIR is never scratch" "$(class_of "$TMP/tmpdir/x")" "outside"
expect_eq "class: main checkout file" "$(class_of "$REPO/README.md")" "outside"
expect_eq "class: symlinked scratch" \
    "$(class_of "$S/clone" LOOM_SCRATCH_DIR="$TMP/scratch-link")" "outside"
other=""
for candidate in /usr /etc /var; do
    if [[ -d "$candidate" && ! -L "$candidate" ]]; then
        owner=$(stat -c '%u' "$candidate" 2>/dev/null || stat -f '%u' "$candidate" 2>/dev/null || true)
        if [[ -n "$owner" && "$owner" != "$(id -u)" ]]; then
            other="$candidate"
            break
        fi
    fi
done
if [[ -n "$other" ]]; then
    expect_eq "class: scratch owned by another uid" \
        "$(class_of "$other/x" LOOM_SCRATCH_DIR="$other")" "outside"
else
    echo "SKIP: no directory owned by another uid for the scratch-owner case"
fi

if ((FAILED)); then
    exit 1
fi
echo "PASS: containment-cwd-resolve"
