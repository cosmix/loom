#!/usr/bin/env bash
# common-block-ledger.sh - loom_record_block (_common.sh) appends one JSON line
# per guard block to $LOOM_WORK_DIR/heartbeat/<stage>.blocks.jsonl, so false
# positives can be counted per guard and rule. It is diagnostics running just
# before a guard's exit 2, so it must never print, never fail the caller, never
# hang on a planted FIFO, never follow a symlink, stop at 4 MiB, and keep the
# subject valid UTF-8 after cutting it to 300 bytes.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOKS="$(cd "$SCRIPT_DIR/.." && pwd)"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-blockledger.XXXXXX")
BSD_DIR=""
trap 'rm -rf "$TMP" ${BSD_DIR:+"$BSD_DIR"}' EXIT
PATH_PREFIX="$PATH"
if [[ "${LOOM_HOOK_TEST_BSD:-}" == "1" ]]; then
    source "$SCRIPT_DIR/_bsd_path.sh"
    BSD_DIR=$(bsd_shim_dir)
    PATH_PREFIX="$BSD_DIR:$PATH"
fi
mkdir -p "$TMP/tmpdir"
chmod 700 "$TMP/tmpdir"

FAILED=0
fail() {
    echo "FAIL: $1"
    FAILED=1
}

DRIVER="$TMP/driver.sh"
cat >"$DRIVER" <<'EOF'
set -euo pipefail
source "$1/_common.sh"
shift
loom_record_block "$@"
EOF

# record <work-dir> [VAR=value ...] -- <guard> <rule> <tool> <subject> - call
# loom_record_block under a 5 s watchdog with LOOM_STAGE_ID=s and
# LOOM_SESSION_ID=x unless overridden; fail on a non-zero exit, a timeout, or
# any output.
record() {
    local work="$1" out err code pid watchdog
    shift
    local -a unsets=(-u LOOM_SCRATCH_DIR -u LOOM_CODEX_FORWARD -u INPUT_JSON)
    local -a vars=()
    local stage=LOOM_STAGE_ID=s session=LOOM_SESSION_ID=x
    while [[ "$1" != "--" ]]; do
        case "$1" in
        -u)
            unsets+=(-u "$2")
            [[ "$2" != LOOM_STAGE_ID ]] || stage=""
            [[ "$2" != LOOM_SESSION_ID ]] || session=""
            shift 2
            continue
            ;;
        esac
        vars+=("$1")
        shift
    done
    [[ -z "$stage" ]] || vars=("$stage" ${vars[@]+"${vars[@]}"})
    [[ -z "$session" ]] || vars=("$session" ${vars[@]+"${vars[@]}"})
    shift
    out="$TMP/out"
    err="$TMP/err"
    env "${unsets[@]}" TMPDIR="$TMP/tmpdir" PATH="$PATH_PREFIX" LOOM_WORK_DIR="$work" \
        ${vars[@]+"${vars[@]}"} bash "$DRIVER" "$HOOKS" "$@" >"$out" 2>"$err" &
    pid=$!
    (sleep 5 && kill -9 "$pid" 2>/dev/null) &
    watchdog=$!
    code=0
    wait "$pid" || code=$?
    kill "$watchdog" 2>/dev/null || true
    wait "$watchdog" 2>/dev/null || true
    [[ "$code" == 0 ]] || fail "$*: exit $code (137 means the 5 s watchdog fired)"
    [[ ! -s "$out" && ! -s "$err" ]] || fail "$*: printed: $(cat "$out" "$err")"
}

bytes_of() {
    wc -c <"$1" | tr -d '[:space:]'
}

# is_byte_prefix <short-file> <long-file> - the bytes of <short-file> start
# <long-file>. Neither holds a newline, so command substitution keeps them.
is_byte_prefix() {
    local LC_ALL=C
    local short long
    short=$(cat "$1")
    long=$(cat "$2")
    [[ "${long:0:${#short}}" == "$short" ]]
}

mode_of() {
    stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1"
}

# --- one line with every field ------------------------------------------------
W="$TMP/work"
mkdir -p "$W"
LEDGER="$W/heartbeat/s.blocks.jsonl"
record "$W" -- worktree-isolation path-traversal Bash "cat ../../x"
if [[ ! -f "$LEDGER" ]]; then
    fail "no ledger at $LEDGER"
else
    expect='{"stage":"s","session":"x","role":"main","guard":"worktree-isolation","rule":"path-traversal","tool":"Bash","subject":"cat ../../x"}'
    got=$(jq -c 'del(.ts)' "$LEDGER")
    [[ "$got" == "$expect" ]] || fail "first line: $got"
    jq -e '.ts | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")' "$LEDGER" >/dev/null ||
        fail "ts field: $(cat "$LEDGER")"
    [[ "$(wc -l <"$LEDGER" | tr -d '[:space:]')" == 1 ]] || fail "expected one line"
    [[ "$(mode_of "$LEDGER")" == 600 ]] || fail "ledger mode $(mode_of "$LEDGER"), want 600"
    [[ "$(mode_of "$W/heartbeat")" == 700 ]] || fail "heartbeat mode $(mode_of "$W/heartbeat"), want 700"
fi

record "$W" INPUT_JSON='{"agent_type":"loom-software-engineer"}' -- \
    subagent-verify-guard subagent-verify Bash "cargo test"
role=$(tail -n 1 "$LEDGER" | jq -r '.role')
[[ "$role" == subagent ]] || fail "agent_type payload recorded role '$role'"
[[ "$(wc -l <"$LEDGER" | tr -d '[:space:]')" == 2 ]] || fail "expected a second line"

# --- subjects cut at 300 bytes stay valid UTF-8 -----------------------------
pad=""
while ((${#pad} < 302)); do pad="${pad}a"; done
for char in $'\xc3\xa9' $'\xe2\x82\xac' $'\xf0\x9f\x98\x80'; do
    for ((p = 294; p <= 301; p++)); do
        subject="${pad:0:p}${char}${char}${char}"
        record "$W" -- g r Bash "$subject"
        printf '%s' "$subject" >"$TMP/subject.in"
        tail -n 1 "$LEDGER" | jq -j '.subject' >"$TMP/subject.out"
        n=$(bytes_of "$TMP/subject.out")
        ((n <= 300 && n >= 297)) || fail "cut at prefix $p: $n bytes"
        iconv -f UTF-8 -t UTF-8 "$TMP/subject.out" >/dev/null 2>&1 || fail "cut at prefix $p: invalid UTF-8"
        is_byte_prefix "$TMP/subject.out" "$TMP/subject.in" ||
            fail "cut at prefix $p: not a prefix of the subject"
    done
done
short=$'caf\xc3\xa9'
record "$W" -- g r Bash "$short"
[[ "$(tail -n 1 "$LEDGER" | jq -r '.subject')" == "$short" ]] || fail "short subject altered"

# --- refusals: nothing written, exit 0, nothing printed ---------------------
lines_before=$(wc -l <"$LEDGER" | tr -d '[:space:]')
record "$W" -u LOOM_STAGE_ID -- g r Bash "no stage"
record "$W" LOOM_STAGE_ID=../x -- g r Bash "traversal stage"
record "$W" -u LOOM_SESSION_ID -- g r Bash "no session"
record "$W" LOOM_CODEX_FORWARD=1 -- g r Bash "codex lane"
record "work-relative" -- g r Bash "relative work dir"
[[ "$(wc -l <"$LEDGER" | tr -d '[:space:]')" == "$lines_before" ]] || fail "a refused call appended"
[[ ! -e "$TMP/x.blocks.jsonl" && ! -e "$TMP/work/x.blocks.jsonl" ]] || fail "traversal stage wrote outside"

W2="$TMP/work-symlinked-heartbeat"
mkdir -p "$W2" "$TMP/elsewhere"
ln -s "$TMP/elsewhere" "$W2/heartbeat"
record "$W2" -- g r Bash "symlinked heartbeat"
[[ -z "$(ls -A "$TMP/elsewhere")" ]] || fail "wrote through a symlinked heartbeat dir"

W3="$TMP/work-fifo"
mkdir -p -m 700 "$W3/heartbeat"
mkfifo "$W3/heartbeat/s.blocks.jsonl"
record "$W3" -- g r Bash "fifo ledger"
[[ -p "$W3/heartbeat/s.blocks.jsonl" ]] || fail "fifo replaced"

W4="$TMP/work-symlinked-ledger"
mkdir -p -m 700 "$W4/heartbeat"
printf 'keep\n' >"$TMP/target"
cp "$TMP/target" "$TMP/target.orig"
ln -s "$TMP/target" "$W4/heartbeat/s.blocks.jsonl"
record "$W4" -- g r Bash "symlinked ledger"
cmp -s "$TMP/target" "$TMP/target.orig" || fail "wrote through a symlinked ledger"

W5="$TMP/work-oversized"
mkdir -p -m 700 "$W5/heartbeat"
dd if=/dev/zero of="$W5/heartbeat/s.blocks.jsonl" bs=1 count=0 seek=4194305 2>/dev/null
record "$W5" -- g r Bash "oversized ledger"
[[ "$(bytes_of "$W5/heartbeat/s.blocks.jsonl")" == 4194305 ]] || fail "appended to a ledger over 4 MiB"

if ((FAILED)); then
    exit 1
fi
echo "PASS: common-block-ledger"
