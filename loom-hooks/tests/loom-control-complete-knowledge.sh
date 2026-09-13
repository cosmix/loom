#!/usr/bin/env bash
# loom-control-complete.sh admission and binary choice (plan section 9): a
# confined Knowledge session (LOOM_SESSION_TYPE=knowledge plus
# LOOM_SCRATCH_DIR) is held to the pinned completion route; Merge,
# adjudication and legacy Knowledge sessions are not engaged; LOOM_BIN is
# preferred when it passes the trusted-binary checks and ignored when it lies
# inside the checkout the session can write.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
# Not under /tmp: the hook never trusts a binary there, and these cases run
# without the LOOM_CONTROL_TESTING override so LOOM_BIN takes the real path.
mkdir -p "$ROOT/loom/target"
TMP=$(mktemp -d "$ROOT/loom/target/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
TMP=$(cd "$TMP" && pwd -P)

WORK_DIR="$TMP/repo/.loom/work"
WORKTREE="$TMP/repo/.worktrees/build-api"
LOG="$TMP/broker.log"
mkdir -p "$TMP/bin" "$WORK_DIR" "$WORKTREE/bin" "$TMP/repo/bin" "$TMP/scratch/session-k"
for bin in "$TMP/bin/loom" "$TMP/repo/bin/loom" "$WORKTREE/bin/loom"; do
	printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >>"$BROKER_LOG"\n' >"$bin"
	chmod +x "$bin"
done

fail() {
	echo "FAIL: $*" >&2
	exit 1
}
pre() { jq -nc --arg c "$1" '{tool_name: "Bash", tool_input: {command: $c}}'; }
post() {
	jq -nc --arg c "$1" --arg o "$2" \
		'{tool_name: "Bash", tool_input: {command: $c}, tool_result: {output: $o, is_error: false}}'
}

# run_hook <payload> [VAR=value...] - no test override: the hook resolves its
# binary exactly as in production, from LOOM_BIN or the fixed locations.
run_hook() {
	local input=$1
	shift
	printf '%s' "$input" | env -i HOME="$TMP/home" PATH="$PATH" BROKER_LOG="$LOG" \
		LOOM_STAGE_ID=kstage LOOM_SESSION_ID=session-k LOOM_WORK_DIR="$WORK_DIR" \
		LOOM_BIN="$TMP/bin/loom" ${1+"$@"} bash "$HOOK"
}
broker_calls() {
	if [[ -e "$LOG" ]]; then wc -l <"$LOG" | tr -d ' '; else echo 0; fi
}

CONFINED=(LOOM_SESSION_TYPE=knowledge "LOOM_SCRATCH_DIR=$TMP/scratch/session-k")
PINNED="$TMP/bin/loom stage complete kstage"
MARKER='LOOM_CONTROL_VERIFICATION_PASSED stage=kstage session=session-k'

# 1. A confined Knowledge session is held to the pinned route, end to end.
if err=$(run_hook "$(pre 'loom stage complete kstage')" "${CONFINED[@]}" 2>&1); then
	fail "knowledge: an unpinned completion was not refused"
fi
[[ "$err" == *"retry with the pinned command: $PINNED"* ]] || fail "knowledge: wrong refusal: $err"
run_hook "$(pre "$PINNED")" "${CONFINED[@]}" >/dev/null || fail "knowledge: the pinned command was refused"
[[ "$(broker_calls)" == 0 ]] || fail "knowledge: PreToolUse reached the broker"
out=$(run_hook "$(post "$PINNED" "$MARKER")" "${CONFINED[@]}") || fail "knowledge: PostToolUse failed"
[[ "$(broker_calls)" == 1 ]] || fail "knowledge: the verified completion did not reach the broker once"
rg -qx 'stage complete kstage --session session-k' "$LOG" || fail "knowledge: broker argv: $(<"$LOG")"
[[ "$out" == *"accepted by the daemon"* ]] || fail "knowledge: no confirmation: $out"
rm -f "$LOG"

# 2. Merge, adjudication and legacy (unconfined) Knowledge sessions stay out.
not_engaged() {
	local label=$1
	shift
	run_hook "$(pre 'loom stage complete kstage')" "$@" >/dev/null 2>&1 || fail "$label: an unpinned completion was blocked"
	run_hook "$(post "$PINNED" "$MARKER")" "$@" >/dev/null 2>&1 || fail "$label: PostToolUse failed"
	[[ "$(broker_calls)" == 0 ]] || fail "$label: reached the broker"
}
not_engaged merge LOOM_SESSION_TYPE=merge "LOOM_SCRATCH_DIR=$TMP/scratch/session-k"
not_engaged adjudication LOOM_SESSION_TYPE=adjudication "LOOM_SCRATCH_DIR=$TMP/scratch/session-k"
not_engaged "legacy knowledge" LOOM_SESSION_TYPE=knowledge

# 3. A LOOM_BIN inside the repository a Knowledge session writes is not trusted.
err=$(run_hook "$(pre 'loom stage complete kstage')" "${CONFINED[@]}" LOOM_BIN="$TMP/repo/bin/loom" 2>&1) &&
	fail "knowledge: a refusal was expected with an untrusted LOOM_BIN"
[[ "$err" != *"$TMP/repo/bin/loom"* ]] || fail "knowledge: a LOOM_BIN inside the repository was trusted: $err"

# 4. A stage worktree session prefers LOOM_BIN, and ignores one inside the worktree.
err=$(run_hook "$(pre 'loom stage complete kstage')" LOOM_WORKTREE_PATH="$WORKTREE" 2>&1) &&
	fail "stage: an unpinned completion was not refused"
[[ "$err" == *"retry with the pinned command: $PINNED"* ]] || fail "stage: LOOM_BIN was not preferred: $err"
err=$(run_hook "$(pre 'loom stage complete kstage')" LOOM_WORKTREE_PATH="$WORKTREE" LOOM_BIN="$WORKTREE/bin/loom" 2>&1) &&
	fail "stage: a refusal was expected with an untrusted LOOM_BIN"
[[ "$err" != *"$WORKTREE/bin/loom"* ]] || fail "stage: a LOOM_BIN inside the worktree was trusted: $err"

echo "loom-control-complete knowledge admission: ok"
