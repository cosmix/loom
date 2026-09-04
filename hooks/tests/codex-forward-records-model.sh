#!/usr/bin/env bash
set -euo pipefail

WRAPPER="$(cd "$(dirname "$0")/.." && pwd)/codex-forward.sh"
if ! TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX") || [ -z "$TMP" ]; then
	printf '%s\n' 'FAIL: mktemp failed to create a scratch directory'
	exit 1
fi
trap 'rm -rf "$TMP"' EXIT

HOME_DIR="$TMP/home"
BIN_DIR="$TMP/bin"
STDOUT="$TMP/stdout"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
mkdir -p "$BIN_DIR" "$COMPANION_DIR"
printf '%s\n' '// fixture' >"$COMPANION_DIR/codex-companion.mjs"

printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$BIN_DIR/node"
chmod +x "$BIN_DIR/node"

# Exercise the companion lane regardless of whether the harness running this
# test is itself inside a sandbox that already refuses a nested Seatbelt
# profile.
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$BIN_DIR/sandbox-exec"
chmod +x "$BIN_DIR/sandbox-exec"

WORK_DIR="$TMP/work"
LEDGER="$WORK_DIR/subagents/my-stage/codex.jsonl"

# Case 1: a valid stage id records exactly one row with the expected fields.
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
	bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"

if [[ ! -f "$LEDGER" ]]; then
	printf '%s\n' "FAIL: ledger file was not created at $LEDGER"
	exit 1
fi
if [[ $(wc -l <"$LEDGER") -ne 1 ]]; then
	printf '%s\n' "FAIL: expected 1 ledger line, got $(wc -l <"$LEDGER")"
	exit 1
fi

model=$(jq -r '.model' "$LEDGER")
effort=$(jq -r '.effort' "$LEDGER")
stage_id=$(jq -r '.stage_id' "$LEDGER")
session_id=$(jq -r '.session_id' "$LEDGER")
ts=$(jq -r '.ts' "$LEDGER")

if [[ "$model" != 'gpt-5.6-terra' ]]; then
	printf '%s\n' "FAIL: expected model gpt-5.6-terra, got $model"
	exit 1
fi
if [[ "$effort" != 'xhigh' ]]; then
	printf '%s\n' "FAIL: expected effort xhigh, got $effort"
	exit 1
fi
if [[ "$stage_id" != 'my-stage' ]]; then
	printf '%s\n' "FAIL: expected stage_id my-stage, got $stage_id"
	exit 1
fi
if [[ "$session_id" != 'session-abc' ]]; then
	printf '%s\n' "FAIL: expected session_id session-abc, got $session_id"
	exit 1
fi
if [[ -z "$ts" || "$ts" == 'null' ]]; then
	printf '%s\n' "FAIL: expected a non-empty ts, got '$ts'"
	exit 1
fi

# Case 2: a second run appends rather than truncating.
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
	bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"

if [[ $(wc -l <"$LEDGER") -ne 2 ]]; then
	printf '%s\n' "FAIL: expected 2 ledger lines after second run, got $(wc -l <"$LEDGER")"
	exit 1
fi

# Case 3: an unsafe stage id must not create any ledger file or directory.
# `set -e` would abort the whole script the instant the wrapper exited
# nonzero, so `|| status=$?` is required to make the "expected 0" check
# below reachable at all.
status=0
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="bad/stage" LOOM_SESSION_ID="session-abc" \
	bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$STDOUT" || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: wrapper with unsafe stage id exited $status, expected 0"
	exit 1
fi
if [[ -e "$WORK_DIR/subagents/bad" || -e "$WORK_DIR/subagents/bad/stage" ]]; then
	printf '%s\n' 'FAIL: unsafe stage id left a ledger artifact under subagents/'
	exit 1
fi

# Case 4: an empty LOOM_WORK_DIR must not create a "subagents" directory,
# whether the wrapper resolved it as an absolute path or (were the empty-string
# guard ever weakened) relative to the wrapper's own working directory. Run
# from a dedicated cwd so that second possibility has somewhere to be checked.
CASE4_CWD="$TMP/case4-cwd"
mkdir -p "$CASE4_CWD"
status=0
(
	cd "$CASE4_CWD"
	HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR= \
		LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
		bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"
) || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: wrapper with empty LOOM_WORK_DIR exited $status, expected 0"
	exit 1
fi
if [[ -e "$CASE4_CWD/subagents" ]]; then
	printf '%s\n' 'FAIL: empty LOOM_WORK_DIR created a subagents directory relative to cwd'
	exit 1
fi

# Case 5: LOOM_STAGE_ID=".." must not escape subagents/<stage_id> - it must
# neither append to work_dir/codex.jsonl (subagents/.. resolves to work_dir
# itself) nor chmod the work dir via the `mkdir -p -m 700`/`chmod 700` calls
# that target that resolved directory.
DOTDOT_WORK_DIR="$TMP/work-dotdot"
mkdir -m 750 "$DOTDOT_WORK_DIR"
mode_before=$(stat -c '%a' "$DOTDOT_WORK_DIR" 2>/dev/null || stat -f '%Lp' "$DOTDOT_WORK_DIR")

status=0
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" LOOM_WORK_DIR="$DOTDOT_WORK_DIR" \
	LOOM_STAGE_ID=".." LOOM_SESSION_ID="session-abc" \
	bash "$WRAPPER" task hello --model gpt-5.6-terra --effort xhigh --write >"$STDOUT" || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: wrapper with LOOM_STAGE_ID=.. exited $status, expected 0"
	exit 1
fi
if [[ -e "$DOTDOT_WORK_DIR/codex.jsonl" ]]; then
	printf '%s\n' 'FAIL: LOOM_STAGE_ID=.. appended to work_dir/codex.jsonl'
	exit 1
fi
if [[ -e "$DOTDOT_WORK_DIR/subagents" ]]; then
	printf '%s\n' 'FAIL: LOOM_STAGE_ID=.. created a subagents directory'
	exit 1
fi
mode_after=$(stat -c '%a' "$DOTDOT_WORK_DIR" 2>/dev/null || stat -f '%Lp' "$DOTDOT_WORK_DIR")
if [[ "$mode_after" != "$mode_before" ]]; then
	printf '%s\n' "FAIL: LOOM_STAGE_ID=.. changed the work dir's mode from $mode_before to $mode_after"
	exit 1
fi

# Case 6: the stdout trailer is unaffected by ledger recording.
rg -qF 'mode: companion' "$STDOUT"

printf '%s\n' 'PASS'
