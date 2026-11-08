#!/usr/bin/env bash
# loom-control-complete.sh rewrites the main agent's plain completion spelling
# (bare `loom`, optional `2>&1`, one `| tail`/`| head` line count) to the pinned
# command through updatedInput, allows the main agent's exact pinned command the
# same way, and still refuses, records and explains every other
# completion-shaped command.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-complete.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-control-complete-rewrite.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
TMP=$(cd "$TMP" && pwd -P)

REPO="$TMP/repo"
WORKTREE="$REPO/.worktrees/build-api"
WORK="$REPO/.loom/work"
LEDGER="$WORK/heartbeat/build-api.blocks.jsonl"
mkdir -p "$TMP/bin" "$TMP/home" "$TMP/tmpdir" "$WORKTREE" "$WORK/stages" "$WORK/heartbeat"
chmod 700 "$TMP/tmpdir"
mkdir -p "$WORKTREE/.loom"
ln -s ../../../.loom/work "$WORKTREE/.loom/work"
printf '%s\n' '---' 'id: build-api' 'session: session-123' '---' '# Stage' >"$WORK/stages/01-build-api.md"
# The hook only compares paths; the binary is never run.
printf '#!/usr/bin/env bash\nexit 0\n' >"$TMP/bin/loom"
chmod +x "$TMP/bin/loom"
PINNED="$TMP/bin/loom stage complete build-api"
RC=0
OUT=""
ERR=""

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# run_payload <payload> - Run the hook from the worktree; sets RC, OUT, ERR.
run_payload() {
	set +e
	printf '%s' "$1" | (cd "$WORKTREE" && env -u LOOM_SCRATCH_DIR TMPDIR="$TMP/tmpdir" \
		PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP/home" LOOM_CONTROL_TESTING=1 \
		LOOM_CONTROL_TEST_BIN="$TMP/bin/loom" LOOM_STAGE_ID=build-api \
		LOOM_SESSION_ID=session-123 LOOM_WORKTREE_PATH="$WORKTREE" LOOM_WORK_DIR="$WORK" \
		bash "$HOOK" >"$TMP/out" 2>"$TMP/err")
	RC=$?
	set -e
	OUT=$(cat "$TMP/out")
	ERR=$(cat "$TMP/err")
}

# run_pre <command> [extra tool_input JSON] [extra payload JSON]
run_pre() {
	local ti=${2:-} extra=${3:-}
	[[ -n "$ti" ]] || ti='{}'
	[[ -n "$extra" ]] || extra='{}'
	run_payload "$(jq -nc --arg c "$1" --argjson ti "$ti" --argjson p "$extra" --arg cwd "$WORKTREE" \
		'{tool_name:"Bash", tool_input:({command:$c} + $ti), cwd:$cwd} + $p')"
}

# first_line <text> - Print the first line of <text> holding a non-blank character.
first_line() {
	local line
	while IFS= read -r line; do
		[[ "$line" == *[![:space:]]* ]] && { printf '%s' "$line"; return; }
	done <<<"$1"
}

expect_rewrite() {
	run_pre "$@"
	[[ "$RC" == 0 ]] || fail "not rewritten (rc $RC): $1: $ERR"
	[[ "$(jq -r '.hookSpecificOutput.updatedInput.command' <<<"$OUT")" == "$PINNED" ]] ||
		fail "rewrite does not name the pinned command: $1: $OUT"
	jq -e '.hookSpecificOutput | .hookEventName == "PreToolUse" and .permissionDecision == "allow"
		and (.additionalContext | contains("pinned completion command"))' <<<"$OUT" >/dev/null ||
		fail "rewrite is not a PreToolUse allow with context: $1: $OUT"
}

expect_refused() {
	local lines_before=0
	if [[ -f "$LEDGER" ]]; then lines_before=$(wc -l <"$LEDGER"); fi
	run_pre "$@"
	[[ "$RC" == 2 ]] || fail "not refused (rc $RC): $1: $OUT"
	[[ "$OUT" != *updatedInput* ]] || fail "a refused command was rewritten: $1: $OUT"
	[[ "$(first_line "$ERR")" == "[loom-control-complete/not-pinned] LOOM_CONTROL_ERROR: "* ]] ||
		fail "refusal lacks the rule prefix: $1: $ERR"
	[[ "$ERR" == *"$PINNED"* ]] || fail "refusal does not name the pinned command: $1: $ERR"
	[[ "$(wc -l <"$LEDGER")" -eq $((lines_before + 1)) ]] || fail "refusal was not recorded: $1"
	tail -n 1 "$LEDGER" | jq -e --arg c "$1" '.guard == "loom-control-complete"
		and .rule == "not-pinned" and .tool == "Bash" and .stage == "build-api"
		and .subject == ($c | .[0:300])' >/dev/null || fail "wrong ledger line for: $1: $(tail -n 1 "$LEDGER")"
}

# The main agent's exact pinned command gets the hook's own allow, so auto
# mode's classifier never judges it; only timeout and description carry over.
rm -f "$WORK/heartbeat/build-api.json"
expect_rewrite "$PINNED" '{"timeout":300000,"description":"Complete","run_in_background":true,"dangerouslyDisableSandbox":true}'
jq -e '.hookSpecificOutput | .permissionDecisionReason == "loom: pinned completion command"
	and (.updatedInput | .timeout == 300000 and .description == "Complete" and (keys | length) == 3)' \
	<<<"$OUT" >/dev/null || fail "pinned allow carried the wrong reason or fields: $OUT"
jq -e '.activity == "Stage completion running" and .session_id == "session-123"' \
	"$WORK/heartbeat/build-api.json" >/dev/null || fail "pinned allow did not write the heartbeat"

# A subagent's exact pinned command is admitted with no allow (commit-filter.sh
# refuses its completion), and still marks the heartbeat.
rm -f "$WORK/heartbeat/build-api.json"
run_pre "$PINNED" '{}' '{"agent_type":"loom-software-engineer"}'
[[ "$RC" == 0 && -z "$OUT" ]] || fail "subagent pinned command: rc $RC, stdout '$OUT'"
[[ -f "$WORK/heartbeat/build-api.json" ]] || fail "subagent pinned command did not write the heartbeat"

expect_rewrite 'loom stage complete build-api'
expect_rewrite 'loom stage complete build-api 2>&1 | tail -30'
expect_rewrite 'loom stage complete build-api 2 >&1'
expect_rewrite 'loom stage complete build-api | tail -n 15'
expect_rewrite 'loom stage complete build-api|head -n 5'
expect_rewrite "$TMP/bin/loom stage complete build-api 2>&1 | head -5"

# The model's timeout and description carry over; nothing else does.
expect_rewrite 'loom stage complete build-api 2>&1 | tail -30' \
	'{"timeout":300000,"description":"Complete","run_in_background":true,"dangerouslyDisableSandbox":true}'
jq -e '.hookSpecificOutput.updatedInput | .timeout == 300000 and .description == "Complete"
	and (has("run_in_background") | not) and (has("dangerouslyDisableSandbox") | not)
	and (keys | length) == 3' <<<"$OUT" >/dev/null || fail "updatedInput carried the wrong fields: $OUT"
expect_rewrite 'loom stage complete build-api'
jq -e '.hookSpecificOutput.updatedInput | keys == ["command"]' <<<"$OUT" >/dev/null ||
	fail "updatedInput invented fields: $OUT"

# A rewrite marks the heartbeat the stall monitor reads, like the pinned form.
jq -e '.activity == "Stage completion running" and .session_id == "session-123"' \
	"$WORK/heartbeat/build-api.json" >/dev/null || fail "rewrite did not write the heartbeat"

REFUSED=(
	'loom stage complete build-api; rm -rf .'
	'loom stage complete build-api && echo ok'
	'env FOO=1 loom stage complete build-api'
	'FOO=1 loom stage complete build-api'
	'loom stage complete other-stage'
	"bash -c 'loom stage complete build-api'"
	'/tmp/fake/loom stage complete build-api'
	'loom stage complete build-api --force-unsafe'
	'loom stage complete build-api | tail -30 | sh'
	'loom stage complete build-api > out.txt'
	'loom stage complete build-api 2>1'
	'loom stage complete "build-api"'
	'loom stage complete build-api | tail -$(id)'
	'$LOOM_BIN stage complete build-api'
	'loom stage complete build-api 2>&1 2>&1'
	'loom stage complete build-api |& tail -30'
	'loom stage complete build-api | tail -f'
	'loom stage complete build-api | tail -n 15 extra'
	'loom stage complete build-api | cat'
	'loom stage complete build-api | tail -30 &'
	'loom stage complete build-api 2 %%SEP%% 1'
	"loom stage complete build-api | tail -30"$'\n'"true"
	'loom stage complete build-ap\i'
	'LOOM stage complete build-api'
)
for command in "${REFUSED[@]}"; do
	expect_refused "$command"
done
run_pre 'loom stage complete other-stage'
[[ "$ERR" == *'a relative loom path is not allowed'* ]] || fail "a non-rewritable refusal lost today's reason: $ERR"

# A subagent's bare form keeps today's refusal.
expect_refused 'loom stage complete build-api' '{}' '{"agent_type":"loom-software-engineer"}'
[[ "$ERR" == *"retry with the pinned command: $PINNED"* ]] || fail "subagent refusal changed: $ERR"
tail -n 1 "$LEDGER" | jq -e '.role == "subagent"' >/dev/null || fail "subagent block recorded as main"

echo PASS
