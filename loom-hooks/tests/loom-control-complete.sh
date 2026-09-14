#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
WORKTREE="$TMP/repo/.worktrees/build-api"
MAIN_REPO="$TMP/repo"
LOG="$TMP/broker.log"
mkdir -p "$TMP/bin" "$WORKTREE" "$TMP/home"

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
{
	printf 'CALL\nARGV=%s\nSTATUS=%s\nSTDIN_BEGIN\n' "$*" "${LOOM_CONTROL_TOOL_STATUS:-missing}"
	cat
	printf '\nSTDIN_END\n'
} >>"$BROKER_LOG"
printf '%s\n' "${FAKE_BROKER_OUTPUT:-LOOM_CONTROL_OUTCOME accepted}"
exit "${FAKE_BROKER_RC:-0}"
SH
chmod +x "$TMP/bin/loom"

PINNED="$TMP/bin/loom stage complete build-api"
HOOK_RC=0
HOOK_OUTPUT=""

run_payload() {
	local payload=$1
	set +e
	HOOK_OUTPUT=$(printf '%s' "$payload" |
		env PATH="$TMP/bin:/usr/bin:/bin" HOME="${TEST_HOME_OVERRIDE:-$TMP/home}" \
			BROKER_LOG="$LOG" FAKE_BROKER_OUTPUT="${FAKE_BROKER_OUTPUT:-LOOM_CONTROL_OUTCOME accepted}" \
			FAKE_BROKER_RC="${FAKE_BROKER_RC:-0}" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="${LOOM_CONTROL_TEST_BIN_OVERRIDE:-$TMP/bin/loom}" \
			LOOM_STAGE_ID="${TEST_STAGE_ID:-build-api}" LOOM_SESSION_ID="${TEST_SESSION_ID:-session-123}" \
			LOOM_SESSION_TYPE="${TEST_SESSION_TYPE:-}" LOOM_SCRATCH_DIR="${TEST_SCRATCH_DIR:-}" \
			LOOM_WORKTREE_PATH="${TEST_WORKTREE_OVERRIDE:-$WORKTREE}" bash "$HOOK" 2>&1)
	HOOK_RC=$?
	set -e
}

run_pre() {
	local payload
	payload=$(jq -n --arg command "$1" '{tool_name:"Bash",tool_input:{command:$command}}')
	run_payload "$payload"
}

run_post() {
	local command=$1 output=$2 is_error=$3 path=${4:-} payload
	if [[ -n "$path" ]]; then
		payload=$(jq -n --arg command "$command" --arg output "$output" --arg path "$path" \
			--argjson is_error "$is_error" \
			'{tool_name:"Bash",tool_input:{command:$command},tool_response:{stdout:$output,is_error:$is_error,persistedOutputPath:$path}}')
	else
		payload=$(jq -n --arg command "$command" --arg output "$output" --argjson is_error "$is_error" \
			'{tool_name:"Bash",tool_input:{command:$command},tool_result:{output:$output,is_error:$is_error}}')
	fi
	run_payload "$payload"
}

assert_rc() {
	[[ "$HOOK_RC" == "$1" ]] || { echo "expected rc $1, got $HOOK_RC: $HOOK_OUTPUT" >&2; exit 1; }
}

assert_output() {
	[[ "$HOOK_OUTPUT" == *"$1"* ]] || { echo "missing output '$1': $HOOK_OUTPUT" >&2; exit 1; }
}

assert_log() {
	[[ -f "$LOG" ]] && rg -Fq -- "$1" "$LOG" || { echo "missing broker log '$1'" >&2; exit 1; }
}

reset_log() { rm -f "$LOG"; }
call_count() { [[ -f "$LOG" ]] && rg -c '^CALL$' "$LOG" || printf '0\n'; }

assert_no_broker() {
	[[ ! -e "$LOG" ]] || { echo "unexpected broker call: $(<"$LOG")" >&2; exit 1; }
}

expect_pre_blocked() {
	local command=$1
	reset_log
	run_pre "$command"
	assert_rc 2
	assert_output 'LOOM_CONTROL_ERROR:'
	assert_no_broker
}

expect_post_blocked() {
	local command=$1
	reset_log
	run_post "$command" "$EVIDENCE" false
	assert_rc 2
	assert_output 'LOOM_CONTROL_ERROR:'
	assert_no_broker
}

run_pre "$PINNED"
assert_rc 0
[[ ! -e "$LOG" ]] || { echo "PreToolUse called broker" >&2; exit 1; }

run_post 'loom stage complete build-api' 'ignored' false
assert_rc 2
assert_output 'completion result was not produced by the exact pinned command'
[[ ! -e "$LOG" ]] || { echo "mismatched PostToolUse called broker" >&2; exit 1; }

EVIDENCE=$'before\nLOOM_CONTROL_EVIDENCE_V1 stage=build-api session=session-123 nonce=n\ncheck output\nLOOM_CONTROL_EVIDENCE_EOF\nafter'

# Old fail-closed regressions plus the detector matrix: every invalid attempt
# is rejected in both hook phases and never reaches the stdin broker.
symlink_bin="$TMP/bin/loom-link"
ln -s "$TMP/bin/loom" "$symlink_bin"
INVALID_COMMANDS=(
	'loom stage complete build-api'
	"loom() { \"$TMP/bin/loom\" \"\$@\"; }; loom stage complete build-api"
	"alias loom='$TMP/bin/loom'; loom stage complete build-api"
	"$symlink_bin stage complete build-api"
	"env -u RUSTC_WRAPPER $PINNED"
	"NAME=value $PINNED"
	"$PINNED | cat"
	"$PINNED ; true"
	"$PINNED && true"
	"$PINNED > out"
	"$PINNED &"
	"$PINNED"$'\n'"true"
	"bash -c '$PINNED'"
	"sh -lc '$PINNED'"
	'$LOOM_BIN stage complete build-api'
	"$TMP/bin/loom stage \"complete\" build-api"
	"$TMP/bin/loom stage com\\plete build-api"
	"$TMP/bin/loom stage compl\"\"ete build-api"
	"$TMP/bin/loom stage com\\"$'\n'"plete build-api"
	"$PINNED --force"
	"$PINNED unexpected"
	'loom stage complete build-api "'
)
for invalid_command in "${INVALID_COMMANDS[@]}"; do
	expect_pre_blocked "$invalid_command"
	expect_post_blocked "$invalid_command"
done

reset_log
LOOM_CONTROL_TEST_BIN_OVERRIDE="$symlink_bin" run_pre "$symlink_bin stage complete build-api"
assert_rc 2
assert_output 'LOOM_CONTROL_ERROR:'
assert_no_broker

for session_type in knowledge merge base-conflict; do
	reset_log
	TEST_SESSION_TYPE="$session_type" TEST_WORKTREE_OVERRIDE="$MAIN_REPO" \
		run_pre 'loom stage complete build-api'
	assert_rc 0
	[[ -z "$HOOK_OUTPUT" ]] || { echo "$session_type main-repo session emitted a pin" >&2; exit 1; }
	assert_no_broker
done

nested_main="$TMP/outer/.worktrees/container/repo"
real_nested_worktree="$nested_main/.worktrees/build-api"
mkdir -p "$real_nested_worktree"
reset_log
TEST_WORKTREE_OVERRIDE="$nested_main" run_pre 'loom stage complete build-api'
assert_rc 0
[[ -z "$HOOK_OUTPUT" ]] || { echo "nested main-repo path emitted a pin" >&2; exit 1; }
assert_no_broker
reset_log
TEST_WORKTREE_OVERRIDE="$real_nested_worktree" run_pre 'loom stage complete build-api'
assert_rc 2
assert_output 'retry with the pinned command'
assert_no_broker

run_post "$PINNED" "$EVIDENCE" false
assert_rc 0
assert_output 'completion was accepted by the daemon'
[[ "$(call_count)" == 1 ]] || { echo "broker was not called once" >&2; exit 1; }
assert_log 'ARGV=stage complete build-api --session session-123'
assert_log 'STATUS=ok'
assert_log 'LOOM_CONTROL_EVIDENCE_V1 stage=build-api session=session-123 nonce=n'
assert_log 'LOOM_CONTROL_EVIDENCE_EOF'

reset_log
FAKE_BROKER_OUTPUT='LOOM_CONTROL_OUTCOME accepted' run_post "$PINNED" "$EVIDENCE" true
assert_rc 2
assert_output 'invalid acceptance outcome'
assert_log 'STATUS=failed'

reset_log
FAKE_BROKER_OUTPUT='LOOM_CONTROL_OUTCOME tool_failed_recorded' run_post "$PINNED" 'failed check details' true
assert_rc 2
assert_output 'diagnostic evidence was recorded; fix the failing check'
assert_log 'STATUS=failed'
assert_log 'failed check details'

while IFS='|' read -r outcome rc phrase; do
	reset_log
	FAKE_BROKER_OUTPUT="LOOM_CONTROL_OUTCOME $outcome" run_post "$PINNED" "$EVIDENCE" false
	assert_rc "$rc"
	assert_output "$phrase"
done <<'CASES'
accepted|0|completion was accepted by the daemon
accepted_reconciled|0|reconciled from durable daemon state after a lost acknowledgement
tool_failed_recorded|2|completion command failed; diagnostic evidence was recorded
evidence_missing_recorded|2|no valid verification evidence record; a diagnostic was recorded
evidence_record_failed disk-full|2|verification evidence could not be recorded durably: disk-full
daemon_rejected stale-session|2|daemon rejected the verified completion: stale-session
verified_pending_ack|2|completion is pending; do not rerun blindly, check loom status
uncertain timeout|2|completion state is uncertain: timeout
CASES

reset_log
FAKE_BROKER_OUTPUT=$'LOOM_CONTROL_OUTCOME accepted\nnoise\nLOOM_CONTROL_OUTCOME daemon_rejected final-decision' \
	run_post "$PINNED" "$EVIDENCE" false
assert_rc 2
assert_output 'daemon rejected the verified completion: final-decision'

reset_log
FAKE_BROKER_OUTPUT='broker crashed without a record' FAKE_BROKER_RC=9 run_post "$PINNED" "$EVIDENCE" false
assert_rc 2
assert_output 'daemon completion broker failed: broker crashed without a record'

reset_log
PERSISTED_HOME="$TMP/persisted-home"
PERSISTED_DIR="$PERSISTED_HOME/.claude/projects/proj/session/tool-results"
PERSISTED_FILE="$PERSISTED_DIR/out..1.txt"
mkdir -p "$PERSISTED_DIR"
printf '%s\n' "$EVIDENCE" >"$PERSISTED_FILE"
TEST_HOME_OVERRIDE="$PERSISTED_HOME" run_post "$PINNED" 'truncated preview' false "$PERSISTED_FILE"
assert_rc 0
assert_log 'LOOM_CONTROL_EVIDENCE_V1'
if rg -Fq 'truncated preview' "$LOG"; then echo "preview was used instead of persisted output" >&2; exit 1; fi

dotdot_escape_path() {
	local base=$1 target=$2 rest=${1#/} ups=""
	while [[ -n "$rest" ]]; do
		ups+="../"
		if [[ "$rest" == */* ]]; then rest=${rest#*/}; else rest=""; fi
	done
	printf '%s/%s%s\n' "$base" "$ups" "${target#/}"
}

for kind in outside symlink dotdot; do
	reset_log
	home="$TMP/home-$kind"
	trusted_dir="$home/.claude/projects/proj/session/tool-results"
	mkdir -p "$trusted_dir" "$TMP/outside/tool-results"
	secret="$TMP/outside/tool-results/$kind.txt"
	printf 'SECRET_%s\n' "$kind" >"$secret"
	case "$kind" in
	outside) path=$secret ;;
	symlink) path="$trusted_dir/link.txt"; ln -s "$secret" "$path" ;;
	dotdot) path=$(dotdot_escape_path "$home/.claude/projects" "$secret") ;;
	esac
	TEST_HOME_OVERRIDE="$home" \
		run_post "$PINNED" 'VISIBLE_PREVIEW' false "$path"
	assert_rc 2
	assert_output 'persisted tool output path is not trusted'
	assert_no_broker
done

production_hook="$TMP/installed/loom-control-complete.sh"
mkdir -p "$(dirname "$production_hook")"
cp "$HOOK" "$production_hook"
cp "$ROOT/loom-hooks/_common.sh" "$(dirname "$production_hook")/_common.sh"
set +e
production_output=$(jq -n --arg command "$PINNED" '{tool_name:"Bash",tool_input:{command:$command}}' |
	env PATH="/usr/bin:/bin" HOME="$TMP/empty-home" LOOM_CONTROL_TESTING=1 \
		LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api LOOM_SESSION_ID=session-123 \
		LOOM_WORKTREE_PATH="$WORKTREE" bash "$production_hook" 2>&1)
rc=$?
set -e
[[ "$rc" == 2 ]] || { echo "test binary override escaped repository harness" >&2; exit 1; }
[[ "$production_output" == *LOOM_CONTROL_ERROR:* ]] || { echo "production rejection lacked a reason" >&2; exit 1; }
assert_no_broker

echo PASS
